//! User-facing notices, shown as toasts in a satellite window.
//!
//! Single channel for messages meant for the person using kosk. Posting is
//! thread-safe, so the config watcher and completion threads can post. The
//! overlay reads [`snapshot`] each frame and the controller path offers each
//! engaged input to [`offer_input`].
//!
//! Every notice needs acknowledgement. Minor events stay in stderr.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use strum::VariantArray;

use crate::controller::{ControllerButton, ControllerInput, ControllerKind};

/// Fade plus slide-in length for a newly shown toast.
const APPEAR: Duration = Duration::from_millis(150);

/// Background notices wait for this much input silence before appearing.
const IDLE_GATE: Duration = Duration::from_secs(1);

/// Repeat failures re-notify at most this often.
const RECURRING_COOLDOWN: Duration = Duration::from_secs(30);

/// Waiting notices beyond the visible one. Overflow folds into a counter.
const QUEUE_CAP: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    fn rank(self) -> u8 {
        match self {
            Severity::Info => 0,
            Severity::Warning => 1,
            Severity::Error => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeAction {
    OpenNextWordGuide,
}

/// Identity for dedup. Variants with data are session-once per value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NoticeKey {
    NewerSettings,
    NotSaved,
    ReloadFailed,
    Wordlist(String),
    TypeFailed,
    RecordFailed,
    UnsupportedButtons(String),
    NextWordSetup,
    OpenConfigFailed,
    SettingsSaveFailed,
    GuideFailed,
}

#[derive(Debug, Clone)]
pub struct Notice {
    pub key: NoticeKey,
    pub severity: Severity,
    pub title: &'static str,
    pub body: String,
    pub action: Option<NoticeAction>,
    /// Wider card for the one-time setup tip.
    pub wide: bool,
    /// Shows without waiting for input silence.
    pub immediate: bool,
}

impl Notice {
    pub fn newer_settings(file_version: i64) -> Self {
        Self {
            key: NoticeKey::NewerSettings,
            severity: Severity::Warning,
            title: "Newer settings ignored",
            body: crate::config_overlay::newer_version_message(file_version),
            action: None,
            wide: false,
            immediate: false,
        }
    }

    pub fn not_saved() -> Self {
        Self {
            key: NoticeKey::NotSaved,
            severity: Severity::Warning,
            title: "Changes not saved",
            body: "You started kosk with a custom settings file. Changes apply until you quit, but are never written to disk.".to_owned(),
            action: None,
            wide: false,
            immediate: false,
        }
    }

    pub fn reload_failed() -> Self {
        Self {
            key: NoticeKey::ReloadFailed,
            severity: Severity::Error,
            title: "Settings failed to reload",
            body: "Your latest settings edit could not be applied. Fix the file and save again."
                .to_owned(),
            action: None,
            wide: false,
            immediate: false,
        }
    }

    /// Settings file was already unusable at launch. Same identity as
    /// [`Self::reload_failed`], but it must show on the first frame.
    pub fn load_failed() -> Self {
        Self {
            key: NoticeKey::ReloadFailed,
            severity: Severity::Error,
            title: "Settings not loaded",
            body: "Your settings file could not be read. kosk is using defaults until you fix the file and save it.".to_owned(),
            action: None,
            wide: false,
            immediate: true,
        }
    }

    pub fn wordlist_missing(name: &str) -> Self {
        Self {
            key: NoticeKey::Wordlist(name.to_owned()),
            severity: Severity::Warning,
            title: "Word list missing",
            body: format!("Suggestions for '{name}' are off. Its word list could not be found."),
            action: None,
            wide: false,
            immediate: false,
        }
    }

    pub fn wordlist_failed(name: &str) -> Self {
        Self {
            key: NoticeKey::Wordlist(name.to_owned()),
            severity: Severity::Warning,
            title: "Word list unreadable",
            body: format!("Suggestions for '{name}' are off. Its word list could not be read."),
            action: None,
            wide: false,
            immediate: false,
        }
    }

    pub fn type_failed() -> Self {
        Self {
            key: NoticeKey::TypeFailed,
            severity: Severity::Error,
            title: "Couldn't type that",
            body: "kosk could not send keys to this app. Click the app you want to type in and try again.".to_owned(),
            action: None,
            wide: false,
            immediate: false,
        }
    }

    pub fn record_needs_location() -> Self {
        Self {
            key: NoticeKey::RecordFailed,
            severity: Severity::Error,
            title: "Couldn't record",
            body: "Recording needs a save location. Add one in settings, then try again."
                .to_owned(),
            action: None,
            wide: false,
            immediate: true,
        }
    }

    pub fn record_failed() -> Self {
        Self {
            key: NoticeKey::RecordFailed,
            severity: Severity::Error,
            title: "Couldn't record",
            body: "Recording could not start. Check the save location in settings.".to_owned(),
            action: None,
            wide: false,
            immediate: true,
        }
    }

    pub fn unsupported_buttons(count: usize) -> Self {
        Self {
            key: NoticeKey::UnsupportedButtons(count.to_string()),
            severity: Severity::Warning,
            title: "Buttons need another controller",
            body: format!(
                "{count} mapped buttons are not on this controller and will not respond."
            ),
            action: None,
            wide: false,
            immediate: false,
        }
    }

    /// One-time setup tip. `None` when it must not show: already shown, the
    /// settings file cannot store the acknowledgement (custom path, unreadable
    /// file, replay, or MCP mode), or suggestions not using next-word tables.
    pub fn next_word_setup_tip(
        already_shown: bool,
        suggestions_on: bool,
        backend_is_ngram: bool,
        can_persist: bool,
    ) -> Option<Self> {
        if already_shown || !suggestions_on || !backend_is_ngram || !can_persist {
            return None;
        }

        Some(Self {
            key: NoticeKey::NextWordSetup,
            severity: Severity::Info,
            title: "Better next-word suggestions",
            body: "kosk can guess your next word after a space.\nThis needs a one-time setup with the completion_build tool.\nWithout it, you only get common words like \"the\".".to_owned(),
            action: Some(NoticeAction::OpenNextWordGuide),
            wide: true,
            immediate: false,
        })
    }

    pub fn open_config_failed() -> Self {
        Self {
            key: NoticeKey::OpenConfigFailed,
            severity: Severity::Error,
            title: "Couldn't open settings",
            body: "The settings file could not be opened in an editor.".to_owned(),
            action: None,
            wide: false,
            immediate: true,
        }
    }

    pub fn settings_save_failed() -> Self {
        Self {
            key: NoticeKey::SettingsSaveFailed,
            severity: Severity::Error,
            title: "Settings not saved",
            body: "Your change applies for now, but it could not be written to disk.".to_owned(),
            action: None,
            wide: false,
            immediate: true,
        }
    }

    pub fn guide_failed() -> Self {
        Self {
            key: NoticeKey::GuideFailed,
            severity: Severity::Error,
            title: "Couldn't open guide",
            body: "The setup guide was saved next to your settings file. Open it by hand."
                .to_owned(),
            action: None,
            wide: false,
            immediate: true,
        }
    }

    fn session_once(&self) -> bool {
        matches!(
            self.key,
            NoticeKey::NewerSettings
                | NoticeKey::NotSaved
                | NoticeKey::Wordlist(_)
                | NoticeKey::UnsupportedButtons(_)
                | NoticeKey::NextWordSetup
        )
    }

    fn cooldown(&self) -> bool {
        matches!(self.key, NoticeKey::ReloadFailed | NoticeKey::TypeFailed)
    }
}

struct Visible {
    notice: Notice,
    shown_at: Instant,
    /// Buttons already down when the toast was shown. A later press of one
    /// of these is not a dismiss.
    baseline: HashSet<ControllerButton>,
}

struct Queued {
    notice: Notice,
    enqueued_at: Instant,
}

struct Store {
    visible: Option<Visible>,
    queue: Vec<Queued>,
    seen_once: HashSet<NoticeKey>,
    last_fire: HashMap<NoticeKey, Instant>,
    last_input: Option<Instant>,
    held: HashSet<ControllerButton>,
    family: ControllerKind,
    overflow_dropped: usize,
}

impl Store {
    fn new() -> Self {
        Self {
            visible: None,
            queue: Vec::new(),
            seen_once: HashSet::new(),
            last_fire: HashMap::new(),
            last_input: None,
            held: HashSet::new(),
            family: ControllerKind::Sc2,
            overflow_dropped: 0,
        }
    }

    fn post(&mut self, notice: Notice, now: Instant) {
        if notice.session_once() && !self.seen_once.insert(notice.key.clone()) {
            return;
        }

        if let Some(visible) = &mut self.visible {
            if visible.notice.key == notice.key {
                keep_immediate(&mut visible.notice, notice);
                return;
            }
        }

        if let Some(queued) = self.queue.iter_mut().find(|q| q.notice.key == notice.key) {
            keep_immediate(&mut queued.notice, notice);
            return;
        }

        if notice.cooldown() {
            if let Some(last) = self.last_fire.get(&notice.key) {
                if now.duration_since(*last) < RECURRING_COOLDOWN {
                    return;
                }
            }

            self.last_fire.insert(notice.key.clone(), now);
        }

        if self.queue.len() >= QUEUE_CAP {
            if let Some(victim) = self
                .queue
                .iter()
                .enumerate()
                .min_by_key(|(_, q)| (q.notice.severity.rank(), q.enqueued_at))
                .map(|(i, _)| i)
            {
                self.queue.remove(victim);
                self.overflow_dropped += 1;
            }
        }

        self.queue.push(Queued {
            notice,
            enqueued_at: now,
        });
    }

    fn offer(&mut self, input: &dyn ControllerInput, now: Instant) -> InputOutcome {
        let held_now: HashSet<ControllerButton> = ControllerButton::VARIANTS
            .iter()
            .copied()
            .filter(|b| input.query(*b))
            .collect();
        self.last_input = Some(now);
        self.family = input.family();

        if self.visible.is_none() {
            self.held = held_now;
            return InputOutcome::Passthrough;
        }

        let fresh: Vec<ControllerButton> = held_now.difference(&self.held).copied().collect();
        self.held = held_now;

        if fresh.is_empty() {
            return InputOutcome::Passthrough;
        }

        let Some(visible) = self.visible.as_ref() else {
            return InputOutcome::Passthrough;
        };
        if fresh.iter().any(|b| visible.baseline.contains(b)) {
            return InputOutcome::Passthrough;
        }

        let visible = self.visible.take().unwrap();
        let open_guide = visible.notice.action == Some(NoticeAction::OpenNextWordGuide)
            && fresh.contains(&ControllerButton::FaceBottom);
        InputOutcome::Swallowed {
            notice: visible.notice,
            open_guide,
        }
    }

    fn view(&mut self, now: Instant) -> Option<ToastSnapshot> {
        self.promote(now);

        let visible = self.visible.as_ref()?;
        let total = self.queue.len() + 1;
        let elapsed = now.duration_since(visible.shown_at);
        Some(ToastSnapshot {
            title: visible.notice.title.to_owned(),
            body: visible.notice.body.clone(),
            severity: visible.notice.severity,
            wide: visible.notice.wide,
            action: visible.notice.action,
            index: 1,
            total,
            more: self.overflow_dropped,
            hint_visible: true,
            family: self.family,
            appear: (elapsed.as_secs_f32() / APPEAR.as_secs_f32()).clamp(0.0, 1.0),
        })
    }
    /// Time until the next queued notice can appear. `None` when nothing is waiting.
    fn queued_delay(&self, now: Instant) -> Option<Duration> {
        if self.visible.is_some() || self.queue.is_empty() {
            return None;
        }

        self.queue.iter().map(|q| self.delay_for(q, now)).min()
    }

    fn delay_for(&self, queued: &Queued, now: Instant) -> Duration {
        if queued.notice.immediate {
            return Duration::ZERO;
        }

        let elapsed = match self.last_input {
            Some(last) => now.saturating_duration_since(last),
            None => now.saturating_duration_since(queued.enqueued_at),
        };
        IDLE_GATE.saturating_sub(elapsed)
    }

    fn idle_since(&self, enqueued_at: Instant, now: Instant) -> bool {
        match self.last_input {
            Some(last) => now.duration_since(last) >= IDLE_GATE,
            None => now.duration_since(enqueued_at) >= IDLE_GATE,
        }
    }

    fn promote(&mut self, now: Instant) {
        if self.visible.is_some() {
            return;
        }

        let ready = self
            .queue
            .iter()
            .position(|q| q.notice.immediate || self.idle_since(q.enqueued_at, now));

        if let Some(i) = ready {
            let queued = self.queue.remove(i);
            self.visible = Some(Visible {
                notice: queued.notice,
                shown_at: now,
                baseline: self.held.clone(),
            });
        }
    }
}

fn store() -> &'static Mutex<Store> {
    static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(Store::new()))
}

fn lock_store() -> std::sync::MutexGuard<'static, Store> {
    store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

type Wake = std::sync::Arc<dyn Fn() + Send + Sync>;

fn wake_slot() -> &'static Mutex<Option<Wake>> {
    static WAKE: OnceLock<Mutex<Option<Wake>>> = OnceLock::new();
    WAKE.get_or_init(|| Mutex::new(None))
}

/// Ask the UI to redraw when a notice is posted. The launch path posts before
/// this exists; later posts (the file watcher) use it to wake an idle window.
pub fn on_posted(callback: impl Fn() + Send + Sync + 'static) {
    *wake_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(std::sync::Arc::new(callback));
}

fn wake() {
    let callback = wake_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if let Some(callback) = callback {
        callback();
    }
}

/// A refresh must not hide a notice that was already allowed to show immediately.
fn keep_immediate(current: &mut Notice, incoming: Notice) {
    let immediate = current.immediate || incoming.immediate;
    *current = incoming;
    current.immediate = immediate;
}

/// Post a notice. Session-once keys show at most once; recurring failures are
/// cooldown-gated; re-posts refresh instead of duplicating.
pub fn notify(notice: Notice) {
    eprintln!("notice: {} — {}", notice.title, notice.body);

    let now = Instant::now();
    lock_store().post(notice, now);
    wake();
}

/// Aggregate key-injection failures into one cooldown-gated notice.
pub fn note_type_failure() {
    notify(Notice::type_failed());
}

/// What an engaged controller input does to the toast layer.
pub enum InputOutcome {
    /// Reaches the current screen unchanged.
    Passthrough,
    /// Dismissed the toast. The press must not reach the screen.
    Swallowed { notice: Notice, open_guide: bool },
}

/// Offer an engaged input. Returns whether the screen may still see it.
///
/// Swallowed presses are a button newly down while the toast is shown. That
/// press dismisses and must not reach the keyboard. A button already down
/// when the toast appeared passes through. Call only for engaged snapshots,
/// never the idle path.
pub fn offer_input(input: &dyn ControllerInput) -> InputOutcome {
    lock_store().offer(input, Instant::now())
}

/// No engaged input this tick. Clears edge baselines without touching timing.
pub fn note_no_input() {
    lock_store().held.clear();
}

/// What the satellite window should draw this frame, if anything.
#[derive(Debug, Clone)]
pub struct ToastSnapshot {
    pub title: String,
    pub body: String,
    pub severity: Severity,
    pub wide: bool,
    pub action: Option<NoticeAction>,
    /// 1-based position among visible + waiting.
    pub index: usize,
    pub total: usize,
    /// Collapsed overflow beyond the queue.
    pub more: usize,
    /// Grace period over: the dismiss hint may show.
    pub hint_visible: bool,
    pub family: ControllerKind,
    /// 0 = just appeared, 1 = fully in.
    pub appear: f32,
}

pub fn snapshot() -> Option<ToastSnapshot> {
    snapshot_at(Instant::now())
}

/// How long until a queued notice can appear. `None` when nothing is waiting.
pub fn queued_delay() -> Option<Duration> {
    lock_store().queued_delay(Instant::now())
}

fn snapshot_at(now: Instant) -> Option<ToastSnapshot> {
    lock_store().view(now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::test_input::ButtonSetInput;

    const BANNED: &[&str] = &[
        "config_version",
        "controller_map",
        "toggleRecord",
        "key sink",
        "process_events",
        "ReturnState",
        "faceTop",
        "faceBottom",
        "KOSK_CONTROLLER_MCP",
        "record_file",
        "config.toml",
        "{{",
    ];

    fn all_notices() -> Vec<Notice> {
        vec![
            Notice::newer_settings(99),
            Notice::not_saved(),
            Notice::reload_failed(),
            Notice::load_failed(),
            Notice::wordlist_missing("browser"),
            Notice::wordlist_failed("browser"),
            Notice::type_failed(),
            Notice::record_needs_location(),
            Notice::record_failed(),
            Notice::unsupported_buttons(2),
            Notice::open_config_failed(),
            Notice::settings_save_failed(),
            Notice::guide_failed(),
        ]
    }

    #[test]
    fn visible_text_is_user_facing() {
        for notice in all_notices() {
            let text = format!("{} {}", notice.title, notice.body);
            for banned in BANNED {
                assert!(!text.contains(banned), "{:?} leaks {banned:?}", notice.key);
            }
        }

        let tip = Notice::next_word_setup_tip(false, true, true, true).unwrap();
        let text = format!("{} {}", tip.title, tip.body);
        let without_tool = text.replace("completion_build", "");
        for banned in BANNED {
            assert!(!without_tool.contains(banned), "tip leaks {banned:?}");
        }

        for notice in all_notices() {
            assert!(
                notice.title.split_whitespace().count() <= 4,
                "{:?} title too long: {:?}",
                notice.key,
                notice.title
            );
        }
    }

    #[test]
    fn tip_gates() {
        assert!(Notice::next_word_setup_tip(false, true, true, true).is_some());
        assert!(Notice::next_word_setup_tip(true, true, true, true).is_none());
        assert!(Notice::next_word_setup_tip(false, false, true, true).is_none());
        assert!(Notice::next_word_setup_tip(false, true, false, true).is_none());
        assert!(Notice::next_word_setup_tip(false, true, true, false).is_none());
    }

    fn buttons(set: &[ControllerButton]) -> ButtonSetInput {
        ButtonSetInput(set.iter().copied().collect())
    }

    #[test]
    fn immediate_promotes_without_idle() {
        let mut store = Store::new();
        let t0 = Instant::now();
        store.post(Notice::record_failed(), t0);
        let shown = store.view(t0).map(|s| s.title);
        assert_eq!(shown.as_deref(), Some("Couldn't record"));
    }

    #[test]
    fn load_failure_shows_while_input_is_active() {
        let mut store = Store::new();
        let t0 = Instant::now();
        store.offer(&buttons(&[ControllerButton::FaceBottom]), t0);
        store.post(Notice::load_failed(), t0);
        assert_eq!(
            store.view(t0).map(|s| s.title),
            Some("Settings not loaded".to_owned())
        );

        let mut store = Store::new();
        store.offer(&buttons(&[ControllerButton::FaceBottom]), t0);
        store.post(Notice::load_failed(), t0);
        store.post(Notice::reload_failed(), t0);
        assert!(store.view(t0).is_some());
    }

    #[test]
    fn deferred_notice_reports_remaining_wait() {
        let mut store = Store::new();
        let t0 = Instant::now();
        store.post(Notice::reload_failed(), t0);
        let wait = store.queued_delay(t0).expect("waiting");
        assert!(wait + Duration::from_millis(50) >= IDLE_GATE);
        assert_eq!(store.queued_delay(t0 + IDLE_GATE), Some(Duration::ZERO));
        assert!(store.view(t0 + IDLE_GATE).is_some());
        assert!(store.queued_delay(t0 + IDLE_GATE).is_none());
    }

    #[test]
    fn background_waits_for_idle() {
        let mut store = Store::new();
        let t0 = Instant::now();
        store.post(Notice::reload_failed(), t0);
        assert!(store.view(t0).is_none());
        assert!(store
            .view(t0 + IDLE_GATE + Duration::from_millis(10))
            .is_some());
    }

    #[test]
    fn session_once_never_repeats() {
        let mut store = Store::new();
        let t0 = Instant::now();
        store.post(Notice::not_saved(), t0);
        store.post(Notice::not_saved(), t0);
        assert_eq!(store.queue.len(), 1);
    }

    #[test]
    fn cooldown_gates_recurring() {
        let mut store = Store::new();
        let t0 = Instant::now();
        store.post(Notice::reload_failed(), t0);
        // Full cycle: show, dismiss, re-post inside the cooldown.
        assert!(store.view(t0).is_none());
        let shown_at = t0 + IDLE_GATE + Duration::from_millis(10);
        assert!(store.view(shown_at).is_some());
        store.offer(&buttons(&[]), shown_at);
        let dismiss_at = shown_at + Duration::from_millis(20);
        assert!(matches!(
            store.offer(&buttons(&[ControllerButton::FaceRight]), dismiss_at),
            InputOutcome::Swallowed { .. }
        ));
        store.post(Notice::reload_failed(), dismiss_at);
        assert_eq!(store.queue.len(), 0);
        store.post(Notice::reload_failed(), t0 + RECURRING_COOLDOWN);
        assert_eq!(store.queue.len(), 1);
    }

    #[test]
    fn queue_cap_folds_overflow() {
        let mut store = Store::new();
        let t0 = Instant::now();
        for i in 0..6 {
            store.post(Notice::wordlist_missing(&format!("app{i}")), t0);
        }
        assert_eq!(store.queue.len(), QUEUE_CAP);
        assert_eq!(store.overflow_dropped, 2);
    }

    #[test]
    fn press_released_before_display_can_dismiss() {
        let mut store = Store::new();
        let t0 = Instant::now();
        let launch = buttons(&[ControllerButton::R4]);
        store.offer(&launch, t0);
        store.held.clear();
        store.post(Notice::load_failed(), t0);
        assert!(store.view(t0).is_some());

        let late = t0 + Duration::from_secs(2);
        assert!(matches!(
            store.offer(&launch, late),
            InputOutcome::Swallowed { .. }
        ));
        assert!(store.view(late).is_none());
    }

    #[test]
    fn dismiss_press_is_swallowed() {
        let mut store = Store::new();
        let t0 = Instant::now();
        store.post(Notice::record_failed(), t0);
        assert!(store.view(t0).is_some());

        match store.offer(
            &buttons(&[ControllerButton::TriggerRight]),
            t0 + Duration::from_millis(20),
        ) {
            InputOutcome::Swallowed { notice, open_guide } => {
                assert_eq!(notice.key, NoticeKey::RecordFailed);
                assert!(!open_guide);
            }
            InputOutcome::Passthrough => panic!("dismiss press reached the keyboard"),
        }
        assert!(store.view(t0 + Duration::from_millis(20)).is_none());
    }

    #[test]
    fn held_before_toast_does_not_dismiss() {
        let mut store = Store::new();
        let t0 = Instant::now();
        let held = buttons(&[ControllerButton::FaceRight]);
        store.offer(&held, t0);

        store.post(Notice::record_failed(), t0);
        // Immediate notice promotes at once with the held button as baseline.
        let at = t0 + Duration::from_millis(20);
        assert!(store.view(t0).is_some());
        assert!(matches!(store.offer(&held, at), InputOutcome::Passthrough));

        // Release and press again: fresh edge, but still the baseline button.
        store.offer(&buttons(&[]), at);
        assert!(matches!(
            store.offer(&held, at + Duration::from_millis(10)),
            InputOutcome::Passthrough
        ));
    }

    #[test]
    fn tip_face_bottom_opens_guide() {
        let tip = || Notice::next_word_setup_tip(false, true, true, true).unwrap();
        let t0 = Instant::now();
        let shown_at = t0 + IDLE_GATE + Duration::from_millis(10);
        let dismiss_at = shown_at + Duration::from_millis(20);

        let mut store = Store::new();
        store.post(tip(), t0);
        assert!(store.view(shown_at).is_some());
        store.offer(&buttons(&[]), shown_at);
        match store.offer(&buttons(&[ControllerButton::FaceBottom]), dismiss_at) {
            InputOutcome::Swallowed { notice, open_guide } => {
                assert_eq!(notice.key, NoticeKey::NextWordSetup);
                assert!(open_guide);
            }
            InputOutcome::Passthrough => panic!("expected swallow"),
        }

        let mut store = Store::new();
        store.post(tip(), t0);
        assert!(store.view(shown_at).is_some());
        store.offer(&buttons(&[]), shown_at);
        match store.offer(&buttons(&[ControllerButton::FaceRight]), dismiss_at) {
            InputOutcome::Swallowed { notice, open_guide } => {
                assert_eq!(notice.key, NoticeKey::NextWordSetup);
                assert!(!open_guide);
            }
            InputOutcome::Passthrough => panic!("expected swallow"),
        }
    }
}

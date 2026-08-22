use crate::config;
use crate::controller::record as input_record;
use crate::controller::ControllerBinding;
use crate::state::{StateId, WindowPos};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventSource {
    MouseClick,
    /// Physical controller binding that produced this commit (per-button debounce bucket).
    Controller(ControllerBinding),
    /// Programmatic events enqueued by `on_return` / other in-process follow-ups.
    /// Not a controller hold and not a mouse click — `end_controller_tick` must ignore it
    /// the same way it ignores `MouseClick` (only `Controller(_)` participates in hold tracking).
    FollowUp,
}

impl fmt::Display for EventSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventSource::MouseClick => f.write_str("mouse"),
            EventSource::Controller(b) => fmt::Display::fmt(b, f),
            EventSource::FollowUp => f.write_str("followUp"),
        }
    }
}

/// Result delivered by a callee via [`Event::ReturnState`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnStateResult {
    Value(String),
    Cancelled,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    SendKey(enigo::Key, enigo::Direction),
    SendText(String),
    ChangeState(StateId),
    /// Push the current mode and switch to `StateId` (sub-UI call).
    CallState(StateId),
    /// Pop the call stack, switch to the caller, deliver an explicit result.
    /// Never encode cancel as an empty string — use [`ReturnStateResult::Cancelled`].
    ReturnState(ReturnStateResult),
    /// Request an egui frame without changing `StateId`.
    /// Use when UI data changed in-place (draft pills, status, dirty flag) and no
    /// Change/Call/Return already covers the refresh.
    Repaint,
    MoveWindow(WindowPos),
    FlipWindowLeftRight,
    FlipWindowAboveBelow,
    RotateWindow,
    Exit,
    ToggleRecord,
    ToggleShift,
    ToggleCtrl,
    ToggleAlt,
}

impl Event {
    /// Sticky modifier toggles: at most one accept per key gesture (see
    /// [`SourceState::suppress_until_release`]).
    fn suppresses_until_release(&self) -> bool {
        matches!(
            self,
            Event::ToggleShift | Event::ToggleCtrl | Event::ToggleAlt
        )
    }
}

/// One debounce decision: a single leaf [`Event`] or a flat sequence of them.
/// `Seq` holds [`Event`], not `EventGroup`, so groups cannot nest.
enum EventGroup {
    Single(Event),
    Seq(Vec<Event>),
}

impl EventGroup {
    fn from_seq(events: Vec<Event>) -> Option<Self> {
        match events.len() {
            0 => None,
            1 => Some(EventGroup::Single(
                events.into_iter().next().expect("len == 1"),
            )),
            _ => Some(EventGroup::Seq(events)),
        }
    }

    fn events(&self) -> impl Iterator<Item = &Event> {
        match self {
            EventGroup::Single(event) => std::slice::from_ref(event).iter(),
            EventGroup::Seq(events) => events.iter(),
        }
    }
}

struct SourceState {
    last: Instant,
    repeat_armed: bool,
    /// Binding went up since [`SourceState::last`]. Next accept is a new press, not a hold.
    released: bool,
    /// After a sticky modifier toggle accept, drop further toggles until release or
    /// selection change clears this flag.
    suppress_until_release: bool,
}

/// Queues outgoing events with optional time-based debouncing by [`EventSource`].
///
/// Each source has its own timeline: alternating sources (e.g. both triggers held)
/// cannot bypass the debounce window for either.
///
/// Repeat arms only while the same controller binding stays down. A release clears
/// the armed flag so a second tap (double letters) uses the initial delay again
/// instead of the short repeat interval.
///
/// [`push`](Self::push) submits one event. [`push_seq`](Self::push_seq) submits several
/// that succeed or fail together (one debounce decision).
pub struct EventQueue {
    pending: Vec<(Event, EventSource)>,
    debounce_initial: Option<Duration>,
    debounce_repeat: Option<Duration>,
    last_commit: HashMap<EventSource, SourceState>,
    /// Controller sources that [`push`](Self::push)/[`push_seq`](Self::push_seq)
    /// this evaluation tick (including debounce drops). Used to detect release.
    held_this_tick: HashSet<EventSource>,
    #[cfg(test)]
    test_now: Option<Instant>,
}

impl EventQueue {
    /// `initial_ms == 0` disables debouncing (same as [`passthrough`](Self)).
    /// `repeat_ms == 0` uses the initial interval for every repeat step.
    pub fn new() -> Self {
        let cfg = config::get();
        let initial_ms = cfg.event_debounce_ms;
        let repeat_ms = cfg.event_debounce_repeat_ms;
        if initial_ms == 0 {
            return Self::passthrough();
        }
        Self::with_debounce(
            Some(Duration::from_millis(initial_ms)),
            if repeat_ms == 0 {
                None
            } else {
                Some(Duration::from_millis(repeat_ms))
            },
        )
    }

    fn with_debounce(initial: Option<Duration>, repeat: Option<Duration>) -> Self {
        Self {
            pending: Vec::new(),
            debounce_initial: initial,
            debounce_repeat: repeat,
            last_commit: HashMap::new(),
            held_this_tick: HashSet::new(),
            #[cfg(test)]
            test_now: None,
        }
    }

    /// `initial_ms == 0` disables debouncing.
    pub fn set_debounce_ms(&mut self, initial_ms: u64, repeat_ms: u64) {
        if initial_ms == 0 {
            self.debounce_initial = None;
            self.debounce_repeat = None;
        } else {
            self.debounce_initial = Some(Duration::from_millis(initial_ms));
            self.debounce_repeat = if repeat_ms == 0 {
                None
            } else {
                Some(Duration::from_millis(repeat_ms))
            };
        }
    }

    /// No debouncing; every [`push`](Self::push) and [`push_seq`](Self::push_seq) is accepted.
    pub fn passthrough() -> Self {
        Self::with_debounce(None, None)
    }

    /// Submits one event. Sticky modifier toggles use suppress-until-release; everything
    /// else uses hold-repeat.
    pub fn push(&mut self, event: Event, source: &EventSource) -> bool {
        self.commit(EventGroup::Single(event), source)
    }

    /// Submits several events as one debounce decision. An empty list is a no-op
    /// (`true`, no debounce). A single-element list is treated as [`push`](Self::push).
    /// Two or more events always use hold-repeat.
    pub fn push_seq(&mut self, events: Vec<Event>, source: &EventSource) -> bool {
        match EventGroup::from_seq(events) {
            Some(group) => self.commit(group, source),
            None => true,
        }
    }

    /// Call once per controller evaluation after bindings have run.
    ///
    /// Any controller source that did not [`push`](Self::push) or [`push_seq`](Self::push_seq)
    /// this tick (including dropped repeats) is treated as released, so the next
    /// accept is a new first press rather than key-repeat.
    pub fn end_controller_tick(&mut self) {
        for (source, state) in self.last_commit.iter_mut() {
            if matches!(source, EventSource::Controller(_)) && !self.held_this_tick.contains(source)
            {
                state.released = true;
                state.repeat_armed = false;
                state.suppress_until_release = false;
            }
        }
        self.held_this_tick.clear();
    }

    /// Clears sticky-modifier suppression for the given sources (e.g. when stick
    /// selection moves to another key while a trigger stays held).
    pub fn clear_toggle_suppress(&mut self, sources: impl IntoIterator<Item = EventSource>) {
        for source in sources {
            if let Some(state) = self.last_commit.get_mut(&source) {
                state.suppress_until_release = false;
            }
        }
    }

    /// Takes queued events for processing. Groups are flattened when accepted, so
    /// this returns leaf [`Event`]s only.
    pub fn drain_pending(&mut self) -> Vec<(Event, EventSource)> {
        std::mem::take(&mut self.pending)
    }

    /// Append leaf events directly onto `pending`, bypassing debounce / `commit`.
    /// Used by `process_events` to merge `on_return` follow-ups.
    pub fn extend_pending(&mut self, events: impl IntoIterator<Item = (Event, EventSource)>) {
        self.pending.extend(events);
    }

    fn touch(&mut self, source: &EventSource) {
        if matches!(source, EventSource::Controller(_)) {
            self.held_this_tick.insert(source.clone());
        }
    }

    fn now(&self) -> Instant {
        #[cfg(test)]
        if let Some(t) = self.test_now {
            return t;
        }
        Instant::now()
    }

    fn commit(&mut self, group: EventGroup, source: &EventSource) -> bool {
        self.touch(source);
        let allowed = match &group {
            EventGroup::Single(event) => self.commit_allows(source, event),
            EventGroup::Seq(_) => self.hold_repeat_allows(source),
        };
        if !allowed {
            self.trace_debounce(false, source);
            return false;
        }
        self.trace_debounce(true, source);
        match &group {
            EventGroup::Single(event) => self.record_commit(source.clone(), event),
            EventGroup::Seq(_) => self.record_hold_repeat_accepted(source.clone()),
        }
        self.pending
            .extend(group.events().cloned().map(|e| (e, source.clone())));
        true
    }

    fn commit_allows(&self, source: &EventSource, event: &Event) -> bool {
        if event.suppresses_until_release() {
            return self.toggle_allows(source);
        }
        self.hold_repeat_allows(source)
    }

    fn toggle_allows(&self, source: &EventSource) -> bool {
        let Some(state) = self.last_commit.get(source) else {
            return true;
        };
        !state.suppress_until_release || state.released
    }

    fn hold_repeat_allows(&self, source: &EventSource) -> bool {
        let Some(initial) = self.debounce_initial else {
            return true;
        };
        let repeat = self.debounce_repeat.unwrap_or(initial);
        let Some(state) = self.last_commit.get(source) else {
            return true;
        };
        let elapsed = self.now().duration_since(state.last);
        if state.repeat_armed {
            elapsed >= repeat
        } else {
            elapsed >= initial
        }
    }

    fn record_commit(&mut self, source: EventSource, event: &Event) {
        if event.suppresses_until_release() {
            self.record_toggle_accepted(source);
            return;
        }
        self.record_hold_repeat_accepted(source);
    }

    fn record_toggle_accepted(&mut self, source: EventSource) {
        let now = self.now();
        let is_mouse = matches!(source, EventSource::MouseClick);
        self.last_commit.insert(
            source,
            SourceState {
                last: now,
                repeat_armed: false,
                released: is_mouse,
                suppress_until_release: true,
            },
        );
    }

    fn record_hold_repeat_accepted(&mut self, source: EventSource) {
        let Some(initial) = self.debounce_initial else {
            return;
        };
        let repeat = self.debounce_repeat.unwrap_or(initial);
        let now = self.now();
        let prev = self.last_commit.get(&source);
        let released = prev.is_some_and(|s| s.released);
        // Hold-repeat: arm only if the binding never went up since the last accept
        // and the gap still looks like a continuous stream.
        let repeat_armed = if released {
            false
        } else {
            prev.is_some_and(|s| {
                let e = now
                    .duration_since(s.last)
                    .saturating_sub(Duration::from_millis(40));
                if s.repeat_armed {
                    e <= repeat
                } else {
                    e <= initial.saturating_add(repeat)
                }
            })
        };
        // Mouse is edge-triggered; never start a hold-repeat chain from clicks.
        // A controller accept means the binding is down, so clear `released`.
        let is_mouse = matches!(source, EventSource::MouseClick);
        let suppress_until_release = prev.is_some_and(|s| s.suppress_until_release);
        self.last_commit.insert(
            source,
            SourceState {
                last: now,
                repeat_armed,
                released: is_mouse,
                suppress_until_release,
            },
        );
    }

    fn trace_debounce(&self, accept: bool, source: &EventSource) {
        let (elapsed_us, armed) = match self.last_commit.get(source) {
            Some(state) => (
                self.now().duration_since(state.last).as_micros() as u64,
                state.repeat_armed,
            ),
            None => (0, false),
        };
        input_record::session().tap_debounce(accept, &source.to_string(), elapsed_us, armed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::{ControllerBinding, ControllerButton};

    fn pad_right() -> EventSource {
        EventSource::Controller(ControllerBinding::Single(ControllerButton::PadRight))
    }

    fn queue() -> EventQueue {
        let mut q = EventQueue::passthrough();
        q.set_debounce_ms(300, 55);
        q.test_now = Some(Instant::now());
        q
    }

    fn advance(q: &mut EventQueue, ms: u64) {
        let now = q.now() + Duration::from_millis(ms);
        q.test_now = Some(now);
    }

    fn commit_letter(q: &mut EventQueue, src: &EventSource) -> bool {
        q.push(Event::SendText("l".into()), src)
    }

    fn accepted_texts(q: &mut EventQueue) -> Vec<String> {
        q.drain_pending()
            .into_iter()
            .filter_map(|(e, _)| match e {
                Event::SendText(s) => Some(s),
                _ => None,
            })
            .collect()
    }

    fn commit_toggle(q: &mut EventQueue, src: &EventSource) -> bool {
        q.push(Event::ToggleShift, src)
    }

    fn accepted_toggles(q: &mut EventQueue) -> usize {
        q.drain_pending()
            .into_iter()
            .filter(|(e, _)| matches!(e, Event::ToggleShift))
            .count()
    }

    #[test]
    fn toggle_hold_suppresses_repeat_until_release() {
        let mut q = queue();
        let src = pad_right();

        assert!(commit_toggle(&mut q, &src));
        q.end_controller_tick();
        assert_eq!(accepted_toggles(&mut q), 1);

        assert!(!commit_toggle(&mut q, &src));
        q.end_controller_tick();
        assert_eq!(accepted_toggles(&mut q), 0);

        advance(&mut q, 500);
        assert!(!commit_toggle(&mut q, &src));
        q.end_controller_tick();
        assert_eq!(accepted_toggles(&mut q), 0);

        q.end_controller_tick();
        assert!(commit_toggle(&mut q, &src));
        q.end_controller_tick();
        assert_eq!(accepted_toggles(&mut q), 1);
    }

    #[test]
    fn toggle_suppress_cleared_when_selection_changes() {
        let mut q = queue();
        let src = pad_right();

        assert!(commit_toggle(&mut q, &src));
        q.end_controller_tick();
        assert_eq!(accepted_toggles(&mut q), 1);

        q.clear_toggle_suppress([src.clone()]);
        assert!(commit_toggle(&mut q, &src));
        q.end_controller_tick();
        assert_eq!(accepted_toggles(&mut q), 1);
    }

    #[test]
    fn hold_arms_repeat_after_initial_delay() {
        let mut q = queue();
        let src = pad_right();

        assert!(commit_letter(&mut q, &src));
        q.end_controller_tick();
        assert_eq!(accepted_texts(&mut q), ["l"]);

        advance(&mut q, 300);
        assert!(commit_letter(&mut q, &src));
        q.end_controller_tick();
        assert_eq!(accepted_texts(&mut q), ["l"]);

        advance(&mut q, 55);
        assert!(commit_letter(&mut q, &src));
        q.end_controller_tick();
        assert_eq!(accepted_texts(&mut q), ["l"]);
    }

    #[test]
    fn release_then_second_tap_does_not_arm_repeat() {
        let mut q = queue();
        let src = pad_right();

        assert!(commit_letter(&mut q, &src));
        q.end_controller_tick();
        assert_eq!(accepted_texts(&mut q), ["l"]);

        // Button up (no commit this tick).
        q.end_controller_tick();

        advance(&mut q, 301);
        assert!(commit_letter(&mut q, &src));
        q.end_controller_tick();
        assert_eq!(accepted_texts(&mut q), ["l"]);

        // Still held, but this is a new press: 55ms is inside the initial window.
        advance(&mut q, 55);
        assert!(!commit_letter(&mut q, &src));
        q.end_controller_tick();
        assert!(accepted_texts(&mut q).is_empty());

        advance(&mut q, 245);
        assert!(commit_letter(&mut q, &src));
        q.end_controller_tick();
        assert_eq!(accepted_texts(&mut q), ["l"]);

        // Holding the second press now arms like a normal first hold.
        advance(&mut q, 55);
        assert!(commit_letter(&mut q, &src));
        q.end_controller_tick();
        assert_eq!(accepted_texts(&mut q), ["l"]);
    }

    #[test]
    fn hello_ll_two_taps_are_two_letters() {
        let mut q = queue();
        let src = pad_right();

        assert!(commit_letter(&mut q, &src));
        q.end_controller_tick();
        q.end_controller_tick();

        advance(&mut q, 301);
        assert!(commit_letter(&mut q, &src));
        q.end_controller_tick();
        // Short second click (~117ms), then release — must not emit extras.
        advance(&mut q, 55);
        assert!(!commit_letter(&mut q, &src));
        q.end_controller_tick();
        advance(&mut q, 55);
        assert!(!commit_letter(&mut q, &src));
        q.end_controller_tick();
        q.end_controller_tick();

        let letters: Vec<_> = q
            .drain_pending()
            .into_iter()
            .filter_map(|(e, _)| match e {
                Event::SendText(s) => Some(s),
                _ => None,
            })
            .collect();
        assert_eq!(letters, ["l", "l"]);
    }

    #[test]
    fn follow_up_display_and_extend_pending() {
        assert_eq!(EventSource::FollowUp.to_string(), "followUp");
        let mut q = EventQueue::passthrough();
        q.extend_pending([
            (Event::Repaint, EventSource::FollowUp),
            (Event::ChangeState(StateId::Menu), EventSource::FollowUp),
        ]);
        let drained = q.drain_pending();
        assert_eq!(drained.len(), 2);
        assert!(matches!(drained[0].0, Event::Repaint));
        assert_eq!(drained[0].1, EventSource::FollowUp);
        assert!(matches!(drained[1].0, Event::ChangeState(StateId::Menu)));
        q.end_controller_tick();
    }
}

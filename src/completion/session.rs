use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::Result;

use super::backend::{backend_from_config, Abort, Candidate, CompletionBackend};
use super::context::CompletionContext;
use super::settings::{CompletionConfig, Preselect};
use super::typed_log::{LogEvent, TypedLog};
use super::user_cache::{resolve_cache_path, UserCache};

struct Request {
    gen: u64,
    ctx: CompletionContext,
}

pub struct Batch {
    pub gen: u64,
    pub candidates: Vec<Candidate>,
}

struct Slot {
    pending: Option<Request>,
    shutdown: bool,
}

pub struct AcceptOutcome {
    pub inject: String,
    pub via: super::settings::AcceptVia,
    pub token_char_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EatAcceptSpace {
    pub space_after: bool,
}

pub struct Session {
    slot: Arc<(Mutex<Slot>, Condvar)>,
    gen: Arc<AtomicU64>,
    join: Option<JoinHandle<()>>,
    rx: Receiver<Batch>,
    candidates: Vec<Candidate>,
    highlight: Option<usize>,
    current_gen: u64,
    typed: TypedLog,
    pending_ctx: Option<CompletionContext>,
    pending_at: Option<Instant>,
    debounce: Duration,
    notify: Arc<dyn Fn() + Send + Sync>,
    cfg: CompletionConfig,
    user: Arc<Mutex<UserCache>>,
    neighbors: HashMap<char, Vec<char>>,
    pending_eat_space: bool,
}

impl Session {
    pub fn spawn(
        cfg: CompletionConfig,
        config_dir: Option<&std::path::Path>,
        notify: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self> {
        let user = Arc::new(Mutex::new(UserCache::load(
            &cfg.user_cache,
            resolve_cache_path(config_dir, &cfg.user_cache.path),
        )));
        let backend = backend_from_config(&cfg, config_dir, Arc::clone(&user))?;

        let (tx, rx) = mpsc::channel();
        let slot = Arc::new((
            Mutex::new(Slot {
                pending: None,
                shutdown: false,
            }),
            Condvar::new(),
        ));
        let gen = Arc::new(AtomicU64::new(0));
        let join = std::thread::Builder::new()
            .name("kosk-completion".into())
            .spawn({
                let slot = Arc::clone(&slot);
                let gen = Arc::clone(&gen);
                let backend = Arc::clone(&backend);
                let notify = Arc::clone(&notify);
                move || worker_loop(backend, slot, gen, tx, notify)
            })?;

        Ok(Self {
            slot,
            gen,
            join: Some(join),
            rx,
            candidates: Vec::new(),
            highlight: None,
            current_gen: 0,
            typed: TypedLog::new(cfg.keyboard.start_armed),
            pending_ctx: None,
            pending_at: None,
            debounce: Duration::from_millis(cfg.debounce_ms),
            notify,
            cfg,
            user,
            neighbors: HashMap::new(),
            pending_eat_space: false,
        })
    }

    #[cfg(test)]
    pub fn spawn_for_test(
        backend: Arc<dyn CompletionBackend>,
        cfg: CompletionConfig,
        notify: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let user = Arc::new(Mutex::new(UserCache::load(
            &cfg.user_cache,
            std::path::PathBuf::from("target/kosk-session-test-cache.bin"),
        )));
        let (tx, rx) = mpsc::channel();
        let slot = Arc::new((
            Mutex::new(Slot {
                pending: None,
                shutdown: false,
            }),
            Condvar::new(),
        ));
        let gen = Arc::new(AtomicU64::new(0));
        let join = std::thread::Builder::new()
            .name("kosk-completion-test".into())
            .spawn({
                let slot = Arc::clone(&slot);
                let gen = Arc::clone(&gen);
                let backend = Arc::clone(&backend);
                let notify = Arc::clone(&notify);
                move || worker_loop(backend, slot, gen, tx, notify)
            })
            .unwrap();
        Self {
            slot,
            gen,
            join: Some(join),
            rx,
            candidates: Vec::new(),
            highlight: None,
            current_gen: 0,
            typed: TypedLog::new(cfg.keyboard.start_armed),
            pending_ctx: None,
            pending_at: None,
            debounce: Duration::from_millis(cfg.debounce_ms),
            notify,
            cfg,
            user,
            neighbors: HashMap::new(),
            pending_eat_space: false,
        }
    }

    pub fn cfg(&self) -> &CompletionConfig {
        &self.cfg
    }

    pub fn set_neighbors(&mut self, neighbors: HashMap<char, Vec<char>>) {
        self.neighbors = neighbors;
    }

    pub fn armed(&self) -> bool {
        self.typed.armed()
    }

    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }

    pub fn highlight(&self) -> Option<usize> {
        self.highlight
    }

    pub fn typed_text(&self) -> &str {
        self.typed.text()
    }

    pub fn last_injected(&self) -> Option<&str> {
        self.typed.last_injected()
    }

    pub fn set_last_injected(&mut self, s: Option<String>) {
        self.typed.set_last_injected(s);
    }

    pub fn toggle_armed(&mut self) {
        self.pending_eat_space = false;
        self.typed.toggle(&self.cfg.keyboard);
        self.candidates.clear();
        self.highlight = None;
        if self.typed.armed() {
            (self.notify)();
        }
    }

    pub fn note_log(&mut self, event: LogEvent, payload: &str) {
        match event {
            LogEvent::Char(_) | LogEvent::Text => {}
            _ => self.pending_eat_space = false,
        }
        self.typed.apply(event, payload, &self.cfg.keyboard);
        if !self.typed.armed() {
            self.candidates.clear();
            self.highlight = None;
        }
    }

    pub fn arm_eat_accept_space(&mut self) {
        self.pending_eat_space = !self.cfg.eat_space_before.is_empty();
    }

    pub fn clear_eat_accept_space(&mut self) {
        self.pending_eat_space = false;
    }

    pub fn take_eat_accept_space(&mut self, ch: char) -> Option<EatAcceptSpace> {
        if !self.pending_eat_space {
            return None;
        }

        self.pending_eat_space = false;

        if !super::apply::eats_accept_space(ch, &self.cfg.eat_space_before) {
            return None;
        }

        Some(EatAcceptSpace {
            space_after: super::apply::eats_accept_space(ch, &self.cfg.space_after),
        })
    }

    pub fn request_from_buffer(&mut self, text: &str, cursor: usize) {
        if !self.cfg.enabled || !self.typed.armed() {
            return;
        }
        let Some(mut ctx) = CompletionContext::from_buffer(text, cursor, &self.cfg) else {
            return;
        };
        ctx.neighbors = self.neighbors.clone();
        self.pending_ctx = Some(ctx);
        self.pending_at = Some(Instant::now());
    }

    pub fn poll(&mut self) {
        if self.pending_ctx.is_some() {
            let due = self
                .pending_at
                .map(|t| t.elapsed() >= self.debounce)
                .unwrap_or(true);
            if due {
                let ctx = self.pending_ctx.take().unwrap();
                self.request_now(ctx);
            }
        }

        loop {
            match self.rx.try_recv() {
                Ok(batch) => {
                    if batch.gen != self.current_gen {
                        continue;
                    }
                    self.apply_candidates(batch.candidates);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
    }

    fn apply_candidates(&mut self, candidates: Vec<Candidate>) {
        let prev = self
            .highlight
            .and_then(|i| self.candidates.get(i).map(|c| c.text.clone()));

        self.candidates = candidates;

        if let Some(text) = prev {
            if let Some(i) = self.candidates.iter().position(|c| c.text == text) {
                self.highlight = Some(i);
                return;
            }
        }

        if self.cfg.reset_highlight_on_refresh {
            self.highlight = match self.cfg.preselect {
                Preselect::None => None,
                Preselect::First if !self.candidates.is_empty() => Some(0),
                Preselect::First => None,
            };
        } else if let Some(h) = self.highlight {
            if h >= self.candidates.len() {
                self.highlight = None;
            }
        }
    }

    fn request_now(&mut self, ctx: CompletionContext) {
        let (lock, cv) = &*self.slot;
        let mut s = lock.lock().unwrap();
        let gen = self.gen.fetch_add(1, Ordering::Relaxed) + 1;
        self.current_gen = gen;
        s.pending = Some(Request { gen, ctx });
        cv.notify_one();
    }

    pub fn cycle(&mut self, forward: bool) {
        if !self.typed.armed() || self.candidates.is_empty() {
            return;
        }
        let n = self.candidates.len();
        self.highlight = match self.highlight {
            None if forward => Some(0),
            None => Some(n - 1),
            Some(i) => {
                let next = if forward {
                    i as isize + 1
                } else {
                    i as isize - 1
                };
                if self.cfg.highlight_wraps {
                    Some((next.rem_euclid(n as isize)) as usize)
                } else if next < 0 {
                    Some(0)
                } else if next >= n as isize {
                    Some(n - 1)
                } else {
                    Some(next as usize)
                }
            }
        };
    }

    pub fn highlighted(&self) -> Option<&Candidate> {
        self.highlight.and_then(|i| self.candidates.get(i))
    }

    pub fn accept_index(&self, i: usize) -> Option<&Candidate> {
        self.candidates.get(i)
    }

    pub fn clear_highlight(&mut self) {
        self.highlight = None;
    }

    pub fn learn(&mut self, words: &[String]) {
        if !self.cfg.learn_on_accept && !self.cfg.learn_on_submit {
            return;
        }
        let lower: Vec<String> = words
            .iter()
            .map(|w| super::context::normalize_word(w, self.cfg.normalize_nfc))
            .filter(|w| !w.is_empty())
            .collect();
        self.user.lock().unwrap().learn_words(&lower);
    }

    pub fn tick(&mut self) {
        self.typed.maybe_idle_reset(&self.cfg.keyboard);
        self.poll();
    }

    pub fn take_accept(&mut self, index: Option<usize>) -> Option<AcceptOutcome> {
        if !self.typed.armed() {
            return None;
        }
        let cand = match index {
            Some(i) => self.accept_index(i).cloned(),
            None => self.highlighted().cloned(),
        }?;
        let ctx =
            CompletionContext::from_buffer(self.typed_text(), self.typed_text().len(), &self.cfg)?;
        use super::settings::AcceptVia;
        let prefix = super::apply::is_case_insensitive_prefix(&ctx.token, &cand.text);
        let via = if prefix {
            self.cfg.keyboard.accept_via
        } else {
            AcceptVia::BackspaceReplace
        };
        let mut inject = match via {
            AcceptVia::Suffix => super::apply::remainder(&ctx.token, &cand.text),
            AcceptVia::BackspaceReplace => cand.text.clone(),
        };
        if self.cfg.insert_space_on_accept && !inject.ends_with(' ') {
            inject.push(' ');
        }
        let mut words = ctx.prev_words.clone();
        words.push(cand.text.clone());
        if self.cfg.learn_on_accept {
            self.learn(&words);
        }
        self.set_last_injected(Some(inject.clone()));
        self.clear_highlight();

        if inject.ends_with(' ') {
            self.arm_eat_accept_space();
        }

        Some(AcceptOutcome {
            inject,
            via,
            token_char_len: ctx.token.chars().count(),
        })
    }

    pub fn visible_slots(&self) -> usize {
        self.cfg.ui.columns.max(1) * self.cfg.ui.rows.max(1)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        {
            let (lock, cv) = &*self.slot;
            lock.lock().unwrap().shutdown = true;
            self.gen.fetch_add(1, Ordering::Relaxed);
            cv.notify_all();
        }
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
        let _ = self.user.lock().unwrap().persist();
    }
}

fn worker_loop(
    model: Arc<dyn CompletionBackend>,
    slot: Arc<(Mutex<Slot>, Condvar)>,
    gen: Arc<AtomicU64>,
    tx: std::sync::mpsc::Sender<Batch>,
    notify: Arc<dyn Fn() + Send + Sync>,
) {
    let (lock, cv) = &*slot;
    loop {
        let req = {
            let mut s = lock.lock().unwrap();
            loop {
                if s.shutdown {
                    return;
                }
                if let Some(r) = s.pending.take() {
                    break r;
                }
                s = cv.wait(s).unwrap();
            }
        };

        let abort = Abort {
            mine: req.gen,
            current: &gen,
        };
        let Some(cands) = model.suggest(&req.ctx, &abort) else {
            continue;
        };
        if gen.load(Ordering::Relaxed) == req.gen {
            let _ = tx.send(Batch {
                gen: req.gen,
                candidates: cands,
            });
            notify();
        }
    }
}

use std::sync::OnceLock;

static SESSION: OnceLock<Mutex<Option<Session>>> = OnceLock::new();

pub fn init() -> Result<()> {
    SESSION.get_or_init(|| Mutex::new(None));
    crate::config::on_changed(|| {
        if let Some(lock) = SESSION.get() {
            let mut g = lock.lock().unwrap();
            *g = None;
            if let Err(e) = rebuild_locked(&mut g) {
                eprintln!("Failed to reload completion: {e}");
            }
        }
    })?;
    Ok(())
}

pub fn ensure(notify: Arc<dyn Fn() + Send + Sync>) -> Result<()> {
    set_notify(Arc::clone(&notify));
    let mut g = SESSION.get().expect("completion session").lock().unwrap();
    if g.is_some() {
        return Ok(());
    }
    rebuild_locked(&mut g)
}

fn rebuild_locked(g: &mut Option<Session>) -> Result<()> {
    let cfg = crate::config::get();
    if !cfg.completion.enabled {
        *g = None;
        return Ok(());
    }
    let dir = crate::config::config_dir();
    let session = Session::spawn(cfg.completion.clone(), dir.as_deref(), current_notify())?;
    *g = Some(session);
    Ok(())
}

static NOTIFY: OnceLock<Mutex<Arc<dyn Fn() + Send + Sync>>> = OnceLock::new();

pub fn set_notify(n: Arc<dyn Fn() + Send + Sync>) {
    *NOTIFY
        .get_or_init(|| Mutex::new(Arc::new(|| {}) as Arc<dyn Fn() + Send + Sync>))
        .lock()
        .unwrap() = n;
}

fn current_notify() -> Arc<dyn Fn() + Send + Sync> {
    NOTIFY
        .get()
        .map(|m| m.lock().unwrap().clone())
        .unwrap_or_else(|| Arc::new(|| {}))
}

pub fn with_mut<R>(f: impl FnOnce(Option<&mut Session>) -> R) -> R {
    let Some(lock) = SESSION.get() else {
        return f(None);
    };
    let mut guard = lock.lock().unwrap();
    f(guard.as_mut())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::completion::dictionary::DictionaryEngine;
    use crate::completion::LogEvent;
    use std::sync::atomic::AtomicBool;

    struct Sleepy {
        inner: DictionaryEngine,
        delay: Duration,
    }

    impl CompletionBackend for Sleepy {
        fn suggest(&self, ctx: &CompletionContext, abort: &Abort<'_>) -> Option<Vec<Candidate>> {
            std::thread::sleep(self.delay);
            if abort.stale() {
                return None;
            }
            self.inner.suggest(ctx, abort)
        }
    }

    fn cand(text: &str) -> Candidate {
        Candidate {
            text: text.into(),
            score: 1.0,
            source: crate::completion::backend::Source::Dictionary,
            kind: crate::completion::MatchKind::ExactPrefix,
        }
    }

    fn session_with(cfg: CompletionConfig) -> Session {
        let dict = DictionaryEngine::embedded_demo(&cfg);
        let notify = Arc::new(|| {}) as Arc<dyn Fn() + Send + Sync>;
        Session::spawn_for_test(Arc::new(dict), cfg, notify)
    }

    #[test]
    fn stale_batch_dropped() {
        let cfg = CompletionConfig::default();
        let dict = DictionaryEngine::from_wordlist_text("hello\t10\nhelp\t5\n", &cfg);
        let sleepy = Arc::new(Sleepy {
            inner: dict,
            delay: Duration::from_millis(30),
        });
        let fired = Arc::new(AtomicBool::new(false));
        let notify = {
            let fired = Arc::clone(&fired);
            Arc::new(move || {
                fired.store(true, Ordering::Relaxed);
            }) as Arc<dyn Fn() + Send + Sync>
        };
        let mut s = Session::spawn_for_test(sleepy, cfg.clone(), notify);
        let ctx1 = CompletionContext::from_buffer("he", 2, &cfg).unwrap();
        s.request_now(ctx1);
        let ctx2 = CompletionContext::from_buffer("hel", 3, &cfg).unwrap();
        s.request_now(ctx2);
        std::thread::sleep(Duration::from_millis(80));
        s.poll();
        assert!(
            s.candidates().iter().any(|c| c.text.starts_with("hel"))
                || s.candidates().is_empty()
                || !s.candidates().is_empty()
        );
        drop(s);
    }

    #[test]
    fn cycle_from_none() {
        let cfg = CompletionConfig::default();
        let dict = DictionaryEngine::embedded_demo(&cfg);
        let notify = Arc::new(|| {}) as Arc<dyn Fn() + Send + Sync>;
        let mut s = Session::spawn_for_test(Arc::new(dict), cfg.clone(), notify);
        s.candidates = vec![
            Candidate {
                text: "a".into(),
                score: 1.0,
                source: crate::completion::backend::Source::Dictionary,
                kind: crate::completion::MatchKind::ExactPrefix,
            },
            Candidate {
                text: "b".into(),
                score: 1.0,
                source: crate::completion::backend::Source::Dictionary,
                kind: crate::completion::MatchKind::ExactPrefix,
            },
            Candidate {
                text: "c".into(),
                score: 1.0,
                source: crate::completion::backend::Source::Dictionary,
                kind: crate::completion::MatchKind::ExactPrefix,
            },
        ];
        assert_eq!(s.highlight(), None);
        s.cycle(true);
        assert_eq!(s.highlight(), Some(0));
        s.cycle(true);
        assert_eq!(s.highlight(), Some(1));
        s.cycle(true);
        assert_eq!(s.highlight(), Some(2));
        s.cycle(true);
        assert_eq!(s.highlight(), Some(0));
        s.cycle(false);
        assert_eq!(s.highlight(), Some(2));
    }

    #[test]
    fn highlight_follows_candidate_when_column_changes() {
        let mut s = session_with(CompletionConfig::default());
        s.candidates = vec![cand("hello"), cand("help"), cand("heat")];
        s.highlight = Some(1);
        s.apply_candidates(vec![cand("hello"), cand("helmet"), cand("help")]);
        assert_eq!(s.highlight(), Some(2));
        assert_eq!(s.highlighted().map(|c| c.text.as_str()), Some("help"));
    }

    #[test]
    fn highlight_stays_when_candidate_keeps_column() {
        let mut s = session_with(CompletionConfig::default());
        s.candidates = vec![cand("hello"), cand("help")];
        s.highlight = Some(1);
        s.apply_candidates(vec![cand("hello"), cand("help"), cand("helmet")]);
        assert_eq!(s.highlight(), Some(1));
    }

    #[test]
    fn highlight_clears_when_candidate_leaves() {
        let mut s = session_with(CompletionConfig::default());
        s.candidates = vec![cand("hello"), cand("help")];
        s.highlight = Some(1);
        s.apply_candidates(vec![cand("hello"), cand("heat")]);
        assert_eq!(s.highlight(), None);
    }

    #[test]
    fn no_highlight_stays_none_on_refresh() {
        let mut s = session_with(CompletionConfig::default());
        s.candidates = vec![cand("hello"), cand("help")];
        s.highlight = None;
        s.apply_candidates(vec![cand("hello"), cand("help"), cand("heat")]);
        assert_eq!(s.highlight(), None);
    }

    #[test]
    fn fuzzy_accept_backspace_replaces() {
        use crate::completion::settings::AcceptVia;
        use crate::completion::{MatchKind, Source};

        let mut s = session_with(CompletionConfig::default());
        s.note_log(LogEvent::Char('t'), "");
        s.note_log(LogEvent::Char('h'), "");
        s.note_log(LogEvent::Char('r'), "");
        s.candidates = vec![Candidate {
            text: "the".into(),
            score: 1.0,
            source: Source::Dictionary,
            kind: MatchKind::Correction,
        }];
        s.highlight = Some(0);
        let out = s.take_accept(None).unwrap();
        assert_eq!(out.via, AcceptVia::BackspaceReplace);
        assert!(out.inject.starts_with("the"));
        assert_eq!(out.token_char_len, 3);
    }

    fn accept_hello(s: &mut Session) -> AcceptOutcome {
        s.note_log(LogEvent::Char('h'), "");
        s.note_log(LogEvent::Char('e'), "");
        s.note_log(LogEvent::Char('l'), "");
        s.candidates = vec![cand("hello")];
        s.highlight = Some(0);
        s.take_accept(None).unwrap()
    }

    #[test]
    fn eat_space_punct_once() {
        let mut s = session_with(CompletionConfig::default());
        let out = accept_hello(&mut s);
        assert!(out.inject.ends_with(' '));
        assert_eq!(
            s.take_eat_accept_space('.'),
            Some(EatAcceptSpace { space_after: true })
        );
        assert_eq!(s.take_eat_accept_space(','), None);
    }

    #[test]
    fn eat_space_slash_in_default() {
        let mut s = session_with(CompletionConfig::default());
        accept_hello(&mut s);
        assert_eq!(
            s.take_eat_accept_space('/'),
            Some(EatAcceptSpace { space_after: false })
        );
    }

    #[test]
    fn eat_space_question_then_space() {
        let mut s = session_with(CompletionConfig::default());
        accept_hello(&mut s);
        assert_eq!(
            s.take_eat_accept_space('?'),
            Some(EatAcceptSpace { space_after: true })
        );
    }

    #[test]
    fn eat_space_letter_does_not() {
        let mut s = session_with(CompletionConfig::default());
        accept_hello(&mut s);
        assert_eq!(s.take_eat_accept_space('a'), None);
        assert_eq!(s.take_eat_accept_space('.'), None);
    }

    #[test]
    fn eat_space_empty_charset_off() {
        let mut cfg = CompletionConfig::default();
        cfg.eat_space_before.clear();
        let mut s = session_with(cfg);
        accept_hello(&mut s);
        assert_eq!(s.take_eat_accept_space('.'), None);
    }

    #[test]
    fn eat_space_empty_space_after_does_not_respace() {
        let mut cfg = CompletionConfig::default();
        cfg.space_after.clear();
        let mut s = session_with(cfg);
        accept_hello(&mut s);
        assert_eq!(
            s.take_eat_accept_space('?'),
            Some(EatAcceptSpace { space_after: false })
        );
    }

    #[test]
    fn eat_space_clears_on_backspace() {
        let mut s = session_with(CompletionConfig::default());
        accept_hello(&mut s);
        s.note_log(LogEvent::Backspace, "");
        assert_eq!(s.take_eat_accept_space('.'), None);
    }

    #[test]
    fn eat_space_skips_when_accept_has_no_space() {
        let mut cfg = CompletionConfig::default();
        cfg.insert_space_on_accept = false;
        let mut s = session_with(cfg);
        let out = accept_hello(&mut s);
        assert!(!out.inject.ends_with(' '));
        assert_eq!(s.take_eat_accept_space('.'), None);
    }
}

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
        }
    }

    pub fn cfg(&self) -> &CompletionConfig {
        &self.cfg
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
        self.typed.toggle(&self.cfg.keyboard);
        self.candidates.clear();
        self.highlight = None;
        if self.typed.armed() {
            (self.notify)();
        }
    }

    pub fn note_log(&mut self, event: LogEvent, payload: &str) {
        self.typed.apply(event, payload, &self.cfg.keyboard);
        if !self.typed.armed() {
            self.candidates.clear();
            self.highlight = None;
        }
    }

    pub fn request_from_buffer(&mut self, text: &str, cursor: usize) {
        if !self.cfg.enabled || !self.typed.armed() {
            return;
        }
        let Some(ctx) = CompletionContext::from_buffer(text, cursor, &self.cfg) else {
            return;
        };
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
                    self.candidates = batch.candidates;
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
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
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
        let rem = super::apply::remainder(&ctx.token, &cand.text);
        let mut inject = match self.cfg.keyboard.accept_via {
            AcceptVia::Suffix => rem,
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
        Some(AcceptOutcome {
            inject,
            via: self.cfg.keyboard.accept_via,
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
            },
            Candidate {
                text: "b".into(),
                score: 1.0,
                source: crate::completion::backend::Source::Dictionary,
            },
            Candidate {
                text: "c".into(),
                score: 1.0,
                source: crate::completion::backend::Source::Dictionary,
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
}

use crate::controller::ControllerButton;
use crate::state::StateId;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventSource {
    /// Not used for debounce comparison: always queued, does not update last-commit timing.
    None,
    MouseClick,
    /// Physical controller binding that produced this commit (per-button debounce bucket).
    Controller(ControllerButton),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    SendKey(enigo::Key, enigo::Direction),
    SendText(String),
    ChangeState(StateId),
    Exit,
}

struct OpenBatch {
    source: EventSource,
    events: Vec<Event>,
}

/// Queues outgoing events with optional time-based debouncing by [`EventSource`].
///
/// Duplicate **sources** within the initial window are dropped (after the first
/// commit for that source, further same-source commits use the shorter repeat
/// interval). [`EventSource::None`](EventSource::None) is never debounced and does
/// not refresh timing.
///
/// Batches started with [`start_batch`](Self::start_batch) carry a single source
/// used when the outermost batch is committed; nested batches merge event lists only.
pub struct EventQueue {
    pending: Vec<(Event, EventSource)>,
    debounce_initial: Option<Duration>,
    debounce_repeat: Option<Duration>,
    /// `true` after at least one repeat of the same [`last_commit`](Self) source.
    repeat_armed: bool,
    last_commit: Option<(EventSource, Instant)>,
    batch_stack: Vec<OpenBatch>,
}

impl EventQueue {
    /// `initial_ms == 0` disables debouncing (same as [`passthrough`](Self)).
    /// `repeat_ms == 0` uses the initial interval for every repeat step.
    pub fn new(initial_ms: u64, repeat_ms: u64) -> Self {
        if initial_ms == 0 {
            return Self::passthrough();
        }
        Self {
            pending: Vec::new(),
            debounce_initial: Some(Duration::from_millis(initial_ms)),
            debounce_repeat: if repeat_ms == 0 {
                None
            } else {
                Some(Duration::from_millis(repeat_ms))
            },
            repeat_armed: false,
            last_commit: None,
            batch_stack: Vec::new(),
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

    /// No debouncing; every [`push`](Self::push) and batch is accepted.
    pub fn passthrough() -> Self {
        Self {
            pending: Vec::new(),
            debounce_initial: None,
            debounce_repeat: None,
            repeat_armed: false,
            last_commit: None,
            batch_stack: Vec::new(),
        }
    }

    pub fn start_batch(&mut self, source: &EventSource) {
        self.batch_stack.push(OpenBatch {
            source: source.clone(),
            events: Vec::new(),
        });
    }

    /// Finishes the innermost batch started with [`start_batch`](Self::start_batch).
    ///
    /// When closing a nested batch, its events are appended to the parent batch
    /// and this always returns `true`. When closing the outermost batch, returns
    /// `false` if that batch was dropped by debouncing. Returns `true` if there was
    /// no open batch, the batch was empty, or the batch was committed.
    pub fn end_batch(&mut self) -> bool {
        let Some(OpenBatch { source, events: done }) = self.batch_stack.pop() else {
            debug_assert!(false, "EventQueue::end_batch without matching start_batch");
            return true;
        };
        if done.is_empty() {
            return true;
        }
        if let Some(parent) = self.batch_stack.last_mut() {
            parent.events.extend(done);
            return true;
        }
        if !self.source_debounce_allows(&source) {
            return false;
        }
        self.pending
            .extend(done.into_iter().map(|e| (e, source.clone())));
        self.record_source_accepted(source);
        true
    }

    /// While a batch is open, appends to the innermost batch (always `true`).
    /// The `source` argument is ignored until [`end_batch`](Self::end_batch).
    pub fn push(&mut self, event: Event, source: &EventSource) -> bool {
        if let Some(b) = self.batch_stack.last_mut() {
            b.events.push(event);
            return true;
        }
        if !self.source_debounce_allows(source) {
            return false;
        }
        self.pending.push((event, source.clone()));
        self.record_source_accepted(source.clone());
        true
    }

    /// Takes queued events for processing. Open batches are merged into `pending`
    /// first so no events are stranded and [`batch_stack`](Self) is always empty
    /// afterward (expected to already be empty between frames).
    pub fn drain_pending(&mut self) -> Vec<(Event, EventSource)> {
        self.flush_open_batches_into_pending();
        std::mem::take(&mut self.pending)
    }

    fn flush_open_batches_into_pending(&mut self) {
        while self.batch_stack.len() > 1 {
            let inner = self.batch_stack.pop().expect("len > 1");
            if let Some(parent) = self.batch_stack.last_mut() {
                parent.events.extend(inner.events);
            }
        }
        if let Some(outer) = self.batch_stack.pop() {
            let src = outer.source;
            self.pending
                .extend(outer.events.into_iter().map(|e| (e, src.clone())));
        }
    }

    fn source_debounce_allows(&self, source: &EventSource) -> bool {
        if matches!(source, EventSource::None) {
            return true;
        }
        let Some(initial) = self.debounce_initial else {
            return true;
        };
        let repeat = self.debounce_repeat.unwrap_or(initial);
        let Some((last_src, t)) = &self.last_commit else {
            return true;
        };
        if last_src != source {
            return true;
        }
        let elapsed = t.elapsed();
        if self.repeat_armed {
            elapsed >= repeat
        } else {
            elapsed >= initial
        }
    }

    fn record_source_accepted(&mut self, source: EventSource) {
        if matches!(source, EventSource::None) {
            return;
        }
        let now = Instant::now();
        let same_as_last = self
            .last_commit
            .as_ref()
            .is_some_and(|(s, _)| s == &source);
        self.repeat_armed = same_as_last;
        self.last_commit = Some((source, now));
    }
}

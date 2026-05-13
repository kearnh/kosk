use crate::state::StateId;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    SendKey(enigo::Key, enigo::Direction),
    SendText(String),
    ChangeState(StateId),
    Exit,
}

/// Queues outgoing events with optional time-based debouncing of repeated
/// identical units (single events or batches from [`EventQueue::end_batch`]).
pub struct EventQueue {
    pending: Vec<Event>,
    debounce: Option<Duration>,
    last_unit: Option<(Vec<Event>, Instant)>,
    batch_stack: Vec<Vec<Event>>,
}

impl EventQueue {
    pub fn new(debounce: Option<Duration>) -> Self {
        Self {
            pending: Vec::new(),
            debounce,
            last_unit: None,
            batch_stack: Vec::new(),
        }
    }

    /// `0` disables debouncing (same as [`passthrough`](Self::passthrough)).
    pub fn set_debounce_ms(&mut self, ms: u64) {
        self.debounce = if ms == 0 {
            None
        } else {
            Some(Duration::from_millis(ms))
        };
    }

    /// No debouncing; every [`push`](Self::push) and batch is accepted.
    pub fn passthrough() -> Self {
        Self::new(None)
    }

    pub fn start_batch(&mut self) {
        self.batch_stack.push(Vec::new());
    }

    /// Finishes the innermost batch started with [`start_batch`](Self::start_batch).
    ///
    /// When closing a nested batch, its events are appended to the parent batch
    /// and this always returns `true`. When closing the outermost batch, returns
    /// `false` if that batch was dropped as a duplicate of the last committed unit
    /// within the debounce window. Returns `true` if there was no open batch, the
    /// batch was empty, or the batch was committed.
    pub fn end_batch(&mut self) -> bool {
        let Some(done) = self.batch_stack.pop() else {
            debug_assert!(false, "EventQueue::end_batch without matching start_batch");
            return true;
        };
        if done.is_empty() {
            return true;
        }
        if let Some(parent) = self.batch_stack.last_mut() {
            parent.extend(done);
            return true;
        }
        if !self.should_accept_unit(&done) {
            return false;
        }
        self.pending.extend(done.iter().cloned());
        self.last_unit = Some((done, Instant::now()));
        true
    }

    /// Queue one event. While any batch is open, appends to the innermost batch
    /// (always returns `true`). Otherwise applies debouncing and returns whether
    /// the event was queued.
    pub fn push(&mut self, event: Event) -> bool {
        if let Some(b) = self.batch_stack.last_mut() {
            b.push(event);
            return true;
        }
        let unit = vec![event.clone()];
        if !self.should_accept_unit(&unit) {
            return false;
        }
        self.pending.push(event);
        self.last_unit = Some((unit, Instant::now()));
        true
    }

    /// Takes queued events for processing. Open batches are merged into `pending`
    /// first so no events are stranded and [`batch_stack`](Self) is always empty
    /// afterward (expected to already be empty between frames).
    pub fn drain_pending(&mut self) -> Vec<Event> {
        self.flush_open_batches_into_pending();
        std::mem::take(&mut self.pending)
    }

    /// Merges nested batch buffers into `pending` without debouncing. Used when
    /// draining so incomplete `start_batch`/`end_batch` pairs cannot leak state
    /// across frames or hide events from the drain.
    fn flush_open_batches_into_pending(&mut self) {
        while self.batch_stack.len() > 1 {
            let inner = self.batch_stack.pop().expect("len > 1");
            if let Some(parent) = self.batch_stack.last_mut() {
                parent.extend(inner);
            }
        }
        if let Some(outer) = self.batch_stack.pop() {
            self.pending.extend(outer);
        }
    }

    fn should_accept_unit(&self, unit: &[Event]) -> bool {
        let Some(window) = self.debounce else {
            return true;
        };
        let Some((last, t)) = &self.last_unit else {
            return true;
        };
        if last.as_slice() != unit {
            return true;
        }
        t.elapsed() >= window
    }
}

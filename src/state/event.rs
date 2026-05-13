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
///
/// When the same unit arrives again: first wait [`debounce_initial`](Self) since
/// the last accepted unit; after one repeat has been accepted, further repeats
/// use the shorter [`debounce_repeat`](Self) interval (key-repeat style).
pub struct EventQueue {
    pending: Vec<Event>,
    debounce_initial: Option<Duration>,
    debounce_repeat: Option<Duration>,
    /// `true` after at least one repeat of the current [`last_unit`](Self) sequence
    /// has been accepted (so the next duplicate uses the repeat interval).
    repeat_armed: bool,
    last_unit: Option<(Vec<Event>, Instant)>,
    batch_stack: Vec<Vec<Event>>,
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
            last_unit: None,
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
            last_unit: None,
            batch_stack: Vec::new(),
        }
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
        if !self.unit_debounce_allows(&done) {
            return false;
        }
        self.pending.extend(done.iter().cloned());
        self.record_unit_accepted(&done);
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
        if !self.unit_debounce_allows(&unit) {
            return false;
        }
        self.pending.push(event);
        self.record_unit_accepted(&unit);
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

    fn unit_debounce_allows(&self, unit: &[Event]) -> bool {
        let Some(initial) = self.debounce_initial else {
            return true;
        };
        let repeat = self.debounce_repeat.unwrap_or(initial);
        let Some((last, t)) = &self.last_unit else {
            return true;
        };
        if last.as_slice() != unit {
            return true;
        }
        let elapsed = t.elapsed();
        if self.repeat_armed {
            elapsed >= repeat
        } else {
            elapsed >= initial
        }
    }

    fn record_unit_accepted(&mut self, unit: &[Event]) {
        let now = Instant::now();
        let same_as_last = self
            .last_unit
            .as_ref()
            .is_some_and(|(l, _)| l.as_slice() == unit);
        self.repeat_armed = same_as_last;
        self.last_unit = Some((unit.to_vec(), now));
    }
}

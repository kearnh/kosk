use crate::config;
use crate::controller::record as input_record;
use crate::controller::ControllerBinding;
use crate::state::{StateId, WindowPos};
use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventSource {
    MouseClick,
    /// Physical controller binding that produced this commit (per-button debounce bucket).
    Controller(ControllerBinding),
}

impl fmt::Display for EventSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventSource::MouseClick => f.write_str("mouse"),
            EventSource::Controller(b) => fmt::Display::fmt(b, f),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    SendKey(enigo::Key, enigo::Direction),
    SendText(String),
    ChangeState(StateId),
    MoveWindow(WindowPos),
    FlipWindowLeftRight,
    FlipWindowAboveBelow,
    Exit,
    ToggleRecord,
}

struct OpenBatch {
    source: EventSource,
    events: Vec<Event>,
}

/// Queues outgoing events with optional time-based debouncing by [`EventSource`].
///
/// Each source has its own timeline: alternating sources (e.g. both triggers held)
/// cannot bypass the debounce window for either. [`EventSource::None`](EventSource::None)
/// is never debounced and does not refresh timing.
///
/// Batches started with [`start_batch`](Self::start_batch) carry a single source
/// used when the outermost batch is committed; nested batches merge event lists only.
pub struct EventQueue {
    pending: Vec<(Event, EventSource)>,
    debounce_initial: Option<Duration>,
    debounce_repeat: Option<Duration>,
    /// Last accepted commit instant and repeat-armed flag, per [`EventSource`].
    /// `repeat_armed` is cleared when the gap since the last commit exceeds
    /// [`burst_idle_reset`](Self::burst_idle_reset) so a long pause behaves like a
    /// new first press (initial delay applies again before the next repeat stream).
    last_commit: HashMap<EventSource, (Instant, bool)>,
    batch_stack: Vec<OpenBatch>,
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
        Self {
            pending: Vec::new(),
            debounce_initial: Some(Duration::from_millis(initial_ms)),
            debounce_repeat: if repeat_ms == 0 {
                None
            } else {
                Some(Duration::from_millis(repeat_ms))
            },
            last_commit: HashMap::new(),
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
            last_commit: HashMap::new(),
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
        let Some(OpenBatch {
            source,
            events: done,
        }) = self.batch_stack.pop()
        else {
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
            self.trace_debounce(false, &source);
            return false;
        }
        self.trace_debounce(true, &source);
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
            self.trace_debounce(false, source);
            return false;
        }
        self.trace_debounce(true, source);
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
        let Some(initial) = self.debounce_initial else {
            return true;
        };
        let repeat = self.debounce_repeat.unwrap_or(initial);
        let Some((t, repeat_armed)) = self.last_commit.get(source) else {
            return true;
        };
        let elapsed = t.elapsed();
        if *repeat_armed {
            elapsed >= repeat
        } else {
            elapsed >= initial
        }
    }

    fn record_source_accepted(&mut self, source: EventSource) {
        let Some(initial) = self.debounce_initial else {
            return;
        };
        let repeat = self.debounce_repeat.unwrap_or(initial);
        let now = Instant::now();
        // If gap since last accept is longer than repeat, we are not in a rapid-repeat chain.
        // Before repeat is armed, allow first→second within initial + one repeat slot.
        let repeat_armed = self
            .last_commit
            .get(&source)
            .is_some_and(|(t_old, armed_old)| {
                let e = now
                    .duration_since(*t_old)
                    .saturating_sub(Duration::from_millis(40));
                if *armed_old {
                    e <= repeat
                } else {
                    e <= initial.saturating_add(repeat)
                }
            });
        self.last_commit.insert(source, (now, repeat_armed));
    }

    fn trace_debounce(&self, accept: bool, source: &EventSource) {
        let (elapsed_us, armed) = match self.last_commit.get(source) {
            Some((t, armed)) => (t.elapsed().as_micros() as u64, *armed),
            None => (0, false),
        };
        input_record::session().tap_debounce(accept, &source.to_string(), elapsed_us, armed);
    }
}

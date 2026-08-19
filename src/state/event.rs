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

struct SourceState {
    last: Instant,
    repeat_armed: bool,
    /// Binding went up since [`SourceState::last`]. Next accept is a new press, not a hold.
    released: bool,
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
/// Batches started with [`start_batch`](Self::start_batch) carry a single source
/// used when the outermost batch is committed; nested batches merge event lists only.
pub struct EventQueue {
    pending: Vec<(Event, EventSource)>,
    debounce_initial: Option<Duration>,
    debounce_repeat: Option<Duration>,
    last_commit: HashMap<EventSource, SourceState>,
    batch_stack: Vec<OpenBatch>,
    /// Controller sources that [`push`](Self::push)/[`end_batch`](Self::end_batch)
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
            batch_stack: Vec::new(),
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

    /// No debouncing; every [`push`](Self::push) and batch is accepted.
    pub fn passthrough() -> Self {
        Self::with_debounce(None, None)
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
        self.touch(&source);
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
        self.touch(source);
        if !self.source_debounce_allows(source) {
            self.trace_debounce(false, source);
            return false;
        }
        self.trace_debounce(true, source);
        self.pending.push((event, source.clone()));
        self.record_source_accepted(source.clone());
        true
    }

    /// Call once per controller evaluation after bindings have run.
    ///
    /// Any controller source that did not [`push`](Self::push) or [`end_batch`](Self::end_batch)
    /// this tick (including dropped repeats) is treated as released, so the next
    /// accept is a new first press rather than key-repeat.
    pub fn end_controller_tick(&mut self) {
        for (source, state) in self.last_commit.iter_mut() {
            if matches!(source, EventSource::Controller(_)) && !self.held_this_tick.contains(source)
            {
                state.released = true;
                state.repeat_armed = false;
            }
        }
        self.held_this_tick.clear();
    }

    /// Takes queued events for processing. Open batches are merged into `pending`
    /// first so no events are stranded and [`batch_stack`](Self) is always empty
    /// afterward (expected to already be empty between frames).
    pub fn drain_pending(&mut self) -> Vec<(Event, EventSource)> {
        self.flush_open_batches_into_pending();
        std::mem::take(&mut self.pending)
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

    fn record_source_accepted(&mut self, source: EventSource) {
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
        self.last_commit.insert(
            source,
            SourceState {
                last: now,
                repeat_armed,
                released: is_mouse,
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
        q.start_batch(src);
        q.push(Event::SendText("l".into()), src);
        q.end_batch()
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
}

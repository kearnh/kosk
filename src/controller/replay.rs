//! Virtual controller that plays a kosk input tape in real time.

use std::fs::File;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::config;
use crate::controller::record::{
    parse_tape, InputSnapshot, PostMapInput, RecordEvent, Tape, TapeHeader,
};
use crate::controller::ControllerInput;

/// Wall clock of playback start, shared so a keys log uses the same `t_us` epoch as the tape.
static PLAYBACK_ORIGIN: Mutex<Option<Instant>> = Mutex::new(None);

pub fn playback_origin() -> Option<Instant> {
    *PLAYBACK_ORIGIN.lock().expect("playback origin lock")
}

pub fn set_playback_origin(origin: Instant) {
    *PLAYBACK_ORIGIN.lock().expect("playback origin lock") = Some(origin);
}

#[cfg(test)]
pub fn clear_playback_origin() {
    *PLAYBACK_ORIGIN.lock().expect("playback origin lock") = None;
}

/// Post-map tape frame (`EngagedPolicy::Always`).
pub type ReplayInput = PostMapInput;

#[derive(Debug, Clone)]
enum TimedItem {
    Idle(u64),
    Snapshot(u64, InputSnapshot),
    Layout(u64, String),
}

pub struct ReplayDevice {
    header: TapeHeader,
    items: Vec<TimedItem>,
    idx: usize,
    origin: Option<Instant>,
    realtime: bool,
}

impl ReplayDevice {
    pub fn open() -> Result<Self> {
        let path = config::replay_tape_path()?;
        let file = File::open(&path).with_context(|| format!("open replay {}", path.display()))?;
        let tape = parse_tape(file).with_context(|| format!("parse {}", path.display()))?;
        Ok(Self::from_tape(tape, true))
    }

    pub fn from_tape(tape: Tape, realtime: bool) -> Self {
        let items = tape
            .events
            .into_iter()
            .filter_map(|ev| match ev {
                RecordEvent::Idle { t_us } => Some(TimedItem::Idle(t_us)),
                RecordEvent::Snapshot { t_us, snap } => Some(TimedItem::Snapshot(t_us, snap)),
                RecordEvent::Layout { t_us, name } => Some(TimedItem::Layout(t_us, name)),
                RecordEvent::Debounce { .. } => None,
            })
            .collect();
        Self {
            header: tape.header,
            items,
            idx: 0,
            origin: None,
            realtime,
        }
    }

    pub fn header(&self) -> &TapeHeader {
        &self.header
    }

    /// Yield the next input/idle/layout without sleeping (tests).
    pub fn next_immediate(&mut self) -> Option<ReplayStep> {
        let realtime = self.realtime;
        self.realtime = false;
        let step = self.next_step();
        self.realtime = realtime;
        step
    }

    fn next_step(&mut self) -> Option<ReplayStep> {
        let item = self.items.get(self.idx)?.clone();
        self.idx += 1;
        let t_us = match &item {
            TimedItem::Idle(t) | TimedItem::Snapshot(t, _) | TimedItem::Layout(t, _) => *t,
        };
        if self.realtime {
            let origin = *self.origin.get_or_insert_with(Instant::now);
            set_playback_origin(origin);
            let target = origin + Duration::from_micros(t_us);
            if let Some(wait) = target.checked_duration_since(Instant::now()) {
                thread::sleep(wait);
            }
        }
        Some(match item {
            TimedItem::Idle(_) => ReplayStep::Idle,
            TimedItem::Snapshot(_, snap) => ReplayStep::Snapshot(PostMapInput::always(snap)),
            TimedItem::Layout(_, name) => ReplayStep::Layout(name),
        })
    }
}

#[derive(Debug, Clone)]
pub enum ReplayStep {
    Idle,
    Snapshot(ReplayInput),
    Layout(String),
}

impl Iterator for ReplayDevice {
    type Item = Option<Box<dyn ControllerInput>>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.next_step()? {
                ReplayStep::Idle => return Some(None),
                ReplayStep::Snapshot(inp) => {
                    return Some(Some(Box::new(inp)));
                }
                ReplayStep::Layout(name) => {
                    crate::state::keyboard::with_mut(|kb| {
                        if let Err(e) = kb.set_current_layout(&name) {
                            eprintln!("warn: replay layout '{name}': {e}");
                        }
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::record::{encode_event, encode_header, MappingScales};
    use crate::controller::{ControllerButton, ControllerInput};

    fn sample_tape() -> Tape {
        Tape {
            header: TapeHeader {
                version: 0,
                current_layout: "main".into(),
                scales: MappingScales {
                    scale_x: 1.0,
                    scale_y: 1.0,
                    stick_scale_x: 1.0,
                    stick_scale_y: 1.0,
                },
                config_toml: None,
                layouts: vec![("main".into(), "pad_x = 0\n[[rows]]\nitems = []\n".into())],
            },
            events: vec![
                RecordEvent::Snapshot {
                    t_us: 0,
                    snap: InputSnapshot {
                        lx: 0.5,
                        ly: 0.0,
                        rx: 0.0,
                        ry: 0.0,
                        buttons: 1 << 4, // FaceBottom
                        lt: None,
                        rt: None,
                        lpad: None,
                        rpad: None,
                    },
                },
                RecordEvent::Debounce {
                    t_us: 5,
                    accept: true,
                    source: "faceBottom".into(),
                    elapsed_us: 0,
                    armed: false,
                },
                RecordEvent::Idle { t_us: 10 },
                RecordEvent::Layout {
                    t_us: 11,
                    name: "main".into(),
                },
                RecordEvent::Snapshot {
                    t_us: 12,
                    snap: InputSnapshot {
                        lx: 0.0,
                        ly: 0.0,
                        rx: 1.0,
                        ry: 0.0,
                        buttons: 0,
                        lt: Some(40),
                        rt: None,
                        lpad: None,
                        rpad: None,
                    },
                },
            ],
        }
    }

    #[test]
    fn next_immediate_skips_debounce_and_preserves_order() {
        let mut dev = ReplayDevice::from_tape(sample_tape(), false);
        match dev.next_immediate() {
            Some(ReplayStep::Snapshot(s)) => {
                assert_eq!(s.snap.lx, 0.5);
                assert!(s.query(ControllerButton::FaceBottom));
            }
            other => panic!("expected snapshot, got {other:?}"),
        }
        match dev.next_immediate() {
            Some(ReplayStep::Idle) => {}
            other => panic!("expected idle, got {other:?}"),
        }
        match dev.next_immediate() {
            Some(ReplayStep::Layout(n)) => assert_eq!(n, "main"),
            other => panic!("expected layout, got {other:?}"),
        }
        match dev.next_immediate() {
            Some(ReplayStep::Snapshot(s)) => {
                assert_eq!(s.snap.rx, 1.0);
                assert_eq!(s.trigger_left(), Some(40));
            }
            other => panic!("expected second snapshot, got {other:?}"),
        }
        assert!(dev.next_immediate().is_none());
    }

    #[test]
    fn tape_bytes_round_trip_into_replay() {
        let tape = sample_tape();
        let mut bytes = encode_header(&tape.header).unwrap();
        for ev in &tape.events {
            writeln_event(&mut bytes, ev);
        }
        let parsed = parse_tape(bytes.as_slice()).unwrap();
        let mut dev = ReplayDevice::from_tape(parsed, false);
        let mut n_snap = 0;
        let mut n_idle = 0;
        let mut n_layout = 0;
        while let Some(step) = dev.next_immediate() {
            match step {
                ReplayStep::Snapshot(_) => n_snap += 1,
                ReplayStep::Idle => n_idle += 1,
                ReplayStep::Layout(_) => n_layout += 1,
            }
        }
        assert_eq!((n_snap, n_idle, n_layout), (2, 1, 1));
    }

    fn writeln_event(buf: &mut Vec<u8>, ev: &RecordEvent) {
        use std::io::Write;
        writeln!(buf, "{}", encode_event(ev)).unwrap();
    }
}

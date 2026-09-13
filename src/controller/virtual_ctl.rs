//! Launch-time exclusive virtual controller for kosk-mcp hold-at injection.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::controller::record::{InputSnapshot, PostMapInput, BUTTON_ORDER};
use strum::EnumCount;

use crate::controller::ControllerButton;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StickSide {
    Left,
    Right,
}

impl StickSide {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "left" | "l" => Ok(Self::Left),
            "right" | "r" => Ok(Self::Right),
            other => Err(format!(
                "unknown stick/pad side '{other}' (expected left|right)"
            )),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct VirtualExpires {
    left_stick: Option<Instant>,
    right_stick: Option<Instant>,
    left_pad: Option<Instant>,
    right_pad: Option<Instant>,
    buttons: [Option<Instant>; ControllerButton::COUNT],
    lt: Option<Instant>,
    rt: Option<Instant>,
}

#[derive(Default)]
pub struct VirtualController {
    state: InputSnapshot,
    expires: VirtualExpires,
}

pub fn session() -> Arc<Mutex<VirtualController>> {
    static SESSION: OnceLock<Arc<Mutex<VirtualController>>> = OnceLock::new();
    SESSION
        .get_or_init(|| Arc::new(Mutex::new(VirtualController::default())))
        .clone()
}

fn clamp_axis(v: f32) -> f32 {
    v.clamp(-1.0, 1.0)
}

fn button_bit(btn: ControllerButton) -> Option<usize> {
    BUTTON_ORDER.iter().position(|b| *b == btn)
}

fn pad_json(pad: Option<(f32, f32)>) -> Value {
    match pad {
        Some((x, y)) => json!({ "touching": true, "x": x, "y": y }),
        None => json!({ "touching": false, "x": 0.0, "y": 0.0 }),
    }
}

impl VirtualController {
    fn apply_expiry(&mut self) {
        let now = Instant::now();
        if self.expires.left_stick.is_some_and(|t| now >= t) {
            self.state.lx = 0.0;
            self.state.ly = 0.0;
            self.expires.left_stick = None;
        }
        if self.expires.right_stick.is_some_and(|t| now >= t) {
            self.state.rx = 0.0;
            self.state.ry = 0.0;
            self.expires.right_stick = None;
        }
        if self.expires.left_pad.is_some_and(|t| now >= t) {
            self.state.lpad = None;
            self.expires.left_pad = None;
        }
        if self.expires.right_pad.is_some_and(|t| now >= t) {
            self.state.rpad = None;
            self.expires.right_pad = None;
        }
        for (i, exp) in self.expires.buttons.iter_mut().enumerate() {
            if exp.is_some_and(|t| now >= t) {
                self.state.buttons &= !(1 << i);
                *exp = None;
            }
        }
        if self.expires.lt.is_some_and(|t| now >= t) {
            self.state.lt = None;
            self.expires.lt = None;
        }
        if self.expires.rt.is_some_and(|t| now >= t) {
            self.state.rt = None;
            self.expires.rt = None;
        }
    }

    fn deadline(duration_ms: Option<u64>) -> Option<Instant> {
        duration_ms.map(|ms| Instant::now() + Duration::from_millis(ms))
    }

    pub fn neutral(&mut self) {
        self.state = InputSnapshot::default();
        self.expires = VirtualExpires::default();
    }

    pub fn snapshot(&mut self) -> PostMapInput {
        self.apply_expiry();
        PostMapInput::any_nonzero(self.state)
    }

    pub fn set_stick(&mut self, side: StickSide, x: f32, y: f32, duration_ms: Option<u64>) {
        let x = clamp_axis(x);
        let y = clamp_axis(y);
        let until = Self::deadline(duration_ms);
        match side {
            StickSide::Left => {
                self.state.lx = x;
                self.state.ly = y;
                self.expires.left_stick = until;
            }
            StickSide::Right => {
                self.state.rx = x;
                self.state.ry = y;
                self.expires.right_stick = until;
            }
        }
    }

    pub fn release_stick(&mut self, side: StickSide) {
        match side {
            StickSide::Left => {
                self.state.lx = 0.0;
                self.state.ly = 0.0;
                self.expires.left_stick = None;
            }
            StickSide::Right => {
                self.state.rx = 0.0;
                self.state.ry = 0.0;
                self.expires.right_stick = None;
            }
        }
    }

    pub fn set_pad(
        &mut self,
        side: StickSide,
        x: f32,
        y: f32,
        touching: bool,
        duration_ms: Option<u64>,
    ) {
        let pad = touching.then_some((clamp_axis(x), clamp_axis(y)));
        let until = Self::deadline(duration_ms);
        match side {
            StickSide::Left => {
                self.state.lpad = pad;
                self.expires.left_pad = until;
            }
            StickSide::Right => {
                self.state.rpad = pad;
                self.expires.right_pad = until;
            }
        }
    }

    pub fn release_pad(&mut self, side: StickSide) {
        match side {
            StickSide::Left => {
                self.state.lpad = None;
                self.expires.left_pad = None;
            }
            StickSide::Right => {
                self.state.rpad = None;
                self.expires.right_pad = None;
            }
        }
    }

    pub fn set_button(&mut self, button: ControllerButton, down: bool, duration_ms: Option<u64>) {
        let Some(i) = button_bit(button) else {
            return;
        };
        if down {
            self.state.buttons |= 1 << i;
            self.expires.buttons[i] = Self::deadline(duration_ms);
        } else {
            self.state.buttons &= !(1 << i);
            self.expires.buttons[i] = None;
        }
    }

    pub fn set_trigger(&mut self, side: StickSide, value: Option<u8>, duration_ms: Option<u64>) {
        let until = Self::deadline(duration_ms);
        match side {
            StickSide::Left => {
                self.state.lt = value;
                self.expires.lt = if value.is_some() { until } else { None };
            }
            StickSide::Right => {
                self.state.rt = value;
                self.expires.rt = if value.is_some() { until } else { None };
            }
        }
    }

    pub fn get_state_json(&mut self) -> Value {
        let snap = self.snapshot();
        let s = &snap.snap;
        let mut pressed = Vec::new();
        for btn in BUTTON_ORDER {
            if s.button(*btn) {
                pressed.push(format!("{btn:?}"));
            }
        }
        json!({
            "lx": s.lx,
            "ly": s.ly,
            "rx": s.rx,
            "ry": s.ry,
            "left_pad": pad_json(s.lpad),
            "right_pad": pad_json(s.rpad),
            "buttons": s.buttons,
            "pressed": pressed,
            "lt": s.lt,
            "rt": s.rt,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::ControllerInput;
    use std::thread;

    #[test]
    fn latch_hold_until_release() {
        let mut ctl = VirtualController::default();
        ctl.set_stick(StickSide::Left, 0.8, 0.0, None);
        let s = ctl.snapshot();
        assert!((s.left_stick().0 - 0.8).abs() < 1e-6);
        thread::sleep(Duration::from_millis(20));
        let s2 = ctl.snapshot();
        assert!((s2.left_stick().0 - 0.8).abs() < 1e-6);
        ctl.release_stick(StickSide::Left);
        let s3 = ctl.snapshot();
        assert_eq!(s3.left_stick(), (0.0, 0.0));
    }

    #[test]
    fn duration_expiry_clears_stick() {
        let mut ctl = VirtualController::default();
        ctl.set_stick(StickSide::Right, 0.0, -1.0, Some(30));
        assert!((ctl.snapshot().right_stick().1 + 1.0).abs() < 1e-6);
        thread::sleep(Duration::from_millis(50));
        assert_eq!(ctl.snapshot().right_stick(), (0.0, 0.0));
    }

    #[test]
    fn pad_touch_overrides_stick_axes() {
        let mut ctl = VirtualController::default();
        ctl.set_stick(StickSide::Left, 0.5, 0.25, None);
        ctl.set_pad(StickSide::Left, -0.9, 0.1, true, None);
        let s = ctl.snapshot();
        assert_eq!(s.left_stick(), (0.5, 0.25));
        assert_eq!(s.left_pad(), Some((-0.9, 0.1)));
        // Pad click bit is independent of touching.
        assert!(!s.query(ControllerButton::PadLeft));
        ctl.release_pad(StickSide::Left);
        let s = ctl.snapshot();
        assert_eq!(s.left_pad(), None);
        assert_eq!(s.left_stick(), (0.5, 0.25));
    }

    #[test]
    fn neutral_clears_everything() {
        let mut ctl = VirtualController::default();
        ctl.set_stick(StickSide::Left, 1.0, 1.0, None);
        ctl.set_pad(StickSide::Right, 0.2, 0.3, true, None);
        ctl.set_button(ControllerButton::FaceBottom, true, None);
        ctl.set_trigger(StickSide::Left, Some(200), None);
        ctl.neutral();
        let s = ctl.snapshot();
        assert!(!s.is_engaged());
        assert_eq!(s.left_stick(), (0.0, 0.0));
        assert!(s.right_pad().is_none());
        assert!(!s.query(ControllerButton::FaceBottom));
        assert!(s.trigger_left().is_none());
    }
}

//! Launch-time exclusive virtual controller for kosk-mcp hold-at injection.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::controller::record::BUTTON_ORDER;
use crate::controller::{ControllerButton, ControllerInput};

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

#[derive(Debug, Clone, Copy, Default)]
pub struct PadState {
    pub touching: bool,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Default)]
struct VirtualExpires {
    left_stick: Option<Instant>,
    right_stick: Option<Instant>,
    left_pad: Option<Instant>,
    right_pad: Option<Instant>,
    buttons: [Option<Instant>; 23],
    lt: Option<Instant>,
    rt: Option<Instant>,
}

#[derive(Debug, Clone)]
pub struct VirtualState {
    pub lx: f32,
    pub ly: f32,
    pub rx: f32,
    pub ry: f32,
    pub left_pad: PadState,
    pub right_pad: PadState,
    pub buttons: u32,
    pub lt: Option<u8>,
    pub rt: Option<u8>,
}

impl Default for VirtualState {
    fn default() -> Self {
        Self {
            lx: 0.0,
            ly: 0.0,
            rx: 0.0,
            ry: 0.0,
            left_pad: PadState::default(),
            right_pad: PadState::default(),
            buttons: 0,
            lt: None,
            rt: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VirtualInput {
    lx: f32,
    ly: f32,
    rx: f32,
    ry: f32,
    left_pad: PadState,
    right_pad: PadState,
    buttons: u32,
    lt: Option<u8>,
    rt: Option<u8>,
}

impl VirtualInput {
    fn button(&self, btn: ControllerButton) -> bool {
        BUTTON_ORDER
            .iter()
            .position(|b| *b == btn)
            .is_some_and(|i| self.buttons & (1 << i) != 0)
    }

    fn axes(&self, side: StickSide) -> (f32, f32) {
        match side {
            StickSide::Left => {
                if self.left_pad.touching {
                    (self.left_pad.x, self.left_pad.y)
                } else {
                    (self.lx, self.ly)
                }
            }
            StickSide::Right => {
                if self.right_pad.touching {
                    (self.right_pad.x, self.right_pad.y)
                } else {
                    (self.rx, self.ry)
                }
            }
        }
    }
}

impl ControllerInput for VirtualInput {
    fn left_stick_raw(&self) -> (f32, f32) {
        self.axes(StickSide::Left)
    }
    fn right_stick_raw(&self) -> (f32, f32) {
        self.axes(StickSide::Right)
    }
    // No re-warp / pad-origin — MCP supplies post-map coordinates (same as ReplayInput).
    fn left_stick(&self) -> (f32, f32) {
        self.axes(StickSide::Left)
    }
    fn right_stick(&self) -> (f32, f32) {
        self.axes(StickSide::Right)
    }
    fn dpad_up(&self) -> bool {
        self.button(ControllerButton::DpadUp)
    }
    fn dpad_down(&self) -> bool {
        self.button(ControllerButton::DpadDown)
    }
    fn dpad_left(&self) -> bool {
        self.button(ControllerButton::DpadLeft)
    }
    fn dpad_right(&self) -> bool {
        self.button(ControllerButton::DpadRight)
    }
    fn face_bottom(&self) -> bool {
        self.button(ControllerButton::FaceBottom)
    }
    fn face_right(&self) -> bool {
        self.button(ControllerButton::FaceRight)
    }
    fn face_top(&self) -> bool {
        self.button(ControllerButton::FaceTop)
    }
    fn face_left(&self) -> bool {
        self.button(ControllerButton::FaceLeft)
    }
    fn shoulder_left(&self) -> bool {
        self.button(ControllerButton::ShoulderLeft)
    }
    fn shoulder_right(&self) -> bool {
        self.button(ControllerButton::ShoulderRight)
    }
    fn stick_left(&self) -> bool {
        self.button(ControllerButton::StickLeft)
    }
    fn stick_right(&self) -> bool {
        self.button(ControllerButton::StickRight)
    }
    fn trigger_left(&self) -> Option<u8> {
        self.lt
    }
    fn trigger_right(&self) -> Option<u8> {
        self.rt
    }
    fn btn_options(&self) -> bool {
        self.button(ControllerButton::Options)
    }
    fn btn_share(&self) -> bool {
        self.button(ControllerButton::Share)
    }
    fn btn_system(&self) -> bool {
        self.button(ControllerButton::System)
    }
    fn pad_left(&self) -> bool {
        self.button(ControllerButton::PadLeft)
    }
    fn pad_right(&self) -> bool {
        self.button(ControllerButton::PadRight)
    }
    fn l4(&self) -> bool {
        self.button(ControllerButton::L4)
    }
    fn l5(&self) -> bool {
        self.button(ControllerButton::L5)
    }
    fn r4(&self) -> bool {
        self.button(ControllerButton::R4)
    }
    fn r5(&self) -> bool {
        self.button(ControllerButton::R5)
    }
    fn is_engaged(&self) -> bool {
        self.lx != 0.0
            || self.ly != 0.0
            || self.rx != 0.0
            || self.ry != 0.0
            || self.left_pad.touching
            || self.right_pad.touching
            || self.buttons != 0
            || self.lt.is_some()
            || self.rt.is_some()
    }
    fn box_clone(&self) -> Box<dyn ControllerInput + Send + Sync> {
        Box::new(self.clone())
    }
}

#[derive(Default)]
pub struct VirtualController {
    state: VirtualState,
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
            self.state.left_pad = PadState::default();
            self.expires.left_pad = None;
        }
        if self.expires.right_pad.is_some_and(|t| now >= t) {
            self.state.right_pad = PadState::default();
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
        self.state = VirtualState::default();
        self.expires = VirtualExpires::default();
    }

    pub fn snapshot(&mut self) -> VirtualInput {
        self.apply_expiry();
        VirtualInput {
            lx: self.state.lx,
            ly: self.state.ly,
            rx: self.state.rx,
            ry: self.state.ry,
            left_pad: self.state.left_pad,
            right_pad: self.state.right_pad,
            buttons: self.state.buttons,
            lt: self.state.lt,
            rt: self.state.rt,
        }
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
        let pad = PadState {
            touching,
            x: clamp_axis(x),
            y: clamp_axis(y),
        };
        let until = Self::deadline(duration_ms);
        match side {
            StickSide::Left => {
                self.state.left_pad = pad;
                self.expires.left_pad = until;
            }
            StickSide::Right => {
                self.state.right_pad = pad;
                self.expires.right_pad = until;
            }
        }
    }

    pub fn release_pad(&mut self, side: StickSide) {
        match side {
            StickSide::Left => {
                self.state.left_pad = PadState::default();
                self.expires.left_pad = None;
            }
            StickSide::Right => {
                self.state.right_pad = PadState::default();
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
        let mut pressed = Vec::new();
        for btn in BUTTON_ORDER {
            if snap.button(btn.clone()) {
                pressed.push(format!("{btn:?}"));
            }
        }
        json!({
            "lx": snap.lx,
            "ly": snap.ly,
            "rx": snap.rx,
            "ry": snap.ry,
            "left_pad": {
                "touching": snap.left_pad.touching,
                "x": snap.left_pad.x,
                "y": snap.left_pad.y,
            },
            "right_pad": {
                "touching": snap.right_pad.touching,
                "x": snap.right_pad.x,
                "y": snap.right_pad.y,
            },
            "buttons": snap.buttons,
            "pressed": pressed,
            "lt": snap.lt,
            "rt": snap.rt,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert!((s.left_stick().0 + 0.9).abs() < 1e-6);
        assert!((s.left_stick().1 - 0.1).abs() < 1e-6);
        // Pad click bit is independent of touching.
        assert!(!s.pad_left());
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
        assert!(!s.right_pad.touching);
        assert!(!s.face_bottom());
        assert!(s.trigger_left().is_none());
    }
}

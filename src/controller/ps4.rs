use anyhow::Result;
use hidapi::HidDevice;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use crate::controller::ControllerInput;

const STICK_OFFSET: i32 = 128;
const STICK_THRESHOLD: i32 = 10;
/// How long to wait after the very first press before firing again.
const DEBOUNCE_INITIAL: Duration = Duration::from_millis(400);
/// Repeat interval once the initial delay has elapsed.
const DEBOUNCE_REPEAT: Duration = Duration::from_millis(50);

/// Digital button and trigger state from the controller (face, shoulders, d-pad, etc.).
#[derive(Default, Clone, Debug)]
pub struct Ps4PhysicalState {
    pub dpad_up: bool,
    pub dpad_down: bool,
    pub dpad_left: bool,
    pub dpad_right: bool,
    pub cross: bool,
    pub circle: bool,
    pub triangle: bool,
    pub square: bool,
    pub l1: bool,
    pub r1: bool,
    pub l3: bool,
    pub r3: bool,
    pub l2: Option<u8>,
    pub r2: Option<u8>,
    pub options: bool,
    pub share: bool,
    pub ps: bool,
}

impl Ps4PhysicalState {
    pub fn any_digital(&self) -> bool {
        self.dpad_up
            || self.dpad_down
            || self.dpad_left
            || self.dpad_right
            || self.cross
            || self.circle
            || self.triangle
            || self.square
            || self.l1
            || self.r1
            || self.l3
            || self.r3
            || self.options
            || self.share
            || self.ps
            || self.l2.is_some()
            || self.r2.is_some()
    }
}

impl ControllerInput for Ps4PhysicalState {
    fn left_stick_raw(&self) -> (f32, f32) {
        (0.0, 0.0)
    }
    fn right_stick_raw(&self) -> (f32, f32) {
        (0.0, 0.0)
    }
    fn dpad_up(&self) -> bool {
        self.dpad_up
    }
    fn dpad_down(&self) -> bool {
        self.dpad_down
    }
    fn dpad_left(&self) -> bool {
        self.dpad_left
    }
    fn dpad_right(&self) -> bool {
        self.dpad_right
    }
    fn face_bottom(&self) -> bool {
        self.cross
    }
    fn face_right(&self) -> bool {
        self.circle
    }
    fn face_top(&self) -> bool {
        self.triangle
    }
    fn face_left(&self) -> bool {
        self.square
    }
    fn shoulder_left(&self) -> bool {
        self.l1
    }
    fn shoulder_right(&self) -> bool {
        self.r1
    }
    fn stick_left(&self) -> bool {
        self.l3
    }
    fn stick_right(&self) -> bool {
        self.r3
    }
    fn trigger_left(&self) -> Option<u8> {
        self.l2
    }
    fn trigger_right(&self) -> Option<u8> {
        self.r2
    }
    fn btn_options(&self) -> bool {
        self.options
    }
    fn btn_share(&self) -> bool {
        self.share
    }
    fn btn_system(&self) -> bool {
        self.ps
    }
    fn physical(&self) -> &dyn ControllerInput {
        self
    }

    fn box_clone(&self) -> Box<dyn ControllerInput + Send + Sync> {
        Box::new(self.clone())
    }
}

#[allow(unused)]
#[derive(Default, Clone, Debug)]
pub struct Ps4InputData {
    left: (i32, i32),
    right: (i32, i32),
    /// Raw HID button state (before repeat debouncing).
    physical: Ps4PhysicalState,
    /// Repeat-debounced button state exposed via [`ControllerInput`].
    debounced: Ps4PhysicalState,
}

impl Ps4InputData {
    fn sticks_active(&self) -> bool {
        self.left != (0, 0) || self.right != (0, 0)
    }
}

#[derive(Debug)]
struct Ps4Input {
    data: RwLock<Ps4InputData>,
}

impl Default for Ps4Input {
    fn default() -> Self {
        Self {
            data: RwLock::new(Ps4InputData::default()),
        }
    }
}

impl Ps4Input {
    fn update_from_report(&self, report: &[u8]) -> bool {
        if report.is_empty() {
            return false;
        }

        let report_id = report[0];

        // Determine the offset where the BasicGetStateData starts
        let state_offset = match report_id {
            0x01 => {
                // Standard report - state data starts at byte 1
                if report.len() < 10 {
                    return false;
                }
                1
            }
            0x11 => {
                // Enhanced report (Steam mode) - state data starts at byte 3
                // Report structure: [0x11, flags1, flags2, state_data...]
                if report.len() < 12 {
                    return false;
                }
                3
            }
            _ => {
                // Unknown report type
                return false;
            }
        };

        // BasicGetStateData is 9 bytes starting at state_offset
        if report.len() < state_offset + 9 {
            return false;
        }

        let left_x = report[state_offset] as i32;
        let left_y = report[state_offset + 1] as i32;
        let right_x = report[state_offset + 2] as i32;
        let right_y = report[state_offset + 3] as i32;

        // Byte 4 of BasicGetStateData: D-pad and face buttons
        let dpad_byte = report[state_offset + 4];
        // Byte 5 of BasicGetStateData: shoulder and stick buttons
        let face_shoulder_byte = report[state_offset + 5];
        // Byte 6 of BasicGetStateData: system buttons
        let system_byte = report[state_offset + 6];
        // Bytes 7-8 of BasicGetStateData: triggers
        let left_trigger_byte = report[state_offset + 7];
        let right_trigger_byte = report[state_offset + 8];

        // D-pad: bits 0-3 of byte 4. Diagonals (1/3/5/7) report no direction so the rest of
        // the app only ever sees a single cardinal direction at a time.
        let (dpad_up, dpad_right, dpad_down, dpad_left) = match dpad_byte & 0x0F {
            0 => (true, false, false, false),
            2 => (false, true, false, false),
            4 => (false, false, true, false),
            6 => (false, false, false, true),
            _ => (false, false, false, false),
        };

        // Face buttons - bits 4-7 of byte 4
        let square = (dpad_byte & 0x10) != 0;
        let cross = (dpad_byte & 0x20) != 0;
        let circle = (dpad_byte & 0x40) != 0;
        let triangle = (dpad_byte & 0x80) != 0;

        // Shoulder buttons - bits 0-1 of byte 5
        let l1 = (face_shoulder_byte & 0x01) != 0;
        let r1 = (face_shoulder_byte & 0x02) != 0;

        // Stick buttons / triggers - bits 2-3 of byte 5
        let l2 = ((face_shoulder_byte & 0x04) != 0).then_some(left_trigger_byte);
        let r2 = ((face_shoulder_byte & 0x08) != 0).then_some(right_trigger_byte);

        // Stick press buttons - bits 6-7 of byte 5
        let l3 = (face_shoulder_byte & 0x40) != 0;
        let r3 = (face_shoulder_byte & 0x80) != 0;

        // System buttons - bits 4-5 of byte 5
        let share = (face_shoulder_byte & 0x10) != 0;
        let options = (face_shoulder_byte & 0x20) != 0;

        // PS button - bit 0 of byte 6
        let ps = (system_byte & 0x01) != 0;

        let lx = left_x - STICK_OFFSET;
        let ly = left_y - STICK_OFFSET;
        let rx = right_x - STICK_OFFSET;
        let ry = right_y - STICK_OFFSET;

        let sticks_active = lx.abs() > STICK_THRESHOLD
            || ly.abs() > STICK_THRESHOLD
            || rx.abs() > STICK_THRESHOLD
            || ry.abs() > STICK_THRESHOLD;

        let dpad_active = dpad_up || dpad_down || dpad_left || dpad_right;
        let is_active = dpad_active
            || sticks_active
            || cross
            || circle
            || triangle
            || square
            || l1
            || r1
            || l2.is_some()
            || r2.is_some()
            || l3
            || r3
            || options
            || share
            || ps;

        let physical = Ps4PhysicalState {
            dpad_up,
            dpad_down,
            dpad_left,
            dpad_right,
            cross,
            circle,
            triangle,
            square,
            l1,
            r1,
            l2,
            r2,
            l3,
            r3,
            options,
            share,
            ps,
        };

        let mut data = self.data.write().unwrap();
        data.left = (lx, ly);
        data.right = (rx, ry);
        data.physical = physical;

        is_active
    }

    fn read(&self) -> Ps4InputData {
        self.data.read().unwrap().clone()
    }
}

/// Per-button state machine for initial-delay + repeat-rate debouncing.
#[derive(Default)]
enum ButtonState {
    /// Button is not held.
    #[default]
    Idle,
    /// Button was just pressed; waiting out the initial delay before repeating.
    InitialDelay { since: Instant },
    /// Initial delay elapsed; firing repeatedly at DEBOUNCE_REPEAT interval.
    Repeating { last: Instant },
}

/// Advance the state machine for one poll tick.
/// Returns `true` when the press should be forwarded to the caller.
fn debounce_allow(slot: &mut ButtonState, pressed: bool) -> bool {
    if !pressed {
        *slot = ButtonState::Idle;
        return false;
    }
    let now = Instant::now();
    match slot {
        // First press — fire immediately and start the initial delay.
        ButtonState::Idle => {
            *slot = ButtonState::InitialDelay { since: now };
            true
        }
        // Still within the initial hold delay — suppress.
        ButtonState::InitialDelay { since } if now.duration_since(*since) < DEBOUNCE_INITIAL => {
            false
        }
        // Initial delay elapsed — switch to repeat mode and fire.
        ButtonState::InitialDelay { .. } => {
            *slot = ButtonState::Repeating { last: now };
            true
        }
        // Repeating, but repeat interval not yet elapsed — suppress.
        ButtonState::Repeating { last } if now.duration_since(*last) < DEBOUNCE_REPEAT => false,
        // Repeat interval elapsed — fire and update timestamp.
        ButtonState::Repeating { last } => {
            *last = now;
            true
        }
    }
}

/// Holds per-button debounce state for every digital input.
#[derive(Default)]
struct DebounceState {
    cross: ButtonState,
    circle: ButtonState,
    triangle: ButtonState,
    square: ButtonState,
    l1: ButtonState,
    r1: ButtonState,
    l3: ButtonState,
    r3: ButtonState,
    options: ButtonState,
    share: ButtonState,
    ps: ButtonState,
    dpad: ButtonState,
}

impl DebounceState {
    fn filter(&mut self, data: &mut Ps4InputData) {
        let p = &data.physical;
        let d = &mut data.debounced;

        d.cross = debounce_allow(&mut self.cross, p.cross);
        d.circle = debounce_allow(&mut self.circle, p.circle);
        d.triangle = debounce_allow(&mut self.triangle, p.triangle);
        d.square = debounce_allow(&mut self.square, p.square);
        d.l1 = debounce_allow(&mut self.l1, p.l1);
        d.r1 = debounce_allow(&mut self.r1, p.r1);
        d.l3 = debounce_allow(&mut self.l3, p.l3);
        d.r3 = debounce_allow(&mut self.r3, p.r3);
        d.options = debounce_allow(&mut self.options, p.options);
        d.share = debounce_allow(&mut self.share, p.share);
        d.ps = debounce_allow(&mut self.ps, p.ps);

        // Triggers (l2/r2) are analog and not debounced.
        d.l2 = p.l2;
        d.r2 = p.r2;

        // The four dpad directions share one debounce slot so direction changes don't
        // bypass the initial-delay window.
        let dpad_pressed = p.dpad_up || p.dpad_down || p.dpad_left || p.dpad_right;
        if debounce_allow(&mut self.dpad, dpad_pressed) {
            d.dpad_up = p.dpad_up;
            d.dpad_down = p.dpad_down;
            d.dpad_left = p.dpad_left;
            d.dpad_right = p.dpad_right;
        } else {
            d.dpad_up = false;
            d.dpad_down = false;
            d.dpad_left = false;
            d.dpad_right = false;
        }
    }
}

pub struct Ps4Device {
    input: Ps4Input,
    device: HidDevice,
    was_active: bool,
    debounce: DebounceState,
}

impl Ps4Device {
    pub fn new(device: HidDevice) -> Self {
        Self {
            input: Default::default(),
            device,
            was_active: false,
            debounce: Default::default(),
        }
    }

    fn poll(&self) -> Result<bool> {
        let mut report = [0u8; 64];
        let mut active = false;
        if self.device.read(&mut report)? >= 10 {
            active = self.input.update_from_report(&report);
        }
        Ok(active)
    }
}

impl Iterator for Ps4Device {
    type Item = Option<Box<dyn ControllerInput>>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.poll() {
                Ok(_) => {
                    let mut data = self.input.read();
                    self.debounce.filter(&mut data);
                    // Stay active while anything is physically held, even if repeat debounce
                    // suppresses the debounced button flags (needed for chord leaders).
                    let still_active = data.physical.any_digital()
                        || data.debounced.any_digital()
                        || data.sticks_active();
                    if still_active {
                        self.was_active = true;
                        return Some(Some(Box::new(data)));
                    }
                    if std::mem::replace(&mut self.was_active, false) {
                        return Some(None);
                    }
                    continue;
                }
                Err(_) => {
                    return None;
                }
            }
        }
    }
}

impl ControllerInput for Ps4InputData {
    fn left_stick_raw(&self) -> (f32, f32) {
        (self.left.0 as f32 / 128.0, self.left.1 as f32 / 128.0)
    }
    fn right_stick_raw(&self) -> (f32, f32) {
        (self.right.0 as f32 / 128.0, self.right.1 as f32 / 128.0)
    }
    fn dpad_up(&self) -> bool {
        self.debounced.dpad_up()
    }
    fn dpad_down(&self) -> bool {
        self.debounced.dpad_down()
    }
    fn dpad_left(&self) -> bool {
        self.debounced.dpad_left()
    }
    fn dpad_right(&self) -> bool {
        self.debounced.dpad_right()
    }
    fn face_bottom(&self) -> bool {
        self.debounced.face_bottom()
    }
    fn face_right(&self) -> bool {
        self.debounced.face_right()
    }
    fn face_top(&self) -> bool {
        self.debounced.face_top()
    }
    fn face_left(&self) -> bool {
        self.debounced.face_left()
    }
    fn shoulder_left(&self) -> bool {
        self.debounced.shoulder_left()
    }
    fn shoulder_right(&self) -> bool {
        self.debounced.shoulder_right()
    }
    fn stick_left(&self) -> bool {
        self.debounced.stick_left()
    }
    fn stick_right(&self) -> bool {
        self.debounced.stick_right()
    }
    fn trigger_left(&self) -> Option<u8> {
        self.debounced.trigger_left()
    }
    fn trigger_right(&self) -> Option<u8> {
        self.debounced.trigger_right()
    }
    fn btn_options(&self) -> bool {
        self.debounced.btn_options()
    }
    fn btn_share(&self) -> bool {
        self.debounced.btn_share()
    }
    fn btn_system(&self) -> bool {
        self.debounced.btn_system()
    }

    fn physical(&self) -> &dyn ControllerInput {
        &self.physical
    }

    fn box_clone(&self) -> Box<dyn ControllerInput + Send + Sync> {
        Box::new(self.clone())
    }
}

use anyhow::Result;
use hidapi::HidDevice;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use crate::controller::{ControllerInput, Dpad};

const STICK_OFFSET: i32 = 128;
const STICK_THRESHOLD: i32 = 10;
/// How long to wait after the very first press before firing again.
const DEBOUNCE_INITIAL: Duration = Duration::from_millis(400);
/// Repeat interval once the initial delay has elapsed.
const DEBOUNCE_REPEAT: Duration = Duration::from_millis(50);

#[allow(unused)]
#[derive(Default, Clone, Debug)]
pub struct Ps4InputData {
    left: (i32, i32),
    right: (i32, i32),
    dpad: Option<Dpad>,
    cross: bool,
    circle: bool,
    triangle: bool,
    square: bool,
    l1: bool,
    r1: bool,
    l3: bool,
    r3: bool,
    l2: Option<u8>,
    r2: Option<u8>,
    options: bool,
    share: bool,
    ps: bool,
}

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

        // D-pad: bits 0-3 of byte 4
        let dpad = {
            use Dpad::*;
            match dpad_byte & 0x0F {
                0 => Some(Up),
                1 => Some(UpRight),
                2 => Some(Right),
                3 => Some(DownRight),
                4 => Some(Down),
                5 => Some(DownLeft),
                6 => Some(Left),
                7 => Some(UpLeft),
                _ => None,
            }
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

        let is_active = dpad.is_some()
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

        let mut data = self.data.write().unwrap();
        *data = Ps4InputData {
            left: (lx, ly),
            right: (rx, ry),
            dpad,
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
        data.cross = debounce_allow(&mut self.cross, data.cross);
        data.circle = debounce_allow(&mut self.circle, data.circle);
        data.triangle = debounce_allow(&mut self.triangle, data.triangle);
        data.square = debounce_allow(&mut self.square, data.square);
        data.l1 = debounce_allow(&mut self.l1, data.l1);
        data.r1 = debounce_allow(&mut self.r1, data.r1);
        data.l3 = debounce_allow(&mut self.l3, data.l3);
        data.r3 = debounce_allow(&mut self.r3, data.r3);
        data.options = debounce_allow(&mut self.options, data.options);
        data.share = debounce_allow(&mut self.share, data.share);
        data.ps = debounce_allow(&mut self.ps, data.ps);

        // Triggers (l2/r2) are analog and not debounced.
        if !debounce_allow(&mut self.dpad, data.dpad.is_some()) {
            data.dpad = None;
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
                    // Only treat the frame as active if anything survived debouncing.
                    let still_active = data.dpad.is_some()
                        || data.cross
                        || data.circle
                        || data.triangle
                        || data.square
                        || data.l1
                        || data.r1
                        || data.l2.is_some()
                        || data.r2.is_some()
                        || data.l3
                        || data.r3
                        || data.options
                        || data.share
                        || data.ps
                        || data.left != (0, 0)
                        || data.right != (0, 0);
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
    fn dpad(&self) -> Option<Dpad> {
        self.dpad.clone()
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

    fn box_clone(&self) -> Box<dyn ControllerInput + Send + Sync> {
        Box::new(self.clone())
    }
}

use anyhow::Result;
use hidapi::{HidApi, HidDevice};
use std::sync::RwLock;

use crate::controller::ControllerInput;

const PS4_VID: u16 = 0x054c;
const PS4_PID: u16 = 0x09cc;
const STICK_OFFSET: i32 = 128;
const STICK_THRESHOLD: i32 = 10;

#[allow(unused)]
#[derive(Default, Clone, Debug)]
pub struct Ps4InputData {
    left: (i32, i32),
    right: (i32, i32),
    dpad_up: bool,
    dpad_down: bool,
    dpad_left: bool,
    dpad_right: bool,
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

impl Ps4InputData {
    fn sticks_active(&self) -> bool {
        self.left != (0, 0) || self.right != (0, 0)
    }

    fn any_digital(&self) -> bool {
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

        let mut data = self.data.write().unwrap();
        data.left = (lx, ly);
        data.right = (rx, ry);
        data.dpad_up = dpad_up;
        data.dpad_down = dpad_down;
        data.dpad_left = dpad_left;
        data.dpad_right = dpad_right;
        data.cross = cross;
        data.circle = circle;
        data.triangle = triangle;
        data.square = square;
        data.l1 = l1;
        data.r1 = r1;
        data.l2 = l2;
        data.r2 = r2;
        data.l3 = l3;
        data.r3 = r3;
        data.options = options;
        data.share = share;
        data.ps = ps;

        is_active
    }

    fn read(&self) -> Ps4InputData {
        self.data.read().unwrap().clone()
    }
}

pub struct Ps4Device {
    input: Ps4Input,
    device: HidDevice,
    was_active: bool,
}

impl Ps4Device {
    pub fn new(device: HidDevice) -> Self {
        Self {
            input: Default::default(),
            device,
            was_active: false,
        }
    }

    pub fn open(hid: &HidApi) -> Option<Self> {
        for device in hid.device_list() {
            if device.vendor_id() == PS4_VID && device.product_id() == PS4_PID {
                if let Ok(dev) = device.open_device(hid) {
                    return Some(Self::new(dev));
                }
            }
        }
        None
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
                    let data = self.input.read();
                    if data.is_engaged() {
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

    fn is_engaged(&self) -> bool {
        self.any_digital() || self.sticks_active()
    }

    fn box_clone(&self) -> Box<dyn ControllerInput + Send + Sync> {
        Box::new(self.clone())
    }
}

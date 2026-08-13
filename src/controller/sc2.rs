//! Steam Controller 2 (Triton) HID driver.
//!
//! Report layout, button masks, and lizard-mode feature report are derived from
//! SDL's hidapi Steam Triton driver (`SDL_hidapi_steam_triton.c`,
//! `controller_structs.h`) — zlib license. Constants and packing only; not a
//! copy of the SDL sources.

use std::time::{Duration, Instant};

use hidapi::{DeviceInfo, HidApi, HidDevice};

use crate::controller::ControllerInput;

const VALVE_VID: u16 = 0x28de;
const PID_WIRED: u16 = 0x1302;
const PID_BLE: u16 = 0x1303;
const PID_PROTEUS: u16 = 0x1304;
const PID_NEREID: u16 = 0x1305;

const REPORT_STATE: u8 = 0x42;
const REPORT_STATE_BLE: u8 = 0x45;
const REPORT_STATE_TS: u8 = 0x47;

const BTN_A: u32 = 0x0000_0001;
const BTN_B: u32 = 0x0000_0002;
const BTN_X: u32 = 0x0000_0004;
const BTN_Y: u32 = 0x0000_0008;
const BTN_R3: u32 = 0x0000_0020;
const BTN_VIEW: u32 = 0x0000_0040;
const BTN_RB: u32 = 0x0000_0200;
const BTN_DPAD_DOWN: u32 = 0x0000_0400;
const BTN_DPAD_RIGHT: u32 = 0x0000_0800;
const BTN_DPAD_LEFT: u32 = 0x0000_1000;
const BTN_DPAD_UP: u32 = 0x0000_2000;
const BTN_MENU: u32 = 0x0000_4000;
const BTN_L3: u32 = 0x0000_8000;
const BTN_STEAM: u32 = 0x0001_0000;
const BTN_LB: u32 = 0x0008_0000;
const BTN_RPAD_TOUCH: u32 = 0x0020_0000;
const BTN_RPAD_CLICK: u32 = 0x0040_0000;
const BTN_RTRIG_CLICK: u32 = 0x0080_0000;
const BTN_LPAD_TOUCH: u32 = 0x0200_0000;
const BTN_LPAD_CLICK: u32 = 0x0400_0000;
const BTN_LTRIG_CLICK: u32 = 0x0800_0000;

const TRIGGER_ANALOG_GATE: i16 = 3276; // ~10% of 32767
const LIZARD_REFRESH: Duration = Duration::from_secs(3);
const READ_TIMEOUT_MS: i32 = 50;
const SLOT_PEEK_MS: i32 = 200;

/// Parsed Triton state (pads, sticks, buttons). IMU is ignored.
#[derive(Debug, Clone, Default)]
pub struct Sc2State {
    pub seq: u8,
    pub buttons: u32,
    pub trigger_left: i16,
    pub trigger_right: i16,
    pub left_stick: (i16, i16),
    pub right_stick: (i16, i16),
    pub left_pad: (i16, i16),
    pub right_pad: (i16, i16),
    pub pressure_left: u16,
    pub pressure_right: u16,
}

impl Sc2State {
    fn bit(&self, mask: u32) -> bool {
        self.buttons & mask != 0
    }

    pub fn left_pad_touch(&self) -> bool {
        self.bit(BTN_LPAD_TOUCH)
    }

    pub fn right_pad_touch(&self) -> bool {
        self.bit(BTN_RPAD_TOUCH)
    }

    pub fn pad_as_stick_left(&self) -> (f32, f32) {
        pad_as_stick(self.left_pad.0, self.left_pad.1, self.left_pad_touch())
    }

    pub fn pad_as_stick_right(&self) -> (f32, f32) {
        pad_as_stick(self.right_pad.0, self.right_pad.1, self.right_pad_touch())
    }

    fn analog_stick_raw(v: (i16, i16)) -> (f32, f32) {
        (
            (v.0 as f32 / 32767.0).clamp(-1.0, 1.0),
            (-(v.1 as f32) / 32767.0).clamp(-1.0, 1.0),
        )
    }

    pub fn physical_left_stick(&self) -> (f32, f32) {
        Self::analog_stick_raw(self.left_stick)
    }

    pub fn physical_right_stick(&self) -> (f32, f32) {
        Self::analog_stick_raw(self.right_stick)
    }
}

pub fn pad_as_stick(pad_x: i16, pad_y: i16, touching: bool) -> (f32, f32) {
    if !touching {
        return (0.0, 0.0);
    }
    (
        (pad_x as f32 / 32767.0).clamp(-1.0, 1.0),
        (-(pad_y as f32) / 32767.0).clamp(-1.0, 1.0),
    )
}

fn scale_trigger(v: i16, click: bool) -> Option<u8> {
    let v = v.max(0);
    if !click && v < TRIGGER_ANALOG_GATE {
        return None;
    }
    Some(((v as u32 * 255) / 32767).min(255) as u8)
}

fn i16_at(data: &[u8], off: usize) -> Option<i16> {
    Some(i16::from_le_bytes([*data.get(off)?, *data.get(off + 1)?]))
}

fn u16_at(data: &[u8], off: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*data.get(off)?, *data.get(off + 1)?]))
}

pub fn parse_input_report(report: &[u8]) -> Option<Sc2State> {
    let id = *report.first()?;
    let data = report.get(1..)?;
    let pad_off = match id {
        REPORT_STATE | REPORT_STATE_BLE => 17,
        REPORT_STATE_TS => 19,
        _ => return None,
    };
    if data.len() < pad_off + 12 {
        return None;
    }

    let buttons = u32::from_le_bytes([*data.get(1)?, *data.get(2)?, *data.get(3)?, *data.get(4)?]);

    Some(Sc2State {
        seq: *data.first()?,
        buttons,
        trigger_left: i16_at(data, 5)?,
        trigger_right: i16_at(data, 7)?,
        left_stick: (i16_at(data, 9)?, i16_at(data, 11)?),
        right_stick: (i16_at(data, 13)?, i16_at(data, 15)?),
        left_pad: (i16_at(data, pad_off)?, i16_at(data, pad_off + 2)?),
        pressure_left: u16_at(data, pad_off + 4)?,
        right_pad: (i16_at(data, pad_off + 6)?, i16_at(data, pad_off + 8)?),
        pressure_right: u16_at(data, pad_off + 10)?,
    })
}

/// Disable lizard-mode keyboard/mouse emulation (SDL SETTING_LIZARD_MODE = OFF).
pub fn disable_lizard_mode(device: &HidDevice) -> Result<(), hidapi::HidError> {
    let mut buf = [0u8; 64];
    buf[0] = 1; // hidapi report-id prefix
    buf[1] = 0x87; // ID_SET_SETTINGS_VALUES
    buf[2] = 3; // sizeof(ControllerSetting)
    buf[3] = 9; // SETTING_LIZARD_MODE
    device.send_feature_report(&buf).map(|_| ())
}

fn is_puck(pid: u16) -> bool {
    pid == PID_PROTEUS || pid == PID_NEREID
}

fn is_state_report(id: u8) -> bool {
    matches!(id, REPORT_STATE | REPORT_STATE_BLE | REPORT_STATE_TS)
}

fn open_and_confirm(hid: &HidApi, info: &DeviceInfo, require_state: bool) -> Option<HidDevice> {
    let dev = info.open_device(hid).ok()?;
    if let Err(e) = disable_lizard_mode(&dev) {
        eprintln!("warn: failed to disable SC2 lizard mode: {e}");
    }
    if !require_state {
        return Some(dev);
    }
    let deadline = Instant::now() + Duration::from_millis(SLOT_PEEK_MS as u64);
    let mut buf = [0u8; 64];
    while Instant::now() < deadline {
        let remaining = deadline
            .saturating_duration_since(Instant::now())
            .as_millis() as i32;
        match dev.read_timeout(&mut buf, remaining.max(1)) {
            Ok(n) if n > 0 && is_state_report(buf[0]) => return Some(dev),
            Ok(_) => continue,
            Err(_) => return None,
        }
    }
    None
}

pub fn open(hid: &HidApi) -> Option<Sc2Device> {
    let devices: Vec<DeviceInfo> = hid.device_list().cloned().collect();

    let mut puck: Vec<&DeviceInfo> = devices
        .iter()
        .filter(|d| d.vendor_id() == VALVE_VID && is_puck(d.product_id()))
        .filter(|d| (2..=5).contains(&d.interface_number()))
        // Windows splits each slot into mouse/keyboard (GD) + vendor state (0xFF00).
        .filter(|d| d.usage_page() >= 0xFF00)
        .collect();
    puck.sort_by_key(|d| d.interface_number());
    for info in puck {
        if let Some(dev) = open_and_confirm(hid, info, true) {
            return Some(Sc2Device::new(dev));
        }
    }

    let mut wired: Vec<&DeviceInfo> = devices
        .iter()
        .filter(|d| d.vendor_id() == VALVE_VID && d.product_id() == PID_WIRED)
        .collect();
    wired.sort_by_key(|d| std::cmp::Reverse(d.usage_page() >= 0xFF00));
    for info in wired {
        if let Some(dev) = open_and_confirm(hid, info, false) {
            return Some(Sc2Device::new(dev));
        }
    }

    let mut ble: Vec<&DeviceInfo> = devices
        .iter()
        .filter(|d| d.vendor_id() == VALVE_VID && d.product_id() == PID_BLE)
        .collect();
    ble.sort_by_key(|d| std::cmp::Reverse(d.usage_page() >= 0xFF00));
    for info in ble {
        if let Some(dev) = open_and_confirm(hid, info, false) {
            return Some(Sc2Device::new(dev));
        }
    }

    None
}

pub struct Sc2Device {
    device: HidDevice,
    state: Sc2State,
    has_state: bool,
    was_engaged: bool,
    last_lizard: Instant,
    last_report: Option<Vec<u8>>,
}

impl Sc2Device {
    fn new(device: HidDevice) -> Self {
        Self {
            device,
            state: Sc2State::default(),
            has_state: false,
            was_engaged: false,
            last_lizard: Instant::now(),
            last_report: None,
        }
    }

    pub fn last_state(&self) -> &Sc2State {
        &self.state
    }

    pub fn last_raw_report(&self) -> Option<&[u8]> {
        self.last_report.as_deref()
    }

    fn maybe_lizard(&mut self) {
        if self.last_lizard.elapsed() >= LIZARD_REFRESH && disable_lizard_mode(&self.device).is_ok()
        {
            self.last_lizard = Instant::now();
        }
    }
}

impl Iterator for Sc2Device {
    type Item = Option<Box<dyn ControllerInput>>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            self.maybe_lizard();
            let mut report = [0u8; 64];
            match self.device.read_timeout(&mut report, READ_TIMEOUT_MS) {
                Ok(0) => continue,
                Ok(_) => {
                    if let Some(state) = parse_input_report(&report) {
                        self.last_report = Some(report.to_vec());
                        self.state = state;
                        self.has_state = true;
                    } else if !self.has_state {
                        continue;
                    }
                    if self.state.is_engaged() {
                        self.was_engaged = true;
                        return Some(Some(Box::new(self.state.clone())));
                    }
                    if std::mem::replace(&mut self.was_engaged, false) {
                        return Some(None);
                    }
                }
                Err(_) => return None,
            }
        }
    }
}

impl ControllerInput for Sc2State {
    fn left_stick_raw(&self) -> (f32, f32) {
        self.pad_as_stick_left()
    }
    fn right_stick_raw(&self) -> (f32, f32) {
        self.pad_as_stick_right()
    }
    fn dpad_up(&self) -> bool {
        self.bit(BTN_DPAD_UP)
    }
    fn dpad_down(&self) -> bool {
        self.bit(BTN_DPAD_DOWN)
    }
    fn dpad_left(&self) -> bool {
        self.bit(BTN_DPAD_LEFT)
    }
    fn dpad_right(&self) -> bool {
        self.bit(BTN_DPAD_RIGHT)
    }
    fn face_bottom(&self) -> bool {
        self.bit(BTN_A)
    }
    fn face_right(&self) -> bool {
        self.bit(BTN_B)
    }
    fn face_left(&self) -> bool {
        self.bit(BTN_X)
    }
    fn face_top(&self) -> bool {
        self.bit(BTN_Y)
    }
    fn shoulder_left(&self) -> bool {
        self.bit(BTN_LB)
    }
    fn shoulder_right(&self) -> bool {
        self.bit(BTN_RB)
    }
    fn stick_left(&self) -> bool {
        self.bit(BTN_L3)
    }
    fn stick_right(&self) -> bool {
        self.bit(BTN_R3)
    }
    fn trigger_left(&self) -> Option<u8> {
        scale_trigger(self.trigger_left, self.bit(BTN_LTRIG_CLICK))
    }
    fn trigger_right(&self) -> Option<u8> {
        scale_trigger(self.trigger_right, self.bit(BTN_RTRIG_CLICK))
    }
    fn btn_options(&self) -> bool {
        self.bit(BTN_MENU)
    }
    fn btn_share(&self) -> bool {
        self.bit(BTN_VIEW)
    }
    fn btn_system(&self) -> bool {
        self.bit(BTN_STEAM)
    }
    fn pad_left(&self) -> bool {
        self.bit(BTN_LPAD_CLICK)
    }
    fn pad_right(&self) -> bool {
        self.bit(BTN_RPAD_CLICK)
    }
    fn is_engaged(&self) -> bool {
        self.left_pad_touch()
            || self.right_pad_touch()
            || self.dpad_up()
            || self.dpad_down()
            || self.dpad_left()
            || self.dpad_right()
            || self.face_bottom()
            || self.face_right()
            || self.face_top()
            || self.face_left()
            || self.shoulder_left()
            || self.shoulder_right()
            || self.stick_left()
            || self.stick_right()
            || self.trigger_left().is_some()
            || self.trigger_right().is_some()
            || self.btn_options()
            || self.btn_share()
            || self.btn_system()
            || self.pad_left()
            || self.pad_right()
    }
    fn box_clone(&self) -> Box<dyn ControllerInput + Send + Sync> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put_i16(buf: &mut [u8], off: usize, v: i16) {
        let b = v.to_le_bytes();
        buf[off] = b[0];
        buf[off + 1] = b[1];
    }

    fn put_u32(buf: &mut [u8], off: usize, v: u32) {
        let b = v.to_le_bytes();
        buf[off..off + 4].copy_from_slice(&b);
    }

    #[test]
    fn parse_0x42_buttons_pads_triggers() {
        let mut r = [0u8; 54];
        r[0] = REPORT_STATE;
        r[1] = 7; // seq
        put_u32(&mut r, 2, BTN_A | BTN_LPAD_TOUCH);
        put_i16(&mut r, 6, 16383); // trigger L ~50%
        put_i16(&mut r, 18, 32767); // left pad X
        put_i16(&mut r, 20, 0); // left pad Y
        let s = parse_input_report(&r).expect("parse 0x42");
        assert_eq!(s.seq, 7);
        assert!(s.face_bottom());
        assert!(s.left_pad_touch());
        assert_eq!(s.left_pad.0, 32767);
        let (x, y) = s.pad_as_stick_left();
        assert!((x - 1.0).abs() < 0.001);
        assert!(y.abs() < 0.001);
        assert!(s.trigger_left().is_some());
    }

    #[test]
    fn parse_0x47_pads_shifted_by_two() {
        let mut r42 = [0u8; 54];
        r42[0] = REPORT_STATE;
        put_i16(&mut r42, 18, 1234);

        let mut r47 = [0u8; 56];
        r47[0] = REPORT_STATE_TS;
        put_i16(&mut r47, 20, 1234); // pads start +2 vs 0x42

        let a = parse_input_report(&r42).unwrap();
        let b = parse_input_report(&r47).unwrap();
        assert_eq!(a.left_pad.0, 1234);
        assert_eq!(b.left_pad.0, 1234);

        // A 0x42 parser on a 0x47 frame would read the timestamp as pad X.
        put_i16(&mut r47, 18, 99); // timestamp
        let b = parse_input_report(&r47).unwrap();
        assert_eq!(b.left_pad.0, 1234);
        let wrong = parse_input_report(&{
            let mut fake = r47;
            fake[0] = REPORT_STATE;
            fake
        })
        .unwrap();
        assert_eq!(wrong.left_pad.0, 99);
    }

    #[test]
    fn unknown_or_short_is_none() {
        assert!(parse_input_report(&[0x40, 0, 0, 0]).is_none());
        assert!(parse_input_report(&[REPORT_STATE, 1, 2, 3]).is_none());
        assert!(parse_input_report(&[]).is_none());
    }

    #[test]
    fn pad_as_stick_touch_and_y_flip() {
        assert_eq!(pad_as_stick(100, 100, false), (0.0, 0.0));
        let (x, y) = pad_as_stick(32767, 32767, true);
        assert!((x - 1.0).abs() < 0.001);
        assert!((y + 1.0).abs() < 0.001);
    }
}

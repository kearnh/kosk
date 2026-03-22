use anyhow::{bail, Result};
use hidapi::{HidApi, HidDevice};
use std::sync::RwLock;

const PS4_VID: u16 = 0x054c;
const PS4_PID: u16 = 0x09cc;
const STICK_OFFSET: i32 = 128;
const STICK_THRESHOLD: i32 = 10;

#[derive(Clone, Copy)]
enum Dpad {
    Up,
    Down,
    Left,
    Right,
    UpRight,
    UpLeft,
    DownRight,
    DownLeft,
}

impl ToString for Dpad {
    fn to_string(&self) -> String {
        match self {
            Dpad::Up => "↑",
            Dpad::UpRight => "↗",
            Dpad::Right => "→",
            Dpad::DownRight => "↘",
            Dpad::Down => "↓",
            Dpad::DownLeft => "↙",
            Dpad::Left => "←",
            Dpad::UpLeft => "↖",
        }
        .to_string()
    }
}

#[derive(Default, Clone)]
struct Ps4InputData {
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
        if report.len() < 10 {
            return false;
        }

        let left_x = report[1] as i32;
        let left_y = report[2] as i32;
        let right_x = report[3] as i32;
        let right_y = report[4] as i32;

        // Byte 5: D-pad (bits 0-3), Share(4), L3(5), R3(6), Options(7)
        // Byte 6: Square(0), Cross(1), Circle(2), Triangle(3), R1(4), L1(5)
        // Byte 7: R2(0-7), L2(0-7) - actually triggers are analog in bytes 8-9
        // Actually triggers are analog at bytes 8-9

        // D-pad: bits 0-3 of byte5

        let dpad = {
            use Dpad::*;
            match report[5] {
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

        // Face buttons
        let square = (report[5] & 0x10) != 0;
        let cross = (report[5] & 0x20) != 0;
        let circle = (report[5] & 0x40) != 0;
        let triangle = (report[5] & 0x80) != 0;

        // Shoulder buttons
        let l1 = (report[6] & 0x01) != 0;
        let r1 = (report[6] & 0x02) != 0;

        // Stick buttons / triggers
        let l2 = ((report[6] & 0x04) != 0).then_some(report[8]);
        let r2 = ((report[6] & 0x08) != 0).then_some(report[9]);

        let l3 = (report[6] & 0x40) != 0;
        let r3 = (report[6] & 0x80) != 0;

        // System buttons
        let share = (report[6] & 0x10) != 0;
        let options = (report[6] & 0x20) != 0;

        // Check for PS button - typically in extended report, but might be in byte7
        let ps = (report[7] & 0x01) != 0;

        let lx_dev = (left_x - STICK_OFFSET).abs();
        let ly_dev = (left_y - STICK_OFFSET).abs();
        let rx_dev = (right_x - STICK_OFFSET).abs();
        let ry_dev = (right_y - STICK_OFFSET).abs();

        let sticks_active = lx_dev > STICK_THRESHOLD
            || ly_dev > STICK_THRESHOLD
            || rx_dev > STICK_THRESHOLD
            || ry_dev > STICK_THRESHOLD;

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

        if is_active {
            let mut data = self.data.write().unwrap();
            *data = Ps4InputData {
                left: (left_x, left_y),
                right: (right_x, right_y),
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
        }

        is_active
    }

    fn read(&self) -> Ps4InputData {
        self.data.read().unwrap().clone()
    }
}

struct Ps4Device {
    input: Ps4Input,
    device: HidDevice,
}

impl Ps4Device {
    fn new(device: HidDevice) -> Self {
        Self {
            input: Default::default(),
            device,
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

impl<'a> Iterator for &'a Ps4Device {
    type Item = Ps4InputData;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.poll() {
                Ok(true) => {
                    // Read and return a clone of the input data
                    return Some(self.input.read());
                }
                Ok(false) => continue, // Keep polling until active
                Err(e) => {
                    eprintln!("error: {}", e);
                    continue;
                }
            }
        }
    }
}

fn find_device(hid: &HidApi, (vid, pid): (u16, u16)) -> Option<HidDevice> {
    for device in hid.device_list() {
        if device.vendor_id() == vid && device.product_id() == pid {
            if let Ok(dev) = device.open_device(hid) {
                return Some(dev);
            }
        }
    }
    None
}

fn main() -> Result<()> {
    println!(
        "Looking for PS4 controller (VID:{:04x}, PID:{:04x})...",
        PS4_VID, PS4_PID
    );

    let hid = HidApi::new()?;

    let device = match find_device(&hid, (PS4_VID, PS4_PID)) {
        Some(device) => device,
        None => bail!("PS4 controller not found."),
    };

    let ps4 = Ps4Device::new(device);

    for input in &ps4 {
        // Clear previous lines and print new output
        print!("\r\r\r\r\r");

        // Sticks
        let (left_x, left_y) = input.left;
        let (right_x, right_y) = input.right;
        print!(
            "L: ({:3}, {:3}) | R: ({:3}, {:3}) | ",
            left_x, left_y, right_x, right_y
        );

        // D-pad
        let dpad = match input.dpad {
            Some(dpad) => dpad.to_string(),
            _ => "·".to_string(),
        };

        // Face buttons
        let face = format!(
            "[{}][{}][{}][{}]",
            if input.square { "□" } else { "·" },
            if input.cross { "✕" } else { "·" },
            if input.circle { "○" } else { "·" },
            if input.triangle { "△" } else { "·" }
        );

        let shoulders = format!(
            "L1:{} R1:{} | L2:{:3} R2:{:3} | L3:{} R3:{}",
            if input.l1 { "█" } else { "·" },
            if input.r1 { "█" } else { "·" },
            input.l2.unwrap_or(0),
            input.r2.unwrap_or(0),
            if input.l3 { "█" } else { "·" },
            if input.r3 { "█" } else { "·" }
        );

        let sys = format!(
            "Share:{} Options:{} PS:{}",
            if input.share { "█" } else { "·" },
            if input.options { "█" } else { "·" },
            if input.ps { "█" } else { "·" }
        );

        print!("{} {} | {} | {}", dpad, face, shoulders, sys);
        print!("\r");
        std::io::Write::flush(&mut std::io::stdout()).ok();
    }

    Ok(())
}

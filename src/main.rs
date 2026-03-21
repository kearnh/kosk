use hidapi::{HidApi, HidDevice};
use std::thread;
use std::time::Duration;

const PS4_VID: u16 = 0x054c;
const PS4_PID: u16 = 0x09cc;
const STICK_OFFSET: i32 = 128;
const STICK_THRESHOLD: i32 = 10;

struct Ps4Buttons {
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
    l2: u8,
    r2: u8,
    l3: bool,
    r3: bool,
    options: bool,
    share: bool,
    ps: bool,
}

impl Ps4Buttons {
    fn from_report(buf: &[u8]) -> Self {
        // Byte 5: D-pad (bits 0-3), Share(4), L3(5), R3(6), Options(7)
        // Byte 6: Square(0), Cross(1), Circle(2), Triangle(3), R1(4), L1(5)
        // Byte 7: R2(0-7), L2(0-7) - actually triggers are analog in bytes 8-9
        // Actually triggers are analog at bytes 8-9

        let byte5 = buf.get(5).copied().unwrap_or(0);
        let byte6 = buf.get(6).copied().unwrap_or(0);
        let byte7 = buf.get(7).copied().unwrap_or(0);
        let r2_analog = buf.get(8).copied().unwrap_or(0);
        let l2_analog = buf.get(9).copied().unwrap_or(0);

        // D-pad: bits 0-3 of byte5
        let dpad = byte5 & 0x0f;
        let dpad_up = dpad == 0 || dpad == 1 || dpad == 7;
        let dpad_down = dpad == 3 || dpad == 4 || dpad == 5;
        let dpad_left = dpad == 5 || dpad == 6 || dpad == 7;
        let dpad_right = dpad == 1 || dpad == 2 || dpad == 3;

        // Face buttons
        let square = (byte6 & 0x01) != 0;
        let cross = (byte6 & 0x02) != 0;
        let circle = (byte6 & 0x04) != 0;
        let triangle = (byte6 & 0x08) != 0;

        // Shoulder buttons
        let r1 = (byte6 & 0x10) != 0;
        let l1 = (byte6 & 0x20) != 0;

        // Stick buttons / triggers
        let l3 = (byte5 & 0x40) != 0;
        let r3 = (byte5 & 0x80) != 0;

        // Triggers (analog)
        // The trigger format in byte7 is different per firmware, using bytes 8-9 for analog
        let r2 = (byte7 & 0x0f) * 17; // Convert to 0-255
        let l2 = ((byte7 >> 4) & 0x0f) * 17;
        let r2 = r2_analog.max(r2);
        let l2 = l2_analog.max(l2);

        // System buttons
        let share = (byte5 & 0x10) != 0;
        let options = (byte5 & 0x20) != 0;

        // Check for PS button - typically in extended report, but might be in byte7
        let ps = (byte7 & 0x40) != 0;

        Self {
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
        }
    }

    #[allow(unused)]
    fn to_string(&self) -> String {
        let mut s = String::new();

        // D-pad
        let dpad = match (
            self.dpad_up,
            self.dpad_down,
            self.dpad_left,
            self.dpad_right,
        ) {
            (true, false, false, false) => "↑",
            (true, false, false, true) => "↗",
            (true, false, true, false) => "↖",
            (false, false, false, true) => "→",
            (false, false, true, false) => "←",
            (false, true, false, false) => "↓",
            (false, true, false, true) => "↘",
            (false, true, true, false) => "↙",
            _ => "·",
        };

        // Face buttons
        let face = format!(
            "[{}][{}][{}][{}]",
            if self.square { "□" } else { "·" },
            if self.cross { "✕" } else { "·" },
            if self.circle { "○" } else { "·" },
            if self.triangle { "△" } else { "·" }
        );

        // Shoulders
        let shoulders = format!(
            "L1:{} R1:{} | L2:{:3} R2:{:3}",
            if self.l1 { "█" } else { "·" },
            if self.r1 { "█" } else { "·" },
            self.l2,
            self.r2
        );

        // Other buttons
        let other = format!(
            "L3:{} R3:{} | Share:{} Options:{} PS:{}",
            if self.l3 { "█" } else { "·" },
            if self.r3 { "█" } else { "·" },
            if self.share { "█" } else { "·" },
            if self.options { "█" } else { "·" },
            if self.ps { "█" } else { "·" }
        );

        s.push_str(&format!("D-Pad: {} | {}", dpad, face));
        s.push_str("\r");
        s.push_str(&format!("{}", shoulders));
        s.push_str("\r");
        s.push_str(&format!("{}", other));

        s
    }

    fn has_input(&self) -> bool {
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
            || self.l2 > 10
            || self.r2 > 10
            || self.l3
            || self.r3
            || self.options
            || self.share
            || self.ps
    }
}

fn find_ps4_controller(api: &HidApi) -> Option<HidDevice> {
    for device in api.device_list() {
        if device.vendor_id() == PS4_VID && device.product_id() == PS4_PID {
            println!("Found PS4 controller: {:04x}:{:04x}", PS4_VID, PS4_PID);
            if let Ok(dev) = device.open_device(api) {
                return Some(dev);
            }
        }
    }
    None
}

fn main() {
    println!(
        "Looking for PS4 controller (VID:{:04x}, PID:{:04x})...",
        PS4_VID, PS4_PID
    );

    match HidApi::new() {
        Ok(api) => {
            println!("\nAll available HID devices:");
            for device in api.device_list() {
                println!(
                    "  {:04x}:{:04x} - {:?}",
                    device.vendor_id(),
                    device.product_id(),
                    device.product_string()
                );
            }
            println!();

            match find_ps4_controller(&api) {
                Some(device) => {
                    println!("Connected to PS4 controller. Polling inputs...\n");

                    let mut buf = [0u8; 64];

                    loop {
                        match device.read_timeout(&mut buf, 100) {
                            Ok(size) if size > 0 => {
                                let left_x = buf[1] as i32;
                                let left_y = buf[2] as i32;
                                let right_x = buf[3] as i32;
                                let right_y = buf[4] as i32;

                                let lx_dev = (left_x - STICK_OFFSET).abs();
                                let ly_dev = (left_y - STICK_OFFSET).abs();
                                let rx_dev = (right_x - STICK_OFFSET).abs();
                                let ry_dev = (right_y - STICK_OFFSET).abs();

                                let sticks_active = lx_dev > STICK_THRESHOLD
                                    || ly_dev > STICK_THRESHOLD
                                    || rx_dev > STICK_THRESHOLD
                                    || ry_dev > STICK_THRESHOLD;

                                let buttons = Ps4Buttons::from_report(&buf);
                                let buttons_active = buttons.has_input();

                                if sticks_active || buttons_active {
                                    // Clear previous lines and print new output
                                    print!("\r\r\r\r\r");

                                    // Sticks
                                    print!(
                                        "L: ({:3}, {:3}) | R: ({:3}, {:3}) | ",
                                        left_x, left_y, right_x, right_y
                                    );

                                    // D-pad
                                    let dpad = match (
                                        buttons.dpad_up,
                                        buttons.dpad_down,
                                        buttons.dpad_left,
                                        buttons.dpad_right,
                                    ) {
                                        (true, false, false, false) => "↑",
                                        (true, false, false, true) => "↗",
                                        (true, false, true, false) => "↖",
                                        (false, false, false, true) => "→",
                                        (false, false, true, false) => "←",
                                        (false, true, false, false) => "↓",
                                        (false, true, false, true) => "↘",
                                        (false, true, true, false) => "↙",
                                        _ => "·",
                                    };

                                    // Face buttons
                                    let face = format!(
                                        "[{}][{}][{}][{}]",
                                        if buttons.square { "□" } else { "·" },
                                        if buttons.cross { "✕" } else { "·" },
                                        if buttons.circle { "○" } else { "·" },
                                        if buttons.triangle { "△" } else { "·" }
                                    );

                                    let shoulders = format!(
                                        "L1:{} R1:{} | L2:{:3} R2:{:3} | L3:{} R3:{}",
                                        if buttons.l1 { "█" } else { "·" },
                                        if buttons.r1 { "█" } else { "·" },
                                        buttons.l2,
                                        buttons.r2,
                                        if buttons.l3 { "█" } else { "·" },
                                        if buttons.r3 { "█" } else { "·" }
                                    );

                                    let sys = format!(
                                        "Share:{} Options:{} PS:{}",
                                        if buttons.share { "█" } else { "·" },
                                        if buttons.options { "█" } else { "·" },
                                        if buttons.ps { "█" } else { "·" }
                                    );

                                    println!("{} {} | {} | {}", dpad, face, shoulders, sys);
                                    print!("\r");
                                    std::io::Write::flush(&mut std::io::stdout()).ok();
                                }
                            }
                            Ok(_) => {}
                            Err(e) => {
                                eprintln!("\r\nRead error: {}", e);
                                break;
                            }
                        }

                        thread::sleep(Duration::from_millis(10));
                    }
                }
                None => {
                    eprintln!("PS4 controller not found.");
                }
            }
        }
        Err(e) => {
            eprintln!("Error initializing HID API: {}", e);
        }
    }
}

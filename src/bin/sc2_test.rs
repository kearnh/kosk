//! Live HID test tool for Steam Controller 2.
//!
//! Quit Steam or disable Steam Input while testing. If lizard mode is not held
//! off, the controller emits keyboard/mouse reports and can steal desktop focus.

use std::fmt::Write as _;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use hidapi::HidApi;
use kosk::controller::pad_origin::PadOriginMapper;
use kosk::controller::sc2;
use kosk::controller::ControllerInput;

const VALVE_VID: u16 = 0x28de;

#[derive(Parser, Debug)]
#[command(
    name = "sc2_test",
    about = "Enumerate and monitor Steam Controller 2 HID input"
)]
struct Args {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// List HID nodes (Valve VID by default).
    Enumerate {
        /// Include every HID device, not just Valve.
        #[arg(long)]
        all: bool,
    },
    /// Stream SC2 state (default).
    Monitor {
        /// Hex-dump accepted state reports.
        #[arg(long)]
        raw: bool,
        /// Load kosk config so warped stick values use stick_warp.
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();
    match args.cmd.unwrap_or(Cmd::Monitor {
        raw: false,
        config: None,
    }) {
        Cmd::Enumerate { all } => enumerate(all),
        Cmd::Monitor { raw, config } => monitor(raw, config),
    }
}

fn enumerate(all: bool) -> Result<()> {
    let hid = HidApi::new().context("HidApi::new")?;
    println!("   vid    pid   if  usage   page  product / path");
    for d in hid.device_list() {
        if !all && d.vendor_id() != VALVE_VID {
            continue;
        }
        let product = d.product_string().unwrap_or("");
        let path = d.path().to_string_lossy();
        println!(
            "{:04x} {:04x} {:>4} {:04x} {:04x}  {}  {}",
            d.vendor_id(),
            d.product_id(),
            d.interface_number(),
            d.usage(),
            d.usage_page(),
            product,
            path
        );
    }
    Ok(())
}

fn monitor(dump_raw: bool, config: Option<PathBuf>) -> Result<()> {
    let with_warp = config.is_some();
    if let Some(path) = config {
        kosk::config::init_from_path(path).context("load config")?;
    }

    let hid = HidApi::new().context("HidApi::new")?;
    let Some(mut device) = sc2::open(&hid) else {
        bail!("no Steam Controller 2 found (close Steam / disable Steam Input and retry)");
    };

    println!("opened SC2; lizard-off sent. Ctrl+C to quit.");
    let mut last = String::new();
    let mut pads = PadOriginMapper::default();
    loop {
        match device.next() {
            Some(Some(input)) => {
                let line = if with_warp {
                    let mapped = pads.map(device.last_state());
                    format_line(&mapped, device.last_state(), true)
                } else {
                    format_line(input.as_ref(), device.last_state(), false)
                };
                if line != last {
                    println!("{line}");
                    last = line;
                }
                if dump_raw {
                    if let Some(report) = device.last_raw_report() {
                        print!("  raw");
                        for b in report.iter().take(report.len().min(54)) {
                            print!(" {b:02x}");
                        }
                        println!();
                    }
                }
            }
            Some(None) => pads.reset(),
            None => break,
        }
    }
    Ok(())
}

fn format_line(input: &dyn ControllerInput, state: &sc2::Sc2State, with_warp: bool) -> String {
    let (lx, ly) = input.left_stick_raw();
    let (rx, ry) = input.right_stick_raw();
    let (slx, sly) = state.physical_left_stick();
    let (srx, sry) = state.physical_right_stick();
    let mut s = String::new();
    let _ = write!(
        s,
        "pad L({lx:+.3},{ly:+.3}) R({rx:+.3},{ry:+.3}) stick L({slx:+.3},{sly:+.3}) R({srx:+.3},{sry:+.3})"
    );
    if with_warp {
        let (lx, ly) = input.left_stick();
        let (rx, ry) = input.right_stick();
        let _ = write!(s, " warp L({lx:+.3},{ly:+.3}) R({rx:+.3},{ry:+.3})");
    }
    let mut btns = Vec::new();
    if input.face_bottom() {
        btns.push("faceBottom");
    }
    if input.face_right() {
        btns.push("faceRight");
    }
    if input.face_left() {
        btns.push("faceLeft");
    }
    if input.face_top() {
        btns.push("faceTop");
    }
    if input.shoulder_left() {
        btns.push("LB");
    }
    if input.shoulder_right() {
        btns.push("RB");
    }
    if input.dpad_up() {
        btns.push("dpadUp");
    }
    if input.dpad_down() {
        btns.push("dpadDown");
    }
    if input.dpad_left() {
        btns.push("dpadLeft");
    }
    if input.dpad_right() {
        btns.push("dpadRight");
    }
    if input.stick_left() {
        btns.push("L3");
    }
    if input.stick_right() {
        btns.push("R3");
    }
    if input.btn_share() {
        btns.push("share");
    }
    if input.btn_options() {
        btns.push("options");
    }
    if input.btn_system() {
        btns.push("system");
    }
    if input.pad_left() {
        btns.push("padLeft");
    }
    if input.pad_right() {
        btns.push("padRight");
    }
    if !btns.is_empty() {
        let _ = write!(s, " [{}]", btns.join(" "));
    }
    if let Some(t) = input.trigger_left() {
        let _ = write!(s, " LT={t}");
    }
    if let Some(t) = input.trigger_right() {
        let _ = write!(s, " RT={t}");
    }
    s
}

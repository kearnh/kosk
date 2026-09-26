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
use kosk::controller::{ControllerButton, ControllerInput};
use strum::IntoEnumIterator;

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
        /// Load kosk config so warped stick values use the stick profile.
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
                    format_line(&mapped, true)
                } else {
                    format_line(input.as_ref(), false)
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

fn format_line(input: &dyn ControllerInput, with_warp: bool) -> String {
    let (lx, ly) = input.left_stick_raw();
    let (rx, ry) = input.right_stick_raw();
    let mut s = String::new();
    let _ = write!(
        s,
        "pad L({}) R({}) stick L({lx:+.3},{ly:+.3}) R({rx:+.3},{ry:+.3})",
        fmt_pad(input.left_pad_raw()),
        fmt_pad(input.right_pad_raw()),
    );
    if with_warp {
        let (lx, ly) = input.left_stick();
        let (rx, ry) = input.right_stick();
        let _ = write!(
            s,
            " warpPad L({}) R({}) warpStick L({lx:+.3},{ly:+.3}) R({rx:+.3},{ry:+.3})",
            fmt_pad(input.left_pad()),
            fmt_pad(input.right_pad()),
        );
    }
    let btns: Vec<String> = ControllerButton::iter()
        .filter(|&b| input.query(b))
        .map(|b| b.to_string())
        .collect();
    if !btns.is_empty() {
        let _ = write!(s, " [{}]", btns.join(" "));
    }
    if let Some(t) = input.trigger_left() {
        let _ = write!(s, " LT={t}");
    }
    if let Some(t) = input.trigger_right() {
        let _ = write!(s, " RT={t}");
    }
    if let Some(b) = input.battery() {
        let _ = write!(
            s,
            " bat={}%{}",
            b.percent,
            if b.charging { "+" } else { "" }
        );
    }
    s
}

fn fmt_pad(p: Option<(f32, f32)>) -> String {
    match p {
        None => "-".to_string(),
        Some((x, y)) => format!("{x:+.3},{y:+.3}"),
    }
}

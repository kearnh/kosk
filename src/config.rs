use clap::Parser;
use std::sync::OnceLock;

#[derive(Parser, Debug)]
#[command(name = "kosk")]
#[command(about = "Keyboard On-Screen for Kontroller", long_about = None)]
pub struct Args {
    /// Path to keyboard layout TOML file
    pub layout: String,

    /// Sensitivity/range multiplier for the horizontal stick axis
    #[arg(long, default_value_t = 3.0)]
    pub stick_x: f32,

    /// Sensitivity/range multiplier for the vertical stick axis
    #[arg(long, default_value_t = 2.5)]
    pub stick_y: f32,

    /// Stick warp factor (0.0 = circle, 1.0 = square)
    #[arg(long, default_value_t = 1.0)]
    pub stick_warp: f32,

    /// Trigger threshold for key press (0-255)
    #[arg(long, default_value_t = 40)]
    pub trigger_threshold: u8,
}

static CONFIG: OnceLock<Args> = OnceLock::new();

pub fn init() {
    let args = Args::parse();
    CONFIG.set(args).expect("Config was already initialized");
}

pub fn get() -> &'static Args {
    CONFIG.get().expect("Config must be initialized before use")
}

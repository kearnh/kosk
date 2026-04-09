use anyhow::{bail, Context, Result};
use clap::Parser;
use serde::Deserialize;
use std::fs;
use std::sync::OnceLock;

#[derive(Parser, Debug)]
#[command(name = "kosk")]
pub struct Args {
    /// Path to the configuration TOML file
    pub config_path: String,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    /// Path to keyboard layout TOML file
    pub layout: String,

    /// Sensitivity/range multiplier for the horizontal stick axis
    #[serde(default = "default_stick_x")]
    pub stick_x: f32,

    /// Sensitivity/range multiplier for the vertical stick axis
    #[serde(default = "default_stick_y")]
    pub stick_y: f32,

    /// Stick warp factor (0.0 = circle, 1.0 = square)
    #[serde(default = "default_stick_warp")]
    pub stick_warp: f32,

    /// Trigger threshold for key press (0-255)
    #[serde(default = "default_trigger_threshold")]
    pub trigger_threshold: u8,
}

fn default_stick_x() -> f32 {
    3.0
}
fn default_stick_y() -> f32 {
    2.5
}
fn default_stick_warp() -> f32 {
    1.0
}
fn default_trigger_threshold() -> u8 {
    40
}

static CONFIG: OnceLock<Config> = OnceLock::new();

pub fn init() -> Result<()> {
    let args = Args::parse();
    let config_content =
        fs::read_to_string(&args.config_path).context("Could not read config file")?;
    let config: Config = toml::from_str(&config_content).context("Could not parse config TOML")?;

    if !fs::exists(&config.layout)? {
        bail!(r#"cannot find layout file "{}""#, &config.layout);
    }

    CONFIG.set(config).expect("Config was already initialized");

    Ok(())
}

pub fn get() -> &'static Config {
    CONFIG.get().expect("Config must be initialized before use")
}

use anyhow::{bail, Context, Result};
use clap::Parser;
use notify::{Event, EventKind, RecommendedWatcher, Watcher};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "kosk")]
pub struct Args {
    /// Path to the configuration TOML file
    pub config_path: String,
}

#[derive(Debug, Deserialize, Clone)]
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

    /// Whether the window should be transparent
    #[serde(default = "default_transparent")]
    pub transparent: bool,
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
fn default_transparent() -> bool {
    true
}

// Static variables for config management
static CONFIG_INSTANCE: std::sync::OnceLock<Arc<Mutex<Config>>> = std::sync::OnceLock::new();
static CONFIG_VERSION: AtomicU32 = AtomicU32::new(0);
static CONFIG_PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

// Reload the config from file
fn reload_config() -> Result<()> {
    let path = CONFIG_PATH.get().ok_or(anyhow::anyhow!("Config path not set"))?;
    
    // Debounce: avoid reloading too frequently
    static LAST_RELOAD: AtomicU32 = AtomicU32::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;
    
    let last = LAST_RELOAD.load(Ordering::Relaxed);
    if now.wrapping_sub(last) < 1 { // 1 second debounce
        return Ok(());
    }
    
    let config_content = fs::read_to_string(path)
        .context("Could not read config file")?;
    let new_config: Config = toml::from_str(&config_content)
        .context("Could not parse config TOML")?;
    
    if !fs::exists(&new_config.layout)? {
        bail!(r#"cannot find layout file "{}""#, &new_config.layout);
    }
    
    // Update the config instance
    if let Some(instance) = CONFIG_INSTANCE.get() {
        let mut config = instance.lock().unwrap();
        *config = new_config;
        CONFIG_VERSION.fetch_add(1, Ordering::SeqCst);
        LAST_RELOAD.store(now, Ordering::Relaxed);
        println!("Config reloaded successfully");
    }
    
    Ok(())
}

pub fn init() -> Result<()> {
    let args = Args::parse();
    let config_path = PathBuf::from(&args.config_path);
    
    // Load initial config
    let config_content = fs::read_to_string(&config_path)
        .context("Could not read config file")?;
    let config: Config = toml::from_str(&config_content)
        .context("Could not parse config TOML")?;
    
    if !fs::exists(&config.layout)? {
        bail!(r#"cannot find layout file "{}""#, &config.layout);
    }
    
    // Store config instance and path
    CONFIG_INSTANCE.set(Arc::new(Mutex::new(config))).expect("Config was already initialized");
    CONFIG_PATH.set(config_path.clone()).expect("Config path was already set");
    
    // Setup file watcher in a separate thread
    std::thread::spawn(move || {
        let mut watcher = match notify::recommended_watcher(|res: Result<Event, notify::Error>| {
            match res {
                Ok(event) => {
                    // Check if it's a modify event for our config file
                    if event.kind == EventKind::Modify(notify::event::ModifyKind::Data(_)) {
                        // Check if the modified file is our config file
                        if event.paths.iter().any(|p| p == &config_path) {
                            if let Err(e) = reload_config() {
                                eprintln!("Failed to reload config: {}", e);
                            }
                        }
                    }
                }
                Err(e) => eprintln!("watch error: {:?}", e),
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("Failed to create file watcher: {}", e);
                return;
            }
        };
        
        if let Err(e) = watcher.watch(&config_path, notify::RecursiveMode::NonRecursive) {
            eprintln!("Failed to watch config file: {}", e);
            return;
        }
        
        // Keep the watcher alive
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    });
    
    Ok(())
}

pub fn get() -> Config {
    let instance = CONFIG_INSTANCE.get().expect("Config must be initialized before use");
    let config = instance.lock().unwrap();
    config.clone()
}

pub fn version() -> u32 {
    CONFIG_VERSION.load(Ordering::SeqCst)
}

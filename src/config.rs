use anyhow::{bail, Context, Result};
use clap::Parser;
use notify::event::ModifyKind;
use notify::{Event, EventKind, Watcher};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};

#[derive(Parser, Debug)]
#[command(name = "kosk")]
pub struct Args {
    /// Path to the configuration TOML file
    pub config_path: String,
}

#[derive(Debug, Default, Deserialize, Clone)]
pub struct Debug {
    #[serde(default)]
    pub show_stick_cursors: bool,

    #[serde(default)]
    pub show_hitboxes: bool,

    #[serde(default)]
    pub show_stick_bounds: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    /// Path to keyboard layout TOML file
    pub layout: String,

    /// Sensitivity/range multiplier for the horizontal stick axis
    #[serde(default = "default_stick_scale_x")]
    pub stick_scale_x: f32,

    /// Sensitivity/range multiplier for the vertical stick axis
    #[serde(default = "default_stick_scale_y")]
    pub stick_scale_y: f32,

    /// Stick warp factor (0.0 = circle, 1.0 = square)
    #[serde(default = "default_stick_warp")]
    pub stick_warp: f32,

    /// Trigger threshold for key press (0-255)
    #[serde(default = "default_trigger_threshold")]
    pub trigger_threshold: u8,

    /// Whether the window should be transparent
    #[serde(default = "default_transparent")]
    pub transparent: bool,

    #[serde(default)]
    pub debug: Option<Debug>,
}

fn default_stick_scale_x() -> f32 {
    3.0
}
fn default_stick_scale_y() -> f32 {
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
static CONFIG_PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Load configuration file and return the resolved layout path
fn load_config() -> Result<PathBuf> {
    let config_path = CONFIG_PATH
        .get()
        .ok_or(anyhow::anyhow!("Config path not set"))?;

    static LAST_LOAD: AtomicU32 = AtomicU32::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;

    let last = LAST_LOAD.load(Ordering::Relaxed);
    if last > 0 && now.wrapping_sub(last) < 1 {
        return Err(anyhow::anyhow!("Reload debounced"));
    }

    let config_content = fs::read_to_string(config_path).context("Could not read config file")?;
    let new_config: Config =
        toml::from_str(&config_content).context("Could not parse config TOML")?;

    let layout_path = if PathBuf::from(&new_config.layout).is_absolute() {
        PathBuf::from(&new_config.layout)
    } else {
        config_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Config file has no parent directory"))?
            .join(&new_config.layout)
    };

    if !fs::exists(&layout_path)? {
        bail!(r#"cannot find layout file "{}""#, layout_path.display());
    }

    if let Some(instance) = CONFIG_INSTANCE.get() {
        let mut config = instance.lock().unwrap();
        *config = new_config;
    } else {
        CONFIG_INSTANCE
            .set(Arc::new(Mutex::new(new_config)))
            .expect("Config was already initialized");
    }

    LAST_LOAD.store(now, Ordering::Relaxed);

    Ok(layout_path)
}

/// Start a watcher thread that monitors both config and layout files
fn start_watcher_thread(config_path: PathBuf, layout_path: PathBuf) -> Result<()> {
    let config_path = std::fs::canonicalize(config_path)?;
    let layout_path = std::fs::canonicalize(layout_path)?;

    std::thread::spawn(move || -> Result<()> {
        let (tx, rx) = mpsc::channel::<notify::Result<Event>>();

        let mut watcher = notify::recommended_watcher(tx)?;

        if let Err(e) = watcher.watch(&config_path, notify::RecursiveMode::NonRecursive) {
            eprintln!("Failed to watch config file: {}", e);
        }

        if let Err(e) = watcher.watch(&layout_path, notify::RecursiveMode::NonRecursive) {
            eprintln!("Failed to watch layout file: {}", e);
        }

        for res in rx {
            match res {
                Ok(event)
                    if matches!(
                        event.kind,
                        EventKind::Modify(ModifyKind::Any | ModifyKind::Data(_))
                    ) =>
                {
                    let should_reload = event
                        .paths
                        .iter()
                        .map(std::fs::canonicalize)
                        .flatten()
                        .any(|p| p == config_path || p == layout_path);

                    if should_reload {
                        match load_config() {
                            Ok(new_layout_path) => {
                                if new_layout_path != layout_path {
                                    start_watcher_thread(config_path.clone(), new_layout_path)?;

                                    ON_CHANGE_CALLBACK.get().map(|f| f());

                                    break;
                                }
                            }
                            Err(e) => {
                                if e.to_string() != "Reload debounced" {
                                    eprintln!("Failed to reload config: {}", e);
                                }
                            }
                        }
                    }
                }
                Err(e) => eprintln!("watch error: {:?}", e),
                _ => {}
            }
        }

        Ok(())
    });

    Ok(())
}

pub fn init() -> Result<()> {
    let args = Args::parse();
    let config_path = PathBuf::from(&args.config_path);

    CONFIG_PATH
        .set(config_path.clone())
        .expect("Config path was already set");

    let initial_layout_path = load_config()?;

    start_watcher_thread(config_path, initial_layout_path)?;

    Ok(())
}

pub fn get() -> Config {
    let instance = CONFIG_INSTANCE
        .get()
        .expect("Config must be initialized before use");
    let config = instance.lock().unwrap();
    config.clone()
}

type ConfigChangeCallback = Box<dyn Fn() + Send + Sync + 'static>;
static ON_CHANGE_CALLBACK: OnceLock<ConfigChangeCallback> = OnceLock::new();

// Add a function to set the callback
pub fn on_changed<F>(callback: F) -> Result<()>
where
    F: Fn() + Send + Sync + 'static,
{
    ON_CHANGE_CALLBACK
        .set(Box::new(callback))
        .map_err(|_| anyhow::anyhow!("could not set config on_change callback"))?;
    Ok(())
}

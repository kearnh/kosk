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
    /// Named layouts: map from layout name to file path
    /// Must contain at least "main" layout
    pub layouts: std::collections::HashMap<String, String>,

    /// Which layout to start with (must exist in layouts)
    #[serde(default = "default_start_layout")]
    pub start_layout: String,

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

    #[serde(default = "default_scale_x")]
    pub scale_x: f32,
    #[serde(default = "default_scale_y")]
    pub scale_y: f32,
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

fn default_scale_x() -> f32 {
    40.0
}

fn default_scale_y() -> f32 {
    40.0
}

fn default_start_layout() -> String {
    "main".to_string()
}

// Static variables for config management
static CONFIG_INSTANCE: std::sync::OnceLock<Arc<Mutex<Config>>> = std::sync::OnceLock::new();
static CONFIG_PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Load configuration file and return all resolved layout paths
fn load_config() -> Result<Vec<PathBuf>> {
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

    // Validate that layouts contains "main"
    if !new_config.layouts.contains_key("main") {
        bail!("Layouts must contain at least a 'main' layout");
    }

    // Validate that start_layout exists in layouts
    if !new_config.layouts.contains_key(&new_config.start_layout) {
        bail!(
            "Start layout '{}' not found in layouts",
            new_config.start_layout
        );
    }

    let mut layout_paths = Vec::new();
    for (name, path) in &new_config.layouts {
        let layout_path = if PathBuf::from(path).is_absolute() {
            PathBuf::from(path)
        } else {
            config_path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Config file has no parent directory"))?
                .join(path)
        };

        if !fs::exists(&layout_path)? {
            bail!(
                r#"cannot find layout file "{}" for layout "{}""#,
                layout_path.display(),
                name
            );
        }
        layout_paths.push(layout_path);
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

    Ok(layout_paths)
}

/// Start a watcher thread that monitors config and all layout files
fn start_watcher_thread(config_path: PathBuf, layout_paths: Vec<PathBuf>) -> Result<()> {
    let config_path = std::fs::canonicalize(config_path)?;
    let layout_paths: Vec<PathBuf> = layout_paths
        .into_iter()
        .map(std::fs::canonicalize)
        .collect::<Result<_, _>>()?;

    std::thread::spawn(move || -> Result<()> {
        let (tx, rx) = mpsc::channel::<notify::Result<Event>>();

        let mut watcher = notify::recommended_watcher(tx)?;

        if let Err(e) = watcher.watch(&config_path, notify::RecursiveMode::NonRecursive) {
            eprintln!("Failed to watch config file: {}", e);
        }

        for path in &layout_paths {
            if let Err(e) = watcher.watch(path, notify::RecursiveMode::NonRecursive) {
                eprintln!("Failed to watch layout file {}: {}", path.display(), e);
            }
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
                        .flat_map(std::fs::canonicalize)
                        .any(|p| p == config_path || layout_paths.contains(&p));

                    if should_reload {
                        match load_config() {
                            Ok(new_layout_paths) => {
                                // Check if layout paths changed
                                let new_set: std::collections::HashSet<_> =
                                    new_layout_paths.iter().collect();
                                let old_set: std::collections::HashSet<_> =
                                    layout_paths.iter().collect();

                                if new_set != old_set {
                                    start_watcher_thread(config_path.clone(), new_layout_paths)?;

                                    if let Some(f) = ON_CHANGE_CALLBACK.get() {
                                        f()
                                    }

                                    break;
                                } else {
                                    // Same layout files, just trigger reload
                                    if let Some(f) = ON_CHANGE_CALLBACK.get() {
                                        f()
                                    }
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

    let initial_layout_paths = load_config()?;

    start_watcher_thread(config_path, initial_layout_paths)?;

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

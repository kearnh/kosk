use crate::controller::ControllerBinding;
use crate::state::window_pos::WindowPos;
use crate::state::StateId;
use anyhow::{bail, Context, Result};
use clap::Parser;
use notify::event::ModifyKind;
use notify::{Event, EventKind, Watcher};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};

#[derive(Parser, Debug)]
#[command(name = "kosk")]
pub struct Args {
    /// Path to the configuration TOML file
    pub config_path: String,

    /// Play this recording instead of the configured controller (`[replay]` / preferred_controller).
    #[arg(long, value_name = "FILE")]
    pub replay: Option<PathBuf>,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct Debug {
    #[serde(default)]
    pub show_stick_cursors: bool,

    #[serde(default)]
    pub show_hitboxes: bool,

    #[serde(default)]
    pub show_stick_bounds: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TextInputStyle {
    /// RGBA background color for the text field as `[r, g, b, a]`.
    #[serde(default = "default_text_input_background_color")]
    pub background_color: [u8; 4],

    /// RGBA text color for the text field as `[r, g, b, a]`.
    #[serde(default = "default_text_input_text_color")]
    pub text_color: [u8; 4],

    /// RGBA color for the synthetic caret when the field is not focused.
    #[serde(default = "default_text_input_cursor_color")]
    pub cursor_color: [u8; 4],

    #[serde(default = "default_text_input_font_size")]
    pub font_size: f32,
}

fn default_text_input_background_color() -> [u8; 4] {
    [255, 255, 255, 255]
}

fn default_text_input_text_color() -> [u8; 4] {
    [0, 0, 0, 255]
}

fn default_text_input_cursor_color() -> [u8; 4] {
    [0, 0, 0, 255]
}

fn default_text_input_font_size() -> f32 {
    22.0
}

impl Default for TextInputStyle {
    fn default() -> Self {
        Self {
            background_color: default_text_input_background_color(),
            text_color: default_text_input_text_color(),
            cursor_color: default_text_input_cursor_color(),
            font_size: default_text_input_font_size(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
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

    /// Try these controller families first. Omitted families are appended in
    /// built-in default order (`sc2`, then `ps4`). Empty / omitted → that default.
    #[serde(default)]
    pub preferred_controller: Vec<crate::controller::ControllerKind>,

    /// Whether the window should be transparent
    #[serde(default = "default_transparent")]
    pub transparent: bool,

    #[serde(default)]
    pub debug: Option<Debug>,

    #[serde(default = "default_window_pos")]
    pub window_pos: WindowPos,

    #[serde(default = "default_scale_x")]
    pub scale_x: f32,
    #[serde(default = "default_scale_y")]
    pub scale_y: f32,

    /// Milliseconds before the first repeat of the same outgoing event unit (a
    /// single [`Event`](crate::state::event::Event) or a completed batch). Use
    /// `0` to disable debouncing entirely.
    #[serde(default = "default_event_debounce_ms")]
    pub event_debounce_ms: u64,

    /// Milliseconds between further repeats of the same unit after the first
    /// repeat has fired (key-repeat style). Ignored when `event_debounce_ms` is
    /// `0`. Use `0` here to use `event_debounce_ms` for every repeat step.
    #[serde(default = "default_event_debounce_repeat_ms")]
    pub event_debounce_repeat_ms: u64,

    #[serde(default)]
    pub text_input: TextInputStyle,

    /// Per-app-state mapping from controller buttons to action names (interpreted by each state).
    ///
    /// Either an inline table, or a string path to a TOML file whose root is the same map shape
    /// (relative paths are resolved against the main config file's directory).
    #[serde(default, deserialize_with = "deserialize_controller_map")]
    pub controller_map: HashMap<StateId, HashMap<ControllerBinding, String>>,

    /// Milliseconds for the stick selection to be locked after key under stick is pressed.
    #[serde(default = "default_stick_select_lock_ms")]
    pub stick_select_lock_ms: u64,

    /// Steam Controller 2 pad mapping and feel. Omitted → defaults.
    #[serde(default)]
    pub sc2: Sc2Config,

    /// DualShock 4 feel. Omitted → defaults.
    #[serde(default)]
    pub ps4: Ps4Config,

    /// Template for `toggleRecord` captures. Must contain exactly one `%` (3-digit index).
    #[serde(default)]
    pub record_file: Option<String>,

    /// Replay device. `[replay].file` is required when `preferred_controller` starts with `replay`.
    #[serde(default)]
    pub replay: ReplayConfig,
}

/// SC2-only pad mapping and feel. Does not affect DualShock 4.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Sc2Config {
    /// Blend toward remaining-range stretch from first-touch origin.
    /// `0` = absolute pad (current behavior), `1` = full short-edge stretch.
    #[serde(default = "default_pad_origin_stretch")]
    pub pad_origin_stretch: f32,

    /// Cap on per-axis short-edge gain (`1` = no extra gain).
    #[serde(default = "default_pad_origin_stretch_max_gain")]
    pub pad_origin_stretch_max_gain: f32,

    /// Wait this long after touch-down before capturing origin (skip contact spike).
    #[serde(default = "default_pad_origin_settle_ms")]
    pub pad_origin_settle_ms: u64,

    #[serde(default = "default_trigger_threshold")]
    pub trigger_left_threshold: u8,

    #[serde(default = "default_trigger_threshold")]
    pub trigger_right_threshold: u8,

    #[serde(default)]
    pub touchpad_left_haptic: HapticIntensity,

    #[serde(default)]
    pub touchpad_right_haptic: HapticIntensity,
}

impl Default for Sc2Config {
    fn default() -> Self {
        Self {
            pad_origin_stretch: default_pad_origin_stretch(),
            pad_origin_stretch_max_gain: default_pad_origin_stretch_max_gain(),
            pad_origin_settle_ms: default_pad_origin_settle_ms(),
            trigger_left_threshold: default_trigger_threshold(),
            trigger_right_threshold: default_trigger_threshold(),
            touchpad_left_haptic: HapticIntensity::default(),
            touchpad_right_haptic: HapticIntensity::default(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Ps4Config {
    #[serde(default = "default_trigger_threshold")]
    pub trigger_left_threshold: u8,

    #[serde(default = "default_trigger_threshold")]
    pub trigger_right_threshold: u8,
}

impl Default for Ps4Config {
    fn default() -> Self {
        Self {
            trigger_left_threshold: default_trigger_threshold(),
            trigger_right_threshold: default_trigger_threshold(),
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct ReplayConfig {
    /// Path to a `.krec` tape. Relative paths are against the config file directory.
    #[serde(default)]
    pub file: Option<String>,
}

/// Pad haptic tick strength. `none` skips the HID pulse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HapticIntensity {
    #[default]
    None,
    Low,
    Medium,
    High,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ControllerMapSource {
    /// `controller_map = "mappings.toml"`
    File(String),
    /// `[controller_map]` / nested tables
    Inline(HashMap<StateId, HashMap<ControllerBinding, String>>),
}

fn deserialize_controller_map<'de, D>(
    deserializer: D,
) -> Result<HashMap<StateId, HashMap<ControllerBinding, String>>, D::Error>
where
    D: Deserializer<'de>,
{
    match ControllerMapSource::deserialize(deserializer)? {
        ControllerMapSource::Inline(m) => Ok(m),
        ControllerMapSource::File(rel) => {
            let config_path = CONFIG_PATH.get().ok_or_else(|| {
                D::Error::custom(
                    "controller_map: file reference is not supported in this context (config path unset)",
                )
            })?;
            let path = if PathBuf::from(&rel).is_absolute() {
                PathBuf::from(rel)
            } else {
                config_path
                    .parent()
                    .ok_or_else(|| D::Error::custom("config file has no parent directory"))?
                    .join(&rel)
            };
            let content = fs::read_to_string(&path).map_err(|e| {
                D::Error::custom(format!(
                    "controller_map: could not read {}: {e}",
                    path.display()
                ))
            })?;
            toml::from_str(&content).map_err(|e| {
                D::Error::custom(format!(
                    "controller_map: could not parse {}: {e}",
                    path.display()
                ))
            })
        }
    }
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
fn default_transparent() -> bool {
    true
}

fn default_window_pos() -> WindowPos {
    WindowPos::BottomRight
}

fn default_scale_x() -> f32 {
    40.0
}

fn default_scale_y() -> f32 {
    40.0
}

fn default_event_debounce_ms() -> u64 {
    400
}

fn default_event_debounce_repeat_ms() -> u64 {
    55
}

fn default_start_layout() -> String {
    "main".to_string()
}

fn default_stick_select_lock_ms() -> u64 {
    100
}

fn default_pad_origin_stretch() -> f32 {
    0.0
}

fn default_pad_origin_stretch_max_gain() -> f32 {
    1.5
}

fn default_pad_origin_settle_ms() -> u64 {
    20
}

fn default_trigger_threshold() -> u8 {
    40
}

/// SC2 settings, or defaults when config is not initialized (`sc2_test` without `--config`).
pub fn sc2() -> Sc2Config {
    CONFIG_INSTANCE
        .get()
        .map(|instance| instance.lock().unwrap().sc2.clone())
        .unwrap_or_default()
}

/// DualShock 4 settings, or defaults when config is not initialized.
pub fn ps4() -> Ps4Config {
    CONFIG_INSTANCE
        .get()
        .map(|instance| instance.lock().unwrap().ps4.clone())
        .unwrap_or_default()
}

// Static variables for config management
static CONFIG_INSTANCE: std::sync::OnceLock<Arc<Mutex<Config>>> = std::sync::OnceLock::new();
static CONFIG_PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
static CLI_REPLAY: OnceLock<Option<PathBuf>> = OnceLock::new();

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

    if let Some(template) = &new_config.record_file {
        crate::controller::record::validate_record_template(template).context("record_file")?;
    }

    if new_config.preferred_controller.first() == Some(&crate::controller::ControllerKind::Replay)
        && cli_replay_file().is_none()
    {
        let rel = new_config.replay.file.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "[replay].file is required when preferred_controller starts with replay"
            )
        })?;
        let replay_path = if PathBuf::from(rel).is_absolute() {
            PathBuf::from(rel)
        } else {
            config_path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Config file has no parent directory"))?
                .join(rel)
        };
        if !replay_path.exists() {
            bail!("replay file not found: {}", replay_path.display());
        }
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

                                    notify_config_changed();

                                    break;
                                } else {
                                    notify_config_changed();
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
    if let Some(ref path) = args.replay {
        if !path.exists() {
            bail!("replay file not found: {}", path.display());
        }
    }
    CLI_REPLAY
        .set(args.replay.clone())
        .expect("CLI replay was already set");
    init_from_path(PathBuf::from(&args.config_path))
}

/// Load config from `config_path` without parsing process args (for auxiliary binaries).
pub fn init_from_path(config_path: PathBuf) -> Result<()> {
    CONFIG_PATH
        .set(config_path.clone())
        .expect("Config path was already set");

    let initial_layout_paths = load_config()?;

    let skip_watcher = preferred_is_replay();
    if !skip_watcher {
        start_watcher_thread(config_path, initial_layout_paths)?;
    }

    Ok(())
}

pub fn config_dir() -> Option<PathBuf> {
    CONFIG_PATH
        .get()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

pub fn cli_replay_file() -> Option<PathBuf> {
    CLI_REPLAY.get().and_then(|p| p.clone())
}

pub fn preferred_is_replay() -> bool {
    if cli_replay_file().is_some() {
        return true;
    }
    get().preferred_controller.first() == Some(&crate::controller::ControllerKind::Replay)
}

/// Path of the tape to play: `--replay` as given, otherwise `[replay].file` relative to the config dir.
pub fn replay_tape_path() -> Result<PathBuf> {
    if let Some(path) = cli_replay_file() {
        return Ok(path);
    }
    let rel = get()
        .replay
        .file
        .ok_or_else(|| anyhow::anyhow!("[replay].file is not set"))?;
    crate::controller::record::resolve_against_config_dir(&rel)
}

pub fn get() -> Config {
    let instance = CONFIG_INSTANCE
        .get()
        .expect("Config must be initialized before use");
    let config = instance.lock().unwrap();
    config.clone()
}

pub fn save(new_config: Config) -> Result<()> {
    let path = CONFIG_PATH
        .get()
        .ok_or(anyhow::anyhow!("Config path not set"))?;

    let toml_string = toml::to_string_pretty(&new_config)?;
    fs::write(path, toml_string)?;

    if let Some(instance) = CONFIG_INSTANCE.get() {
        let mut config = instance.lock().unwrap();
        *config = new_config;
    }
    Ok(())
}

type ConfigChangeCallback = Arc<dyn Fn() + Send + Sync + 'static>;
static ON_CHANGE_CALLBACKS: OnceLock<Mutex<Vec<ConfigChangeCallback>>> = OnceLock::new();

fn on_change_callbacks() -> &'static Mutex<Vec<ConfigChangeCallback>> {
    ON_CHANGE_CALLBACKS.get_or_init(|| Mutex::new(Vec::new()))
}

fn notify_config_changed() {
    let callbacks: Vec<ConfigChangeCallback> = {
        let guard = on_change_callbacks().lock().unwrap();
        guard.clone()
    };
    for f in callbacks {
        f();
    }
}

pub fn on_changed<F>(callback: F) -> Result<()>
where
    F: Fn() + Send + Sync + 'static,
{
    on_change_callbacks()
        .lock()
        .unwrap()
        .push(Arc::new(callback));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn parse_replay_flag() {
        let args =
            Args::try_parse_from(["kosk", "config.toml", "--replay", "captures/kosk-000.krec"])
                .unwrap();
        assert_eq!(args.config_path, "config.toml");
        assert_eq!(
            args.replay.as_deref(),
            Some(Path::new("captures/kosk-000.krec"))
        );
    }

    #[test]
    fn parse_without_replay_flag() {
        let args = Args::try_parse_from(["kosk", "config.toml"]).unwrap();
        assert!(args.replay.is_none());
    }
}

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
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};

#[derive(Parser, Debug)]
#[command(name = "kosk")]
pub struct Args {
    /// Path to the configuration TOML file
    pub config_path: String,

    /// Play this recording instead of the configured controller (`[replay]` / preferred_controller).
    #[arg(long, value_name = "FILE")]
    pub replay: Option<PathBuf>,

    /// Log outgoing keys/text to FILE instead of injecting them (`-` = stdout).
    #[arg(long, value_name = "FILE")]
    pub keys_log: Option<PathBuf>,

    /// Use the config file instead of config embedded in a recording.
    #[arg(long)]
    pub ignore_recorded_config: bool,
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

    /// Overlay clear/panel alpha while `transparent` is true, on Keyboard / TextInput.
    #[serde(default = "default_keyboard_opacity")]
    pub keyboard_opacity: f32,

    /// Overlay clear/panel alpha while `transparent` is true, on Menu / Mappings / MoveWindow / SelectKey.
    #[serde(default = "default_ui_opacity")]
    pub ui_opacity: f32,

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

    /// Where to send outgoing keys/text. Omitted → Enigo injection.
    #[serde(default)]
    pub key_sink: KeySinkConfig,
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

/// Destination for `SendKey` / `SendText`. Internally tagged on `type`.
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum KeySinkConfig {
    #[default]
    Enigo,
    Log {
        file: String,
    },
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

fn default_keyboard_opacity() -> f32 {
    0.3
}

fn default_ui_opacity() -> f32 {
    0.92
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
static DISK_CONFIG: OnceLock<Mutex<Config>> = OnceLock::new();
/// Relative path string from `controller_map = "…"` on the last successful load, or `None` if inline/absent.
static CONTROLLER_MAP_FILE: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static TAPE_OVERLAY_ACTIVE: AtomicBool = AtomicBool::new(false);
static CLI_REPLAY: OnceLock<Option<PathBuf>> = OnceLock::new();
static CLI_KEYS_LOG: OnceLock<Option<PathBuf>> = OnceLock::new();
static CLI_IGNORE_RECORDED_CONFIG: OnceLock<bool> = OnceLock::new();

const TAPE_CONFIG_SKIP: &[&str] = &[
    "layouts",
    "record_file",
    "replay",
    "preferred_controller",
    "key_sink",
    "debug",
    "transparent",
    "keyboard_opacity",
    "ui_opacity",
    "window_pos",
    "text_input",
];

/// Config TOML stored in a recording: live config minus [`TAPE_CONFIG_SKIP`].
pub fn tape_config_toml(cfg: &Config) -> Result<String> {
    let serialized = toml::to_string(cfg).context("serialize config for tape")?;
    let mut val: toml::Value = serialized.parse().context("reparse config toml")?;
    let table = val
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("config did not serialize as a table"))?;
    for k in TAPE_CONFIG_SKIP {
        table.remove(*k);
    }
    Ok(toml::to_string(&val)?)
}

/// Merge recorded config onto `live`. Blacklisted keys in the blob are ignored.
pub fn overlay_tape_config(live: &Config, recorded: &str) -> Result<Config> {
    let mut overlay: toml::Value =
        toml::from_str(recorded).context("parse recorded config TOML")?;
    if let Some(table) = overlay.as_table_mut() {
        for k in TAPE_CONFIG_SKIP {
            table.remove(*k);
        }
    } else {
        bail!("recorded config must be a TOML table");
    }
    let live_serialized = toml::to_string(live).context("serialize live config")?;
    let mut live_val: toml::Value = live_serialized.parse().context("reparse live config")?;
    let live_table = live_val
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("live config did not serialize as a table"))?;
    let overlay_table = overlay
        .as_table()
        .ok_or_else(|| anyhow::anyhow!("recorded config must be a TOML table"))?;
    for (k, v) in overlay_table {
        live_table.insert(k.clone(), v.clone());
    }
    live_val
        .try_into()
        .map_err(|e| anyhow::anyhow!("apply recorded config: {e}"))
}

/// Install recorded config into the process (no-op for v0 / `--ignore-recorded-config`).
pub fn apply_recorded_tape_config(header: &crate::controller::record::TapeHeader) -> Result<()> {
    if header.version == 0 {
        let path = CONFIG_PATH
            .get()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "config.toml".into());
        eprintln!(
            "warn: recording has no version (treated as 0); using config from {path} (tape has no recorded config)"
        );
        return Ok(());
    }
    if ignore_recorded_config() {
        return Ok(());
    }
    let toml = header
        .config_toml
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("version {} recording is missing config", header.version))?;
    let disk = DISK_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("config is not initialized"))?
        .lock()
        .unwrap()
        .clone();
    let merged = overlay_tape_config(&disk, toml)?;
    let instance = CONFIG_INSTANCE
        .get()
        .ok_or_else(|| anyhow::anyhow!("config is not initialized"))?;
    *instance.lock().unwrap() = merged;
    TAPE_OVERLAY_ACTIVE.store(true, Ordering::Relaxed);
    Ok(())
}

fn store_disk_config(cfg: Config) {
    TAPE_OVERLAY_ACTIVE.store(false, Ordering::Relaxed);
    match DISK_CONFIG.get() {
        Some(m) => *m.lock().unwrap() = cfg,
        None => {
            DISK_CONFIG
                .set(Mutex::new(cfg))
                .expect("disk config was already set");
        }
    }
}

fn store_controller_map_file(rel: Option<String>) {
    match CONTROLLER_MAP_FILE.get() {
        Some(m) => *m.lock().unwrap() = rel,
        None => {
            CONTROLLER_MAP_FILE
                .set(Mutex::new(rel))
                .expect("controller map file path was already set");
        }
    }
}

fn controller_map_file() -> Option<String> {
    CONTROLLER_MAP_FILE
        .get()
        .and_then(|m| m.lock().unwrap().clone())
}

/// Convert a `toml::Value` into a `toml_edit::Item` (toml_edit 0.22 has no `ser::to_item`).
fn toml_to_item(value: &toml::Value) -> Result<toml_edit::Item> {
    use serde::Serialize;
    let edit_value = value
        .serialize(toml_edit::ser::ValueSerializer::new())
        .map_err(|e| anyhow::anyhow!("serialize TOML value: {e}"))?;
    let item = toml_edit::Item::Value(edit_value);
    match item.into_table() {
        Ok(table) => Ok(toml_edit::Item::Table(table)),
        Err(item) => Ok(item),
    }
}

/// Diff `old` → `new` into an existing `toml_edit` table, preserving unrelated formatting.
fn apply_toml_diff(
    table: &mut toml_edit::Table,
    old: &toml::Value,
    new: &toml::Value,
) -> Result<()> {
    let old_table = old
        .as_table()
        .ok_or_else(|| anyhow::anyhow!("apply_toml_diff: old value must be a table"))?;
    let new_table = new
        .as_table()
        .ok_or_else(|| anyhow::anyhow!("apply_toml_diff: new value must be a table"))?;

    for (key, new_child) in new_table {
        match old_table.get(key) {
            Some(old_child) if old_child == new_child => {}
            Some(old_child) if old_child.is_table() && new_child.is_table() => {
                let can_recurse = table.get(key).is_some_and(|item| item.is_table());
                if can_recurse {
                    let child_table = table
                        .get_mut(key)
                        .and_then(|item| item.as_table_mut())
                        .expect("checked is_table");
                    apply_toml_diff(child_table, old_child, new_child)?;
                } else {
                    table[key] = toml_to_item(new_child)?;
                }
            }
            _ => {
                table[key] = toml_to_item(new_child)?;
            }
        }
    }

    for key in old_table.keys() {
        if !new_table.contains_key(key) {
            table.remove(key);
        }
    }

    Ok(())
}

fn write_toml_diff_preserving(path: &Path, old: &toml::Value, new: &toml::Value) -> Result<()> {
    if !old.is_table() || !new.is_table() {
        bail!("write_toml_diff_preserving: old and new must be tables");
    }
    let content =
        fs::read_to_string(path).with_context(|| format!("Could not read {}", path.display()))?;
    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("Could not parse {}", path.display()))?;
    apply_toml_diff(doc.as_table_mut(), old, new)?;
    fs::write(path, doc.to_string())
        .with_context(|| format!("Could not write {}", path.display()))?;
    Ok(())
}

fn write_config_preserving(
    path: &Path,
    old: &Config,
    new: &Config,
    controller_map_file: Option<&str>,
) -> Result<()> {
    let mut old_val =
        toml::Value::try_from(old).context("serialize old config for format-preserving save")?;
    let mut new_val =
        toml::Value::try_from(new).context("serialize new config for format-preserving save")?;

    if let Some(rel) = controller_map_file {
        if let Some(table) = old_val.as_table_mut() {
            table.remove("controller_map");
        }
        if let Some(table) = new_val.as_table_mut() {
            table.remove("controller_map");
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("Could not read {}", path.display()))?;
        let mut doc = content
            .parse::<toml_edit::DocumentMut>()
            .with_context(|| format!("Could not parse {}", path.display()))?;
        apply_toml_diff(doc.as_table_mut(), &old_val, &new_val)?;

        let needs_path = match doc.get("controller_map") {
            Some(item) => item.as_str() != Some(rel),
            None => true,
        };
        if needs_path {
            doc["controller_map"] = toml_edit::value(rel);
        }

        fs::write(path, doc.to_string())
            .with_context(|| format!("Could not write {}", path.display()))?;

        if old.controller_map != new.controller_map {
            let mappings_path = path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("config file has no parent directory"))?
                .join(rel);
            let old_map = toml::Value::try_from(&old.controller_map)
                .context("serialize old controller_map")?;
            let new_map = toml::Value::try_from(&new.controller_map)
                .context("serialize new controller_map")?;
            write_toml_diff_preserving(&mappings_path, &old_map, &new_map).with_context(|| {
                format!(
                    "failed to update controller_map file {}",
                    mappings_path.display()
                )
            })?;
        }
    } else {
        write_toml_diff_preserving(path, &old_val, &new_val)?;
    }

    Ok(())
}

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
    let raw_root: toml::Value = config_content
        .parse()
        .context("Could not parse config TOML")?;
    let map_file = match raw_root.get("controller_map") {
        Some(toml::Value::String(s)) => Some(s.clone()),
        _ => None,
    };
    store_controller_map_file(map_file);
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
        *config = new_config.clone();
    } else {
        CONFIG_INSTANCE
            .set(Arc::new(Mutex::new(new_config.clone())))
            .expect("Config was already initialized");
    }
    store_disk_config(new_config);

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
    CLI_KEYS_LOG
        .set(args.keys_log.clone())
        .expect("CLI keys-log was already set");
    CLI_IGNORE_RECORDED_CONFIG
        .set(args.ignore_recorded_config)
        .expect("CLI ignore-recorded-config was already set");
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

pub fn cli_keys_log() -> Option<PathBuf> {
    CLI_KEYS_LOG.get().and_then(|p| p.clone())
}

pub fn ignore_recorded_config() -> bool {
    CLI_IGNORE_RECORDED_CONFIG.get().copied().unwrap_or(false)
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
    let map_file = controller_map_file();
    let map_file_ref = map_file.as_deref();

    if TAPE_OVERLAY_ACTIVE.load(Ordering::Relaxed) {
        let disk = DISK_CONFIG
            .get()
            .ok_or_else(|| anyhow::anyhow!("disk config is not initialized"))?;
        let disk_before = disk.lock().unwrap().clone();
        let mut persist = disk_before.clone();
        persist.window_pos = new_config.window_pos;
        write_config_preserving(path, &disk_before, &persist, map_file_ref)?;
        *disk.lock().unwrap() = persist;
        if let Some(instance) = CONFIG_INSTANCE.get() {
            instance.lock().unwrap().window_pos = new_config.window_pos;
        }
        return Ok(());
    }

    let disk_snapshot = DISK_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("disk config is not initialized"))?
        .lock()
        .unwrap()
        .clone();
    write_config_preserving(path, &disk_snapshot, &new_config, map_file_ref)?;

    store_disk_config(new_config.clone());
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

/// Invoke `on_changed` callbacks (e.g. after an in-app mappings write the watcher misses).
pub fn notify_changed() {
    notify_config_changed();
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
        assert!(args.keys_log.is_none());
        assert!(!args.ignore_recorded_config);
    }

    #[test]
    fn parse_keys_log_flag() {
        let args = Args::try_parse_from(["kosk", "config.toml", "--keys-log", "captures/keys.log"])
            .unwrap();
        assert_eq!(
            args.keys_log.as_deref(),
            Some(Path::new("captures/keys.log"))
        );
    }

    #[test]
    fn parse_keys_log_stdout() {
        let args = Args::try_parse_from(["kosk", "config.toml", "--keys-log", "-"]).unwrap();
        assert_eq!(args.keys_log.as_deref(), Some(Path::new("-")));
    }

    #[test]
    fn parse_key_sink_enigo() {
        let sink: KeySinkConfig = toml::from_str("type = \"enigo\"\n").unwrap();
        assert_eq!(sink, KeySinkConfig::Enigo);
    }

    #[test]
    fn parse_key_sink_log() {
        let sink: KeySinkConfig = toml::from_str(
            r#"
            type = "log"
            file = "captures/keys.log"
            "#,
        )
        .unwrap();
        assert_eq!(
            sink,
            KeySinkConfig::Log {
                file: "captures/keys.log".into()
            }
        );
    }

    #[test]
    fn parse_ignore_recorded_config_flag() {
        let args =
            Args::try_parse_from(["kosk", "config.toml", "--ignore-recorded-config"]).unwrap();
        assert!(args.ignore_recorded_config);
    }

    fn sample_cfg() -> Config {
        toml::from_str(
            r#"
            layouts = { main = "kb.toml" }
            event_debounce_ms = 400
            event_debounce_repeat_ms = 55
            window_pos = "mouse pointer"
            [key_sink]
            type = "enigo"
            "#,
        )
        .unwrap()
    }

    #[test]
    fn tape_config_omits_blacklisted_keys() {
        let toml = tape_config_toml(&sample_cfg()).unwrap();
        assert!(!toml.contains("layouts"), "{toml}");
        assert!(!toml.contains("window_pos"), "{toml}");
        assert!(!toml.contains("key_sink"), "{toml}");
        assert!(!toml.contains("text_input"), "{toml}");
        assert!(toml.contains("event_debounce_ms"), "{toml}");
    }

    #[test]
    fn overlay_changes_debounce_keeps_window_and_sink() {
        let live = sample_cfg();
        let merged = overlay_tape_config(&live, "event_debounce_ms = 123\n").unwrap();
        assert_eq!(merged.event_debounce_ms, 123);
        assert_eq!(merged.event_debounce_repeat_ms, 55);
        assert_eq!(merged.window_pos, live.window_pos);
        assert_eq!(merged.key_sink, KeySinkConfig::Enigo);
        assert_eq!(merged.layouts, live.layouts);
    }

    #[test]
    fn overlay_ignores_blacklisted_keys_in_blob() {
        let live = sample_cfg();
        let merged = overlay_tape_config(
            &live,
            r#"
            event_debounce_ms = 1
            window_pos = "top left"
            [key_sink]
            type = "log"
            file = "x.log"
            "#,
        )
        .unwrap();
        assert_eq!(merged.event_debounce_ms, 1);
        assert_eq!(merged.window_pos, live.window_pos);
        assert_eq!(merged.key_sink, KeySinkConfig::Enigo);
    }

    fn keyboard_face_top_map(action: &str) -> HashMap<StateId, HashMap<ControllerBinding, String>> {
        let mut keyboard = HashMap::new();
        keyboard.insert(
            ControllerBinding::from(crate::controller::ControllerButton::FaceTop),
            action.to_string(),
        );
        let mut map = HashMap::new();
        map.insert(StateId::Keyboard, keyboard);
        map
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kosk-config-save-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn write_preserves_main_and_mappings_on_window_pos_only() {
        let dir = temp_dir("window-pos");
        let config_path = dir.join("config.toml");
        let mappings_path = dir.join("mappings.toml");

        let mappings = "\
# keep-me-map\n\
\n\
[Keyboard]\n\
\"faceTop\" = \"toggleShift\"\n\
";
        fs::write(&mappings_path, mappings).unwrap();
        fs::write(
            &config_path,
            "\
# keep-me-main\n\
layouts = { main = \"kb.toml\" }\n\
window_pos = \"top left\"\n\
controller_map = \"mappings.toml\"\n\
",
        )
        .unwrap();

        let map = keyboard_face_top_map("toggleShift");
        let mut old = sample_cfg();
        old.window_pos = WindowPos::TopLeft;
        old.controller_map = map.clone();
        let mut new = old.clone();
        new.window_pos = WindowPos::TopRight;

        write_config_preserving(&config_path, &old, &new, Some("mappings.toml")).unwrap();

        let main = fs::read_to_string(&config_path).unwrap();
        assert!(main.contains("# keep-me-main"), "{main}");
        assert!(
            main.contains("controller_map = \"mappings.toml\""),
            "{main}"
        );
        assert!(!main.contains("[controller_map"), "{main}");
        assert!(main.contains("window_pos = \"top right\""), "{main}");

        let mappings_after = fs::read_to_string(&mappings_path).unwrap();
        assert_eq!(mappings_after, mappings);
    }

    #[test]
    fn write_keeps_inline_controller_map() {
        let dir = temp_dir("inline-map");
        let config_path = dir.join("config.toml");
        fs::write(
            &config_path,
            "\
# keep-inline\n\
layouts = { main = \"kb.toml\" }\n\
window_pos = \"top left\"\n\
\n\
[controller_map.Keyboard]\n\
\"faceTop\" = \"toggleShift\"\n\
",
        )
        .unwrap();

        let map = keyboard_face_top_map("toggleShift");
        let mut old = sample_cfg();
        old.window_pos = WindowPos::TopLeft;
        old.controller_map = map.clone();
        let mut new = old.clone();
        new.window_pos = WindowPos::BottomLeft;

        write_config_preserving(&config_path, &old, &new, None).unwrap();

        let main = fs::read_to_string(&config_path).unwrap();
        assert!(main.contains("# keep-inline"), "{main}");
        assert!(main.contains("[controller_map"), "{main}");
        assert!(!main.contains("controller_map = \""), "{main}");
        assert!(!dir.join("mappings.toml").exists());
        assert!(main.contains("window_pos = \"bottom left\""), "{main}");
    }

    #[test]
    fn write_preserves_mappings_format_when_binding_changes() {
        let dir = temp_dir("map-change");
        let config_path = dir.join("config.toml");
        let mappings_path = dir.join("mappings.toml");

        let mappings = "\
# keep-me-map\n\
\n\
[Keyboard]\n\
\"faceTop\" = \"toggleShift\"\n\
\"faceBottom\" = \"sendKey.enter\"\n\
";
        fs::write(&mappings_path, mappings).unwrap();
        fs::write(
            &config_path,
            "\
# keep-me-main\n\
layouts = { main = \"kb.toml\" }\n\
window_pos = \"top left\"\n\
controller_map = \"mappings.toml\"\n\
",
        )
        .unwrap();

        let mut keyboard = HashMap::new();
        keyboard.insert(
            ControllerBinding::from(crate::controller::ControllerButton::FaceTop),
            "toggleShift".to_string(),
        );
        keyboard.insert(
            ControllerBinding::from(crate::controller::ControllerButton::FaceBottom),
            "sendKey.enter".to_string(),
        );
        let mut map = HashMap::new();
        map.insert(StateId::Keyboard, keyboard);

        let mut old = sample_cfg();
        old.window_pos = WindowPos::TopLeft;
        old.controller_map = map;
        let mut new = old.clone();
        new.controller_map
            .get_mut(&StateId::Keyboard)
            .unwrap()
            .insert(
                ControllerBinding::from(crate::controller::ControllerButton::FaceTop),
                "toggleCtrl".to_string(),
            );

        write_config_preserving(&config_path, &old, &new, Some("mappings.toml")).unwrap();

        let main = fs::read_to_string(&config_path).unwrap();
        assert!(main.contains("# keep-me-main"), "{main}");
        assert!(
            main.contains("controller_map = \"mappings.toml\""),
            "{main}"
        );
        assert!(!main.contains("[controller_map"), "{main}");

        let mappings_after = fs::read_to_string(&mappings_path).unwrap();
        assert!(mappings_after.contains("# keep-me-map"), "{mappings_after}");
        assert!(
            mappings_after.contains("\n\n[Keyboard]\n"),
            "{mappings_after}"
        );
        assert!(
            mappings_after.contains("\"faceBottom\" = \"sendKey.enter\""),
            "{mappings_after}"
        );
        assert!(mappings_after.contains("toggleCtrl"), "{mappings_after}");
        assert!(
            !mappings_after.contains(r#""faceTop" = "toggleShift""#),
            "{mappings_after}"
        );
    }
    #[test]
    fn write_preserves_real_config_shape_on_window_pos_change() {
        let dir = temp_dir("real-shape");
        let config_path = dir.join("config.toml");
        let mappings_path = dir.join("mappings.toml");

        let mappings = [
            "# UNIQUE-MAP-COMMENT",
            "# header line two",
            "",
            "[Keyboard]",
            "\"faceTop\" = \"toggleShift\"",
            "\"faceBottom\" = \"sendKey.enter\"",
            "",
            "[Menu]",
            "\"faceBottom\" = \"activate\"",
            "",
        ]
        .join("\n");
        fs::write(&mappings_path, &mappings).unwrap();

        let config = [
            "# UNIQUE-MAIN-COMMENT",
            "start_layout = \"main\"",
            "window_pos = \"mouse pointer\"",
            "transparent = true",
            "controller_map = \"mappings.toml\"",
            "",
            "[sc2]",
            "pad_origin_stretch = 0.3",
            "",
            "[layouts]",
            "main = \"kb.toml\"",
            "",
            "[debug]",
            "show_stick_cursors = true",
            "",
        ]
        .join("\n");
        fs::write(&config_path, &config).unwrap();

        let mut keyboard = HashMap::new();
        keyboard.insert(
            ControllerBinding::from(crate::controller::ControllerButton::FaceTop),
            "toggleShift".to_string(),
        );
        keyboard.insert(
            ControllerBinding::from(crate::controller::ControllerButton::FaceBottom),
            "sendKey.enter".to_string(),
        );
        let mut menu = HashMap::new();
        menu.insert(
            ControllerBinding::from(crate::controller::ControllerButton::FaceBottom),
            "activate".to_string(),
        );
        let mut map = HashMap::new();
        map.insert(StateId::Keyboard, keyboard);
        map.insert(StateId::Menu, menu);

        let mut old = sample_cfg();
        old.window_pos = WindowPos::MousePointer;
        old.transparent = true;
        old.controller_map = map;
        old.sc2.pad_origin_stretch = 0.3;
        old.debug = Some(Debug {
            show_stick_cursors: true,
            show_hitboxes: false,
            show_stick_bounds: false,
        });
        let mut new = old.clone();
        new.window_pos = WindowPos::Absolute(12.0, 34.0);

        write_config_preserving(&config_path, &old, &new, Some("mappings.toml")).unwrap();

        let main = fs::read_to_string(&config_path).unwrap();
        assert!(main.contains("# UNIQUE-MAIN-COMMENT"), "{main}");
        assert!(
            main.contains("controller_map = \"mappings.toml\""),
            "{main}"
        );
        assert!(!main.contains("[controller_map"), "{main}");
        assert!(main.contains("transparent = true"), "{main}");
        assert!(main.contains("pad_origin_stretch = 0.3"), "{main}");
        assert!(main.contains("[debug]"), "{main}");
        assert!(main.contains("[layouts]"), "{main}");
        assert!(main.contains("window_pos = ["), "{main}");
        assert_eq!(fs::read_to_string(&mappings_path).unwrap(), mappings);
    }
}

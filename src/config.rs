use crate::controller::mapping::MappingValue;

pub use crate::config_overlay::{builtin_unigrams, UNBIND_ACTION};
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
    /// Path to the user configuration TOML file. Omitted: `%LOCALAPPDATA%\kosk\config.toml`.
    pub config_path: Option<String>,

    /// Play this recording instead of the configured controller (`[replay]` / preferred_controller).
    #[arg(long, value_name = "FILE")]
    pub replay: Option<PathBuf>,

    /// Log outgoing keys/text to FILE instead of injecting them (`-` = stdout).
    #[arg(long, value_name = "FILE")]
    pub keys_log: Option<PathBuf>,

    /// Use the config file instead of config embedded in a recording.
    #[arg(long)]
    pub ignore_recorded_config: bool,

    /// Exclusive virtual controller + local control port for kosk-mcp.
    /// Also enabled by env `KOSK_CONTROLLER_MCP` (see `mcp_controller_mode`).
    #[arg(long)]
    pub mcp_controller: bool,

    /// Place the overlay at the mouse cursor, ignoring config window_pos.
    #[arg(long)]
    pub at_mouse: bool,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReachOverlay {
    Stick,
    Pad,
    #[default]
    None,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct Debug {
    #[serde(default)]
    pub show_stick_cursors: bool,

    #[serde(default)]
    pub show_hitboxes: bool,

    #[serde(default)]
    pub show_stick_bounds: bool,

    #[serde(default)]
    pub reach_overlay: ReachOverlay,
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
    /// Schema version of the user file. Missing counts as 0.
    #[serde(default)]
    pub config_version: u64,

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
    /// MoveWindow uses a see-through ghost and ignores this value.
    #[serde(default = "default_keyboard_opacity")]
    pub keyboard_opacity: f32,

    /// Overlay clear/panel alpha while `transparent` is true, on Settings / Mappings / SelectKey / SelectLayout.
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

    #[serde(default)]
    pub completion: crate::completion::CompletionConfig,

    /// Per-app-state mapping from controller buttons to action names (interpreted by each state).
    ///
    /// Either an inline table, or a string path to a TOML file whose root is the same map shape
    /// (relative paths are resolved against the main config file's directory).
    #[serde(default, deserialize_with = "deserialize_controller_map")]
    pub controller_map: HashMap<StateId, HashMap<ControllerBinding, MappingValue>>,

    /// Milliseconds for the stick selection to be locked after key under stick is pressed.
    #[serde(default = "default_stick_select_lock_ms")]
    pub stick_select_lock_ms: u64,

    /// Extra hit-test margin for the currently selected key (`1` = off).
    #[serde(default = "default_stick_select_sticky")]
    pub stick_select_sticky: f32,

    /// Steam Controller 2 pad mapping and feel. Omitted → defaults.
    #[serde(default)]
    pub sc2: Sc2Config,

    /// DualShock 4 feel. Omitted → defaults.
    #[serde(default)]
    pub ps4: Ps4Config,

    /// Battery indicator appearance.
    #[serde(default)]
    pub battery: BatteryConfig,

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
    /// Where pad zero sits: `0` = pad center, `1` = first settled touch.
    #[serde(default = "default_pad_origin_relative")]
    pub pad_origin_relative: f32,

    /// Short-edge leftover stretch from the blended origin. `0` = 1:1 delta.
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
            pad_origin_relative: default_pad_origin_relative(),
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

const BATTERY_COLOR_EMPTY: [u8; 4] = [220, 50, 50, 255];
const BATTERY_COLOR_LOW: [u8; 4] = [230, 140, 40, 255];
const BATTERY_COLOR_MEDIUM: [u8; 4] = [230, 200, 60, 255];
const BATTERY_COLOR_HIGH: [u8; 4] = [120, 190, 80, 255];
const BATTERY_COLOR_FULL: [u8; 4] = [50, 200, 90, 255];
const BATTERY_COLOR_CHARGING: [u8; 4] = [70, 180, 220, 255];
const BATTERY_COLOR_UNKNOWN: [u8; 4] = [180, 180, 180, 255];

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct BatteryConfig {
    /// Draw the battery slot with the same filled key background.
    #[serde(default = "default_battery_draw_button")]
    pub draw_button: bool,
    #[serde(default = "default_battery_color_empty")]
    pub empty: [u8; 4],
    #[serde(default = "default_battery_color_low")]
    pub low: [u8; 4],
    #[serde(default = "default_battery_color_medium")]
    pub medium: [u8; 4],
    #[serde(default = "default_battery_color_high")]
    pub high: [u8; 4],
    #[serde(default = "default_battery_color_full")]
    pub full: [u8; 4],
    #[serde(default = "default_battery_color_charging")]
    pub charging: [u8; 4],
    #[serde(default = "default_battery_color_unknown")]
    pub unknown: [u8; 4],
}

fn default_battery_draw_button() -> bool {
    false
}
fn default_battery_color_empty() -> [u8; 4] {
    BATTERY_COLOR_EMPTY
}
fn default_battery_color_low() -> [u8; 4] {
    BATTERY_COLOR_LOW
}
fn default_battery_color_medium() -> [u8; 4] {
    BATTERY_COLOR_MEDIUM
}
fn default_battery_color_high() -> [u8; 4] {
    BATTERY_COLOR_HIGH
}
fn default_battery_color_full() -> [u8; 4] {
    BATTERY_COLOR_FULL
}
fn default_battery_color_charging() -> [u8; 4] {
    BATTERY_COLOR_CHARGING
}
fn default_battery_color_unknown() -> [u8; 4] {
    BATTERY_COLOR_UNKNOWN
}

impl Default for BatteryConfig {
    fn default() -> Self {
        Self {
            draw_button: default_battery_draw_button(),
            empty: default_battery_color_empty(),
            low: default_battery_color_low(),
            medium: default_battery_color_medium(),
            high: default_battery_color_high(),
            full: default_battery_color_full(),
            charging: default_battery_color_charging(),
            unknown: default_battery_color_unknown(),
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
    Inline(HashMap<StateId, HashMap<ControllerBinding, MappingValue>>),
}

fn deserialize_controller_map<'de, D>(
    deserializer: D,
) -> Result<HashMap<StateId, HashMap<ControllerBinding, MappingValue>>, D::Error>
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

fn default_stick_select_sticky() -> f32 {
    1.25
}

fn default_pad_origin_relative() -> f32 {
    0.0
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

/// Battery indicator settings, or defaults when config is not initialized.
pub fn battery() -> BatteryConfig {
    CONFIG_INSTANCE
        .get()
        .map(|instance| instance.lock().unwrap().battery.clone())
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
static CLI_MCP_CONTROLLER: OnceLock<bool> = OnceLock::new();
static CLI_AT_MOUSE: OnceLock<bool> = OnceLock::new();
static CLI_KEYS_LOG: OnceLock<Option<PathBuf>> = OnceLock::new();
static CLI_IGNORE_RECORDED_CONFIG: OnceLock<bool> = OnceLock::new();
/// Write a migrated user file back. Off when the path was passed on the command line.
static PERSIST_MIGRATION: AtomicBool = AtomicBool::new(false);

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
    "completion",
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
    let recorded = crate::config_overlay::migrate_toml(recorded)?;
    let mut overlay: toml::Value =
        toml::from_str(&recorded).context("parse recorded config TOML")?;
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

/// Decimal places when writing a config float. `None` keeps the shortest `f32` text.
fn config_float_digits(path: &str) -> Option<usize> {
    match path {
        "keyboard_opacity"
        | "ui_opacity"
        | "stick_warp"
        | "stick_select_sticky"
        | "sc2.pad_origin_relative"
        | "sc2.pad_origin_stretch" => Some(2),
        "stick_scale_x"
        | "stick_scale_y"
        | "scale_x"
        | "scale_y"
        | "sc2.pad_origin_stretch_max_gain"
        | "text_input.font_size"
        | "completion.ui.max_chip_width"
        | "completion.ui.min_chip_width"
        | "completion.ui.font_size"
        | "completion.ui.corner_radius"
        | "completion.ui.selected_outline_width"
        | "completion.ui.padding_x"
        | "completion.ui.padding_y"
        | "completion.ui.gap"
        | "completion.ui.armed_dot_radius"
        | "completion.ngram.backoff_alpha"
        | "completion.ngram.lambda_trigram"
        | "completion.ngram.lambda_bigram"
        | "completion.ngram.lambda_unigram"
        | "completion.ngram.lambda_exact"
        | "completion.ngram.lambda_typo" => Some(1),
        _ => None,
    }
}

fn format_config_float(path: &str, value: f64) -> String {
    if let Some(digits) = config_float_digits(path) {
        return format!("{:.*}", digits, value);
    }

    let mut text = (value as f32).to_string();
    if !text.contains(['.', 'e', 'E']) {
        text.push_str(".0");
    }
    text
}

fn child_config_path(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

/// Convert a `toml::Value` into a `toml_edit::Item` (toml_edit 0.22 has no `ser::to_item`).
fn toml_to_item(path: &str, value: &toml::Value) -> Result<toml_edit::Item> {
    if let Some(n) = value.as_float() {
        let literal = format_config_float(path, n);
        let parsed: toml_edit::Value = literal
            .parse()
            .map_err(|e| anyhow::anyhow!("config float {path} = {literal}: {e}"))?;
        return Ok(toml_edit::Item::Value(parsed));
    }

    if let Some(table) = value.as_table() {
        let mut out = toml_edit::Table::new();
        for (key, child) in table {
            out.insert(key, toml_to_item(&child_config_path(path, key), child)?);
        }
        return Ok(toml_edit::Item::Table(out));
    }

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
#[cfg(test)]
fn apply_toml_diff(
    path: &str,
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
                let child_path = child_config_path(path, key);
                if can_recurse {
                    let child_table = table
                        .get_mut(key)
                        .and_then(|item| item.as_table_mut())
                        .expect("checked is_table");
                    apply_toml_diff(&child_path, child_table, old_child, new_child)?;
                } else {
                    table[key] = toml_to_item(&child_path, new_child)?;
                }
            }
            _ => {
                table[key] = toml_to_item(&child_config_path(path, key), new_child)?;
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

#[cfg(test)]
fn write_toml_diff_preserving(path: &Path, old: &toml::Value, new: &toml::Value) -> Result<()> {
    if !old.is_table() || !new.is_table() {
        bail!("write_toml_diff_preserving: old and new must be tables");
    }
    let content =
        fs::read_to_string(path).with_context(|| format!("Could not read {}", path.display()))?;
    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("Could not parse {}", path.display()))?;
    apply_toml_diff("", doc.as_table_mut(), old, new)?;
    fs::write(path, doc.to_string())
        .with_context(|| format!("Could not write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
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
        apply_toml_diff("", doc.as_table_mut(), &old_val, &new_val)?;

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

/// Load the built-in defaults, then the user file on top.
fn load_config() -> Result<()> {
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

    let user_text = fs::read_to_string(config_path)
        .with_context(|| format!("Could not read {}", config_path.display()))?;
    let mut user_doc = user_text
        .parse::<toml_edit::DocumentMut>()
        .context("Could not parse config TOML")?;
    let migrated = crate::config_overlay::migrate_document(&mut user_doc)?;
    if migrated && PERSIST_MIGRATION.load(Ordering::Relaxed) {
        fs::write(config_path, user_doc.to_string())
            .with_context(|| format!("Could not write {}", config_path.display()))?;
    }

    let user_value: toml::Value = user_doc
        .to_string()
        .parse()
        .context("Could not parse config TOML")?;
    let mut merged: toml::Value = crate::config_overlay::builtin_config_toml()
        .parse()
        .context("built-in config")?;
    let map_rel = controller_map_rel(&user_value, &merged);
    store_controller_map_file(map_rel.clone());
    crate::config_overlay::merge_toml(&mut merged, &user_value);

    let mut mappings: toml::Value = crate::config_overlay::builtin_mappings_toml()
        .parse()
        .context("built-in mappings")?;
    if let Some(dir) = config_path.parent() {
        if let Some(rel) = &map_rel {
            if let Some(over) = crate::config_overlay::read_user_mappings(dir, rel)? {
                crate::config_overlay::merge_mappings(&mut mappings, &over);
            }
        }
    }
    if let Some(inline) = user_value.get("controller_map").filter(|v| v.is_table()) {
        crate::config_overlay::merge_mappings(&mut mappings, inline);
    }
    if let Some(table) = merged.as_table_mut() {
        table.insert("controller_map".to_owned(), mappings);
    }

    let new_config: Config = merged
        .try_into()
        .map_err(|e| anyhow::anyhow!("Could not parse config TOML: {e}"))?;

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

    let config_parent = config_path.parent();
    for (name, path) in &new_config.layouts {
        crate::config_overlay::read_layout(config_parent, path)
            .with_context(|| format!(r#"cannot find layout file "{path}" for layout "{name}""#))?;
    }

    if let Some(template) = &new_config.record_file {
        crate::controller::record::validate_record_template(template).context("record_file")?;
    }

    crate::completion::validate_app_types(&new_config.completion.app_types)
        .context("completion.app_types")?;

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

    Ok(())
}

fn controller_map_rel(user: &toml::Value, builtin: &toml::Value) -> Option<String> {
    match user.get("controller_map") {
        Some(toml::Value::String(path)) => Some(path.clone()),
        Some(_) => None,
        None => builtin
            .get("controller_map")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
    }
}

/// Layout TOML: user folder, working directory, then the built-in copy.
pub fn read_layout_source(rel: &str) -> Result<String> {
    crate::config_overlay::read_layout(config_dir().as_deref(), rel)
}

/// Word list or model directory: beside the config file, else the working directory.
pub fn resolve_data_path(rel: &Path) -> PathBuf {
    if rel.is_absolute() {
        return rel.to_path_buf();
    }
    if let Some(path) =
        crate::config_overlay::existing_file(config_dir().as_deref(), &rel.to_string_lossy())
    {
        return path;
    }
    if let Some(path) = crate::config_overlay::existing_dir(config_dir().as_deref(), rel) {
        return path;
    }
    config_dir()
        .map(|dir| dir.join(rel))
        .unwrap_or_else(|| rel.to_path_buf())
}

/// Watch the user config directory so config, mappings, and layouts reload together.
fn start_watcher_thread(config_path: PathBuf) -> Result<()> {
    let config_dir = config_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("config file has no parent directory"))?
        .to_path_buf();
    let config_dir = std::fs::canonicalize(&config_dir)
        .with_context(|| format!("watch {}", config_dir.display()))?;

    std::thread::spawn(move || -> Result<()> {
        let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
        let mut watcher = notify::recommended_watcher(tx)?;
        if let Err(e) = watcher.watch(&config_dir, notify::RecursiveMode::NonRecursive) {
            eprintln!("Failed to watch config directory: {e}");
        }

        for res in rx {
            match res {
                Ok(event)
                    if matches!(
                        event.kind,
                        EventKind::Modify(ModifyKind::Any | ModifyKind::Data(_))
                            | EventKind::Create(_)
                    ) =>
                {
                    let in_dir = event.paths.iter().any(|p| {
                        p.parent()
                            .and_then(|parent| std::fs::canonicalize(parent).ok())
                            == Some(config_dir.clone())
                    });
                    if !in_dir {
                        continue;
                    }
                    match load_config() {
                        Ok(()) => notify_config_changed(),
                        Err(e) => {
                            if e.to_string() != "Reload debounced" {
                                eprintln!("Failed to reload config: {e}");
                            }
                        }
                    }
                }
                Err(e) => eprintln!("watch error: {e:?}"),
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
    CLI_MCP_CONTROLLER
        .set(args.mcp_controller)
        .expect("CLI mcp-controller was already set");
    CLI_AT_MOUSE
        .set(args.at_mouse)
        .expect("CLI at-mouse was already set");
    let (path, persist) = match &args.config_path {
        Some(path) => (PathBuf::from(path), false),
        None => (crate::config_overlay::ensure_user_config_file()?, true),
    };
    PERSIST_MIGRATION.store(persist, Ordering::Relaxed);
    init_from_path(path)?;
    if mcp_controller_mode() && preferred_is_replay() {
        bail!(
            "mcp-controller mode cannot be combined with replay (--replay / preferred_controller=replay)"
        );
    }
    Ok(())
}

/// Load config from `config_path` without parsing process args (for auxiliary binaries).
pub fn init_from_path(config_path: PathBuf) -> Result<()> {
    CONFIG_PATH
        .set(config_path.clone())
        .expect("Config path was already set");

    load_config()?;

    let skip_watcher = preferred_is_replay();
    if !skip_watcher {
        start_watcher_thread(config_path)?;
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

pub fn cli_at_mouse() -> bool {
    CLI_AT_MOUSE.get().copied().unwrap_or(false)
}

pub fn ignore_recorded_config() -> bool {
    CLI_IGNORE_RECORDED_CONFIG.get().copied().unwrap_or(false)
}

/// True if `--mcp-controller` OR env `KOSK_CONTROLLER_MCP` is set truthy/address.
pub fn mcp_controller_mode() -> bool {
    if CLI_MCP_CONTROLLER.get().copied().unwrap_or(false) {
        return true;
    }
    match std::env::var("KOSK_CONTROLLER_MCP") {
        Err(_) => false,
        Ok(v) => {
            let t = v.trim();
            !(t.is_empty() || t == "0" || t.eq_ignore_ascii_case("false"))
        }
    }
}

/// Bind address when MCP-controller mode is on. Default `127.0.0.1:5720`.
///
/// Env `KOSK_CONTROLLER_MCP`:
/// - `1` / `true` / CLI-only → `127.0.0.1:5720`
/// - otherwise parse as `host:port` (same idea as `EGUI_INSPECTION`)
pub fn mcp_controller_bind() -> std::net::SocketAddr {
    const DEFAULT: &str = "127.0.0.1:5720";
    match std::env::var("KOSK_CONTROLLER_MCP") {
        Ok(v) => {
            let t = v.trim();
            if t.is_empty()
                || t == "0"
                || t.eq_ignore_ascii_case("false")
                || t == "1"
                || t.eq_ignore_ascii_case("true")
            {
                DEFAULT.parse().expect("default mcp controller bind")
            } else {
                t.parse().unwrap_or_else(|_| {
                    eprintln!(
                        "warn: could not parse KOSK_CONTROLLER_MCP={t:?} as host:port; using {DEFAULT}"
                    );
                    DEFAULT.parse().expect("default mcp controller bind")
                })
            }
        }
        Err(_) => DEFAULT.parse().expect("default mcp controller bind"),
    }
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

pub(crate) fn try_get() -> Option<Config> {
    CONFIG_INSTANCE
        .get()
        .map(|instance| instance.lock().unwrap().clone())
}

/// Replace the in-memory config. Does not write the file or notify listeners.
pub(crate) fn replace_live(cfg: Config) {
    if let Some(instance) = CONFIG_INSTANCE.get() {
        *instance.lock().unwrap() = cfg;
    }
}

fn builtin_merged_config() -> Result<Config> {
    let mut merged: toml::Value = crate::config_overlay::builtin_config_toml()
        .parse()
        .context("built-in config")?;
    let mappings: toml::Value = crate::config_overlay::builtin_mappings_toml()
        .parse()
        .context("built-in mappings")?;
    if let Some(table) = merged.as_table_mut() {
        table.insert("controller_map".to_owned(), mappings);
    }
    merged
        .try_into()
        .map_err(|e| anyhow::anyhow!("built-in config: {e}"))
}

/// Write `new` into the user file as values that differ from the built-in default.
fn write_user_overlay(path: &Path, new: &Config) -> Result<()> {
    let builtin = builtin_merged_config()?;
    let mut default_val = toml::Value::try_from(&builtin).context("serialize built-in config")?;
    let mut new_val = toml::Value::try_from(new).context("serialize config")?;
    if let Some(table) = default_val.as_table_mut() {
        table.remove("controller_map");
    }
    if let Some(table) = new_val.as_table_mut() {
        table.remove("controller_map");
    }

    let content = fs::read_to_string(path).unwrap_or_default();
    let mut doc = if content.trim().is_empty() {
        toml_edit::DocumentMut::new()
    } else {
        content
            .parse::<toml_edit::DocumentMut>()
            .with_context(|| format!("Could not parse {}", path.display()))?
    };
    sync_overlay("", doc.as_table_mut(), &default_val, &new_val)?;
    doc["config_version"] = toml_edit::value(crate::config_overlay::CONFIG_VERSION);
    fs::write(path, doc.to_string())
        .with_context(|| format!("Could not write {}", path.display()))?;

    if let Some(rel) = controller_map_file() {
        let mappings_path = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("config file has no parent directory"))?
            .join(rel);
        write_mappings_overlay(&mappings_path, &builtin.controller_map, &new.controller_map)?;
    }
    Ok(())
}

fn sync_overlay(
    path: &str,
    user: &mut toml_edit::Table,
    default: &toml::Value,
    new: &toml::Value,
) -> Result<()> {
    let default_table = default
        .as_table()
        .ok_or_else(|| anyhow::anyhow!("sync_overlay: default must be a table"))?;
    let new_table = new
        .as_table()
        .ok_or_else(|| anyhow::anyhow!("sync_overlay: new must be a table"))?;

    for (key, new_child) in new_table {
        if key == "config_version" {
            continue;
        }
        let child_path = child_config_path(path, key);
        match default_table.get(key) {
            Some(default_child) if default_child == new_child => {
                user.remove(key);
            }
            Some(default_child) if default_child.is_table() && new_child.is_table() => {
                if !user.get(key).is_some_and(|item| item.is_table()) {
                    user.insert(key, toml_edit::Item::Table(toml_edit::Table::new()));
                }
                let child = user
                    .get_mut(key)
                    .and_then(|item| item.as_table_mut())
                    .expect("inserted table");
                sync_overlay(&child_path, child, default_child, new_child)?;
                if child.is_empty() {
                    user.remove(key);
                }
            }
            _ => {
                user.insert(key, toml_to_item(&child_path, new_child)?);
            }
        }
    }

    let stale: Vec<String> = user
        .iter()
        .map(|(key, _)| key.to_owned())
        .filter(|key| key != "config_version" && !new_table.contains_key(key))
        .collect();
    for key in stale {
        user.remove(&key);
    }
    Ok(())
}

fn write_mappings_overlay(
    path: &Path,
    default_map: &HashMap<StateId, HashMap<ControllerBinding, MappingValue>>,
    new_map: &HashMap<StateId, HashMap<ControllerBinding, MappingValue>>,
) -> Result<()> {
    let default_val = toml::Value::try_from(default_map).context("serialize default mappings")?;
    let new_val = toml::Value::try_from(new_map).context("serialize mappings")?;
    let content = fs::read_to_string(path).unwrap_or_default();
    let mut doc = if content.trim().is_empty() {
        toml_edit::DocumentMut::new()
    } else {
        content
            .parse::<toml_edit::DocumentMut>()
            .with_context(|| format!("Could not parse {}", path.display()))?
    };
    sync_mappings(doc.as_table_mut(), &default_val, &new_val)?;
    if doc.as_table().is_empty() {
        if path.exists() {
            fs::remove_file(path)
                .with_context(|| format!("Could not remove {}", path.display()))?;
        }
        return Ok(());
    }
    fs::write(path, doc.to_string())
        .with_context(|| format!("Could not write {}", path.display()))?;
    Ok(())
}

fn sync_mappings(
    user: &mut toml_edit::Table,
    default: &toml::Value,
    new: &toml::Value,
) -> Result<()> {
    let empty = toml::map::Map::new();
    let default_modes = default.as_table().unwrap_or(&empty);
    let new_modes = new.as_table().unwrap_or(&empty);
    let mut modes: Vec<String> = default_modes.keys().cloned().collect();
    for key in new_modes.keys() {
        if !modes.contains(key) {
            modes.push(key.clone());
        }
    }
    for mode in modes {
        let default_mode = default_modes
            .get(&mode)
            .and_then(|v| v.as_table())
            .cloned()
            .unwrap_or_default();
        let new_mode = new_modes
            .get(&mode)
            .and_then(|v| v.as_table())
            .cloned()
            .unwrap_or_default();
        if !user.get(&mode).is_some_and(|item| item.is_table()) {
            user.insert(&mode, toml_edit::Item::Table(toml_edit::Table::new()));
        }
        let mode_user = user
            .get_mut(&mode)
            .and_then(|item| item.as_table_mut())
            .expect("inserted mode table");
        let mut bindings: Vec<String> = default_mode.keys().cloned().collect();
        for key in new_mode.keys() {
            if !bindings.contains(key) {
                bindings.push(key.clone());
            }
        }
        for binding in bindings {
            match (default_mode.get(&binding), new_mode.get(&binding)) {
                (Some(default_action), Some(action)) if default_action == action => {
                    mode_user.remove(&binding);
                }
                (Some(_), None) => {
                    mode_user.insert(
                        &binding,
                        toml_edit::Item::Value(toml_edit::Value::from(
                            crate::config_overlay::UNBIND_ACTION,
                        )),
                    );
                }
                (None, None) => {
                    mode_user.remove(&binding);
                }
                (_, Some(action)) => {
                    mode_user.insert(&binding, toml_to_item("", action)?);
                }
            }
        }
        if mode_user.is_empty() {
            user.remove(&mode);
        }
    }
    Ok(())
}

/// Write settings edits. While a tape config is overlaid, `copy_edits` copies only
/// the changed fields onto the disk snapshot so tape-merged values stay off disk.
/// Otherwise the whole live config is saved, same as [`save`].
pub(crate) fn persist_settings(live: &Config, copy_edits: impl FnOnce(&mut Config)) -> Result<()> {
    if !TAPE_OVERLAY_ACTIVE.load(Ordering::Relaxed) {
        return save(live.clone());
    }

    let path = CONFIG_PATH
        .get()
        .ok_or(anyhow::anyhow!("Config path not set"))?;
    let disk = DISK_CONFIG
        .get()
        .ok_or_else(|| anyhow::anyhow!("disk config is not initialized"))?;
    let disk_before = disk.lock().unwrap().clone();
    let mut persist = disk_before.clone();
    copy_edits(&mut persist);
    write_user_overlay(path, &persist)?;
    *disk.lock().unwrap() = persist;
    Ok(())
}

pub fn save(new_config: Config) -> Result<()> {
    let path = CONFIG_PATH
        .get()
        .ok_or(anyhow::anyhow!("Config path not set"))?;

    if TAPE_OVERLAY_ACTIVE.load(Ordering::Relaxed) {
        let disk = DISK_CONFIG
            .get()
            .ok_or_else(|| anyhow::anyhow!("disk config is not initialized"))?;
        let disk_before = disk.lock().unwrap().clone();
        let mut persist = disk_before.clone();
        persist.window_pos = new_config.window_pos;
        write_user_overlay(path, &persist)?;
        *disk.lock().unwrap() = persist;
        if let Some(instance) = CONFIG_INSTANCE.get() {
            instance.lock().unwrap().window_pos = new_config.window_pos;
        }
        return Ok(());
    }

    if DISK_CONFIG.get().is_none() {
        bail!("disk config is not initialized");
    }
    write_user_overlay(path, &new_config)?;

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
        assert_eq!(args.config_path.as_deref(), Some("config.toml"));
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
    fn parse_reach_overlay_modes() {
        for (value, expected) in [
            ("stick", ReachOverlay::Stick),
            ("pad", ReachOverlay::Pad),
            ("none", ReachOverlay::None),
        ] {
            let debug: Debug = toml::from_str(&format!("reach_overlay = \"{value}\"\n")).unwrap();
            assert_eq!(debug.reach_overlay, expected);
        }
        assert_eq!(Debug::default().reach_overlay, ReachOverlay::None);
    }

    #[test]
    fn parse_ignore_recorded_config_flag() {
        let args =
            Args::try_parse_from(["kosk", "config.toml", "--ignore-recorded-config"]).unwrap();
        assert!(args.ignore_recorded_config);
    }

    #[test]
    fn parse_at_mouse_flag() {
        let args = Args::try_parse_from(["kosk", "config.toml", "--at-mouse"]).unwrap();
        assert!(args.at_mouse);
        let args = Args::try_parse_from(["kosk", "config.toml"]).unwrap();
        assert!(!args.at_mouse);
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
        assert!(!toml.contains("completion"), "{toml}");
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

    fn keyboard_face_top_map(
        action: &str,
    ) -> HashMap<StateId, HashMap<ControllerBinding, MappingValue>> {
        let mut keyboard = HashMap::new();
        keyboard.insert(
            ControllerBinding::from(crate::controller::ControllerButton::FaceTop),
            MappingValue::from_action(action),
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
            MappingValue::from_action("toggleShift"),
        );
        keyboard.insert(
            ControllerBinding::from(crate::controller::ControllerButton::FaceBottom),
            MappingValue::from_action("sendKey.enter"),
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
                MappingValue::from_action("toggleCtrl"),
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
            "[Settings]",
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
            MappingValue::from_action("toggleShift"),
        );
        keyboard.insert(
            ControllerBinding::from(crate::controller::ControllerButton::FaceBottom),
            MappingValue::from_action("sendKey.enter"),
        );
        let mut menu = HashMap::new();
        menu.insert(
            ControllerBinding::from(crate::controller::ControllerButton::FaceBottom),
            MappingValue::from_action("activate"),
        );
        let mut map = HashMap::new();
        map.insert(StateId::Keyboard, keyboard);
        map.insert(StateId::Settings, menu);

        let mut old = sample_cfg();
        old.window_pos = WindowPos::MousePointer;
        old.transparent = true;
        old.controller_map = map;
        old.sc2.pad_origin_stretch = 0.3;
        old.debug = Some(Debug {
            show_stick_cursors: true,
            show_hitboxes: false,
            show_stick_bounds: false,
            reach_overlay: ReachOverlay::None,
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

    #[test]
    fn write_rounds_floats_to_field_decimals() {
        let dir = temp_dir("float-decimals");
        let config_path = dir.join("config.toml");
        fs::write(
            &config_path,
            "\
layouts = { main = \"kb.toml\" }\n\
keyboard_opacity = 0.30\n\
ui_opacity = 1.00\n\
stick_scale_x = 3.0\n\
scale_x = 40.0\n\
stick_select_sticky = 1.00\n\
",
        )
        .unwrap();

        let mut old = sample_cfg();
        old.keyboard_opacity = 0.3;
        old.ui_opacity = 1.0;
        old.stick_scale_x = 3.0;
        old.scale_x = 40.0;
        old.stick_select_sticky = 1.0;
        let mut new = old.clone();
        new.keyboard_opacity = 0.7;
        new.ui_opacity = 0.95;
        new.stick_scale_x = 3.8;
        new.scale_x = 30.0;
        new.stick_select_sticky = 1.25;

        write_config_preserving(&config_path, &old, &new, None).unwrap();

        let main = fs::read_to_string(&config_path).unwrap();
        assert!(main.contains("keyboard_opacity = 0.70"), "{main}");
        assert!(main.contains("ui_opacity = 0.95"), "{main}");
        assert!(main.contains("stick_scale_x = 3.8"), "{main}");
        assert!(main.contains("scale_x = 30.0"), "{main}");
        assert!(main.contains("stick_select_sticky = 1.25"), "{main}");
        assert!(!main.contains("699999"), "{main}");
    }

    #[test]
    fn parse_without_config_path() {
        let args = Args::try_parse_from(["kosk"]).unwrap();
        assert!(args.config_path.is_none());
    }

    #[test]
    fn overlay_save_keeps_only_differences() {
        let dir = temp_dir("overlay-save");
        let config_path = dir.join("config.toml");
        fs::write(&config_path, "config_version = 1\n").unwrap();
        store_controller_map_file(Some("mappings.toml".into()));

        let mut cfg = builtin_merged_config().unwrap();
        cfg.keyboard_opacity = 0.7;
        cfg.completion.ui.columns = 4;
        write_user_overlay(&config_path, &cfg).unwrap();

        let text = fs::read_to_string(&config_path).unwrap();
        assert!(text.contains("config_version = 1"), "{text}");
        assert!(text.contains("keyboard_opacity = 0.70"), "{text}");
        assert!(text.contains("columns = 4"), "{text}");
        assert!(!text.contains("stick_scale_x"), "{text}");
        assert!(!dir.join("mappings.toml").exists());

        cfg.keyboard_opacity = builtin_merged_config().unwrap().keyboard_opacity;
        write_user_overlay(&config_path, &cfg).unwrap();
        let text = fs::read_to_string(&config_path).unwrap();
        assert!(!text.contains("keyboard_opacity"), "{text}");
        assert!(text.contains("columns = 4"), "{text}");
    }

    #[test]
    fn overlay_save_unbinds_default_mapping() {
        let dir = temp_dir("overlay-unbind");
        let config_path = dir.join("config.toml");
        fs::write(&config_path, "config_version = 1\n").unwrap();
        store_controller_map_file(Some("mappings.toml".into()));

        let mut cfg = builtin_merged_config().unwrap();
        cfg.controller_map
            .get_mut(&StateId::Keyboard)
            .unwrap()
            .remove(&ControllerBinding::from(
                crate::controller::ControllerButton::FaceTop,
            ));
        write_user_overlay(&config_path, &cfg).unwrap();

        let mappings = fs::read_to_string(dir.join("mappings.toml")).unwrap();
        assert!(
            mappings.contains("\"faceTop\" = \"none\"") || mappings.contains("faceTop = \"none\""),
            "{mappings}"
        );
    }

    #[test]
    fn recorded_config_migrates_before_overlay() {
        let live = sample_cfg();
        let merged = overlay_tape_config(&live, "event_debounce_ms = 12\n").unwrap();
        assert_eq!(merged.event_debounce_ms, 12);
        assert!(crate::config_overlay::migrate_toml("config_version = 99\n").is_err());
    }
}

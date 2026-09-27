use crate::controller::mapping::MappingValue;

pub(crate) use crate::config_overlay::{builtin_unigrams, UNBIND_ACTION};
use crate::config_overlay::{CONFIG_VERSION_KEY, CONTROLLER_MAP_KEY};
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

use kosk_config_derive::{Choice, ConfigSection};

#[path = "config/schema.rs"]
pub(crate) mod schema;
pub(crate) use schema::{device_name, DevicePage, Page, Setting};
use schema::{save_decimals, DEFAULT_MAPPINGS_FILE};

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

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, Choice)]
#[serde(rename_all = "lowercase")]
pub enum ReachOverlay {
    #[default]
    None,
    Stick,
    Pad,
}

#[derive(Debug, Deserialize, Serialize, Clone, ConfigSection)]
pub struct Debug {
    #[config(default = true)]
    #[serde(default = "Debug::__sdef_show_stick_cursors")]
    #[setting(
        label = "Stick cursors",
        explain = "Draw where the sticks are pointing."
    )]
    pub show_stick_cursors: bool,

    #[serde(default)]
    #[setting(label = "Hitboxes", explain = "Draw the region each key occupies.")]
    pub show_hitboxes: bool,

    #[serde(default)]
    #[setting(
        label = "Stick bounds",
        explain = "Draw the rectangle the sticks can reach."
    )]
    pub show_stick_bounds: bool,

    #[serde(default)]
    #[setting(
        label = "Reach overlay",
        explain = "Draw which keys a stick or a pad can reach. None leaves the keyboard as it is."
    )]
    pub reach_overlay: ReachOverlay,
}

#[derive(Debug, Deserialize, Serialize, Clone, ConfigSection)]
pub struct TextInputStyle {
    /// RGBA background color for the text field as `[r, g, b, a]`.
    #[config(default = [255, 255, 255, 255])]
    #[serde(default = "TextInputStyle::__sdef_background_color")]
    pub background_color: [u8; 4],

    /// RGBA text color for the text field as `[r, g, b, a]`.
    #[config(default = [0, 0, 0, 255])]
    #[serde(default = "TextInputStyle::__sdef_text_color")]
    pub text_color: [u8; 4],

    /// RGBA color for the synthetic caret when the field is not focused.
    #[config(default = [0, 0, 0, 255])]
    #[serde(default = "TextInputStyle::__sdef_cursor_color")]
    pub cursor_color: [u8; 4],

    #[config(default = 22.0)]
    #[serde(default = "TextInputStyle::__sdef_font_size")]
    #[setting(
        label = "Text field size",
        explain = "How big the text is on the text-input screen.",
        range = 10.0..=32.0,
        step = 1.0,
        decimals = 0
    )]
    pub font_size: f32,
}

#[derive(Debug, Deserialize, Serialize, Clone, ConfigSection)]
pub struct Config {
    /// Schema version of the user file. Missing counts as 0.
    ///
    /// The serde default stays 0 (unknown version); `Default` uses the current
    /// version for the built-in base layer.
    #[config(default = crate::config_overlay::CONFIG_VERSION)]
    #[serde(default)]
    pub config_version: i64,

    /// Named layouts: map from layout name to file path
    /// Must contain at least "main" layout
    #[config(default = {
        let mut layouts = std::collections::HashMap::new();
        layouts.insert("main".to_owned(), "old_sc.toml".to_owned());
        layouts.insert("symbols".to_owned(), "old_sc_symbols.toml".to_owned());
        layouts
    })]
    pub layouts: std::collections::HashMap<String, String>,

    /// Which layout to start with (must exist in layouts)
    #[config(default = "main".to_owned())]
    #[serde(default = "Config::__sdef_start_layout")]
    pub start_layout: String,

    /// Try these controller families first. Omitted families are appended in
    /// built-in default order (`sc2`, then `ps4`). Empty / omitted → that default.
    #[config(default = vec![crate::controller::ControllerKind::Sc2])]
    #[serde(default = "Config::__sdef_preferred_controller")]
    pub preferred_controller: Vec<crate::controller::ControllerKind>,

    /// Whether the window should be transparent
    #[config(default = true)]
    #[serde(default = "Config::__sdef_transparent")]
    #[setting(
        page = Overlay,
        label = "See-through window",
        explain = "Draws the overlay as a layered window. Keyboard opacity and menu opacity apply only while this is on."
    )]
    pub transparent: bool,

    /// Overlay clear/panel alpha while `transparent` is true, on Keyboard / TextInput.
    /// MoveWindow uses a see-through ghost and ignores this value.
    #[config(default = 1.0)]
    #[serde(default = "Config::__sdef_keyboard_opacity")]
    #[setting(
        page = Overlay,
        label = "Keyboard opacity",
        explain = "How solid the keyboard and the text field are. 1 is opaque.",
        range = 0.2..=1.0,
        step = 0.05,
        decimals = 2
    )]
    pub keyboard_opacity: f32,

    /// Overlay clear/panel alpha while `transparent` is true, on Settings / Mappings / SelectKey / SelectLayout.
    #[config(default = 1.0)]
    #[serde(default = "Config::__sdef_ui_opacity")]
    #[setting(
        page = Overlay,
        label = "Menu opacity",
        explain = "How solid settings, mappings, and the layout picker are.",
        range = 0.4..=1.0,
        step = 0.05,
        decimals = 2
    )]
    pub ui_opacity: f32,

    #[serde(default)]
    #[setting(section, page = Debug)]
    pub debug: Debug,

    #[config(default = crate::state::window_pos::WindowPos::MousePointer)]
    #[serde(default = "Config::__sdef_window_pos")]
    pub window_pos: WindowPos,

    #[config(default = 30.0)]
    #[serde(default = "Config::__sdef_scale_x")]
    #[setting(
        page = Overlay,
        label = "Key width",
        explain = "Horizontal size of a key. This is layout scale, not how far the stick moves.",
        range = 16.0..=48.0,
        step = 1.0,
        decimals = 0
    )]
    pub scale_x: f32,
    #[config(default = 32.0)]
    #[serde(default = "Config::__sdef_scale_y")]
    #[setting(
        page = Overlay,
        label = "Key height",
        explain = "Vertical size of a key.",
        range = 16.0..=48.0,
        step = 1.0,
        decimals = 0
    )]
    pub scale_y: f32,

    /// Milliseconds before the first repeat of the same outgoing event unit (a
    /// single [`Event`](crate::state::event::Event) or a completed batch). Use
    /// `0` to disable debouncing entirely.
    #[config(default = 240)]
    #[serde(default = "Config::__sdef_event_debounce_ms")]
    #[setting(
        page = Typing,
        label = "Delay before repeat",
        explain = "How long a key must be held before it starts repeating. 0 sends a repeat on every poll.",
        range = 0..=500,
        step = 10,
        unit = "ms"
    )]
    pub event_debounce_ms: u64,

    /// Milliseconds between further repeats of the same unit after the first
    /// repeat has fired (key-repeat style). Ignored when `event_debounce_ms` is
    /// `0`. Use `0` here to use `event_debounce_ms` for every repeat step.
    #[config(default = 55)]
    #[serde(default = "Config::__sdef_event_debounce_repeat_ms")]
    #[setting(
        page = Typing,
        label = "Repeat interval",
        explain = "Time between repeats after the first one. 0 uses the delay before repeat for every step.",
        range = 0..=200,
        step = 5,
        unit = "ms"
    )]
    pub event_debounce_repeat_ms: u64,

    #[serde(default)]
    #[setting(section, page = Overlay)]
    pub text_input: TextInputStyle,

    #[serde(default)]
    #[setting(section, page = Suggestions)]
    pub completion: crate::completion::CompletionConfig,

    /// Per-app-state mapping from controller buttons to action names (interpreted by each state).
    ///
    /// Either an inline table, or a string path to a TOML file whose root is the same map shape
    /// (relative paths are resolved against the main config file's directory).
    #[serde(default, deserialize_with = "deserialize_controller_map")]
    pub controller_map: HashMap<StateId, HashMap<ControllerBinding, MappingValue>>,

    /// Steam Controller 2 pad mapping and feel. Omitted → defaults.
    #[serde(default)]
    #[setting(section, page = Device(Sc2, Pads))]
    pub sc2: Sc2Config,

    /// DualShock 4 feel. Omitted → defaults.
    #[serde(default)]
    #[setting(section)]
    pub ps4: Ps4Config,

    /// Battery indicator appearance.
    #[serde(default)]
    #[setting(section, page = Overlay)]
    pub battery: BatteryConfig,

    /// Template for `toggleRecord` captures. Must contain exactly one `%` (3-digit index).
    #[config(default = Some("captures/kosk-%.krec".to_owned()))]
    #[serde(default = "Config::__sdef_record_file")]
    pub record_file: Option<String>,

    /// Replay device. `[replay].file` is required when `preferred_controller` starts with `replay`.
    #[serde(default)]
    pub replay: ReplayConfig,

    /// Where to send outgoing keys/text. Omitted → Enigo injection.
    #[serde(default)]
    pub key_sink: KeySinkConfig,

    /// One-time tutorial cards.
    #[serde(default)]
    pub tips: TipsConfig,
}

/// Which analog surface an aim profile applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AimSurface {
    Pad,
    Stick,
}

/// Range, circle-to-square warp, and selection hold for one analog surface.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, ConfigSection)]
pub struct AimProfile {
    #[config(default = 3.8)]
    #[serde(default = "AimProfile::__sdef_scale_x")]
    #[setting(
        label = "Horizontal range",
        explain = "A full movement left or right covers this much of the keyboard. Raise it when the outer columns stay out of reach.",
        range = 1.0..=8.0,
        step = 0.1,
        decimals = 1
    )]
    pub scale_x: f32,

    #[config(default = 2.8)]
    #[serde(default = "AimProfile::__sdef_scale_y")]
    #[setting(
        label = "Vertical range",
        explain = "A full movement up or down covers this much of the keyboard.",
        range = 1.0..=8.0,
        step = 0.1,
        decimals = 1
    )]
    pub scale_y: f32,

    /// `0` keeps the raw direction. `1` fills the square corners.
    #[config(default = 1.0)]
    #[serde(default = "AimProfile::__sdef_warp")]
    #[setting(
        label = "Square the corners",
        explain = "A diagonal falls short of the corner keys. 0 keeps the raw direction. 1 stretches a full diagonal out to the corner keys. A value in between is a partial stretch.",
        range = 0.0..=1.0,
        step = 0.05,
        decimals = 2
    )]
    pub warp: f32,

    /// Extra hit-test margin for the current key (`1` = off).
    #[config(default = 1.25)]
    #[serde(default = "AimProfile::__sdef_select_sticky")]
    #[setting(
        label = "Stickiness",
        explain = "The key you are already on keeps the highlight until another key is this many times closer to your thumb. At 1.25 a neighbor has to be noticeably closer before the highlight moves. 1 turns that off, and the nearest key wins immediately.",
        range = 1.0..=2.0,
        step = 0.05,
        decimals = 2
    )]
    pub select_sticky: f32,

    /// Milliseconds the highlight stays on a key after it is sent.
    #[config(default = 100)]
    #[serde(default = "AimProfile::__sdef_select_lock_ms")]
    #[setting(
        label = "Hold after a key",
        explain = "After a letter is sent, the highlight stays on that key for this long. 0 releases it immediately.",
        range = 0..=300,
        step = 10,
        unit = "ms"
    )]
    pub select_lock_ms: u64,
}

/// Press distance for the left and right triggers. Shared by both controller
/// families; each family inherits its own settings page.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, ConfigSection)]
pub struct TriggerThresholds {
    #[config(default = 40)]
    #[serde(default = "TriggerThresholds::__sdef_trigger_left_threshold")]
    #[setting(
        label = "Left trigger",
        explain = "How far the trigger must travel before it counts as pressed. Raise it if a resting finger sends keys.",
        range = 0..=255,
        step = 5
    )]
    pub trigger_left_threshold: u8,

    #[config(default = 40)]
    #[serde(default = "TriggerThresholds::__sdef_trigger_right_threshold")]
    #[setting(
        label = "Right trigger",
        explain = "The same cutoff, on the right trigger.",
        range = 0..=255,
        step = 5
    )]
    pub trigger_right_threshold: u8,
}

/// SC2-only pad mapping and feel. Does not affect DualShock 4.
#[derive(Debug, Deserialize, Serialize, Clone, ConfigSection)]
pub struct Sc2Config {
    #[serde(default)]
    #[setting(section)]
    pub pad: AimProfile,

    #[serde(default)]
    #[setting(section, page = Device(Sc2, Stick))]
    pub stick: AimProfile,

    #[serde(flatten, default)]
    #[setting(section, page = Device(Sc2, Triggers))]
    pub triggers: TriggerThresholds,

    /// Where pad zero sits: `0` = pad center, `1` = first settled touch.
    ///
    /// What "rest" means when a thumb lands: 0 treats the pad location as the
    /// key, 1 treats the first contact as rest and only tracks slides away
    /// from it. Values in between start part-way between those two.
    #[config(default = 0.6)]
    #[serde(default = "Sc2Config::__sdef_pad_origin_relative")]
    #[setting(
        label = "Thumb rest",
        explain = "0 treats the place you touch as the key. A thumb on the upper right of the pad highlights an upper-right key. 1 treats the first contact as rest: the highlight starts on that pad's home-row key and only moves as you slide away from where you landed. A value in between starts part-way between those two.",
        range = 0.0..=1.0,
        step = 0.05,
        decimals = 2
    )]
    pub pad_origin_relative: f32,

    /// Short-edge leftover stretch from the blended origin. `0` = 1:1 delta.
    ///
    /// When rest is not the pad center, one slide direction has less pad left.
    /// 1 speeds up only that short leftover direction so the far keys stay
    /// reachable; the long side is left as it is.
    #[config(default = 1.0)]
    #[serde(default = "Sc2Config::__sdef_pad_origin_stretch")]
    #[setting(
        label = "Stretch the short side",
        explain = "If rest is not the center of the pad, one direction has less pad left. 0 follows your thumb one-to-one, so you can run out of pad before the far keys. 1 speeds up only that short direction, so those keys stay reachable. The long direction is left as it is.",
        range = 0.0..=1.0,
        step = 0.05,
        decimals = 2
    )]
    pub pad_origin_stretch: f32,

    /// Cap on per-axis short-edge gain (`1` = no extra gain).
    #[config(default = 1.5)]
    #[serde(default = "Sc2Config::__sdef_pad_origin_stretch_max_gain")]
    #[setting(
        label = "Stretch limit",
        explain = "Caps how much the short direction of the pad can speed up. 1 turns that extra speed-up off.",
        range = 1.0..=3.0,
        step = 0.05,
        decimals = 2,
        advanced
    )]
    pub pad_origin_stretch_max_gain: f32,

    /// Wait this long after touch-down before capturing origin (skip contact spike).
    #[config(default = 20)]
    #[serde(default = "Sc2Config::__sdef_pad_origin_settle_ms")]
    #[setting(
        label = "Touch settle time",
        explain = "How long to wait after your thumb lands before reading the touch. Raise it if the first key picked is jumpy.",
        range = 0..=100,
        step = 5,
        unit = "ms",
        advanced
    )]
    pub pad_origin_settle_ms: u64,

    #[config(default = HapticIntensity::Low)]
    #[serde(default = "Sc2Config::__sdef_touchpad_left_haptic")]
    #[setting(
        label = "Pad click",
        explain = "How hard the pads click when you press them. Off is silent. Both pads use this level.",
        mirror = touchpad_right_haptic
    )]
    pub touchpad_left_haptic: HapticIntensity,

    #[config(default = HapticIntensity::Low)]
    #[serde(default = "Sc2Config::__sdef_touchpad_right_haptic")]
    pub touchpad_right_haptic: HapticIntensity,
}

#[derive(Debug, Deserialize, Serialize, Clone, ConfigSection)]
pub struct Ps4Config {
    #[serde(default)]
    #[setting(section, page = Device(Ps4, Stick))]
    pub stick: AimProfile,

    #[serde(flatten, default)]
    #[setting(section, page = Device(Ps4, Triggers))]
    pub triggers: TriggerThresholds,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, ConfigSection)]
pub struct BatteryConfig {
    /// Draw the battery slot with the same filled key background.
    #[serde(default)]
    #[setting(
        label = "Battery as key",
        explain = "Draw the battery readout with the same background as the keys around it."
    )]
    pub draw_button: bool,
    #[config(default = [220, 50, 50, 255])]
    #[serde(default = "BatteryConfig::__sdef_empty")]
    pub empty: [u8; 4],
    #[config(default = [230, 140, 40, 255])]
    #[serde(default = "BatteryConfig::__sdef_low")]
    pub low: [u8; 4],
    #[config(default = [230, 200, 60, 255])]
    #[serde(default = "BatteryConfig::__sdef_medium")]
    pub medium: [u8; 4],
    #[config(default = [120, 190, 80, 255])]
    #[serde(default = "BatteryConfig::__sdef_high")]
    pub high: [u8; 4],
    #[config(default = [50, 200, 90, 255])]
    #[serde(default = "BatteryConfig::__sdef_full")]
    pub full: [u8; 4],
    #[config(default = [70, 180, 220, 255])]
    #[serde(default = "BatteryConfig::__sdef_charging")]
    pub charging: [u8; 4],
    #[config(default = [180, 180, 180, 255])]
    #[serde(default = "BatteryConfig::__sdef_unknown")]
    pub unknown: [u8; 4],
}

#[derive(Debug, Deserialize, Serialize, Clone, ConfigSection)]
pub struct ReplayConfig {
    /// Path to a `.krec` tape. Relative paths are against the config file directory.
    #[config(default = Some("captures/kosk-000.krec".to_owned()))]
    #[serde(default = "ReplayConfig::__sdef_file")]
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

/// One-time tutorial cards. kosk marks each shown after acknowledgement.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, ConfigSection)]
pub struct TipsConfig {
    #[serde(default)]
    pub completion_next_word_setup_shown: bool,
}

/// Pad haptic tick strength. `none` skips the HID pulse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Choice)]
#[serde(rename_all = "lowercase")]
pub enum HapticIntensity {
    #[default]
    #[choice(label = "Off")]
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

static REPLAY_AIM_FAMILY: OnceLock<Mutex<Option<crate::controller::ControllerKind>>> =
    OnceLock::new();

fn replay_aim_family_slot() -> &'static Mutex<Option<crate::controller::ControllerKind>> {
    REPLAY_AIM_FAMILY.get_or_init(|| Mutex::new(None))
}

/// Family stored on the tape being replayed. `None` means old tapes and live play.
pub fn set_replay_aim_family(kind: Option<crate::controller::ControllerKind>) {
    let kind = kind.filter(|kind| {
        matches!(
            kind,
            crate::controller::ControllerKind::Sc2 | crate::controller::ControllerKind::Ps4
        )
    });
    *replay_aim_family_slot().lock().unwrap() = kind;
}

pub fn replay_aim_family() -> Option<crate::controller::ControllerKind> {
    *replay_aim_family_slot().lock().unwrap()
}

/// Replay with no stored family uses Steam Controller 2.
pub fn resolved_controller(
    kind: crate::controller::ControllerKind,
) -> crate::controller::ControllerKind {
    use crate::controller::ControllerKind;
    match kind {
        ControllerKind::Replay => replay_aim_family().unwrap_or(ControllerKind::Sc2),
        other => other,
    }
}

impl Config {
    pub fn aim(&self, kind: crate::controller::ControllerKind, surface: AimSurface) -> &AimProfile {
        use crate::controller::ControllerKind;
        match resolved_controller(kind) {
            ControllerKind::Ps4 => &self.ps4.stick,
            ControllerKind::Sc2 | ControllerKind::Replay => match surface {
                AimSurface::Pad => &self.sc2.pad,
                AimSurface::Stick => &self.sc2.stick,
            },
        }
    }
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
/// Unix seconds of the last published load. Debounces watcher bursts.
static LAST_LOAD: AtomicU32 = AtomicU32::new(0);
static DISK_CONFIG: OnceLock<Mutex<Config>> = OnceLock::new();
/// Relative path string from `controller_map = "…"` on the last successful load, or `None` if inline/absent.
static CONTROLLER_MAP_FILE: OnceLock<Mutex<Option<String>>> = OnceLock::new();
static TAPE_OVERLAY_ACTIVE: AtomicBool = AtomicBool::new(false);
static CLI_REPLAY: OnceLock<Option<PathBuf>> = OnceLock::new();
static CLI_MCP_CONTROLLER: OnceLock<bool> = OnceLock::new();
static CLI_AT_MOUSE: OnceLock<bool> = OnceLock::new();
static CLI_KEYS_LOG: OnceLock<Option<PathBuf>> = OnceLock::new();
static CLI_IGNORE_RECORDED_CONFIG: OnceLock<bool> = OnceLock::new();
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConfigSource {
    /// `%LOCALAPPDATA%\kosk\config.toml`. Saves and migrations are written here.
    User,
    /// Path given on the command line. Never written, even if it is the user file.
    Explicit,
}

static CONFIG_SOURCE: OnceLock<ConfigSource> = OnceLock::new();
/// Startup could not read the user file, so live settings are the built-in defaults.
static USER_CONFIG_UNREADABLE: AtomicBool = AtomicBool::new(false);

fn config_source() -> ConfigSource {
    CONFIG_SOURCE
        .get()
        .copied()
        .unwrap_or(ConfigSource::Explicit)
}

pub(crate) fn uses_user_config() -> bool {
    config_source() == ConfigSource::User
}

pub(crate) fn user_config_unreadable() -> bool {
    USER_CONFIG_UNREADABLE.load(Ordering::Relaxed)
}

fn set_user_config_unreadable(unreadable: bool) {
    USER_CONFIG_UNREADABLE.store(unreadable, Ordering::Relaxed);
}

const DEFAULT_CONFIG_EDITOR: &str = "notepad";

/// `EDITOR` when set, otherwise Notepad. Extra words in `EDITOR` are arguments.
fn editor_invocation(editor: Option<&str>) -> (String, Vec<String>) {
    let mut parts = editor.unwrap_or("").split_whitespace();
    let program = parts
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or(DEFAULT_CONFIG_EDITOR);
    (program.to_owned(), parts.map(str::to_owned).collect())
}

/// Open `%LOCALAPPDATA%\kosk\config.toml` in a text editor, creating it when absent.
/// A config path on the command line does nothing. Returns whether it acted.
pub(crate) fn open_user_config_in_editor() -> bool {
    open_user_config_for_source(config_source())
}

fn open_user_config_for_source(source: ConfigSource) -> bool {
    if source != ConfigSource::User {
        return false;
    }
    let path = match crate::config_overlay::ensure_user_config_file() {
        Ok(path) => path,
        Err(_) => {
            crate::user_notify::notify(crate::user_notify::Notice::open_config_failed());
            return false;
        }
    };
    let (program, args) = editor_invocation(std::env::var("EDITOR").ok().as_deref());
    if std::process::Command::new(&program)
        .args(&args)
        .arg(&path)
        .spawn()
        .is_err()
    {
        crate::user_notify::notify(crate::user_notify::Notice::open_config_failed());
    }
    true
}

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
    "tips",
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
fn format_config_float(path: &str, value: f64) -> String {
    if let Some(digits) = save_decimals(path) {
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

/// Checks a freshly read config. Fails hard: the watcher keeps the previous
/// config, startup falls back to the built-in defaults.
fn validate_loaded_config(new_config: &Config, config_path: &Path) -> Result<()> {
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

    Ok(())
}

fn now_secs() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32
}

/// Publish a loaded config and return its watched files.
fn publish_loaded_config(
    new_config: Config,
    map_rel: Option<String>,
    config_path: &Path,
) -> Vec<PathBuf> {
    store_controller_map_file(map_rel.clone());
    if let Some(instance) = CONFIG_INSTANCE.get() {
        *instance.lock().unwrap() = new_config.clone();
    } else {
        CONFIG_INSTANCE
            .set(Arc::new(Mutex::new(new_config.clone())))
            .expect("Config was already initialized");
    }
    store_disk_config(new_config.clone());
    LAST_LOAD.store(now_secs(), Ordering::Relaxed);

    let layouts: Vec<(String, String)> = new_config.layouts.into_iter().collect();
    crate::config_overlay::config_file_paths(config_path, map_rel.as_deref(), &layouts)
}

/// Load the built-in defaults, then the active config file on top.
/// Returns the files that define this config (see [`crate::config_overlay::config_file_paths`]).
fn load_config() -> Result<Vec<PathBuf>> {
    let config_path = CONFIG_PATH
        .get()
        .ok_or(anyhow::anyhow!("Config path not set"))?;

    let now = now_secs();
    let last = LAST_LOAD.load(Ordering::Relaxed);
    if last > 0 && now.wrapping_sub(last) < 1 {
        return Err(anyhow::anyhow!("Reload debounced"));
    }

    let (new_config, map_rel) = read_merged_config(config_path, config_source())?;
    validate_loaded_config(&new_config, config_path)?;
    set_user_config_unreadable(false);

    Ok(publish_loaded_config(new_config, map_rel, config_path))
}

/// Read `config_path`, migrate it, and merge it onto the built-in defaults.
/// A migration is written back only for [`ConfigSource::User`].
/// Returns the config and the `controller_map` file name, if any.
fn read_merged_config(
    config_path: &Path,
    source: ConfigSource,
) -> Result<(Config, Option<String>)> {
    let user_text = fs::read_to_string(config_path)
        .with_context(|| format!("Could not read {}", config_path.display()))?;
    let mut user_doc = user_text
        .parse::<toml_edit::DocumentMut>()
        .context("Could not parse config TOML")?;
    match crate::config_overlay::migrate_document(&mut user_doc)? {
        crate::config_overlay::MigrateOutcome::Newer => {
            crate::user_notify::notify(crate::user_notify::Notice::newer_settings(
                crate::config_overlay::file_version(&user_doc),
            ));
        }
        crate::config_overlay::MigrateOutcome::Migrated if source == ConfigSource::User => {
            fs::write(config_path, user_doc.to_string())
                .with_context(|| format!("Could not write {}", config_path.display()))?;
        }
        crate::config_overlay::MigrateOutcome::Migrated
        | crate::config_overlay::MigrateOutcome::Unchanged => {}
    }

    let user_value: toml::Value = user_doc
        .to_string()
        .parse()
        .context("Could not parse config TOML")?;
    let mut merged: toml::Value =
        toml::Value::try_from(Config::default()).context("built-in config")?;
    let map_rel = controller_map_rel(&user_value);
    crate::config_overlay::merge_toml(&mut merged, &user_value);

    let mut mappings: toml::Value = crate::config_overlay::builtin_mappings_toml()
        .parse()
        .context("built-in mappings")?;
    if let (Some(dir), Some(rel)) = (config_path.parent(), &map_rel) {
        if let Some(over) = crate::config_overlay::read_user_mappings(dir, rel)? {
            crate::config_overlay::merge_mappings(&mut mappings, &over);
        }
    }
    if let Some(inline) = user_value.get(CONTROLLER_MAP_KEY).filter(|v| v.is_table()) {
        crate::config_overlay::merge_mappings(&mut mappings, inline);
    }
    if let Some(table) = merged.as_table_mut() {
        table.insert(CONTROLLER_MAP_KEY.to_owned(), mappings);
    }

    let config: Config = merged
        .try_into()
        .map_err(|e| anyhow::anyhow!("Could not parse config TOML: {e}"))?;
    Ok((config, map_rel))
}

fn controller_map_rel(user: &toml::Value) -> Option<String> {
    match user.get(CONTROLLER_MAP_KEY) {
        Some(toml::Value::String(path)) => Some(path.clone()),
        Some(_) => None,
        None => Some(DEFAULT_MAPPINGS_FILE.to_owned()),
    }
}

/// Layout TOML beside the active config file, then the built-in copy.
pub(crate) fn read_layout_source(rel: &str) -> Result<String> {
    crate::config_overlay::read_layout(config_dir().as_deref(), rel)
}

/// `config_files` are [`crate::config_overlay::watch_key`]s; event paths from the OS may not be.
fn is_watched_path(path: &Path, config_files: &[PathBuf]) -> bool {
    config_files.contains(&crate::config_overlay::watch_key(path))
}

/// Watch the config directory, and config files outside it. Reload only for `config_files`.
fn start_watcher_thread(config_path: PathBuf, config_files: Vec<PathBuf>) -> Result<()> {
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
        for path in &config_files {
            if path.parent() == Some(config_dir.as_path()) || !path.is_file() {
                continue;
            }
            if let Err(e) = watcher.watch(path, notify::RecursiveMode::NonRecursive) {
                eprintln!("Failed to watch config file {}: {e}", path.display());
            }
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
                    let relevant = event
                        .paths
                        .iter()
                        .any(|p| is_watched_path(p, &config_files));
                    if !relevant {
                        continue;
                    }
                    match load_config() {
                        Ok(new_files) => {
                            notify_config_changed();
                            if new_files == config_files {
                                continue;
                            }
                            match start_watcher_thread(config_path.clone(), new_files) {
                                Ok(()) => break,
                                Err(e) => eprintln!("Failed to restart config watcher: {e}"),
                            }
                        }
                        Err(e) => {
                            if e.to_string() != "Reload debounced" {
                                eprintln!("Failed to reload config: {e}");
                                crate::user_notify::notify(
                                    crate::user_notify::Notice::reload_failed(),
                                );
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
    let (path, source) = match &args.config_path {
        Some(path) => (PathBuf::from(path), ConfigSource::Explicit),
        None => (
            crate::config_overlay::ensure_user_config_file()?,
            ConfigSource::User,
        ),
    };
    CONFIG_SOURCE
        .set(source)
        .expect("config source was already set");
    init_from_path(path)?;
    if mcp_controller_mode() && preferred_is_replay() {
        bail!(
            "mcp-controller mode cannot be combined with replay (--replay / preferred_controller=replay)"
        );
    }
    Ok(())
}

/// Startup load that never fails hard. An unusable file posts a notice and
/// yields the built-in defaults. Saves wait until a later load succeeds.
/// Returns the config, the `controller_map` file name, and whether defaults won.
fn load_initial(config_path: &Path, source: ConfigSource) -> (Config, Option<String>, bool) {
    let loaded = read_merged_config(config_path, source).and_then(|(cfg, map_rel)| {
        validate_loaded_config(&cfg, config_path)?;
        Ok((cfg, map_rel))
    });
    match loaded {
        Ok((cfg, map_rel)) => (cfg, map_rel, false),
        Err(e) => {
            eprintln!("Failed to load config (using defaults): {e:#}");
            crate::user_notify::notify(crate::user_notify::Notice::load_failed());
            let builtin = builtin_merged_config().expect("built-in config is valid");
            (builtin, builtin_controller_map_rel(), true)
        }
    }
}

/// The built-in `controller_map` file name, as a fallback watch target.
fn builtin_controller_map_rel() -> Option<String> {
    Some(DEFAULT_MAPPINGS_FILE.to_owned())
}

/// Load config from `config_path` without parsing process args (for auxiliary binaries).
pub fn init_from_path(config_path: PathBuf) -> Result<()> {
    CONFIG_PATH
        .set(config_path.clone())
        .expect("Config path was already set");
    let _ = CONFIG_SOURCE.set(ConfigSource::Explicit);

    let (new_config, map_rel, used_defaults) = load_initial(&config_path, config_source());
    set_user_config_unreadable(used_defaults);
    let config_files = publish_loaded_config(new_config, map_rel, &config_path);

    let skip_watcher = preferred_is_replay();
    if !skip_watcher {
        start_watcher_thread(config_path, config_files)?;
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
    let mut cfg = Config::default();
    let mappings: toml::Value = crate::config_overlay::builtin_mappings_toml()
        .parse()
        .context("built-in mappings")?;
    cfg.controller_map = mappings
        .try_into()
        .map_err(|e| anyhow::anyhow!("built-in mappings: {e}"))?;
    Ok(cfg)
}

/// Write `new` into the user file as values that differ from the built-in default.
fn write_user_overlay(path: &Path, new: &Config, mappings_file: Option<&str>) -> Result<()> {
    let builtin = builtin_merged_config()?;
    let mut default_val = toml::Value::try_from(&builtin).context("serialize built-in config")?;
    let mut new_val = toml::Value::try_from(new).context("serialize config")?;
    if let Some(table) = default_val.as_table_mut() {
        table.remove(CONTROLLER_MAP_KEY);
    }
    if let Some(table) = new_val.as_table_mut() {
        table.remove(CONTROLLER_MAP_KEY);
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
    let kept_version =
        crate::config_overlay::file_version(&doc).max(crate::config_overlay::CONFIG_VERSION);
    doc[CONFIG_VERSION_KEY] = toml_edit::value(kept_version);
    fs::write(path, doc.to_string())
        .with_context(|| format!("Could not write {}", path.display()))?;

    if let Some(rel) = mappings_file {
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
        if key == CONFIG_VERSION_KEY {
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
    write_if_user_config(path, &persist)?;
    *disk.lock().unwrap() = persist;
    Ok(())
}

fn write_if_user_config(path: &Path, cfg: &Config) -> Result<()> {
    write_for_source(config_source(), path, cfg, controller_map_file().as_deref())
}

/// The setup tip may be acknowledged only when that acknowledgement can be stored.
fn can_persist_tips_when(user_file: bool, unreadable: bool, replay: bool, mcp: bool) -> bool {
    user_file && !unreadable && !replay && !mcp
}

pub(crate) fn can_persist_tips() -> bool {
    can_persist_tips_when(
        uses_user_config(),
        user_config_unreadable(),
        preferred_is_replay(),
        mcp_controller_mode(),
    )
}

fn write_user_file(
    source: ConfigSource,
    unreadable: bool,
    path: &Path,
    cfg: &Config,
    mappings_file: Option<&str>,
) -> Result<()> {
    if source != ConfigSource::User {
        crate::user_notify::notify(crate::user_notify::Notice::not_saved());
        return Ok(());
    }
    if unreadable {
        return Ok(());
    }
    write_user_overlay(path, cfg, mappings_file)
}

fn write_for_source(
    source: ConfigSource,
    path: &Path,
    cfg: &Config,
    mappings_file: Option<&str>,
) -> Result<()> {
    write_user_file(source, user_config_unreadable(), path, cfg, mappings_file)
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
        write_if_user_config(path, &persist)?;
        *disk.lock().unwrap() = persist;
        if let Some(instance) = CONFIG_INSTANCE.get() {
            instance.lock().unwrap().window_pos = new_config.window_pos;
        }
        return Ok(());
    }

    if DISK_CONFIG.get().is_none() {
        bail!("disk config is not initialized");
    }
    write_if_user_config(path, &new_config)?;

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
    fn explicit_config_skips_open_in_editor() {
        assert!(!open_user_config_for_source(ConfigSource::Explicit));
    }

    #[test]
    fn editor_falls_back_to_notepad() {
        assert_eq!(editor_invocation(None).0, "notepad");
        assert_eq!(editor_invocation(Some("  ")).0, "notepad");
        let (program, args) = editor_invocation(Some("code --wait"));
        assert_eq!(program, "code");
        assert_eq!(args, ["--wait"]);
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
    fn parse_without_config_path() {
        let args = Args::try_parse_from(["kosk"]).unwrap();
        assert!(args.config_path.is_none());
    }

    #[test]
    fn overlay_save_keeps_only_differences() {
        let dir = temp_dir("overlay-save");
        let config_path = dir.join("config.toml");
        fs::write(&config_path, "config_version = 1\n").unwrap();

        let mut cfg = builtin_merged_config().unwrap();
        cfg.keyboard_opacity = 0.7;
        cfg.completion.ui.columns = 4;
        write_user_overlay(&config_path, &cfg, Some("mappings.toml")).unwrap();

        let text = fs::read_to_string(&config_path).unwrap();
        assert!(text.contains("config_version = 2"), "{text}");
        assert!(text.contains("keyboard_opacity = 0.70"), "{text}");
        assert!(text.contains("columns = 4"), "{text}");
        assert!(!text.contains("scale_x"), "{text}");
        assert!(!dir.join("mappings.toml").exists());

        cfg.keyboard_opacity = builtin_merged_config().unwrap().keyboard_opacity;
        write_user_overlay(&config_path, &cfg, Some("mappings.toml")).unwrap();
        let text = fs::read_to_string(&config_path).unwrap();
        assert!(!text.contains("keyboard_opacity"), "{text}");
        assert!(text.contains("columns = 4"), "{text}");
    }

    #[test]
    fn overlay_save_unbinds_default_mapping() {
        let dir = temp_dir("overlay-unbind");
        let config_path = dir.join("config.toml");
        fs::write(&config_path, "config_version = 1\n").unwrap();

        let mut cfg = builtin_merged_config().unwrap();
        cfg.controller_map
            .get_mut(&StateId::Keyboard)
            .unwrap()
            .remove(&ControllerBinding::from(
                crate::controller::ControllerButton::FaceTop,
            ));
        write_user_overlay(&config_path, &cfg, Some("mappings.toml")).unwrap();

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
        assert_eq!(merged.config_version, crate::config_overlay::CONFIG_VERSION);
    }

    #[test]
    fn overlay_save_keeps_comments_floats_and_map_path() {
        let dir = temp_dir("overlay-keep");
        let config_path = dir.join("config.toml");
        fs::write(
            &config_path,
            "\
# keep-me\n\
config_version = 1\n\
controller_map = \"custom.toml\"\n\
future_key = true\n\
",
        )
        .unwrap();
        let mut cfg = builtin_merged_config().unwrap();
        cfg.keyboard_opacity = 0.7;
        cfg.ui_opacity = 0.95;
        write_user_overlay(&config_path, &cfg, Some("mappings.toml")).unwrap();
        let text = fs::read_to_string(&config_path).unwrap();
        assert!(text.contains("# keep-me"), "{text}");
        assert!(text.contains("controller_map = \"custom.toml\""), "{text}");
        assert!(text.contains("future_key = true"), "{text}");
        assert!(text.contains("keyboard_opacity = 0.70"), "{text}");
        assert!(text.contains("ui_opacity = 0.95"), "{text}");
        assert!(!text.contains("699999"), "{text}");
    }

    #[test]
    fn overlay_save_keeps_newer_version_and_debug_off() {
        let dir = temp_dir("overlay-newer");
        let config_path = dir.join("config.toml");
        fs::write(
            &config_path,
            "\
config_version = 9\n\
\n\
[debug]\n\
show_stick_cursors = false\n\
",
        )
        .unwrap();
        let mut cfg = builtin_merged_config().unwrap();
        cfg.debug = Debug {
            show_stick_cursors: false,
            ..Debug::default()
        };
        write_user_overlay(&config_path, &cfg, Some("mappings.toml")).unwrap();
        let text = fs::read_to_string(&config_path).unwrap();
        assert!(text.contains("config_version = 9"), "{text}");
        assert!(text.contains("show_stick_cursors = false"), "{text}");
    }

    #[test]
    fn unreadable_config_hides_setup_tip_and_skips_save() {
        assert!(!can_persist_tips_when(true, true, false, false));
        assert!(can_persist_tips_when(true, false, false, false));

        let dir = temp_dir("unreadable-save");
        let config_path = dir.join("config.toml");
        let original = "keyboard_opacity = \"abc\"\n";
        fs::write(&config_path, original).unwrap();
        let cfg = builtin_merged_config().unwrap();
        write_user_file(ConfigSource::User, true, &config_path, &cfg, None).unwrap();
        assert_eq!(fs::read_to_string(&config_path).unwrap(), original);
    }

    #[test]
    fn explicit_save_does_not_touch_the_file() {
        let dir = temp_dir("explicit-save");
        let config_path = dir.join("config.toml");
        let original = "config_version = 0\nkeyboard_opacity = 0.2\n";
        fs::write(&config_path, original).unwrap();
        let cfg = builtin_merged_config().unwrap();
        write_for_source(
            ConfigSource::Explicit,
            &config_path,
            &cfg,
            Some("mappings.toml"),
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&config_path).unwrap(), original);
        assert!(!dir.join("mappings.toml").exists());
    }

    #[test]
    fn explicit_load_does_not_write_migration() {
        let dir = temp_dir("explicit-migrate");
        let config_path = dir.join("config.toml");
        let original = "keyboard_opacity = 0.2\n";
        fs::write(&config_path, original).unwrap();
        let (cfg, _) = read_merged_config(&config_path, ConfigSource::Explicit).unwrap();
        assert_eq!(cfg.config_version, crate::config_overlay::CONFIG_VERSION);
        assert_eq!(fs::read_to_string(&config_path).unwrap(), original);
    }

    #[test]
    fn user_load_writes_migration() {
        let dir = temp_dir("user-migrate");
        let config_path = dir.join("config.toml");
        fs::write(&config_path, "keyboard_opacity = 0.2\n").unwrap();
        read_merged_config(&config_path, ConfigSource::User).unwrap();
        let text = fs::read_to_string(&config_path).unwrap();
        assert!(text.contains("config_version = 2"), "{text}");
        assert!(text.contains("keyboard_opacity = 0.2"), "{text}");
    }

    #[test]
    fn explicit_config_uses_only_mappings_beside_it() {
        let dir = temp_dir("explicit-mappings");
        let config_path = dir.join("config.toml");
        fs::write(&config_path, "config_version = 1\n").unwrap();
        fs::write(
            dir.join("mappings.toml"),
            "[Keyboard]\n\"faceTop\" = \"toggleCtrl\"\n",
        )
        .unwrap();
        let (cfg, map_rel) = read_merged_config(&config_path, ConfigSource::Explicit).unwrap();
        assert_eq!(map_rel.as_deref(), Some("mappings.toml"));
        let face_top = ControllerBinding::from(crate::controller::ControllerButton::FaceTop);
        assert_eq!(
            cfg.controller_map[&StateId::Keyboard][&face_top],
            MappingValue::Action("toggleCtrl".into())
        );

        let other = temp_dir("explicit-no-mappings");
        let other_path = other.join("config.toml");
        fs::write(&other_path, "config_version = 1\n").unwrap();
        let (cfg, _) = read_merged_config(&other_path, ConfigSource::Explicit).unwrap();
        assert_eq!(
            cfg.controller_map[&StateId::Keyboard][&face_top],
            MappingValue::Action("toggleShift".into())
        );
    }

    #[test]
    fn newer_version_message_is_user_facing() {
        let message = crate::config_overlay::newer_version_message(99);
        assert!(message.contains("99"), "{message}");
        assert!(!message.contains("config_version"), "{message}");
    }

    #[test]
    fn newer_version_loads_unchanged() {
        let dir = temp_dir("newer-notify");
        let config_path = dir.join("config.toml");
        let original = "config_version = 99\nfuture_key = 1\n";
        fs::write(&config_path, original).unwrap();
        let (cfg, _) = read_merged_config(&config_path, ConfigSource::User).unwrap();
        assert_eq!(cfg.config_version, 99);
        assert_eq!(fs::read_to_string(&config_path).unwrap(), original);
    }

    #[test]
    fn invalid_config_falls_back_to_defaults() {
        let dir = temp_dir("invalid-fallback");
        let config_path = dir.join("config.toml");
        fs::write(&config_path, "keyboard_opacity = \"abc\"\n").unwrap();
        let (cfg, _, used_defaults) = super::load_initial(&config_path, ConfigSource::Explicit);
        assert!(used_defaults);
        assert_eq!(
            cfg.keyboard_opacity,
            super::builtin_merged_config().unwrap().keyboard_opacity
        );
    }

    #[test]
    fn tips_flag_round_trips() {
        let dir = temp_dir("tips-flag");
        let config_path = dir.join("config.toml");
        fs::write(&config_path, "config_version = 1\n").unwrap();
        let (cfg, _) = read_merged_config(&config_path, ConfigSource::User).unwrap();
        assert!(!cfg.tips.completion_next_word_setup_shown);

        let mut shown = cfg.clone();
        shown.tips.completion_next_word_setup_shown = true;
        write_user_overlay(&config_path, &shown, None).unwrap();
        let text = fs::read_to_string(&config_path).unwrap();
        assert!(
            text.contains("completion_next_word_setup_shown = true"),
            "{text}"
        );

        let (reloaded, _) = read_merged_config(&config_path, ConfigSource::User).unwrap();
        assert!(reloaded.tips.completion_next_word_setup_shown);
    }

    fn watched_files(dir: &Path, external_layout: &Path) -> Vec<PathBuf> {
        let layouts = vec![
            ("main".to_owned(), "layout.toml".to_owned()),
            (
                "symbols".to_owned(),
                external_layout.to_string_lossy().into_owned(),
            ),
        ];
        crate::config_overlay::config_file_paths(
            &dir.join("config.toml"),
            Some("mappings.toml"),
            &layouts,
        )
    }

    #[test]
    fn watched_path_matches_non_canonical_event_path() {
        let dir = temp_dir("watch-path");
        let other = temp_dir("watch-other");
        let external = other.join("symbols.toml");
        fs::write(&external, "x\n").unwrap();
        let files = watched_files(&dir, &external);

        assert!(is_watched_path(&dir.join("config.toml"), &files));
        assert!(is_watched_path(&dir.join("mappings.toml"), &files));
        assert!(is_watched_path(&dir.join("layout.toml"), &files));
        assert!(is_watched_path(&dir.join(".").join("layout.toml"), &files));
        assert!(is_watched_path(&external, &files));
        assert!(!is_watched_path(&other.join("other.toml"), &files));
    }

    #[test]
    fn watcher_ignores_completion_cache() {
        let dir = temp_dir("watch-cache");
        let cache = dir.join("completion-cache.bin");
        fs::write(&cache, "x").unwrap();
        let files = watched_files(&dir, &dir.join("symbols.toml"));

        assert!(!is_watched_path(&cache, &files));
        assert!(!is_watched_path(
            &dir.join("completion-cache.bin.tmp"),
            &files
        ));
    }

    #[test]
    fn user_can_turn_default_stick_cursors_off() {
        let mut merged: toml::Value = toml::Value::try_from(Config::default()).unwrap();
        let over: toml::Value = toml::from_str("[debug]\nshow_stick_cursors = false\n").unwrap();
        crate::config_overlay::merge_toml(&mut merged, &over);
        let mappings: toml::Value = crate::config_overlay::builtin_mappings_toml()
            .parse()
            .unwrap();
        merged
            .as_table_mut()
            .unwrap()
            .insert(CONTROLLER_MAP_KEY.into(), mappings);
        let cfg: Config = merged.try_into().unwrap();
        assert!(!cfg.debug.show_stick_cursors);
    }

    #[test]
    fn stepped_float_keeps_two_decimals_through_save() {
        let dir = temp_dir("float-precision");
        let config_path = dir.join("config.toml");
        fs::write(&config_path, "config_version = 1\n").unwrap();

        let mut cfg = builtin_merged_config().unwrap();
        cfg.completion.ngram.lambda_trigram = 0.35;
        cfg.completion.ngram.backoff_alpha = 0.35;
        cfg.sc2.pad_origin_stretch_max_gain = 1.35;
        write_user_overlay(&config_path, &cfg, None).unwrap();

        let text = fs::read_to_string(&config_path).unwrap();
        assert!(text.contains("lambda_trigram = 0.35"), "{text}");
        assert!(text.contains("backoff_alpha = 0.35"), "{text}");
        assert!(
            text.contains("pad_origin_stretch_max_gain = 1.35"),
            "{text}"
        );
    }

    /// The derived `Default` is the built-in base layer: it must round-trip
    /// through TOML exactly (`f32` values included). Formerly this compared
    /// against the deleted `config.toml`; the defaults now live on the fields.
    #[test]
    fn struct_defaults_match_builtin_toml() {
        let value = toml::Value::try_from(Config::default()).unwrap();
        let back: Config = value.try_into().unwrap();
        let again = toml::Value::try_from(&back).unwrap();
        let first = toml::Value::try_from(Config::default()).unwrap();
        assert_eq!(again, first);
        assert_eq!(back.completion.max_suggestions, 6);
        assert!((back.keyboard_opacity - 1.0).abs() < f32::EPSILON);
        assert_eq!(
            back.layouts.get("main").map(String::as_str),
            Some("old_sc.toml")
        );
    }
}

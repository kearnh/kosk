use std::fmt::{self, Debug};
use std::str::FromStr;

use crate::config;

pub mod bindings;
pub mod control_server;
pub mod mapping;
pub mod pad_origin;
pub mod ps4;
pub mod record;
pub mod replay;
pub mod sc2;
#[cfg(test)]
pub(crate) mod test_input;
pub mod virtual_ctl;

use hidapi::HidApi;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StickSide {
    Left,
    Right,
}

impl StickSide {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "left" | "l" => Ok(Self::Left),
            "right" | "r" => Ok(Self::Right),
            other => Err(format!(
                "unknown stick/pad side '{other}' (expected left|right)"
            )),
        }
    }
}

pub(crate) fn warp((mut x, mut y): (f32, f32), warp: f32) -> (f32, f32) {
    if warp > 0.0 {
        let u2 = x * x;
        let v2 = y * y;
        let offset = (u2 + v2).sqrt();
        if offset > 0.001 {
            // Determine how much to scale based on the warp factor
            // At warp=1.0, this pushes the circle out to fill the square corners.
            let scale = (offset / (x.abs().max(y.abs()))).powf(warp);
            x *= scale;
            y *= scale;
        }
    }

    // Clip to square bounds
    x = x.clamp(-1.0, 1.0);
    y = y.clamp(-1.0, 1.0);

    (x, y)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryStatus {
    pub percent: u8,
    pub charging: bool,
}

#[allow(unused)]
pub trait ControllerInput: Debug {
    fn left_stick_raw(&self) -> (f32, f32);
    fn right_stick_raw(&self) -> (f32, f32);

    /// Analog stick after circle-to-square `stick_warp` only (no pad-origin stretch).
    // FIXME can we cache stick_warp somehow so we don't have to constantly lock config mutex?
    fn left_stick(&self) -> (f32, f32) {
        warp(self.left_stick_raw(), config::get().stick_warp)
    }
    fn right_stick(&self) -> (f32, f32) {
        warp(self.right_stick_raw(), config::get().stick_warp)
    }

    /// Trackpad sample, or `None` when the thumb is lifted. Centered touch is
    /// `Some((0.0, 0.0))`, not a lift.
    fn left_pad_raw(&self) -> Option<(f32, f32)> {
        None
    }
    fn right_pad_raw(&self) -> Option<(f32, f32)> {
        None
    }
    fn left_pad(&self) -> Option<(f32, f32)> {
        self.left_pad_raw()
            .map(|p| warp(p, config::get().stick_warp))
    }
    fn right_pad(&self) -> Option<(f32, f32)> {
        self.right_pad_raw()
            .map(|p| warp(p, config::get().stick_warp))
    }

    fn trigger_left(&self) -> Option<u8>;
    fn trigger_right(&self) -> Option<u8>;

    /// Whether `button` is held. Exhaustive over [`ControllerButton`].
    fn query(&self, button: ControllerButton) -> bool;

    /// Whether this snapshot should reach the app (vs being swallowed as idle).
    fn is_engaged(&self) -> bool;

    /// Last known battery, if this device reports it.
    fn battery(&self) -> Option<BatteryStatus> {
        None
    }

    fn family(&self) -> ControllerKind;

    fn box_clone(&self) -> Box<dyn ControllerInput + Send + Sync>;
}

/// A controller binding spec.
///
/// Serialized as a single string so it can be used as a TOML table key
/// (e.g. `faceTop`, `stickLeft`, `triggerLeft`). Names are matched
/// case-insensitively and `-` / `_` are ignored, so `stick-left`, `stick_left`,
/// and `stickLeft` all parse to the same value. Device feel (trigger threshold,
/// pad haptics) lives in `[ps4]` / `[sc2]`, not in this key.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, strum::EnumIter, strum::EnumCount, strum::VariantArray,
)]
pub enum ControllerButton {
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    FaceBottom,
    FaceRight,
    FaceLeft,
    FaceTop,
    ShoulderLeft,
    ShoulderRight,
    StickLeft,
    StickRight,
    TriggerLeft,
    TriggerRight,
    Options,
    Share,
    System,
    PadLeft,
    PadRight,
    L4,
    L5,
    R4,
    R5,
    QuickAccess,
}

/// Lower-case the input and strip `-` / `_` so `stick-left`, `stick_left`, and
/// `stickLeft` all normalize to the same identifier.
fn normalize_ident(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '-' || c == '_' {
            continue;
        }
        out.extend(c.to_lowercase());
    }
    out
}

fn no_args(name: &str, args: &[&str]) -> Result<(), String> {
    if args.is_empty() {
        Ok(())
    } else if name == "triggerLeft" || name == "triggerRight" {
        Err(format!(
            "'{}' does not take arguments (got {:?}); set trigger_left_threshold / trigger_right_threshold in [ps4] or [sc2]",
            name, args
        ))
    } else {
        Err(format!(
            "controller button '{}' does not take arguments (got {:?})",
            name, args
        ))
    }
}

impl FromStr for ControllerButton {
    type Err = String;

    fn from_str(spec: &str) -> Result<Self, Self::Err> {
        let trimmed = spec.trim();
        let (head, args) = match trimmed.split_once(',') {
            Some((h, rest)) => {
                let args: Vec<&str> = rest
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect();
                (h.trim(), args)
            }
            None => (trimmed, Vec::new()),
        };

        match normalize_ident(head).as_str() {
            "dpadup" => no_args("dpadUp", &args).map(|_| ControllerButton::DpadUp),
            "dpaddown" => no_args("dpadDown", &args).map(|_| ControllerButton::DpadDown),
            "dpadleft" => no_args("dpadLeft", &args).map(|_| ControllerButton::DpadLeft),
            "dpadright" => no_args("dpadRight", &args).map(|_| ControllerButton::DpadRight),
            "facebottom" => no_args("faceBottom", &args).map(|_| ControllerButton::FaceBottom),
            "faceright" => no_args("faceRight", &args).map(|_| ControllerButton::FaceRight),
            "faceleft" => no_args("faceLeft", &args).map(|_| ControllerButton::FaceLeft),
            "facetop" => no_args("faceTop", &args).map(|_| ControllerButton::FaceTop),
            "shoulderleft" => {
                no_args("shoulderLeft", &args).map(|_| ControllerButton::ShoulderLeft)
            }
            "shoulderright" => {
                no_args("shoulderRight", &args).map(|_| ControllerButton::ShoulderRight)
            }
            "stickleft" => no_args("stickLeft", &args).map(|_| ControllerButton::StickLeft),
            "stickright" => no_args("stickRight", &args).map(|_| ControllerButton::StickRight),
            "options" => no_args("options", &args).map(|_| ControllerButton::Options),
            "share" => no_args("share", &args).map(|_| ControllerButton::Share),
            "system" => no_args("system", &args).map(|_| ControllerButton::System),
            "padleft" => no_args("padLeft", &args).map(|_| ControllerButton::PadLeft),
            "padright" => no_args("padRight", &args).map(|_| ControllerButton::PadRight),
            "l4" => no_args("l4", &args).map(|_| ControllerButton::L4),
            "l5" => no_args("l5", &args).map(|_| ControllerButton::L5),
            "r4" => no_args("r4", &args).map(|_| ControllerButton::R4),
            "r5" => no_args("r5", &args).map(|_| ControllerButton::R5),
            "quickaccess" | "qam" => {
                no_args("quickAccess", &args).map(|_| ControllerButton::QuickAccess)
            }
            "triggerleft" => no_args("triggerLeft", &args).map(|_| ControllerButton::TriggerLeft),
            "triggerright" => {
                no_args("triggerRight", &args).map(|_| ControllerButton::TriggerRight)
            }
            _ => Err(format!("unknown controller button '{}'", spec)),
        }
    }
}

impl fmt::Display for ControllerButton {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ControllerButton::DpadUp => f.write_str("dpadUp"),
            ControllerButton::DpadDown => f.write_str("dpadDown"),
            ControllerButton::DpadLeft => f.write_str("dpadLeft"),
            ControllerButton::DpadRight => f.write_str("dpadRight"),
            ControllerButton::FaceBottom => f.write_str("faceBottom"),
            ControllerButton::FaceRight => f.write_str("faceRight"),
            ControllerButton::FaceLeft => f.write_str("faceLeft"),
            ControllerButton::FaceTop => f.write_str("faceTop"),
            ControllerButton::ShoulderLeft => f.write_str("shoulderLeft"),
            ControllerButton::ShoulderRight => f.write_str("shoulderRight"),
            ControllerButton::StickLeft => f.write_str("stickLeft"),
            ControllerButton::StickRight => f.write_str("stickRight"),
            ControllerButton::TriggerLeft => f.write_str("triggerLeft"),
            ControllerButton::TriggerRight => f.write_str("triggerRight"),
            ControllerButton::Options => f.write_str("options"),
            ControllerButton::Share => f.write_str("share"),
            ControllerButton::System => f.write_str("system"),
            ControllerButton::PadLeft => f.write_str("padLeft"),
            ControllerButton::PadRight => f.write_str("padRight"),
            ControllerButton::L4 => f.write_str("l4"),
            ControllerButton::L5 => f.write_str("l5"),
            ControllerButton::R4 => f.write_str("r4"),
            ControllerButton::R5 => f.write_str("r5"),
            ControllerButton::QuickAccess => f.write_str("quickAccess"),
        }
    }
}

impl Serialize for ControllerButton {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// A controller map key: one button or a two-button chord (`leader + follower`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ControllerBinding {
    Single(ControllerButton),
    Chord {
        leader: ControllerButton,
        follower: ControllerButton,
    },
}

impl From<ControllerButton> for ControllerBinding {
    fn from(button: ControllerButton) -> Self {
        ControllerBinding::Single(button)
    }
}

impl FromStr for ControllerBinding {
    type Err = String;

    fn from_str(spec: &str) -> Result<Self, Self::Err> {
        let trimmed = spec.trim();
        if !trimmed.contains('+') {
            return ControllerButton::from_str(trimmed).map(ControllerBinding::Single);
        }
        let parts: Vec<&str> = trimmed
            .split('+')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if parts.len() != 2 {
            return Err(format!(
                "chord must have exactly two buttons separated by '+', got {} segment(s) in '{}'",
                parts.len(),
                spec
            ));
        }
        let leader = ControllerButton::from_str(parts[0])?;
        let follower = ControllerButton::from_str(parts[1])?;
        if leader == follower {
            return Err(format!(
                "chord leader and follower must differ in '{}'",
                spec
            ));
        }
        Ok(ControllerBinding::Chord { leader, follower })
    }
}

impl fmt::Display for ControllerBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ControllerBinding::Single(b) => fmt::Display::fmt(b, f),
            ControllerBinding::Chord { leader, follower } => {
                write!(f, "{} + {}", leader, follower)
            }
        }
    }
}

impl Serialize for ControllerBinding {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ControllerBinding {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BindingVisitor;
        impl de::Visitor<'_> for BindingVisitor {
            type Value = ControllerBinding;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a controller binding like \"faceTop\" or \"options + faceTop\"")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                ControllerBinding::from_str(v).map_err(de::Error::custom)
            }
        }
        deserializer.deserialize_str(BindingVisitor)
    }
}

impl<'de> Deserialize<'de> for ControllerButton {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ButtonVisitor;
        impl de::Visitor<'_> for ButtonVisitor {
            type Value = ControllerButton;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a controller button spec like \"faceTop\" or \"triggerLeft\"")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                ControllerButton::from_str(v).map_err(de::Error::custom)
            }
        }
        deserializer.deserialize_str(ButtonVisitor)
    }
}

/// Controller family for discovery / `preferred_controller` config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ControllerKind {
    Sc2,
    Ps4,
    Replay,
}

/// Built-in try order. New families are appended here.
pub const DEFAULT_CONTROLLER_ORDER: &[ControllerKind] = &[ControllerKind::Sc2, ControllerKind::Ps4];

/// User-specified names first (deduped); omitted families follow `DEFAULT_CONTROLLER_ORDER`.
pub fn resolve_controller_order(preferred: &[ControllerKind]) -> Vec<ControllerKind> {
    let mut out = Vec::new();
    for kind in preferred {
        if !out.contains(kind) {
            out.push(*kind);
        }
    }
    for kind in DEFAULT_CONTROLLER_ORDER {
        if !out.contains(kind) {
            out.push(*kind);
        }
    }
    out
}

pub enum ConnectedController {
    Ps4(ps4::Ps4Device),
    Sc2 {
        device: sc2::Sc2Device,
        pads: pad_origin::PadOriginMapper,
    },
    Replay(replay::ReplayDevice),
    /// Exclusive virtual device for MCP-controller mode (process lifetime).
    Virtual,
}

impl Iterator for ConnectedController {
    type Item = Option<Box<dyn ControllerInput>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            ConnectedController::Ps4(device) => device.next(),
            ConnectedController::Sc2 { device, pads } => match device.next()? {
                None => {
                    pads.reset();
                    Some(None)
                }
                Some(_) => Some(Some(Box::new(pads.map(device.last_state())))),
            },
            ConnectedController::Replay(device) => device.next(),
            ConnectedController::Virtual => {
                std::thread::sleep(std::time::Duration::from_millis(4));
                let snap = virtual_ctl::session()
                    .lock()
                    .expect("virtual controller lock")
                    .snapshot();
                if snap.is_engaged() {
                    Some(Some(Box::new(snap)))
                } else {
                    Some(None)
                }
            }
        }
    }
}

impl ConnectedController {
    pub fn is_replay(&self) -> bool {
        matches!(self, ConnectedController::Replay(_))
    }

    pub fn replay_header(&self) -> Option<&record::TapeHeader> {
        match self {
            ConnectedController::Replay(d) => Some(d.header()),
            _ => None,
        }
    }
}

/// Enumerate HID, try families in resolved `preferred_controller` order, return the first open.
pub fn find_device() -> Option<ConnectedController> {
    if config::mcp_controller_mode() {
        record::session().set_replay(false);
        return Some(ConnectedController::Virtual);
    }
    let preferred = config::get().preferred_controller.clone();
    if config::preferred_is_replay() {
        record::session().set_replay(true);
        return match replay::ReplayDevice::open() {
            Ok(device) => Some(ConnectedController::Replay(device)),
            Err(e) => {
                eprintln!("replay: {e:#}");
                None
            }
        };
    }
    record::session().set_replay(false);

    let hid = HidApi::new().ok()?;
    let order = resolve_controller_order(&preferred);
    for kind in order {
        match kind {
            ControllerKind::Replay => {}
            ControllerKind::Sc2 => {
                if let Some(device) = sc2::open(&hid) {
                    return Some(ConnectedController::Sc2 {
                        device,
                        pads: pad_origin::PadOriginMapper::default(),
                    });
                }
            }
            ControllerKind::Ps4 => {
                if let Some(device) = ps4::Ps4Device::open(&hid) {
                    return Some(ConnectedController::Ps4(device));
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn parse(s: &str) -> ControllerButton {
        ControllerButton::from_str(s).expect("parse")
    }

    #[test]
    fn parses_unit_variants_case_and_kebab_insensitive() {
        assert_eq!(parse("faceTop"), ControllerButton::FaceTop);
        assert_eq!(parse("face-top"), ControllerButton::FaceTop);
        assert_eq!(parse("FACE_TOP"), ControllerButton::FaceTop);
        assert_eq!(parse("stickLeft"), ControllerButton::StickLeft);
        assert_eq!(parse("options"), ControllerButton::Options);
    }

    #[test]
    fn parses_trigger_as_unit() {
        assert_eq!(parse("triggerLeft"), ControllerButton::TriggerLeft);
        assert_eq!(parse("trigger-right"), ControllerButton::TriggerRight);
        let err = ControllerButton::from_str("triggerLeft,threshold=40").unwrap_err();
        assert!(
            err.contains("[ps4]") && err.contains("[sc2]"),
            "expected pointer to device config, got {err}"
        );
    }

    #[test]
    fn parses_dpad_unit_variants() {
        assert_eq!(parse("dpadUp"), ControllerButton::DpadUp);
        assert_eq!(parse("dpad-down"), ControllerButton::DpadDown);
        assert_eq!(parse("DPAD_LEFT"), ControllerButton::DpadLeft);
        assert_eq!(parse("dpadright"), ControllerButton::DpadRight);
    }

    #[test]
    fn round_trips_through_display() {
        let cases = [
            ControllerButton::FaceTop,
            ControllerButton::StickLeft,
            ControllerButton::TriggerLeft,
            ControllerButton::DpadUp,
            ControllerButton::DpadRight,
        ];
        for c in cases {
            assert_eq!(parse(&c.to_string()), c);
        }
    }

    #[test]
    fn rejects_unknown_and_extra_args() {
        assert!(ControllerButton::from_str("nope").is_err());
        assert!(ControllerButton::from_str("faceTop,oops=1").is_err());
        assert_eq!(
            ControllerButton::from_str("triggerLeft").unwrap(),
            ControllerButton::TriggerLeft
        );
    }

    fn parse_binding(s: &str) -> ControllerBinding {
        ControllerBinding::from_str(s).expect("parse binding")
    }

    #[test]
    fn parses_two_button_chords() {
        assert_eq!(
            parse_binding("options + faceTop"),
            ControllerBinding::Chord {
                leader: ControllerButton::Options,
                follower: ControllerButton::FaceTop,
            }
        );
        assert_eq!(
            parse_binding("face-top + OPTIONS"),
            ControllerBinding::Chord {
                leader: ControllerButton::FaceTop,
                follower: ControllerButton::Options,
            }
        );
        assert_eq!(
            parse_binding("triggerLeft + options"),
            ControllerBinding::Chord {
                leader: ControllerButton::TriggerLeft,
                follower: ControllerButton::Options,
            }
        );
    }

    #[test]
    fn rejects_invalid_chords() {
        assert!(ControllerBinding::from_str("faceTop").is_ok());
        assert!(ControllerBinding::from_str("a + b + c").is_err());
        assert!(ControllerBinding::from_str("faceTop + faceTop").is_err());
    }

    #[test]
    fn deserializes_full_mappings_toml() {
        let toml_src = r#"
[Keyboard]
"triggerLeft" = "sendKeyUnderLeftStick"
"triggerRight" = "sendKeyUnderRightStick"
"options + faceTop" = "switchState.menu"
"faceTop" = "toggleShift"
"stickLeft" = "toggleCtrl"
"stickRight" = "toggleAlt"
"faceRight" = "switchState.textInput"
"faceLeft" = "backspace"
"#;
        let map: HashMap<String, HashMap<ControllerBinding, String>> =
            toml::from_str(toml_src).expect("parse mappings");
        let kb = map.get("Keyboard").expect("keyboard section");
        assert_eq!(
            kb.get(&ControllerBinding::Single(ControllerButton::TriggerLeft)),
            Some(&"sendKeyUnderLeftStick".to_string())
        );
        assert_eq!(
            kb.get(&ControllerBinding::Single(ControllerButton::FaceTop)),
            Some(&"toggleShift".to_string())
        );
        assert_eq!(
            kb.get(&ControllerBinding::Chord {
                leader: ControllerButton::Options,
                follower: ControllerButton::FaceTop,
            }),
            Some(&"switchState.menu".to_string())
        );
    }

    #[test]
    fn parses_pad_click_buttons() {
        assert_eq!(parse("padLeft"), ControllerButton::PadLeft);
        assert_eq!(parse("pad-right"), ControllerButton::PadRight);
        assert_eq!(
            parse(&ControllerButton::PadLeft.to_string()),
            ControllerButton::PadLeft
        );
        assert_eq!(
            parse(&ControllerButton::PadRight.to_string()),
            ControllerButton::PadRight
        );
    }

    #[test]
    fn parses_paddle_buttons() {
        assert_eq!(parse("l4"), ControllerButton::L4);
        assert_eq!(parse("L5"), ControllerButton::L5);
        assert_eq!(parse("r4"), ControllerButton::R4);
        assert_eq!(parse("R5"), ControllerButton::R5);
        assert_eq!(
            parse(&ControllerButton::L4.to_string()),
            ControllerButton::L4
        );
        assert_eq!(
            parse(&ControllerButton::R5.to_string()),
            ControllerButton::R5
        );
    }

    #[test]
    fn parses_quick_access_button() {
        assert_eq!(parse("quickAccess"), ControllerButton::QuickAccess);
        assert_eq!(parse("quick-access"), ControllerButton::QuickAccess);
        assert_eq!(parse("qam"), ControllerButton::QuickAccess);
        assert_eq!(
            parse(&ControllerButton::QuickAccess.to_string()),
            ControllerButton::QuickAccess
        );
    }

    #[test]
    fn preferred_controller_fill_in() {
        assert_eq!(
            resolve_controller_order(&[]),
            vec![ControllerKind::Sc2, ControllerKind::Ps4]
        );
        assert_eq!(
            resolve_controller_order(&[ControllerKind::Ps4]),
            vec![ControllerKind::Ps4, ControllerKind::Sc2]
        );
        assert_eq!(
            resolve_controller_order(&[ControllerKind::Sc2]),
            vec![ControllerKind::Sc2, ControllerKind::Ps4]
        );
        assert_eq!(
            resolve_controller_order(&[ControllerKind::Ps4, ControllerKind::Sc2]),
            vec![ControllerKind::Ps4, ControllerKind::Sc2]
        );
        assert_eq!(
            resolve_controller_order(&[ControllerKind::Ps4, ControllerKind::Ps4]),
            vec![ControllerKind::Ps4, ControllerKind::Sc2]
        );
    }

    #[test]
    fn preferred_controller_unknown_name_errors() {
        assert!(toml::from_str::<Vec<ControllerKind>>(r#"["xbox"]"#).is_err());
    }

    #[test]
    fn preferred_controller_replay_name() {
        #[derive(Deserialize)]
        struct P {
            preferred_controller: Vec<ControllerKind>,
        }
        let parsed: P = toml::from_str(r#"preferred_controller = ["replay"]"#).unwrap();
        assert_eq!(parsed.preferred_controller, vec![ControllerKind::Replay]);
        assert_eq!(
            resolve_controller_order(&[ControllerKind::Replay]),
            vec![
                ControllerKind::Replay,
                ControllerKind::Sc2,
                ControllerKind::Ps4
            ]
        );
    }

    #[test]
    fn controller_button_variants_match_count_and_order() {
        use strum::{EnumCount, IntoEnumIterator, VariantArray};

        assert_eq!(ControllerButton::VARIANTS.len(), ControllerButton::COUNT);
        assert_eq!(
            record::BUTTON_ORDER.as_ptr(),
            ControllerButton::VARIANTS.as_ptr()
        );
        assert_eq!(record::BUTTON_ORDER.len(), ControllerButton::VARIANTS.len());
        assert!(ControllerButton::VARIANTS.contains(&ControllerButton::QuickAccess));
        assert_eq!(
            *ControllerButton::VARIANTS.last().unwrap(),
            ControllerButton::QuickAccess
        );
        let via_iter: Vec<_> = ControllerButton::iter().collect();
        assert_eq!(via_iter.as_slice(), ControllerButton::VARIANTS);
    }

    #[derive(Debug, Clone)]
    struct FaceBottomOnly;

    impl ControllerInput for FaceBottomOnly {
        fn left_stick_raw(&self) -> (f32, f32) {
            (0.0, 0.0)
        }
        fn right_stick_raw(&self) -> (f32, f32) {
            (0.0, 0.0)
        }
        fn trigger_left(&self) -> Option<u8> {
            None
        }
        fn trigger_right(&self) -> Option<u8> {
            None
        }
        fn query(&self, button: ControllerButton) -> bool {
            button == ControllerButton::FaceBottom
        }
        fn is_engaged(&self) -> bool {
            true
        }
        fn family(&self) -> ControllerKind {
            ControllerKind::Sc2
        }
        fn box_clone(&self) -> Box<dyn ControllerInput + Send + Sync> {
            Box::new(self.clone())
        }
    }

    #[test]
    fn query_round_trip_filters_held_buttons() {
        use strum::IntoEnumIterator;

        let stub = FaceBottomOnly;
        let held: Vec<_> = ControllerButton::iter()
            .filter(|&b| stub.query(b))
            .collect();
        assert_eq!(held, vec![ControllerButton::FaceBottom]);
    }
}

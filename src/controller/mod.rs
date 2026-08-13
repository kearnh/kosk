use std::fmt::{self, Debug};
use std::str::FromStr;

use crate::config;

pub mod bindings;
pub mod pad_origin;
pub mod ps4;
pub mod sc2;

use hidapi::HidApi;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

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

#[allow(unused)]
pub trait ControllerInput: Debug {
    fn left_stick_raw(&self) -> (f32, f32);
    fn right_stick_raw(&self) -> (f32, f32);

    /// Stick used by the keyboard. Default is circle-to-square `stick_warp`.
    /// SC2 OSK mapping overrides this to apply pad-origin stretch first.
    // FIXME can we cache stick_warp somehow so we don't have to constantly lock config mutex?
    fn left_stick(&self) -> (f32, f32) {
        warp(self.left_stick_raw(), config::get().stick_warp)
    }
    fn right_stick(&self) -> (f32, f32) {
        warp(self.right_stick_raw(), config::get().stick_warp)
    }

    fn dpad_up(&self) -> bool;
    fn dpad_down(&self) -> bool;
    fn dpad_left(&self) -> bool;
    fn dpad_right(&self) -> bool;
    fn face_bottom(&self) -> bool;
    fn face_right(&self) -> bool;
    fn face_top(&self) -> bool;
    fn face_left(&self) -> bool;
    fn shoulder_left(&self) -> bool;
    fn shoulder_right(&self) -> bool;
    fn stick_left(&self) -> bool;
    fn stick_right(&self) -> bool;
    fn trigger_left(&self) -> Option<u8>;
    fn trigger_right(&self) -> Option<u8>;
    fn btn_options(&self) -> bool;
    fn btn_share(&self) -> bool;
    fn btn_system(&self) -> bool;
    fn pad_left(&self) -> bool {
        false
    }
    fn pad_right(&self) -> bool {
        false
    }

    /// Whether this snapshot should reach the app (vs being swallowed as idle).
    fn is_engaged(&self) -> bool;

    fn box_clone(&self) -> Box<dyn ControllerInput + Send + Sync>;
}

/// A controller binding spec.
///
/// Serialized as a single string so it can be used as a TOML table key. The format is
/// `<variantName>` for unit variants (e.g. `faceTop`, `stickLeft`, `dpadUp`) and
/// `<variantName>,<key>=<value>[,<key>=<value>...]` for variants carrying data
/// (e.g. `triggerLeft,threshold=40`). Names are matched case-insensitively and `-` / `_`
/// are ignored, so `stick-left`, `stick_left`, and `stickLeft` all parse to the same value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
    TriggerLeft { threshold: u8 },
    TriggerRight { threshold: u8 },
    Options,
    Share,
    System,
    PadLeft,
    PadRight,
}

impl ControllerButton {
    pub(crate) fn query(&self, input: &dyn ControllerInput) -> bool {
        match self {
            ControllerButton::DpadUp => input.dpad_up(),
            ControllerButton::DpadDown => input.dpad_down(),
            ControllerButton::DpadLeft => input.dpad_left(),
            ControllerButton::DpadRight => input.dpad_right(),
            ControllerButton::FaceBottom => input.face_bottom(),
            ControllerButton::FaceRight => input.face_right(),
            ControllerButton::FaceLeft => input.face_left(),
            ControllerButton::FaceTop => input.face_top(),
            ControllerButton::ShoulderLeft => input.shoulder_left(),
            ControllerButton::ShoulderRight => input.shoulder_right(),
            ControllerButton::StickLeft => input.stick_left(),
            ControllerButton::StickRight => input.stick_right(),
            ControllerButton::TriggerLeft { threshold } => input
                .trigger_left()
                .map(|t| t >= *threshold)
                .unwrap_or(false),
            ControllerButton::TriggerRight { threshold } => input
                .trigger_right()
                .map(|t| t >= *threshold)
                .unwrap_or(false),
            ControllerButton::Options => input.btn_options(),
            ControllerButton::Share => input.btn_share(),
            ControllerButton::System => input.btn_system(),
            ControllerButton::PadLeft => input.pad_left(),
            ControllerButton::PadRight => input.pad_right(),
        }
    }
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

/// Split `"k=v"` into `(k, v)` with whitespace trimmed; returns `None` if there is no `=`.
fn split_kv(arg: &str) -> Option<(&str, &str)> {
    arg.split_once('=').map(|(k, v)| (k.trim(), v.trim()))
}

fn parse_threshold(name: &str, args: &[&str]) -> Result<u8, String> {
    let mut threshold = 40; // default threshold
    for arg in args {
        let (k, v) = split_kv(arg).ok_or_else(|| {
            format!(
                "'{}' argument must be 'threshold=<0-255>', got '{}'",
                name, arg
            )
        })?;
        match normalize_ident(k).as_str() {
            "threshold" => {
                threshold = v
                    .parse::<u8>()
                    .map_err(|e| format!("'{}': invalid threshold value '{}': {}", name, v, e))?;
            }
            _ => return Err(format!("'{}': unknown argument '{}'", name, k)),
        }
    }
    Ok(threshold)
}

fn no_args(name: &str, args: &[&str]) -> Result<(), String> {
    if args.is_empty() {
        Ok(())
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
            "triggerleft" => parse_threshold("triggerLeft", &args)
                .map(|threshold| ControllerButton::TriggerLeft { threshold }),
            "triggerright" => parse_threshold("triggerRight", &args)
                .map(|threshold| ControllerButton::TriggerRight { threshold }),
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
            ControllerButton::TriggerLeft { threshold } => {
                write!(f, "triggerLeft,threshold={}", threshold)
            }
            ControllerButton::TriggerRight { threshold } => {
                write!(f, "triggerRight,threshold={}", threshold)
            }
            ControllerButton::Options => f.write_str("options"),
            ControllerButton::Share => f.write_str("share"),
            ControllerButton::System => f.write_str("system"),
            ControllerButton::PadLeft => f.write_str("padLeft"),
            ControllerButton::PadRight => f.write_str("padRight"),
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
                f.write_str(
                    "a controller button spec like \"faceTop\" or \"triggerLeft,threshold=40\"",
                )
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
        }
    }
}

/// Enumerate HID, try families in resolved `preferred_controller` order, return the first open.
pub fn find_device() -> Option<ConnectedController> {
    let hid = HidApi::new().ok()?;
    let order = resolve_controller_order(&config::get().preferred_controller);
    for kind in order {
        match kind {
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
    fn parses_trigger_with_named_threshold() {
        assert_eq!(
            parse("triggerLeft,threshold=40"),
            ControllerButton::TriggerLeft { threshold: 40 }
        );
        assert_eq!(
            parse("triggerRight, threshold = 200"),
            ControllerButton::TriggerRight { threshold: 200 }
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
            ControllerButton::TriggerLeft { threshold: 40 },
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
            ControllerButton::TriggerLeft { threshold: 40 }
        );
        assert!(ControllerButton::from_str("triggerLeft,threshold=999").is_err());
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
            parse_binding("triggerLeft,threshold=40 + options"),
            ControllerBinding::Chord {
                leader: ControllerButton::TriggerLeft { threshold: 40 },
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
"triggerLeft,threshold=40" = "sendKeyUnderLeftStick"
"triggerRight,threshold=40" = "sendKeyUnderRightStick"
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
            kb.get(&ControllerBinding::Single(ControllerButton::TriggerLeft {
                threshold: 40
            })),
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
}

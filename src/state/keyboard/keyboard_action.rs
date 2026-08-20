use serde::Deserialize;
use std::any::Any;
use strum::VariantNames;

use crate::state::{actions::Action, actions::TriggerMode, StateId};

#[derive(Deserialize, strum::VariantNames, Eq, Hash, PartialEq, Clone, Debug)]
pub enum KeyboardAction {
    SendKeyUnderLeftStick,
    SendKeyUnderRightStick,
    SendKey(char),
    SendEnigoKey(enigo::Key),
    ToggleShift,
    ToggleCtrl,
    ToggleAlt,
    Paste,
    SwitchState(StateId),
    SwitchLayout(String),
    FlipWindowLeftRight,
    FlipWindowAboveBelow,
    RotateWindow,
    Exit,
    ToggleRecord,
}

impl Action for KeyboardAction {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn trigger_mode(&self) -> TriggerMode {
        use KeyboardAction::*;
        match self {
            SendKeyUnderLeftStick | SendKeyUnderRightStick | SendKey(_) | SendEnigoKey(_) => {
                TriggerMode::WhileHeld
            }
            ToggleShift | ToggleCtrl | ToggleAlt | Paste | SwitchState(_) | SwitchLayout(_)
            | FlipWindowLeftRight | FlipWindowAboveBelow | RotateWindow | Exit | ToggleRecord => {
                TriggerMode::Edge
            }
        }
    }
}

impl TryFrom<&str> for KeyboardAction {
    type Error = anyhow::Error;

    /// Plain-text actions: unit variants use serde_plain on the canonical name (add variants on the
    /// enum only). Tuple variants use `<Variant>.<payload>` (split on first `.`); payload parsing
    /// stays explicit below. serde_plain has no case folding—matching uses [`VariantNames`] first.
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let (head, tail_opt) = match value.split_once('.') {
            Some((h, t)) => (h, Some(t)),
            None => (value, None),
        };

        let variant = KeyboardAction::VARIANTS
            .iter()
            .find(|v| head.eq_ignore_ascii_case(v))
            .copied()
            .ok_or_else(|| anyhow::anyhow!("unknown keyboard action '{}'", value))?;

        match (variant, tail_opt) {
            ("SendKey", Some(data)) => {
                if data.chars().count() == 1 {
                    Ok(KeyboardAction::SendKey(data.chars().next().unwrap()))
                } else {
                    match data.to_lowercase().as_str() {
                        "space" => Ok(KeyboardAction::SendKey(' ')),
                        "enter" => Ok(KeyboardAction::SendKey('\n')),
                        "tab" => Ok(KeyboardAction::SendKey('\t')),
                        "backspace" => Ok(KeyboardAction::SendEnigoKey(enigo::Key::Backspace)),
                        "delete" => Ok(KeyboardAction::SendEnigoKey(enigo::Key::Delete)),
                        "left" => Ok(KeyboardAction::SendEnigoKey(enigo::Key::LeftArrow)),
                        "right" => Ok(KeyboardAction::SendEnigoKey(enigo::Key::RightArrow)),
                        "up" => Ok(KeyboardAction::SendEnigoKey(enigo::Key::UpArrow)),
                        "down" => Ok(KeyboardAction::SendEnigoKey(enigo::Key::DownArrow)),
                        _ => Err(anyhow::anyhow!("unknown key '{}'", data)),
                    }
                }
            }
            ("SwitchState", Some(data)) => {
                let canon = StateId::VARIANTS
                    .iter()
                    .find(|v| data.eq_ignore_ascii_case(v))
                    .copied()
                    .ok_or_else(|| anyhow::anyhow!("unknown state '{}'", data))?;
                Ok(KeyboardAction::SwitchState(
                    serde_plain::from_str(canon)
                        .map_err(|e: serde_plain::Error| anyhow::anyhow!(e))?,
                ))
            }
            ("SwitchLayout", Some(data)) => Ok(KeyboardAction::SwitchLayout(data.to_owned())),
            (_, Some(_)) => Err(anyhow::anyhow!(
                "keyboard action '{}' does not take a '.' payload",
                variant
            )),
            (v, None) => {
                serde_plain::from_str(v).map_err(|e: serde_plain::Error| anyhow::anyhow!(e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_toggle_record() {
        assert_eq!(
            KeyboardAction::try_from("toggleRecord").unwrap(),
            KeyboardAction::ToggleRecord
        );
        assert_eq!(
            KeyboardAction::try_from("ToggleRecord").unwrap(),
            KeyboardAction::ToggleRecord
        );
    }

    #[test]
    fn parses_rotate_window() {
        assert_eq!(
            KeyboardAction::try_from("rotateWindow").unwrap(),
            KeyboardAction::RotateWindow
        );
        assert_eq!(
            KeyboardAction::try_from("RotateWindow").unwrap(),
            KeyboardAction::RotateWindow
        );
    }
}

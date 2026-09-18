use serde::Deserialize;
use std::any::Any;
use strum::VariantNames;

use crate::state::{actions::Action, actions::TriggerMode, StateId};

#[derive(Deserialize, strum::VariantNames, Eq, Hash, PartialEq, Clone, Debug)]
pub enum KeyboardAction {
    SendKeyUnderLeftStick,
    SendKeyUnderRightStick,
    SendKeyUnderLeftStickOrAcceptSuggestion,
    SendKeyUnderRightStickOrAcceptSuggestion,
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
    CycleSuggestion,
    CycleSuggestionPrev,
    EnterOrAcceptSuggestion,
    CancelSuggestion,
    ToggleCompletion,
    AcceptSuggestion(Option<usize>),
}

impl Action for KeyboardAction {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn trigger_mode(&self) -> TriggerMode {
        use KeyboardAction::*;
        match self {
            SendKeyUnderLeftStick
            | SendKeyUnderRightStick
            | SendKeyUnderLeftStickOrAcceptSuggestion
            | SendKeyUnderRightStickOrAcceptSuggestion
            | SendKey(_)
            | SendEnigoKey(_) => TriggerMode::WhileHeld,
            ToggleShift
            | ToggleCtrl
            | ToggleAlt
            | Paste
            | SwitchState(_)
            | SwitchLayout(_)
            | FlipWindowLeftRight
            | FlipWindowAboveBelow
            | RotateWindow
            | Exit
            | ToggleRecord
            | CycleSuggestion
            | CycleSuggestionPrev
            | EnterOrAcceptSuggestion
            | CancelSuggestion
            | ToggleCompletion
            | AcceptSuggestion(_) => TriggerMode::Edge,
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
            ("SwitchState", Some(data)) => Ok(KeyboardAction::SwitchState(
                crate::state::actions::parse_state_id(data)?,
            )),
            ("SwitchLayout", Some(data)) => Ok(KeyboardAction::SwitchLayout(data.to_owned())),
            ("AcceptSuggestion", Some(data)) => {
                let i: usize = data
                    .parse()
                    .map_err(|_| anyhow::anyhow!("acceptSuggestion index '{data}'"))?;
                Ok(KeyboardAction::AcceptSuggestion(Some(i)))
            }
            ("AcceptSuggestion", None) => Ok(KeyboardAction::AcceptSuggestion(None)),
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

impl KeyboardAction {
    /// Stable wire id for mappings / MCP geometry (`sendKey.space`, `toggleShift`, …).
    pub fn wire_id(&self) -> String {
        use KeyboardAction::*;
        match self {
            SendKeyUnderLeftStick => "sendKeyUnderLeftStick".into(),
            SendKeyUnderRightStick => "sendKeyUnderRightStick".into(),
            SendKeyUnderLeftStickOrAcceptSuggestion => {
                "sendKeyUnderLeftStickOrAcceptSuggestion".into()
            }
            SendKeyUnderRightStickOrAcceptSuggestion => {
                "sendKeyUnderRightStickOrAcceptSuggestion".into()
            }
            SendKey(c) => {
                let payload = match *c {
                    ' ' => "space".to_owned(),
                    '\n' => "enter".to_owned(),
                    '\t' => "tab".to_owned(),
                    other => other.to_string(),
                };
                format!("sendKey.{payload}")
            }
            SendEnigoKey(k) => {
                let payload = match k {
                    enigo::Key::Backspace => "backspace".to_owned(),
                    enigo::Key::Delete => "delete".to_owned(),
                    enigo::Key::LeftArrow => "left".to_owned(),
                    enigo::Key::RightArrow => "right".to_owned(),
                    enigo::Key::UpArrow => "up".to_owned(),
                    enigo::Key::DownArrow => "down".to_owned(),
                    other => serde_plain::to_string(other).unwrap_or_else(|_| format!("{other:?}")),
                };
                format!("sendKey.{payload}")
            }
            ToggleShift => "toggleShift".into(),
            ToggleCtrl => "toggleCtrl".into(),
            ToggleAlt => "toggleAlt".into(),
            Paste => "paste".into(),
            SwitchState(s) => format!("switchState.{}", s.wire_id()),
            SwitchLayout(name) => format!("switchLayout.{name}"),
            FlipWindowLeftRight => "flipWindowLeftRight".into(),
            FlipWindowAboveBelow => "flipWindowAboveBelow".into(),
            RotateWindow => "rotateWindow".into(),
            Exit => "exit".into(),
            ToggleRecord => "toggleRecord".into(),
            CycleSuggestion => "cycleSuggestion".into(),
            CycleSuggestionPrev => "cycleSuggestionPrev".into(),
            EnterOrAcceptSuggestion => "enterOrAcceptSuggestion".into(),
            CancelSuggestion => "cancelSuggestion".into(),
            ToggleCompletion => "toggleCompletion".into(),
            AcceptSuggestion(None) => "acceptSuggestion".into(),
            AcceptSuggestion(Some(i)) => format!("acceptSuggestion.{i}"),
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

    #[test]
    fn wire_id_round_trips() {
        let cases = [
            KeyboardAction::ToggleShift,
            KeyboardAction::ToggleRecord,
            KeyboardAction::RotateWindow,
            KeyboardAction::SendKey('a'),
            KeyboardAction::SendKey(' '),
            KeyboardAction::SendKey('\n'),
            KeyboardAction::SendEnigoKey(enigo::Key::Backspace),
            KeyboardAction::SwitchState(StateId::Menu),
            KeyboardAction::SwitchLayout("main".into()),
            KeyboardAction::SendKeyUnderLeftStick,
            KeyboardAction::SendKeyUnderLeftStickOrAcceptSuggestion,
            KeyboardAction::SendKeyUnderRightStickOrAcceptSuggestion,
            KeyboardAction::CycleSuggestion,
            KeyboardAction::EnterOrAcceptSuggestion,
            KeyboardAction::ToggleCompletion,
            KeyboardAction::AcceptSuggestion(Some(2)),
        ];
        for action in cases {
            let wire = action.wire_id();
            let parsed = KeyboardAction::try_from(wire.as_str()).unwrap();
            assert_eq!(parsed, action, "wire={wire}");
        }
    }

    #[test]
    fn send_under_stick_or_accept_is_while_held() {
        assert_eq!(
            KeyboardAction::SendKeyUnderLeftStickOrAcceptSuggestion.trigger_mode(),
            TriggerMode::WhileHeld
        );
        assert_eq!(
            KeyboardAction::SendKeyUnderRightStickOrAcceptSuggestion.trigger_mode(),
            TriggerMode::WhileHeld
        );
    }
}

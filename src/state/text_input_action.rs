use serde::Deserialize;
use std::any::Any;
use strum::VariantNames;

use crate::state::actions::{self, Action, TriggerMode};
use crate::state::StateId;

#[derive(Deserialize, strum::VariantNames, Eq, Hash, PartialEq, Clone, Debug)]
pub enum TextInputAction {
    MoveCursorLeft,
    MoveCursorRight,
    SwitchState(StateId),
    CycleSuggestion,
    CycleSuggestionPrev,
    EnterOrAcceptSuggestion,
    CancelSuggestion,
    ToggleCompletion,
    AcceptSuggestion(Option<usize>),
}

impl Action for TextInputAction {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn trigger_mode(&self) -> TriggerMode {
        match self {
            TextInputAction::MoveCursorLeft | TextInputAction::MoveCursorRight => {
                TriggerMode::WhileHeld
            }
            _ => TriggerMode::Edge,
        }
    }
}

impl TryFrom<&str> for TextInputAction {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let (head, tail_opt) = match value.split_once('.') {
            Some((h, t)) => (h, Some(t)),
            None => (value, None),
        };

        let variant = TextInputAction::VARIANTS
            .iter()
            .find(|v| head.eq_ignore_ascii_case(v))
            .copied()
            .ok_or_else(|| anyhow::anyhow!("unknown text input action '{value}'"))?;

        match (variant, tail_opt) {
            ("SwitchState", Some(data)) => {
                Ok(TextInputAction::SwitchState(actions::parse_state_id(data)?))
            }
            ("AcceptSuggestion", Some(data)) => {
                let i: usize = data
                    .parse()
                    .map_err(|_| anyhow::anyhow!("acceptSuggestion index '{data}'"))?;
                Ok(TextInputAction::AcceptSuggestion(Some(i)))
            }
            ("AcceptSuggestion", None) => Ok(TextInputAction::AcceptSuggestion(None)),
            (_, Some(_)) => Err(anyhow::anyhow!(
                "text input action '{variant}' does not take a '.' payload"
            )),
            (v, None) => {
                serde_plain::from_str(v).map_err(|e: serde_plain::Error| anyhow::anyhow!(e))
            }
        }
    }
}

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
            TextInputAction::SwitchState(_) => TriggerMode::Edge,
        }
    }
}

impl TryFrom<&str> for TextInputAction {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        actions::parse_unit_or_switch_state(
            value,
            TextInputAction::VARIANTS,
            "text input",
            TextInputAction::SwitchState,
        )
    }
}

use serde::Deserialize;
use std::any::Any;
use strum::VariantNames;

use crate::state::actions::{self, Action, TriggerMode};
use crate::state::StateId;

#[derive(Deserialize, strum::VariantNames, Eq, Hash, PartialEq, Clone, Debug)]
pub enum MoveWindowAction {
    Save,
    SwitchState(StateId),
}

impl Action for MoveWindowAction {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn trigger_mode(&self) -> TriggerMode {
        TriggerMode::Edge
    }
}

impl TryFrom<&str> for MoveWindowAction {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        actions::parse_unit_or_switch_state(
            value,
            MoveWindowAction::VARIANTS,
            "move window",
            MoveWindowAction::SwitchState,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_save_and_switch() {
        assert_eq!(
            MoveWindowAction::try_from("save").unwrap(),
            MoveWindowAction::Save
        );
        assert_eq!(
            MoveWindowAction::try_from("switchState.settings").unwrap(),
            MoveWindowAction::SwitchState(StateId::Settings)
        );
    }
}

use serde::Deserialize;
use std::any::Any;
use strum::VariantNames;

use crate::state::actions::{self, Action, TriggerMode};
use crate::state::StateId;

#[derive(Deserialize, strum::VariantNames, Eq, Hash, PartialEq, Clone, Debug)]
pub enum SelectLayoutAction {
    SelectUp,
    SelectDown,
    Activate,
    SwitchState(StateId),
}

impl Action for SelectLayoutAction {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn trigger_mode(&self) -> TriggerMode {
        TriggerMode::Edge
    }
}

impl TryFrom<&str> for SelectLayoutAction {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        actions::parse_unit_or_switch_state(
            value,
            SelectLayoutAction::VARIANTS,
            "select layout",
            SelectLayoutAction::SwitchState,
        )
    }
}

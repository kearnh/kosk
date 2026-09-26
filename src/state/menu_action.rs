use serde::Deserialize;
use std::any::Any;
use strum::VariantNames;

use crate::state::actions::{self, Action, TriggerMode};
use crate::state::StateId;

#[derive(Deserialize, strum::VariantNames, Eq, Hash, PartialEq, Clone, Debug)]
pub enum MenuAction {
    SelectUp,
    SelectDown,
    SelectLeft,
    SelectRight,
    Activate,
    PagePrev,
    PageNext,
    Back,
    OpenConfig,
    ToggleShowAllControllers,
    ToggleShowAllSettings,
    SwitchState(StateId),
}

impl Action for MenuAction {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn trigger_mode(&self) -> TriggerMode {
        TriggerMode::Edge
    }
}

impl TryFrom<&str> for MenuAction {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        actions::parse_unit_or_switch_state(
            value,
            MenuAction::VARIANTS,
            "settings",
            MenuAction::SwitchState,
        )
    }
}

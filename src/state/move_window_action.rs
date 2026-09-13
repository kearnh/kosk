use serde::Deserialize;
use std::any::Any;
use strum::VariantNames;

use crate::state::actions::{self, Action, TriggerMode};
use crate::state::StateId;

#[derive(Deserialize, strum::VariantNames, Eq, Hash, PartialEq, Clone, Debug)]
pub enum MoveWindowAction {
    SnapTopLeft,
    SnapTopRight,
    SnapBottomLeft,
    SnapBottomRight,
    FlipWindowLeftRight,
    FlipWindowAboveBelow,
    RotateWindow,
    NudgeUp,
    NudgeDown,
    NudgeLeft,
    NudgeRight,
    SwitchState(StateId),
}

impl Action for MoveWindowAction {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn trigger_mode(&self) -> TriggerMode {
        match self {
            MoveWindowAction::NudgeUp
            | MoveWindowAction::NudgeDown
            | MoveWindowAction::NudgeLeft
            | MoveWindowAction::NudgeRight => TriggerMode::WhileHeld,
            MoveWindowAction::SnapTopLeft
            | MoveWindowAction::SnapTopRight
            | MoveWindowAction::SnapBottomLeft
            | MoveWindowAction::SnapBottomRight
            | MoveWindowAction::FlipWindowLeftRight
            | MoveWindowAction::FlipWindowAboveBelow
            | MoveWindowAction::RotateWindow
            | MoveWindowAction::SwitchState(_) => TriggerMode::Edge,
        }
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

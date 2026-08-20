use serde::Deserialize;
use std::any::Any;
use strum::VariantNames;

use crate::state::{actions::Action, actions::TriggerMode, StateId};

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
        let (head, tail_opt) = match value.split_once('.') {
            Some((h, t)) => (h, Some(t)),
            None => (value, None),
        };

        let variant = MoveWindowAction::VARIANTS
            .iter()
            .find(|v| head.eq_ignore_ascii_case(v))
            .copied()
            .ok_or_else(|| anyhow::anyhow!("unknown move window action '{}'", value))?;

        match (variant, tail_opt) {
            ("SwitchState", Some(data)) => {
                let canon = StateId::VARIANTS
                    .iter()
                    .find(|v| data.eq_ignore_ascii_case(v))
                    .copied()
                    .ok_or_else(|| anyhow::anyhow!("unknown state '{}'", data))?;
                Ok(MoveWindowAction::SwitchState(
                    serde_plain::from_str(canon)
                        .map_err(|e: serde_plain::Error| anyhow::anyhow!(e))?,
                ))
            }
            (_, Some(_)) => Err(anyhow::anyhow!(
                "move window action '{}' does not take a '.' payload",
                variant
            )),
            (v, None) => {
                serde_plain::from_str(v).map_err(|e: serde_plain::Error| anyhow::anyhow!(e))
            }
        }
    }
}

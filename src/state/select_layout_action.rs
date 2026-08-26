use serde::Deserialize;
use std::any::Any;
use strum::VariantNames;

use crate::state::{actions::Action, actions::TriggerMode, StateId};

#[derive(Deserialize, strum::VariantNames, Eq, Hash, PartialEq, Clone, Debug)]
pub enum SelectLayoutAction {
    SelectUp,
    SelectDown,
    Activate,
    TogglePreview,
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
        let (head, tail_opt) = match value.split_once('.') {
            Some((h, t)) => (h, Some(t)),
            None => (value, None),
        };

        let variant = SelectLayoutAction::VARIANTS
            .iter()
            .find(|v| head.eq_ignore_ascii_case(v))
            .copied()
            .ok_or_else(|| anyhow::anyhow!("unknown select layout action '{}'", value))?;

        match (variant, tail_opt) {
            ("SwitchState", Some(data)) => {
                let canon = StateId::VARIANTS
                    .iter()
                    .find(|v| data.eq_ignore_ascii_case(v))
                    .copied()
                    .ok_or_else(|| anyhow::anyhow!("unknown state '{}'", data))?;
                Ok(SelectLayoutAction::SwitchState(
                    serde_plain::from_str(canon)
                        .map_err(|e: serde_plain::Error| anyhow::anyhow!(e))?,
                ))
            }
            (_, Some(_)) => Err(anyhow::anyhow!(
                "select layout action '{}' does not take a '.' payload",
                variant
            )),
            (v, None) => {
                serde_plain::from_str(v).map_err(|e: serde_plain::Error| anyhow::anyhow!(e))
            }
        }
    }
}

use std::any::Any;
use std::collections::HashMap;

use super::actions::{self, Action, TriggerMode};
use super::StateId;
use crate::config::Config;
use crate::controller::bindings::BindingEngine;
use crate::controller::mapping::MappingValue;
use crate::controller::{ControllerBinding, ControllerInput};
use crate::when::WhenContext;

pub(super) const TOGGLE_OVERLAY_VISIBILITY_ACTION: &str = "toggleOverlayVisibility";

#[derive(Clone)]
pub(super) enum OverlayAction {
    Toggle,
    Ignore,
}

impl Action for OverlayAction {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn trigger_mode(&self) -> TriggerMode {
        TriggerMode::Edge
    }
}

#[derive(Default)]
pub(super) struct OverlayBindings {
    mode: Option<StateId>,
    source: HashMap<ControllerBinding, MappingValue>,
    engine: Option<BindingEngine<OverlayAction>>,
}

impl OverlayBindings {
    pub(super) fn toggle_requested(
        &mut self,
        mode: StateId,
        cfg: &Config,
        input: &dyn ControllerInput,
        context: &WhenContext,
    ) -> anyhow::Result<bool> {
        let mode = match mode {
            StateId::Mappings | StateId::SelectKey => StateId::Keyboard,
            mode => mode,
        };
        let source = cfg.controller_map.get(&mode);
        let changed = source.map_or(!self.source.is_empty(), |source| source != &self.source);
        if self.mode != Some(mode) || changed {
            let mut engine = actions::load_overlay_bindings(mode, cfg)?;
            if self.mode.is_some() {
                engine.reset(Some(input));
            }
            self.engine = Some(engine);
            self.source = source.cloned().unwrap_or_default();
            self.mode = Some(mode);
        }
        Ok(self.engine.as_mut().is_some_and(|engine| {
            engine
                .evaluate(input, context)
                .iter()
                .any(|(_, action)| matches!(action, OverlayAction::Toggle))
        }))
    }

    pub(super) fn reset(&mut self) {
        if let Some(engine) = &mut self.engine {
            engine.reset(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::test_input::ButtonSetInput;
    use crate::controller::ControllerButton;

    #[test]
    fn visibility_action_fires_once_per_press_in_every_mapping_mode() {
        let input = ButtonSetInput([ControllerButton::FaceBottom].into());
        for mode in [
            StateId::Keyboard,
            StateId::Settings,
            StateId::TextInput,
            StateId::MoveWindow,
            StateId::SelectLayout,
        ] {
            let mut cfg = Config::default();
            cfg.controller_map.insert(
                mode,
                [(
                    ControllerBinding::Single(ControllerButton::FaceBottom),
                    MappingValue::from_action(TOGGLE_OVERLAY_VISIBILITY_ACTION),
                )]
                .into(),
            );
            let mut bindings = OverlayBindings::default();
            assert!(bindings
                .toggle_requested(mode, &cfg, &input, &WhenContext::default())
                .unwrap());
            assert!(!bindings
                .toggle_requested(mode, &cfg, &input, &WhenContext::default())
                .unwrap());
            bindings.reset();
            assert!(bindings
                .toggle_requested(mode, &cfg, &input, &WhenContext::default())
                .unwrap());
        }
    }

    #[test]
    fn ordinary_rule_blocks_visibility_fallback_when_selected() {
        let mut cfg = Config::default();
        cfg.controller_map.insert(
            StateId::Keyboard,
            [(
                ControllerBinding::Single(ControllerButton::FaceBottom),
                MappingValue::Rules(vec![
                    crate::controller::mapping::MappingRule {
                        action: "sendKey.a".into(),
                        when: Some("modifier.ctrl".into()),
                    },
                    crate::controller::mapping::MappingRule {
                        action: TOGGLE_OVERLAY_VISIBILITY_ACTION.into(),
                        when: None,
                    },
                ]),
            )]
            .into(),
        );
        let input = ButtonSetInput([ControllerButton::FaceBottom].into());
        let mut bindings = OverlayBindings::default();
        assert!(!bindings
            .toggle_requested(
                StateId::Keyboard,
                &cfg,
                &input,
                &WhenContext {
                    ctrl: true,
                    ..Default::default()
                }
            )
            .unwrap());
        bindings.reset();
        assert!(bindings
            .toggle_requested(StateId::Keyboard, &cfg, &input, &WhenContext::default())
            .unwrap());
    }
}

use std::any::Any;
use std::collections::HashMap;

use crate::config;
use crate::controller::bindings::BindingEngine;
use crate::state::keyboard::KeyboardAction;
use crate::state::menu_action::MenuAction;
use crate::state::move_window_action::MoveWindowAction;
use crate::state::text_input_action::TextInputAction;
use crate::state::StateId;

/// How a controller mapping should fire while its button is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerMode {
    /// Once per press (fires on transition to down).
    Edge,
    /// Every evaluation tick while physically held (repeat actions use [`EventQueue`] debounce).
    WhileHeld,
}

pub trait Action: Any {
    fn as_any(&self) -> &dyn Any;
    fn trigger_mode(&self) -> TriggerMode;
}

pub fn get_action(state: StateId, name: &str) -> Option<Box<dyn Action>> {
    match state {
        StateId::Keyboard => {
            let action = KeyboardAction::try_from(name).ok()?;
            Some(Box::new(action))
        }
        StateId::Menu => {
            let action = MenuAction::try_from(name).ok()?;
            Some(Box::new(action))
        }
        StateId::TextInput => {
            let action = TextInputAction::try_from(name).ok()?;
            Some(Box::new(action))
        }
        StateId::MoveWindow => {
            let action = MoveWindowAction::try_from(name).ok()?;
            Some(Box::new(action))
        }
    }
}

pub fn load_bindings<A: Action + Clone + 'static>(
    state_id: StateId,
) -> Result<BindingEngine<A>, anyhow::Error> {
    let cfg = config::get();
    let mut raw_bindings = HashMap::new();
    if let Some(raw_mapping) = cfg.controller_map.get(&state_id).cloned() {
        for (binding, action_name) in raw_mapping {
            if let Some(action) = get_action(state_id, &action_name) {
                if let Some(action) = action.as_ref().as_any().downcast_ref::<A>() {
                    raw_bindings.insert(binding, action.clone());
                }
            }
        }
    }
    BindingEngine::try_from_raw(raw_bindings)
        .map_err(|e| anyhow::anyhow!("{state_id:?} controller_map: {e}"))
}

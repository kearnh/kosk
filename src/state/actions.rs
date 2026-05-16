use std::any::Any;
use std::collections::HashMap;

use crate::config;
use crate::controller::bindings::BindingEngine;
use crate::state::keyboard::KeyboardAction;
use crate::state::menu_action::MenuAction;
use crate::state::move_window_action::MoveWindowAction;
use crate::state::text_input_action::TextInputAction;
use crate::state::StateId;

pub trait Action: Any {
    fn as_any(&self) -> &dyn Any;
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

pub fn load_bindings<A: Clone + 'static>(
    state_id: StateId,
) -> Result<BindingEngine<A>, anyhow::Error>
where
    A: Action,
{
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

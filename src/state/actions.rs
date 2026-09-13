use std::any::Any;
use std::collections::HashMap;

use crate::config;
use crate::controller::bindings::BindingEngine;
use crate::state::keyboard::KeyboardAction;
use crate::state::menu_action::MenuAction;
use crate::state::move_window_action::MoveWindowAction;
use crate::state::select_layout_action::SelectLayoutAction;
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
        StateId::SelectLayout => {
            let action = SelectLayoutAction::try_from(name).ok()?;
            Some(Box::new(action))
        }
        StateId::Mappings | StateId::SelectKey => None,
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

/// PascalCase / serde_plain name → camelCase wire id (`ToggleShift` → `toggleShift`).
pub(crate) fn to_camel(pascal: &str) -> String {
    let mut chars = pascal.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_lowercase().chain(chars).collect(),
    }
}

pub(crate) fn parse_state_id(data: &str) -> Result<StateId, anyhow::Error> {
    use strum::VariantNames;
    let canon = StateId::VARIANTS
        .iter()
        .find(|v| data.eq_ignore_ascii_case(v))
        .copied()
        .ok_or_else(|| anyhow::anyhow!("unknown state '{}'", data))?;
    serde_plain::from_str(canon).map_err(|e: serde_plain::Error| anyhow::anyhow!(e))
}

/// Unit variants via serde_plain, or `SwitchState.<state>` via `make_switch`.
pub(crate) fn parse_unit_or_switch_state<T>(
    value: &str,
    variants: &[&'static str],
    kind: &str,
    make_switch: impl FnOnce(StateId) -> T,
) -> Result<T, anyhow::Error>
where
    T: serde::de::DeserializeOwned,
{
    let (head, tail_opt) = match value.split_once('.') {
        Some((h, t)) => (h, Some(t)),
        None => (value, None),
    };

    let variant = variants
        .iter()
        .find(|v| head.eq_ignore_ascii_case(v))
        .copied()
        .ok_or_else(|| anyhow::anyhow!("unknown {kind} action '{value}'"))?;

    match (variant, tail_opt) {
        ("SwitchState", Some(data)) => Ok(make_switch(parse_state_id(data)?)),
        (_, Some(_)) => Err(anyhow::anyhow!(
            "{kind} action '{variant}' does not take a '.' payload"
        )),
        (v, None) => serde_plain::from_str(v).map_err(|e: serde_plain::Error| anyhow::anyhow!(e)),
    }
}

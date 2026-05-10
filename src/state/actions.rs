use std::any::Any;

use crate::state::KeyboardAction;

pub trait Action: Any {
    fn as_any(&self) -> &dyn Any;
}

pub fn get_action(name: &str) -> Option<Box<dyn Action>> {
    if name.starts_with("keyboard.") {
        let name = name.strip_prefix("keyboard.").unwrap();
        let action = KeyboardAction::try_from(name).ok()?;
        return Some(Box::new(action));
    }
    None
}

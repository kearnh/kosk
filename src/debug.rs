use egui::{Context, Plugin};

use crate::controller::ControllerInput;

pub struct DebugPlugin {
    pub controller_input: Option<Box<dyn ControllerInput + Send + Sync>>,
}

impl DebugPlugin {
    fn new() -> Self {
        Self {
            controller_input: None,
        }
    }
}

impl Plugin for DebugPlugin {
    fn debug_name(&self) -> &'static str {
        "debug"
    }
}

pub fn register(ctx: &Context) {
    let plugin = DebugPlugin::new();
    ctx.add_plugin(plugin);
}

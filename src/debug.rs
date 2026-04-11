use egui::{Context, Plugin};

use crate::ps4::Ps4InputData;

pub struct DebugPlugin {
    pub controller_input: Option<Ps4InputData>,
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

use crate::{
    config,
    keyboard::Keyboard,
    ps4::Ps4InputData,
    state::{keyboard::KeyboardState, menu::MenuState},
};
use anyhow::Result;
use egui::{Context, Ui};

mod keyboard;
mod menu;

pub enum StateId {
    Menu,
    Keyboard,
}

pub struct AppState {
    state: StateId,
    kb: KeyboardState,
    menu: MenuState,
}

impl AppState {
    pub fn new(monitor_size: (f32, f32)) -> Result<Self> {
        let cfg = config::get();
        let kb_inner = Keyboard::new(&cfg.layout, cfg.stick_x, cfg.stick_y, cfg.stick_warp)?;
        let kb = KeyboardState {
            kb: kb_inner,
            l2_was_pressed: false,
            r2_was_pressed: false,
            trigger_threshold: cfg.trigger_threshold,
            pos: 0,
            monitor_size,
        };
        Ok(Self {
            state: StateId::Keyboard,
            kb,
            menu: MenuState,
        })
    }

    // Add this new method to reload from config
    pub fn reload_from_config(&mut self) -> Result<()> {
        let cfg = config::get();
        
        // Recreate keyboard with new config
        let kb_inner = Keyboard::new(&cfg.layout, cfg.stick_x, cfg.stick_y, cfg.stick_warp)?;
        self.kb.kb = kb_inner;
        self.kb.trigger_threshold = cfg.trigger_threshold;
        
        Ok(())
    }

    pub fn window_size(&self) -> (f32, f32) {
        self.kb.kb.layout.get_dimensions()
    }

    pub fn draw_ui(&mut self, ctx: &Context, ui: &mut Ui) {
        let id = match self.state {
            StateId::Keyboard => self.kb.draw_ui(ctx, ui),
            StateId::Menu => self.menu.draw_ui(ctx, ui),
        };
        self.state = id;
    }

    pub fn handle_controller_input(
        &mut self,
        ctx: &Context,
        input: &Option<Ps4InputData>,
    ) -> Result<()> {
        let id = match self.state {
            StateId::Keyboard => self.kb.handle_controller_input(ctx, input)?,
            StateId::Menu => self.menu.handle_controller_input(ctx, input)?,
        };
        self.state = id;
        Ok(())
    }
}

use crate::{
    config,
    keyboard::Keyboard,
    ps4::{Dpad, Ps4InputData},
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

pub enum WindowPos {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
    // Todo
    // Absolute(f32, f32)
}

impl WindowPos {
    fn incr(&mut self) {
        *self = match self {
            WindowPos::TopLeft => WindowPos::TopRight,
            WindowPos::TopRight => WindowPos::BottomRight,
            WindowPos::BottomRight => WindowPos::BottomLeft,
            WindowPos::BottomLeft => WindowPos::TopLeft,
        };
    }
}

pub struct AppState {
    state: StateId,
    kb: KeyboardState,
    menu: MenuState,
    pos: WindowPos,
    monitor_size: (f32, f32),
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
        };

        Ok(Self {
            state: StateId::Keyboard,
            kb,
            menu: MenuState,
            pos: WindowPos::BottomRight,
            monitor_size,
        })
    }

    pub fn get_position(&self) -> (f32, f32) {
        let (kb_width, kb_height) = self.window_size();
        let (size_x, size_y) = self.monitor_size;
        [
            (0.0, 0.0),
            (size_x - kb_width, 0.0),
            (size_x - kb_width, size_y - kb_height),
            (0.0, size_y - kb_height),
        ][match self.pos {
            WindowPos::TopLeft => 0,
            WindowPos::TopRight => 1,
            WindowPos::BottomRight => 2,
            WindowPos::BottomLeft => 3,
        }]
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
        // temporary, position will be set in menu somehow
        if matches!(input.clone().and_then(|i| i.dpad), Some(Dpad::Down)) {
            self.pos.incr();
            let pos = self.get_position();
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos.into()));
            return Ok(());
        }

        let id = match self.state {
            StateId::Keyboard => self.kb.handle_controller_input(ctx, input)?,
            StateId::Menu => self.menu.handle_controller_input(ctx, input)?,
        };
        self.state = id;
        Ok(())
    }
}

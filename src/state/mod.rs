use crate::{
    config,
    controller::ControllerInput,
    debug::DebugPlugin,
    state::{
        keyboard::{Keyboard, KeyboardState},
        menu::MenuState,
        move_window::MoveWindowState,
    },
};
use anyhow::Result;
use egui::{Context, Rect, Ui};

mod keyboard;
mod menu;
mod move_window;
mod text_input;

pub enum StateId {
    Menu,
    Keyboard,
    MoveWindow,
    TextInput,
}

pub enum WindowPos {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
    Absolute(f32, f32),
}

pub struct AppState {
    state: StateId,
    kb: KeyboardState,
    menu: MenuState,
    move_window: MoveWindowState,
    text_input: text_input::TextInputState,
    pos: WindowPos,
    monitor_size: (f32, f32),
}

impl AppState {
    pub fn new(monitor_size: (f32, f32)) -> Result<Self> {
        let cfg = config::get();

        let kb = KeyboardState::new()?;
        let text_input = text_input::TextInputState::new();

        Ok(Self {
            state: StateId::Keyboard,
            kb,
            menu: MenuState::new(),
            move_window: MoveWindowState::new(cfg.scale_x, cfg.scale_y),
            text_input,
            pos: WindowPos::BottomRight,
            monitor_size,
        })
    }

    pub fn get_position(&self, content_rect: Rect) -> (f32, f32) {
        if let WindowPos::Absolute(x, y) = self.pos {
            return (x, y);
        }
        let (w, h) = content_rect.max.into();
        let (size_x, size_y) = self.monitor_size;
        [
            (0.0, 0.0),
            (size_x - w, 0.0),
            (size_x - w, size_y - h),
            (0.0, size_y - h),
        ][match self.pos {
            WindowPos::TopLeft => 0,
            WindowPos::TopRight => 1,
            WindowPos::BottomRight => 2,
            WindowPos::BottomLeft => 3,
            WindowPos::Absolute(..) => unreachable!(),
        }]
    }

    // Add this new method to reload from config
    pub fn reload_from_config(&mut self) -> Result<()> {
        let cfg = config::get();
        self.kb.kb = Keyboard::new()?;
        self.kb.trigger_threshold = cfg.trigger_threshold;
        self.move_window = MoveWindowState::new(cfg.scale_x, cfg.scale_y);
        Ok(())
    }

    pub fn draw_ui(&mut self, ctx: &Context, ui: &mut Ui) {
        let r = ctx.content_rect();
        let id = match self.state {
            StateId::Keyboard => self.kb.draw_ui(ctx, ui),
            StateId::Menu => self.menu.draw_ui(ctx, ui),
            StateId::MoveWindow => {
                let (x, y) = self.get_position(r);
                let (next_state, movement) = self.move_window.draw_ui(ctx, ui, (x, y));
                if let Some(new_pos) = movement {
                    if let WindowPos::Absolute(mut x, mut y) = new_pos {
                        let (mon_w, mon_h) = self.monitor_size;

                        // Clamp X between 0 and (Monitor Width - Window Width)
                        x = x.clamp(0.0, mon_w - r.width());
                        // Clamp Y between 0 and (Monitor Height - Window Height)
                        y = y.clamp(0.0, mon_h - r.height());

                        self.pos = WindowPos::Absolute(x, y);
                    } else {
                        self.pos = new_pos;
                    }
                }
                next_state
            }
            StateId::TextInput => self.text_input.draw_ui(ctx, ui, &mut self.kb),
        };
        self.state = id;
    }

    pub fn handle_controller_input(
        &mut self,
        ctx: &Context,
        input: &Option<Box<dyn ControllerInput>>,
    ) -> Result<()> {
        if let Some(input) = input {
            if config::get().debug.is_some() {
                ctx.with_plugin::<DebugPlugin, _>(|d| d.controller_input = Some(input.box_clone()));
            }
        }

        let id = match self.state {
            StateId::Keyboard => self.kb.handle_controller_input(ctx, input)?,
            StateId::Menu => self.menu.handle_controller_input(ctx, input)?,
            StateId::MoveWindow => self.move_window.handle_controller_input(ctx, input)?,
            StateId::TextInput => self.text_input.handle_controller_input(ctx, input, &mut self.kb)?,
        };
        self.state = id;
        Ok(())
    }
}

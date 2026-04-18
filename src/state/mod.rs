use crate::{
    config,
    debug::DebugPlugin,
    keyboard::Keyboard,
    ps4::Ps4InputData,
    state::{keyboard::KeyboardState, menu::MenuState, move_window::MoveWindowState},
};
use anyhow::Result;
use egui::{Context, Rect, Ui};

mod keyboard;
mod menu;
mod move_window;

pub enum StateId {
    Menu,
    Keyboard,
    MoveWindow,
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
    pos: WindowPos,
    monitor_size: (f32, f32),
}

impl AppState {
    pub fn new(monitor_size: (f32, f32)) -> Result<Self> {
        let cfg = config::get();

        let kb_inner = Keyboard::new()?;
        let kb = KeyboardState {
            kb: kb_inner,
            l2_was_pressed: false,
            r2_was_pressed: false,
            trigger_threshold: cfg.trigger_threshold,
        };

        Ok(Self {
            state: StateId::Keyboard,
            kb,
            menu: MenuState::new(),
            move_window: MoveWindowState::new(cfg.scale_x, cfg.scale_y),
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

    pub fn window_size(&self) -> (f32, f32) {
        self.kb.kb.layout.get_dimensions()
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
        };
        self.state = id;
    }

    pub fn handle_controller_input(
        &mut self,
        ctx: &Context,
        input: &Option<Ps4InputData>,
    ) -> Result<()> {
        if let Some(input) = input {
            if config::get().debug.is_some() {
                ctx.with_plugin::<DebugPlugin, _>(|d| d.controller_input = Some(input.clone()));
            }
        }

        let id = match self.state {
            StateId::Keyboard => self.kb.handle_controller_input(ctx, input)?,
            StateId::Menu => self.menu.handle_controller_input(ctx, input)?,
            StateId::MoveWindow => self.move_window.handle_controller_input(ctx, input)?,
        };
        self.state = id;
        Ok(())
    }
}

use crate::{
    config,
    controller::ControllerInput,
    debug::DebugPlugin,
    state::{
        event::Event, keyboard::KeyboardState, menu::MenuState, move_window::MoveWindowState,
        window_pos::WindowPos,
    },
};
use anyhow::Result;
use egui::{Context, Rect, Ui};
use enigo::{Enigo, Keyboard as _};

mod event;
mod keyboard;
mod menu;
mod move_window;
mod text_input;
pub mod window_pos;

#[derive(PartialEq)]
pub enum StateId {
    Menu,
    Keyboard,
    MoveWindow,
    TextInput,
}

pub struct AppState {
    state: StateId,
    kb: KeyboardState,
    menu: MenuState,
    move_window: MoveWindowState,
    text_input: text_input::TextInputState,
    pos: WindowPos,
    monitor_size: (f32, f32),
    // reusable events buffer
    events: Vec<Event>,
    enigo: Enigo,
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
            pos: cfg.window_pos,
            monitor_size,
            events: vec![],
            enigo: Enigo::new(&Default::default())?,
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
        self.kb.reload_from_config()?;
        self.move_window = MoveWindowState::new(cfg.scale_x, cfg.scale_y);
        Ok(())
    }

    fn process_events(&mut self) {
        for event in self.events.drain(..) {
            match event {
                Event::SendKey(key, direction) => {
                    let _ = self.enigo.key(key, direction);
                }
                Event::SendText(text) => {
                    let _ = self.enigo.text(&text);
                }
                Event::ChangeState(state) => {
                    self.state = state;
                }
            }
        }
    }

    pub fn draw_ui(&mut self, ctx: &Context, ui: &mut Ui) {
        match self.state {
            StateId::Keyboard => self.kb.draw_ui(ctx, ui, &mut self.events),
            StateId::Menu => self.menu.draw_ui(ctx, ui, &mut self.events),
            StateId::MoveWindow => {
                let (x, y) = self.get_position(ctx.content_rect());
                let movement = self.move_window.draw_ui(ctx, ui, (x, y), &mut self.events);
                if let Some(new_pos) = movement {
                    let final_pos = if let WindowPos::Absolute(mut x, mut y) = new_pos {
                        let (mon_w, mon_h) = self.monitor_size;

                        // Clamp X between 0 and (Monitor Width - Window Width)
                        x = x.clamp(0.0, mon_w - ctx.content_rect().width());
                        // Clamp Y between 0 and (Monitor Height - Window Height)
                        y = y.clamp(0.0, mon_h - ctx.content_rect().height());

                        WindowPos::Absolute(x, y)
                    } else {
                        new_pos
                    };

                    if self.pos != final_pos {
                        self.pos = final_pos;
                        let mut cfg = config::get();
                        cfg.window_pos = self.pos;
                        let _ = config::save(cfg);
                    }
                }
            }
            StateId::TextInput => self
                .text_input
                .draw_ui(ctx, ui, &mut self.events, &mut self.kb),
        }
        self.process_events();
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

        match self.state {
            StateId::Keyboard => self
                .kb
                .handle_controller_input(ctx, input, &mut self.events)?,
            StateId::Menu => self
                .menu
                .handle_controller_input(ctx, input, &mut self.events)?,
            StateId::MoveWindow => {
                self.move_window
                    .handle_controller_input(ctx, input, &mut self.events)?
            }
            StateId::TextInput => self.text_input.handle_controller_input(
                ctx,
                input,
                &mut self.events,
                &mut self.kb,
            )?,
        }
        self.process_events();
        Ok(())
    }
}

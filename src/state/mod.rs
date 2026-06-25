use crate::{
    config,
    controller::ControllerInput,
    debug::DebugPlugin,
    state::{
        event::Event,
        event::EventQueue,
        window_pos::{resolve_position, WindowPos},
    },
};
use anyhow::Result;
use egui::{Context, Rect, Ui};
use enigo::{Enigo, Keyboard as _};
use serde::{Deserialize, Serialize};

pub mod actions;
mod event;
mod keyboard;
mod menu;
mod menu_action;
mod move_window;
mod move_window_action;
mod text_input;
mod text_input_action;
pub mod window_pos;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Hash, strum::VariantNames)]
pub enum StateId {
    Menu,
    Keyboard,
    MoveWindow,
    TextInput,
}

pub struct AppState {
    state: StateId,
    pos: WindowPos,
    monitor_size: (f32, f32),
    events: EventQueue,
    enigo: Enigo,
}

impl AppState {
    pub fn new(monitor_size: (f32, f32)) -> Result<Self> {
        let cfg = config::get();

        keyboard::init()?;
        move_window::init()?;
        menu::init()?;
        text_input::init()?;

        Ok(Self {
            state: StateId::Keyboard,
            pos: cfg.window_pos,
            monitor_size,
            events: EventQueue::new(),
            enigo: Enigo::new(&Default::default())?,
        })
    }

    pub fn set_monitor_size(&mut self, monitor_size: (f32, f32)) {
        self.monitor_size = monitor_size;
    }

    fn window_size_from(content_rect: Rect) -> (f32, f32) {
        (content_rect.width(), content_rect.height())
    }

    fn clamp_absolute_pos(&self, pos: WindowPos, content_rect: Rect) -> WindowPos {
        let (resolved, _) =
            resolve_position(pos, Self::window_size_from(content_rect), self.monitor_size);
        resolved
    }

    /// Resolve the configured position for the current monitor and window size.
    ///
    /// If an absolute position had to be clamped (e.g. after display scaling changed),
    /// the corrected value is persisted to config.
    pub fn get_position(&mut self, content_rect: Rect) -> (f32, f32) {
        let window_size = Self::window_size_from(content_rect);
        let (resolved, coords) = resolve_position(self.pos, window_size, self.monitor_size);
        if matches!(self.pos, WindowPos::Absolute(..)) && resolved != self.pos {
            self.pos = resolved;
            let mut cfg = config::get();
            cfg.window_pos = self.pos;
            let _ = config::save(cfg);
        }
        coords
    }

    fn process_events(&mut self, ctx: &Context) {
        for (event, _) in self.events.drain_pending() {
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
                Event::MoveWindow(new_pos) => {
                    let final_pos = self.clamp_absolute_pos(new_pos, ctx.content_rect());
                    if self.pos != final_pos {
                        self.pos = final_pos;
                        let mut cfg = config::get();
                        cfg.window_pos = self.pos;
                        let _ = config::save(cfg);
                    }
                }
                Event::Exit => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    pub fn draw_ui(&mut self, ctx: &Context, ui: &mut Ui) {
        let cfg = config::get();
        self.events
            .set_debounce_ms(cfg.event_debounce_ms, cfg.event_debounce_repeat_ms);
        match self.state {
            StateId::Keyboard => {
                keyboard::with_mut(|kb| kb.draw_ui(ctx, ui, &mut self.events));
            }
            StateId::Menu => {
                menu::with_mut(|m| m.draw_ui(ctx, ui, &mut self.events));
            }
            StateId::MoveWindow => {
                let (x, y) = self.get_position(ctx.content_rect());
                move_window::with_mut(|mw| {
                    let movement = mw.draw_ui(ctx, ui, (x, y), &mut self.events);
                    if let Some(new_pos) = movement {
                        let final_pos = self.clamp_absolute_pos(new_pos, ctx.content_rect());

                        if self.pos != final_pos {
                            self.pos = final_pos;
                            let mut cfg = config::get();
                            cfg.window_pos = self.pos;
                            let _ = config::save(cfg);
                        }
                    }
                });
            }
            StateId::TextInput => {
                text_input::with_mut(|ti| ti.draw_ui(ctx, ui, &mut self.events));
            }
        }
        self.process_events(ctx);
    }

    pub fn handle_controller_input(
        &mut self,
        ctx: &Context,
        input: &Option<Box<dyn ControllerInput>>,
    ) -> Result<()> {
        let cfg = config::get();

        self.events
            .set_debounce_ms(cfg.event_debounce_ms, cfg.event_debounce_repeat_ms);

        if let Some(input) = input {
            if cfg.debug.is_some() {
                ctx.with_plugin::<DebugPlugin, _>(|d| d.controller_input = Some(input.box_clone()));
            }
        }

        match self.state {
            StateId::Keyboard => {
                keyboard::with_mut(|kb| kb.handle_controller_input(input, &mut self.events))?
            }
            StateId::Menu => {
                menu::with_mut(|m| m.handle_controller_input(ctx, input, &mut self.events))?
            }
            StateId::MoveWindow => {
                let (x, y) = self.get_position(ctx.content_rect());
                move_window::with_mut(|mw| {
                    mw.handle_controller_input(ctx, input, (x, y), &mut self.events)
                })?
            }
            StateId::TextInput => {
                text_input::with_mut(|ti| ti.handle_controller_input(input, &mut self.events))?
            }
        }
        self.process_events(ctx);
        Ok(())
    }
}

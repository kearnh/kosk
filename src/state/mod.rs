use crate::{
    config,
    controller::record as input_record,
    controller::ControllerInput,
    debug::DebugPlugin,
    state::{
        event::Event,
        event::EventQueue,
        key_sink::{open_key_sink, KeySink},
        window_pos::{capture_pointer_snapshot, resolve_position, PointerSnapshot, WindowPos},
    },
};
use anyhow::Result;
use egui::{Context, Rect, Ui};
use serde::{Deserialize, Serialize};

pub mod actions;
mod event;
pub(crate) mod key_sink;
pub(crate) mod keyboard;
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
    pointer_snapshot: Option<PointerSnapshot>,
    events: EventQueue,
    key_sink: Box<dyn KeySink>,
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
            pointer_snapshot: None,
            events: EventQueue::new(),
            key_sink: open_key_sink()?,
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
    /// the corrected value is persisted to config. `MousePointer` is snapped once
    /// at launch and not written back as coordinates.
    pub fn get_position(&mut self, content_rect: Rect, pixels_per_point: f32) -> (f32, f32) {
        let window_size = Self::window_size_from(content_rect);
        if self.pos == WindowPos::MousePointer {
            if self.pointer_snapshot.is_none() {
                self.pointer_snapshot = capture_pointer_snapshot();
            }
            if let Some(snap) = &mut self.pointer_snapshot {
                return snap.coords_points(window_size, pixels_per_point);
            }
        }
        let (resolved, coords) = resolve_position(self.pos, window_size, self.monitor_size);
        if matches!(self.pos, WindowPos::Absolute(..)) && resolved != self.pos {
            self.pos = resolved;
            let mut cfg = config::get();
            cfg.window_pos = self.pos;
            let _ = config::save(cfg);
        }
        coords
    }

    fn flip_pointer(&mut self, ctx: &Context, vertical: bool) {
        if self.pos != WindowPos::MousePointer {
            return;
        }
        if self.pointer_snapshot.is_none() {
            self.pointer_snapshot = capture_pointer_snapshot();
        }
        let Some(snap) = self.pointer_snapshot.as_mut() else {
            return;
        };
        let window_size = Self::window_size_from(ctx.content_rect());
        let ppp = ctx.pixels_per_point();
        if vertical {
            snap.flip_vertical(window_size, ppp);
        } else {
            snap.flip_horizontal(window_size, ppp);
        }
    }

    fn process_events(&mut self, ctx: &Context) {
        for (event, _) in self.events.drain_pending() {
            match event {
                Event::SendKey(key, direction) => {
                    if let Err(e) = self.key_sink.key(key, direction) {
                        eprintln!("key sink: {e:#}");
                    }
                }
                Event::SendText(text) => {
                    if let Err(e) = self.key_sink.text(&text) {
                        eprintln!("key sink: {e:#}");
                    }
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
                Event::FlipWindowLeftRight => {
                    self.flip_pointer(ctx, false);
                }
                Event::FlipWindowAboveBelow => {
                    self.flip_pointer(ctx, true);
                }
                Event::Exit => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Event::ToggleRecord => {
                    if let Err(e) = toggle_recording() {
                        eprintln!("toggleRecord: {e:#}");
                    }
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
                let (x, y) = self.get_position(ctx.content_rect(), ctx.pixels_per_point());
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
                let (x, y) = self.get_position(ctx.content_rect(), ctx.pixels_per_point());
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

    pub fn apply_replay_header(
        &mut self,
        header: &crate::controller::record::TapeHeader,
    ) -> Result<()> {
        keyboard::with_mut(|kb| kb.install_recorded_layouts(header))
    }
}

fn toggle_recording() -> Result<()> {
    let session = input_record::session();
    if session.is_replay() {
        eprintln!("toggleRecord: ignored on replay controller");
        return Ok(());
    }
    if session.is_recording() {
        session.stop();
        eprintln!("recording stopped");
        return Ok(());
    }
    let cfg = config::get();
    let template = cfg
        .record_file
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("record_file is not set in config"))?;
    input_record::validate_record_template(template)?;
    let resolved = input_record::resolve_against_config_dir(template)?;
    let path = input_record::next_record_path(&resolved)?;
    let header = keyboard::with_mut(|kb| kb.tape_header());
    session.start(path.clone(), header)?;
    eprintln!("recording {}", path.display());
    Ok(())
}

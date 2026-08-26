use crate::{
    config,
    controller::record as input_record,
    controller::ControllerInput,
    debug::DebugPlugin,
    state::{
        event::{CallRequest, Event, EventQueue, ReturnStateResult},
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
mod mappings;
mod menu;
mod menu_action;
mod move_window;
mod move_window_action;
pub mod os_focus;
mod select_key;
mod select_layout;
mod select_layout_action;
mod text_input;
mod text_input_action;
pub mod window_pos;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Hash, strum::VariantNames)]
pub enum StateId {
    Menu,
    Keyboard,
    MoveWindow,
    TextInput,
    Mappings,
    SelectKey,
    SelectLayout,
}

pub struct AppState {
    state: StateId,
    /// Modes pushed by `CallState`, most-recent callee's caller at the end.
    call_stack: Vec<StateId>,
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
        select_layout::init()?;
        text_input::init()?;
        select_key::init()?;
        mappings::init()?;

        Ok(Self {
            state: StateId::Keyboard,
            call_stack: Vec::new(),
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

    pub fn current_state(&self) -> StateId {
        self.state
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

    fn rotate_pointer(&mut self, ctx: &Context) {
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
        snap.rotate(window_size, ctx.pixels_per_point());
    }

    fn process_events(&mut self, ctx: &Context, holdover: Option<&dyn ControllerInput>) {
        /// Max leaf events handled in one `process_events` call (follow-up storms).
        const PROCESS_EVENTS_BUDGET: usize = 32;

        let mut processed = 0usize;
        loop {
            let batch = self.events.drain_pending();
            if batch.is_empty() {
                break;
            }
            let mut follow_up = EventQueue::passthrough();
            for (event, _source) in batch {
                processed += 1;
                if processed > PROCESS_EVENTS_BUDGET {
                    eprintln!(
                        "process_events: exceeded budget of {PROCESS_EVENTS_BUDGET} events; dropping remainder"
                    );
                    follow_up.drain_pending();
                    let dropped = self.events.drain_pending().len();
                    if dropped > 0 {
                        eprintln!("process_events: dropped {dropped} further pending event(s)");
                    }
                    return;
                }
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
                        self.switch_state(state, holdover);
                        ctx.request_repaint();
                    }
                    Event::CallState(request) => {
                        self.call_state(request, holdover);
                        ctx.request_repaint();
                    }
                    Event::ReturnState(result) => {
                        self.return_state(result, holdover, &mut follow_up);
                        ctx.request_repaint();
                    }
                    Event::Repaint => {
                        ctx.request_repaint();
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
                    Event::RotateWindow => {
                        self.rotate_pointer(ctx);
                    }
                    Event::Exit => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Event::ToggleRecord => {
                        if let Err(e) = toggle_recording() {
                            eprintln!("toggleRecord: {e:#}");
                        }
                    }
                    Event::ToggleShift => {
                        keyboard::with_mut(|kb| kb.toggle_shift());
                    }
                    Event::ToggleCtrl => {
                        keyboard::with_mut(|kb| kb.toggle_ctrl());
                    }
                    Event::ToggleAlt => {
                        keyboard::with_mut(|kb| kb.toggle_alt());
                    }
                }
            }
            self.events.extend_pending(follow_up.drain_pending());
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
            StateId::Mappings => {
                mappings::with_mut(|m| m.draw_ui(ctx, ui, &mut self.events));
            }
            StateId::SelectKey => {
                select_key::with_mut(|s| s.draw_ui(ctx, ui, &mut self.events));
            }
            StateId::SelectLayout => {
                select_layout::with_mut(|s| s.draw_ui(ctx, ui, &mut self.events));
            }
        }
        self.process_events(ctx, None);
    }

    fn switch_state(&mut self, state: StateId, holdover: Option<&dyn ControllerInput>) {
        self.call_stack.clear();
        if state == StateId::Mappings {
            mappings::with_mut(|m| m.begin_session());
        }
        if state == StateId::SelectLayout {
            select_layout::with_mut(|s| s.begin());
        }
        self.state = state;
        self.reset_current_mode_controller(holdover);
    }

    fn call_state(&mut self, request: CallRequest, holdover: Option<&dyn ControllerInput>) {
        self.call_stack.push(self.state);
        self.state = request.callee();
        match request {
            CallRequest::SelectKey {
                binding,
                action,
                mode,
                draft_mode,
                editing,
            } => select_key::with_mut(|s| s.begin(binding, action, mode, draft_mode, editing)),
        }
        self.reset_current_mode_controller(holdover);
    }

    fn return_state(
        &mut self,
        result: ReturnStateResult,
        holdover: Option<&dyn ControllerInput>,
        follow_up: &mut EventQueue,
    ) {
        let Some(caller) = self.call_stack.pop() else {
            eprintln!("ReturnState with empty call_stack; ignored");
            return;
        };
        self.state = caller;
        self.reset_current_mode_controller(holdover);
        match caller {
            // FIXME(HACK): ReturnState mutates the caller's process-wide singleton (Mappings)
            // to deliver SelectKey's result. Prefer stack-owned session frames (or another
            // AppState-owned return channel that does not poke mode singletons) so resume
            // does not require `mappings::with_mut` side effects. See mappings SelectKey plan.
            StateId::Mappings => mappings::with_mut(|m| m.on_return(result, follow_up)),
            _ => {
                let _ = result;
            }
        }
    }

    fn reset_current_mode_controller(&mut self, holdover: Option<&dyn ControllerInput>) {
        match self.state {
            StateId::Keyboard => keyboard::with_mut(|kb| kb.reset_controller_input(holdover)),
            StateId::Menu => menu::with_mut(|m| m.reset_controller_input(holdover)),
            StateId::MoveWindow => move_window::with_mut(|mw| mw.reset_controller_input(holdover)),
            StateId::TextInput => text_input::with_mut(|ti| ti.reset_controller_input(holdover)),
            StateId::Mappings => mappings::with_mut(|m| m.reset_controller_input(holdover)),
            StateId::SelectKey => select_key::with_mut(|s| s.reset_controller_input(holdover)),
            StateId::SelectLayout => {
                select_layout::with_mut(|s| s.reset_controller_input(holdover))
            }
        }
    }

    pub fn handle_controller_input(
        &mut self,
        ctx: &Context,
        input: &dyn ControllerInput,
    ) -> Result<()> {
        let cfg = config::get();

        self.events
            .set_debounce_ms(cfg.event_debounce_ms, cfg.event_debounce_repeat_ms);

        if cfg.debug.is_some() {
            ctx.with_plugin::<DebugPlugin, _>(|d| d.controller_input = Some(input.box_clone()));
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
            StateId::Mappings => {
                mappings::with_mut(|m| m.handle_controller_input(ctx, input, &mut self.events))?
            }
            StateId::SelectKey => {
                select_key::with_mut(|s| s.handle_controller_input(input, &mut self.events))?
            }
            StateId::SelectLayout => select_layout::with_mut(|s| {
                s.handle_controller_input(ctx, input, &mut self.events)
            })?,
        }
        self.events.end_controller_tick();
        self.process_events(ctx, Some(input));
        Ok(())
    }

    /// Iterator yielded idle (`None`): clear edge baselines; do not run handle.
    pub fn reset_controller_input(&mut self, ctx: &Context) -> Result<()> {
        self.reset_current_mode_controller(None);
        if config::get().debug.is_some() {
            ctx.with_plugin::<DebugPlugin, _>(|d| d.controller_input = None);
        }
        self.events.end_controller_tick();
        self.process_events(ctx, None);
        Ok(())
    }

    pub fn apply_replay_header(
        &mut self,
        header: &crate::controller::record::TapeHeader,
    ) -> Result<()> {
        config::apply_recorded_tape_config(header)?;
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
    let header = keyboard::with_mut(|kb| kb.tape_header())?;
    session.start(path.clone(), header)?;
    eprintln!("recording {}", path.display());
    Ok(())
}

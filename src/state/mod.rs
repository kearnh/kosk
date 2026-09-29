use crate::{
    config,
    controller::record as input_record,
    controller::{ControllerInput, ControllerKind},
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
use std::sync::Arc;

pub mod actions;
mod completion_ui;
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
mod settings_form;
mod text_input;
mod text_input_action;
pub mod toasts;
pub mod window_pos;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControllerConnection {
    #[default]
    Searching,
    Sc2,
    Ds4,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Hash, strum::VariantNames)]
pub enum StateId {
    Settings,
    Keyboard,
    MoveWindow,
    TextInput,
    Mappings,
    SelectKey,
    SelectLayout,
}

impl StateId {
    pub(crate) fn wire_id(self) -> String {
        let pascal = serde_plain::to_string(&self).unwrap_or_else(|_| format!("{self:?}"));
        actions::to_camel(&pascal)
    }
}

pub struct AppState {
    state: StateId,
    /// Modes pushed by `CallState`, most-recent callee's caller at the end.
    call_stack: Vec<StateId>,
    pos: WindowPos,
    /// Placement when MoveWindow was entered; restored on cancel.
    move_origin: Option<WindowPos>,
    /// True after analog motion in the current MoveWindow visit.
    moved_during_place: bool,
    controller_kind: ControllerKind,
    monitor_size: (f32, f32),
    pointer_snapshot: Option<PointerSnapshot>,
    events: EventQueue,
    key_sink: Box<dyn KeySink>,
    unsupported_checked_for: Option<ControllerKind>,
}

impl AppState {
    pub fn new(monitor_size: (f32, f32)) -> Result<Self> {
        let cfg = config::get();

        keyboard::init()?;
        if config::mcp_controller_mode() || config::preferred_is_replay() {
            keyboard::with_mut(|kb| kb.set_controller_connection(ControllerConnection::Hidden));
        }
        move_window::init()?;
        menu::init()?;
        select_layout::init()?;
        text_input::init()?;
        select_key::init()?;
        mappings::init()?;
        crate::completion::init()?;

        Ok(Self {
            state: StateId::Keyboard,
            call_stack: Vec::new(),
            pos: if config::cli_at_mouse() {
                WindowPos::MousePointer
            } else {
                cfg.window_pos
            },
            move_origin: None,
            moved_during_place: false,
            controller_kind: ControllerKind::Sc2,
            monitor_size,
            pointer_snapshot: None,
            events: EventQueue::new(),
            key_sink: open_key_sink()?,
            unsupported_checked_for: None,
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
            if self.state != StateId::MoveWindow {
                let mut cfg = config::get();
                cfg.window_pos = self.pos;
                let _ = config::save(cfg);
            }
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
                            crate::user_notify::note_type_failure();
                        }
                    }
                    Event::SendText(text) => {
                        if let Err(e) = self.key_sink.text(&text) {
                            eprintln!("key sink: {e:#}");
                            crate::user_notify::note_type_failure();
                        }
                    }
                    Event::ChangeState(state) => {
                        if self.state == StateId::MoveWindow {
                            if let Some(origin) = self.move_origin.take() {
                                self.pos = origin;
                            }
                            self.moved_during_place = false;
                        }
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
                    Event::SaveWindowPos => {
                        let persist = move_window::pos_to_persist(
                            self.move_origin.unwrap_or(self.pos),
                            self.pos,
                            self.moved_during_place,
                        );
                        let final_pos = self.clamp_absolute_pos(persist, ctx.content_rect());
                        self.pos = final_pos;
                        self.move_origin = None;
                        self.moved_during_place = false;
                        let mut cfg = config::get();
                        cfg.window_pos = self.pos;
                        let _ = config::save(cfg);
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
                        if let Err(e) = toggle_recording(self.controller_kind) {
                            eprintln!("toggleRecord: {e:#}");
                            crate::user_notify::notify(crate::user_notify::Notice::record_failed());
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
        let ctx_notify = ctx.clone();
        if let Err(e) = crate::completion::ensure(Arc::new(move || {
            ctx_notify.request_repaint();
        })) {
            eprintln!("completion ensure: {e:#}");
        }
        let shown = crate::completion::with_mut(|s| {
            let Some(s) = s else {
                return Vec::new();
            };
            s.tick();
            s.take_shown()
        });
        if input_record::session().is_recording() {
            for shot in shown {
                let chips: Vec<input_record::RecordedChip> = shot
                    .chips
                    .into_iter()
                    .map(|c| input_record::RecordedChip {
                        text: c.text,
                        current_word: c.current_word,
                    })
                    .collect();
                input_record::session().tap_suggestions(&shot.prefix, &chips);
            }
        }

        let cfg = config::get();
        self.events
            .set_debounce_ms(cfg.event_debounce_ms, cfg.event_debounce_repeat_ms);
        match self.state {
            StateId::Keyboard => {
                keyboard::with_mut(|kb| kb.draw_ui(ctx, ui, &mut self.events));
            }
            StateId::Settings => {
                menu::with_mut(|m| m.draw_ui(ctx, ui, &mut self.events, self.controller_kind));
                menu::flush_pending_notify();
            }
            StateId::MoveWindow => {
                move_window::with_mut(|mw| {
                    mw.draw_ui(ui, self.controller_kind, &mut self.events);
                });
            }
            StateId::TextInput => {
                text_input::with_mut(|ti| ti.draw_ui(ctx, ui, &mut self.events));
            }
            StateId::Mappings => {
                mappings::with_mut(|m| m.draw_ui(ctx, ui, &mut self.events, self.controller_kind));
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
        if state == StateId::MoveWindow {
            self.move_origin = Some(self.pos);
            self.moved_during_place = false;
            move_window::with_mut(|mw| mw.begin());
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
            StateId::Settings => menu::with_mut(|m| m.reset_controller_input(holdover)),
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

        self.controller_kind = input.family();

        match crate::user_notify::offer_input(input) {
            crate::user_notify::InputOutcome::Passthrough => {}
            crate::user_notify::InputOutcome::Consumed => {
                self.reset_current_mode_controller(Some(input));
                self.events.end_controller_tick();
                return Ok(());
            }
            crate::user_notify::InputOutcome::Swallowed { notice, open_guide } => {
                if toasts::is_tip_acknowledge(&notice.key) {
                    toasts::acknowledge_tip();
                }
                if open_guide {
                    if let Err(e) = toasts::open_setup_guide() {
                        eprintln!("setup guide: {e:#}");
                        crate::user_notify::notify(crate::user_notify::Notice::guide_failed());
                    }
                }
                self.reset_current_mode_controller(Some(input));
                self.events.end_controller_tick();
                return Ok(());
            }
        }

        // The merged config always carries `[debug]` (as before, when the
        // built-in TOML supplied it), so the plugin always gets its input.
        ctx.with_plugin::<DebugPlugin, _>(|d| d.controller_input = Some(input.box_clone()));

        match self.state {
            StateId::Keyboard => {
                keyboard::with_mut(|kb| kb.handle_controller_input(input, &mut self.events))?
            }
            StateId::Settings => {
                menu::with_mut(|m| {
                    m.handle_controller_input(ctx, input, &mut self.events, self.controller_kind)
                })?;
                menu::flush_pending_notify();
            }
            StateId::MoveWindow => {
                let (x, y) = self.get_position(ctx.content_rect(), ctx.pixels_per_point());
                let window_size = Self::window_size_from(ctx.content_rect());
                move_window::with_mut(|mw| {
                    if let Some((nx, ny)) =
                        mw.apply_analog(input, (x, y), window_size, self.monitor_size)
                    {
                        self.pos = WindowPos::Absolute(nx, ny);
                        self.moved_during_place = true;
                        ctx.request_repaint();
                    }
                    mw.handle_controller_input(input, &mut self.events)
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
        if self.unsupported_checked_for != Some(self.controller_kind) {
            self.unsupported_checked_for = Some(self.controller_kind);
            toasts::check_unsupported(self.controller_kind);
        }
        Ok(())
    }

    pub fn note_battery(&mut self, input: &dyn ControllerInput) {
        self.controller_kind = input.family();
        keyboard::with_mut(|kb| kb.note_battery(input.battery()));
    }

    pub fn set_controller_connection(&mut self, connection: ControllerConnection) {
        keyboard::with_mut(|kb| kb.set_controller_connection(connection));
    }

    /// Iterator yielded idle (`None`): clear edge baselines; do not run handle.
    pub fn reset_controller_input(&mut self, ctx: &Context) -> Result<()> {
        if self.state == StateId::MoveWindow {
            let (x, y) = self.get_position(ctx.content_rect(), ctx.pixels_per_point());
            let window_size = Self::window_size_from(ctx.content_rect());
            move_window::with_mut(|mw| {
                if let Some((nx, ny)) = mw.flush_pad_lift((x, y), window_size, self.monitor_size) {
                    self.pos = WindowPos::Absolute(nx, ny);
                    ctx.request_repaint();
                }
            });
        }
        self.reset_current_mode_controller(None);
        crate::user_notify::note_no_input();
        // See above: `[debug]` is always present in the merged config.
        ctx.with_plugin::<DebugPlugin, _>(|d| d.controller_input = None);
        self.events.end_controller_tick();
        self.process_events(ctx, None);
        Ok(())
    }

    pub fn apply_replay_header(
        &mut self,
        header: &crate::controller::record::TapeHeader,
    ) -> Result<()> {
        config::set_replay_aim_family(header.controller);
        config::apply_recorded_tape_config(header)?;
        keyboard::with_mut(|kb| kb.install_recorded_layouts(header))
    }
}

fn toggle_recording(kind: crate::controller::ControllerKind) -> Result<()> {
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
    let template = match cfg.record_file.as_ref() {
        Some(template) => template.clone(),
        None => {
            crate::user_notify::notify(crate::user_notify::Notice::record_needs_location());
            return Ok(());
        }
    };
    input_record::validate_record_template(&template)?;
    let resolved = input_record::resolve_against_config_dir(&template)?;
    let path = input_record::next_record_path(&resolved)?;
    let header = keyboard::with_mut(|kb| kb.tape_header(kind))?;
    session.start(path.clone(), header)?;
    eprintln!("recording {}", path.display());
    Ok(())
}

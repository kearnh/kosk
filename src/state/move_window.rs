use crate::{
    config,
    controller::ControllerInput,
    state::{
        actions::load_bindings,
        event::{Event, EventQueue, EventSource},
        StateId, WindowPos,
    },
};
use anyhow::Result;
use egui::{Button, Context, Label, Ui};
use std::sync::{Mutex, OnceLock};

use crate::controller::bindings::BindingEngine;

use crate::state::move_window_action::MoveWindowAction;

const NUDGE: f32 = 100.0;

pub struct MoveWindowState {
    bindings: BindingEngine<MoveWindowAction>,
}

impl MoveWindowState {
    pub fn new() -> Result<Self> {
        Ok(Self {
            bindings: load_bindings(StateId::MoveWindow)?,
        })
    }

    pub fn draw_ui(
        &mut self,
        _ctx: &Context,
        ui: &mut Ui,
        current_coords: (f32, f32),
        events: &mut EventQueue,
    ) -> Option<WindowPos> {
        let cfg = config::get();
        let mut movement: Option<WindowPos> = None;
        let (x, y) = current_coords;

        ui.vertical_centered(|ui| {
            ui.heading("Move Window");

            egui::Grid::new("move_grid")
                .spacing([10.0, 10.0])
                .show(ui, |ui| {
                    let size = [1.5 * cfg.scale_x, 1.5 * cfg.scale_y];

                    if ui.add_sized(size, Button::new("\u{25f0}")).clicked() {
                        movement = Some(WindowPos::TopLeft);
                    }
                    if ui.add_sized(size, Button::new("↑")).clicked() {
                        movement = Some(WindowPos::Absolute(x, y - NUDGE));
                    }
                    if ui.add_sized(size, Button::new("\u{25f3}")).clicked() {
                        movement = Some(WindowPos::TopRight);
                    }
                    ui.end_row();

                    if ui.add_sized(size, Button::new("←")).clicked() {
                        movement = Some(WindowPos::Absolute(x - NUDGE, y));
                    }
                    ui.add_sized(size, Label::new(""));
                    if ui.add_sized(size, Button::new("→")).clicked() {
                        movement = Some(WindowPos::Absolute(x + NUDGE, y));
                    }
                    ui.end_row();

                    if ui.add_sized(size, Button::new("\u{25f1}")).clicked() {
                        movement = Some(WindowPos::BottomLeft);
                    }
                    if ui.add_sized(size, Button::new("↓")).clicked() {
                        movement = Some(WindowPos::Absolute(x, y + NUDGE));
                    }
                    if ui.add_sized(size, Button::new("\u{25f2}")).clicked() {
                        movement = Some(WindowPos::BottomRight);
                    }
                    ui.end_row();
                });

            if ui.button("Back").clicked() {
                let _ = events.push(Event::ChangeState(StateId::Menu), &EventSource::MouseClick);
            }
        });

        movement
    }

    fn do_action(
        &self,
        action: &MoveWindowAction,
        coords: (f32, f32),
        events: &mut EventQueue,
        source: &EventSource,
    ) -> Result<()> {
        use MoveWindowAction::*;
        let (x, y) = coords;
        match action {
            SnapTopLeft => {
                let _ = events.push(Event::MoveWindow(WindowPos::TopLeft), source);
            }
            SnapTopRight => {
                let _ = events.push(Event::MoveWindow(WindowPos::TopRight), source);
            }
            SnapBottomLeft => {
                let _ = events.push(Event::MoveWindow(WindowPos::BottomLeft), source);
            }
            SnapBottomRight => {
                let _ = events.push(Event::MoveWindow(WindowPos::BottomRight), source);
            }
            FlipWindowLeftRight => {
                let _ = events.push(Event::FlipWindowLeftRight, source);
            }
            FlipWindowAboveBelow => {
                let _ = events.push(Event::FlipWindowAboveBelow, source);
            }
            RotateWindow => {
                let _ = events.push(Event::RotateWindow, source);
            }
            NudgeUp => {
                let _ = events.push(Event::MoveWindow(WindowPos::Absolute(x, y - NUDGE)), source);
            }
            NudgeDown => {
                let _ = events.push(Event::MoveWindow(WindowPos::Absolute(x, y + NUDGE)), source);
            }
            NudgeLeft => {
                let _ = events.push(Event::MoveWindow(WindowPos::Absolute(x - NUDGE, y)), source);
            }
            NudgeRight => {
                let _ = events.push(Event::MoveWindow(WindowPos::Absolute(x + NUDGE, y)), source);
            }
            SwitchState(state) => {
                let _ = events.push(Event::ChangeState(*state), source);
            }
        }
        Ok(())
    }

    pub fn reset_controller_input(&mut self, holdover: Option<&dyn ControllerInput>) {
        self.bindings.reset(holdover);
    }

    pub fn handle_controller_input(
        &mut self,
        _ctx: &Context,
        input: &dyn ControllerInput,
        coords: (f32, f32),
        events: &mut EventQueue,
    ) -> Result<()> {
        for (binding, action) in self.bindings.evaluate(input) {
            let src = EventSource::Controller(binding);
            self.do_action(&action, coords, events, &src)?;
        }

        Ok(())
    }

    fn reload_from_config(&mut self) -> Result<()> {
        self.bindings = load_bindings(StateId::MoveWindow)?;
        Ok(())
    }
}

static MOVE_WINDOW: OnceLock<Mutex<MoveWindowState>> = OnceLock::new();

pub(crate) fn with_mut<R>(f: impl FnOnce(&mut MoveWindowState) -> R) -> R {
    let mut guard = MOVE_WINDOW
        .get()
        .expect("move window state not initialized")
        .lock()
        .unwrap();
    f(&mut guard)
}

pub fn init() -> Result<()> {
    MOVE_WINDOW
        .set(Mutex::new(MoveWindowState::new()?))
        .map_err(|_| anyhow::anyhow!("move window state already initialized"))?;

    crate::config::on_changed(|| {
        let result = with_mut(|mw| mw.reload_from_config());
        if let Err(e) = result {
            eprintln!("Failed to reload move window from config: {}", e);
        }
    })?;

    Ok(())
}

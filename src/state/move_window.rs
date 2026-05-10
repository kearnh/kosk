use crate::{
    config,
    controller::ControllerInput,
    state::{event::Event, StateId, WindowPos},
};
use anyhow::Result;
use egui::{Button, Context, Label, Ui};
use std::sync::{Mutex, OnceLock};

pub struct MoveWindowState {
    scale_x: f32,
    scale_y: f32,
}

impl MoveWindowState {
    pub fn new(scale_x: f32, scale_y: f32) -> Self {
        Self { scale_x, scale_y }
    }

    pub fn reload_from_config(&mut self) {
        let cfg = config::get();
        self.scale_x = cfg.scale_x;
        self.scale_y = cfg.scale_y;
    }

    pub fn draw_ui(
        &mut self,
        _ctx: &Context,
        ui: &mut Ui,
        current_coords: (f32, f32),
        events: &mut Vec<Event>,
    ) -> Option<WindowPos> {
        let mut movement: Option<WindowPos> = None;
        let (x, y) = current_coords;

        ui.vertical_centered(|ui| {
            ui.heading("Move Window");

            egui::Grid::new("move_grid")
                .spacing([10.0, 10.0])
                .show(ui, |ui| {
                    let size = [1.5 * self.scale_x, 1.5 * self.scale_y];

                    // Row 1
                    if ui.add_sized(size, Button::new("\u{25f0}")).clicked() {
                        movement = Some(WindowPos::TopLeft);
                    }
                    if ui.add_sized(size, Button::new("↑")).clicked() {
                        movement = Some(WindowPos::Absolute(x, y - 100.0));
                    }
                    if ui.add_sized(size, Button::new("\u{25f3}")).clicked() {
                        movement = Some(WindowPos::TopRight);
                    }
                    ui.end_row();

                    // Row 2
                    if ui.add_sized(size, Button::new("←")).clicked() {
                        movement = Some(WindowPos::Absolute(x - 100.0, y));
                    }
                    ui.add_sized(size, Label::new(""));
                    if ui.add_sized(size, Button::new("→")).clicked() {
                        movement = Some(WindowPos::Absolute(x + 100.0, y));
                    }
                    ui.end_row();

                    // Row 3
                    if ui.add_sized(size, Button::new("\u{25f1}")).clicked() {
                        movement = Some(WindowPos::BottomLeft);
                    }
                    if ui.add_sized(size, Button::new("↓")).clicked() {
                        movement = Some(WindowPos::Absolute(x, y + 100.0));
                    }
                    if ui.add_sized(size, Button::new("\u{25f2}")).clicked() {
                        movement = Some(WindowPos::BottomRight);
                    }
                    ui.end_row();
                });

            if ui.button("Back").clicked() {
                events.push(Event::ChangeState(StateId::Menu));
            }
        });

        movement
    }

    pub fn handle_controller_input(
        &mut self,
        _ctx: &Context,
        input: &Option<Box<dyn ControllerInput>>,
        events: &mut Vec<Event>,
    ) -> Result<()> {
        if let Some(input) = input {
            if input.face_right() {
                events.push(Event::ChangeState(StateId::Menu));
            }
        }
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
    let cfg = config::get();
    let mw = MoveWindowState::new(cfg.scale_x, cfg.scale_y);
    MOVE_WINDOW
        .set(Mutex::new(mw))
        .map_err(|_| anyhow::anyhow!("move window state already initialized"))?;

    crate::config::on_changed(|| {
        with_mut(|mw| mw.reload_from_config());
    })?;

    Ok(())
}

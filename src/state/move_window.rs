use crate::{
    controller::ControllerInput,
    state::{StateId, WindowPos},
};
use anyhow::Result;
use egui::{Button, Context, Label, Ui};

pub struct MoveWindowState {
    scale_x: f32,
    scale_y: f32,
}

impl MoveWindowState {
    pub fn new(scale_x: f32, scale_y: f32) -> Self {
        Self { scale_x, scale_y }
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
                        movement = Some(WindowPos::Absolute(x - 100.0, nun_close));
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

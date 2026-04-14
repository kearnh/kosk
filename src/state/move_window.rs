use crate::ps4::Ps4InputData;
use crate::state::{StateId, WindowPos};
use anyhow::Result;
use egui::{Context, Ui};

pub struct MoveWindowState;

impl MoveWindowState {
    pub fn new() -> Self {
        Self
    }

    pub fn draw_ui(
        &mut self,
        _ctx: &Context,
        ui: &mut Ui,
        current_coords: (f32, f32),
    ) -> (StateId, Option<WindowPos>) {
        let mut movement: Option<WindowPos> = None;
        let (x, y) = current_coords;

        let next_state = ui
            .vertical_centered(|ui| {
                ui.heading("Move Window");

                egui::Grid::new("move_grid")
                    .spacing([10.0, 10.0])
                    .show(ui, |ui| {
                        // Row 1
                        if ui.button("↖").clicked() {
                            movement = Some(WindowPos::TopLeft);
                        }
                        ui.label("");
                        if ui.button("↑ B").clicked() {
                            movement = Some(WindowPos::Absolute(x, y - 100.0));
                        }
                        ui.label("");
                        if ui.button("↗").clicked() {
                            movement = Some(WindowPos::TopRight);
                        }
                        ui.end_row();

                        // Row 2
                        ui.label("");
                        if ui.button("↑ s").clicked() {
                            movement = Some(WindowPos::Absolute(x, y - 10.0));
                        }
                        ui.label("");
                        ui.label("");
                        ui.label("");
                        ui.end_row();

                        // Row 3
                        if ui.button("← B").clicked() {
                            movement = Some(WindowPos::Absolute(x - 100.0, y));
                        }
                        if ui.button("← s").clicked() {
                            movement = Some(WindowPos::Absolute(x - 10.0, y));
                        }
                        ui.label("");
                        if ui.button("→ s").clicked() {
                            movement = Some(WindowPos::Absolute(x + 10.0, y));
                        }
                        if ui.button("→ B").clicked() {
                            movement = Some(WindowPos::Absolute(x + 100.0, y));
                        }
                        ui.end_row();

                        // Row 4
                        ui.label("");
                        if ui.button("↓ s").clicked() {
                            movement = Some(WindowPos::Absolute(x, y + 10.0));
                        }
                        ui.label("");
                        ui.label("");
                        ui.label("");
                        ui.end_row();

                        // Row 5
                        if ui.button("↙").clicked() {
                            movement = Some(WindowPos::BottomLeft);
                        }
                        ui.label("");
                        if ui.button("↓ B").clicked() {
                            movement = Some(WindowPos::Absolute(x, y + 100.0));
                        }
                        ui.label("");
                        if ui.button("↘").clicked() {
                            movement = Some(WindowPos::BottomRight);
                        }
                        ui.end_row();
                    });

                if ui.button("Back").clicked() {
                    return StateId::Menu;
                }
                StateId::MoveWindow
            })
            .inner;

        (next_state, movement)
    }

    pub fn handle_controller_input(
        &mut self,
        _ctx: &Context,
        input: &Option<Ps4InputData>,
    ) -> Result<StateId> {
        if let Some(input) = input {
            if input.circle {
                return Ok(StateId::Menu);
            }
        }
        Ok(StateId::MoveWindow)
    }
}

use crate::state::StateId;
use crate::ps4::Ps4InputData;
use egui::{Context, Ui};
use anyhow::Result;

pub struct MoveWindowState;

impl MoveWindowState {
    pub fn new() -> Self { Self }

    pub fn draw_ui(&mut self, _ctx: &Context, ui: &mut Ui) -> (StateId, Option<(f32, f32)>) {
        let mut movement = None;
        
        let next_state = ui.vertical_centered(|ui| {
            ui.heading("Move Window");
            
            egui::Grid::new("move_grid").spacing([10.0, 10.0]).show(ui, |ui| {
                // Row 1: Corner, Big, Corner
                if ui.button("↖").clicked() { movement = Some((-10.0, -10.0)); }
                if ui.button("↑ (Big)").clicked() { movement = Some((0.0, -50.0)); }
                if ui.button("↗").clicked() { movement = Some((10.0, -10.0)); }
                ui.end_row();

                // Row 2: Big, Small, Big
                if ui.button("← (Big)").clicked() { movement = Some((-50.0, 0.0)); }
                if ui.button("↑").clicked() { movement = Some((0.0, -10.0)); }
                if ui.button("→ (Big)").clicked() { movement = Some((50.0, 0.0)); }
                ui.end_row();

                // Row 3: Corner, Small, Corner
                if ui.button("↙").clicked() { movement = Some((-10.0, 10.0)); }
                if ui.button("↓").clicked() { movement = Some((0.0, 10.0)); }
                if ui.button("↘").clicked() { movement = Some((10.0, 10.0)); }
                ui.end_row();
            });

            if ui.button("Back").clicked() { 
                return StateId::Menu; 
            }
            StateId::MoveWindow
        });

        (next_state, movement)
    }

    pub fn handle_controller_input(&mut self, _ctx: &Context, input: &Option<Ps4InputData>) -> Result<StateId> {
        if let Some(input) = input {
            if input.circle { return Ok(StateId::Menu); }
        }
        Ok(StateId::MoveWindow)
    }
}

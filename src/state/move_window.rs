use crate::state::{StateId, WindowPos};
use crate::ps4::Ps4InputData;
use egui::{Context, Ui};
use anyhow::Result;

pub struct MoveWindowState;

impl MoveWindowState {
    pub fn new() -> Self { Self }

    pub fn draw_ui(&mut self, _ctx: &Context, ui: &mut Ui, _current_pos: &WindowPos, current_coords: (f32, f32)) -> (StateId, Option<WindowPos>) {
        let mut movement: Option<WindowPos> = None;
        let (x, y) = current_coords;
        
        let next_state = ui.vertical_centered(|ui| {
            ui.heading("Move Window");
            
            egui::Grid::new("move_grid").spacing([10.0, 10.0]).show(ui, |ui| {
                // Row 1: Corner, Up, Corner
                if ui.button("↖").clicked() { movement = Some(WindowPos::TopLeft); }
                if ui.button("↑").clicked() { movement = Some(WindowPos::Absolute(x, y - 10.0)); }
                if ui.button("↗").clicked() { movement = Some(WindowPos::TopRight); }
                ui.end_row();

                // Row 2: Left, Spacer, Right
                if ui.button("←").clicked() { movement = Some(WindowPos::Absolute(x - 50.0, y)); }
                ui.label(""); 
                if ui.button("→").clicked() { movement = Some(WindowPos::Absolute(x + 50.0, y)); }
                ui.end_row();

                // Row 3: Corner, Down, Corner
                if ui.button("↙").clicked() { movement = Some(WindowPos::BottomLeft); }
                if ui.button("↓").clicked() { movement = Some(WindowPos::Absolute(x, y + 10.0)); }
                if ui.button("↘").clicked() { movement = Some(WindowPos::BottomRight); }
                ui.end_row();
            });

            if ui.button("Back").clicked() { 
                return StateId::Menu; 
            }
            StateId::MoveWindow
        }).inner;

        (next_state, movement)
    }

    pub fn handle_controller_input(&mut self, _ctx: &Context, input: &Option<Ps4InputData>) -> Result<StateId> {
        if let Some(input) = input {
            if input.circle { return Ok(StateId::Menu); }
        }
        Ok(StateId::MoveWindow)
    }
}

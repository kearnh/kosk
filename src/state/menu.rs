use crate::{ps4::Ps4InputData, state::StateId};
use anyhow::Result;
use egui::{Button, Context, Ui};

pub(super) struct MenuState;

impl MenuState {
    pub(super) fn draw_ui(&mut self, ctx: &Context, ui: &mut Ui) -> StateId {
        if ui.add(Button::new("back")).clicked() {
            return StateId::Keyboard;
        }
        StateId::Menu
    }

    pub(super) fn handle_controller_input(
        &mut self,
        ctx: &Context,
        input: &Option<Ps4InputData>,
    ) -> Result<StateId> {
        if let Some(input) = input {
            if input.cross {
                return Ok(StateId::Keyboard);
            }
        }
        Ok(StateId::Menu)
    }
}

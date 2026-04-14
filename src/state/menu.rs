use crate::{
    ps4::{Dpad, Ps4InputData},
    state::StateId,
};
use anyhow::Result;
use egui::{Button, Context, Ui};

struct MenuButton {
    text: &'static str,
    callback: Box<dyn Fn() -> Option<StateId> + Send + Sync>,
}

pub(super) struct MenuState {
    buttons: Vec<MenuButton>,
    selected: usize,
}

fn on_move() -> Option<StateId> {
    Some(StateId::MoveWindow)
}

fn on_back() -> Option<StateId> {
    Some(StateId::Keyboard)
}

impl MenuState {
    pub(super) fn new() -> Self {
        Self {
            buttons: vec![
                MenuButton {
                    text: "Move",
                    callback: Box::new(on_move),
                },
                MenuButton {
                    text: "Back",
                    callback: Box::new(on_back),
                },
            ],
            selected: 0,
        }
    }

    fn btn(&self, ui: &mut Ui, text: &str, n: &mut usize) -> egui::Response {
        let clicked = ui.add(Button::new(text).selected(*n == self.selected));
        *n += 1;
        clicked
    }

    pub(super) fn draw_ui(&mut self, _: &Context, ui: &mut Ui) -> StateId {
        let mut n = 0;

        for b in self.buttons.iter() {
            if self.btn(ui, b.text, &mut n).clicked() {
                if let Some(id) = (b.callback)() {
                    return id;
                }
            }
        }

        StateId::Menu
    }

    pub(super) fn handle_controller_input(
        &mut self,
        _: &Context,
        input: &Option<Ps4InputData>,
    ) -> Result<StateId> {
        let input = match input {
            Some(input) => input,
            None => return Ok(StateId::Menu),
        };

        let n = match input.dpad {
            Some(Dpad::Up) => -1,
            Some(Dpad::Down) => 1,
            _ => 0,
        };
        self.selected = (self.selected as isize + n).rem_euclid(self.buttons.len() as isize) as usize;

        if input.circle {
            return Ok(StateId::Keyboard);
        }

        if input.cross {
            return Ok((self.buttons[self.selected].callback)().unwrap_or(StateId::Menu));
        }

        Ok(StateId::Menu)
    }
}

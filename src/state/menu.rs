use crate::{
    controller::{ControllerInput, Dpad},
    state::StateId,
};
use anyhow::Result;
use egui::{Button, Context, Ui};

struct MenuButton {
    text: &'static str,
    callback: Box<dyn Fn() -> Option<StateId> + Send + Sync>,
}

pub struct MenuState {
    buttons: Vec<MenuButton>,
    selected: usize,
}

impl MenuState {
    pub fn new() -> Self {
        Self {
            buttons: vec![
                MenuButton {
                    text: "Move",
                    callback: Box::new(|| Some(StateId::MoveWindow)),
                },
                MenuButton {
                    text: "Back",
                    callback: Box::new(|| Some(StateId::Keyboard)),
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

    pub fn draw_ui(&mut self, _: &Context, ui: &mut Ui, events: &mut Vec<Event>) {
        let mut n = 0;

        for b in self.buttons.iter() {
            if self.btn(ui, b.text, &mut n).clicked() {
                if let Some(id) = (b.callback)() {
                    events.push(Event::ChangeState(id));
                }
            }
        }
    }

    pub fn handle_controller_input(
        &mut self,
        _: &Context,
        input: &Option<Box<dyn ControllerInput>>,
        events: &mut Vec<Event>,
    ) -> Result<()> {
        let input = match input {
            Some(input) => input,
            None => return Ok(()),
        };

        let n = match input.dpad() {
            Some(Dpad::Up) => -1,
            Some(Dpad::Down) => 1,
            _ => 0,
        };
        self.selected =
            (self.selected as isize + n).rem_euclid(self.buttons.len() as isize) as usize;

        if input.face_right() {
            events.push(Event::ChangeState(StateId::Keyboard));
            return Ok(());
        }

        if input.face_bottom() {
            if let Some(id) = (self.buttons[self.selected].callback)() {
                events.push(Event::ChangeState(id));
            }
            return Ok(());
        }

        Ok(())
    }
}

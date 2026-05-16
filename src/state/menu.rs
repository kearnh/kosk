use crate::{
    controller::{ControllerBinding, ControllerButton, ControllerInput},
    state::{event::Event, event::EventQueue, event::EventSource, StateId},
};
use anyhow::Result;
use egui::{Button, Context, Ui};
use std::sync::{Mutex, OnceLock};

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

    pub fn draw_ui(&mut self, _: &Context, ui: &mut Ui, events: &mut EventQueue) {
        let mut n = 0;

        for b in self.buttons.iter() {
            if self.btn(ui, b.text, &mut n).clicked() {
                if let Some(id) = (b.callback)() {
                    let _ = events.push(Event::ChangeState(id), &EventSource::MouseClick);
                }
            }
        }
    }

    pub fn handle_controller_input(
        &mut self,
        _: &Context,
        input: &Option<Box<dyn ControllerInput>>,
        events: &mut EventQueue,
    ) -> Result<()> {
        let input = match input {
            Some(input) => input,
            None => return Ok(()),
        };

        let n = if input.dpad_up() {
            -1
        } else if input.dpad_down() {
            1
        } else {
            0
        };
        self.selected =
            (self.selected as isize + n).rem_euclid(self.buttons.len() as isize) as usize;

        if input.face_right() {
            let _ = events.push(
                Event::ChangeState(StateId::Keyboard),
                &EventSource::Controller(ControllerBinding::Single(ControllerButton::FaceRight)),
            );
            return Ok(());
        }

        if input.face_bottom() {
            if let Some(id) = (self.buttons[self.selected].callback)() {
                let _ = events.push(
                    Event::ChangeState(id),
                    &EventSource::Controller(ControllerBinding::Single(ControllerButton::FaceBottom)),
                );
            }
            return Ok(());
        }

        Ok(())
    }
}

static MENU: OnceLock<Mutex<MenuState>> = OnceLock::new();

pub fn init() -> Result<()> {
    let menu = MenuState::new();
    MENU.set(Mutex::new(menu))
        .map_err(|_| anyhow::anyhow!("menu state already initialized"))?;
    Ok(())
}

pub(crate) fn with_mut<R>(f: impl FnOnce(&mut MenuState) -> R) -> R {
    let mut guard = MENU
        .get()
        .expect("menu state not initialized")
        .lock()
        .unwrap();
    f(&mut guard)
}

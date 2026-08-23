use crate::{
    controller::ControllerInput,
    state::{
        actions::load_bindings,
        event::{Event, EventQueue, EventSource},
        StateId,
    },
};

use anyhow::Result;
use egui::{Button, Context, Ui};
use std::sync::{Mutex, OnceLock};

use crate::controller::bindings::BindingEngine;
use crate::state::menu_action::MenuAction;

struct MenuButton {
    text: &'static str,
    callback: Box<dyn Fn() -> Option<StateId> + Send + Sync>,
}

pub struct MenuState {
    buttons: Vec<MenuButton>,
    selected: usize,
    bindings: BindingEngine<MenuAction>,
}

impl MenuState {
    pub fn new() -> Result<Self> {
        let state = Self {
            buttons: vec![
                MenuButton {
                    text: "Move",
                    callback: Box::new(|| Some(StateId::MoveWindow)),
                },
                MenuButton {
                    text: "Mappings",
                    callback: Box::new(|| Some(StateId::Mappings)),
                },
                MenuButton {
                    text: "Back",
                    callback: Box::new(|| Some(StateId::Keyboard)),
                },
            ],

            selected: 0,
            bindings: load_bindings(StateId::Menu)?,
        };
        Ok(state)
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

    fn do_action(
        &mut self,
        action: &MenuAction,
        events: &mut EventQueue,
        source: &EventSource,
    ) -> Result<()> {
        use MenuAction::*;
        match action {
            SelectUp => {
                self.selected =
                    (self.selected as isize - 1).rem_euclid(self.buttons.len() as isize) as usize;
            }
            SelectDown => {
                self.selected =
                    (self.selected as isize + 1).rem_euclid(self.buttons.len() as isize) as usize;
            }
            Activate => {
                if let Some(id) = (self.buttons[self.selected].callback)() {
                    let _ = events.push(Event::ChangeState(id), source);
                }
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
        _: &Context,
        input: &dyn ControllerInput,
        events: &mut EventQueue,
    ) -> Result<()> {
        for (binding, action) in self.bindings.evaluate(input) {
            let src = EventSource::Controller(binding);
            self.do_action(&action, events, &src)?;
        }

        Ok(())
    }

    fn reload_from_config(&mut self) -> Result<()> {
        self.bindings = load_bindings(StateId::Menu)?;
        Ok(())
    }
}

static MENU: OnceLock<Mutex<MenuState>> = OnceLock::new();

pub fn init() -> Result<()> {
    let menu = MenuState::new()?;
    MENU.set(Mutex::new(menu))
        .map_err(|_| anyhow::anyhow!("menu state already initialized"))?;

    crate::config::on_changed(|| {
        let result = with_mut(|m| m.reload_from_config());
        if let Err(e) = result {
            eprintln!("Failed to reload menu from config: {}", e);
        }
    })?;

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

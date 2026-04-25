use crate::{
    controller::{ControllerInput, Dpad},
    state::{event::Event, keyboard::key::RawKey, StateId},
};
use anyhow::Result;
use egui::{Context, Ui};
pub use ui::Keyboard;

mod key;
mod layout;
mod ui;

// FIXME why is state spread across this struct and Keyboard?
pub struct KeyboardState {
    pub kb: Keyboard,
    pub l2_was_pressed: bool,
    pub r2_was_pressed: bool,
    pub trigger_threshold: u8,
    pub shift_state: bool,
    pub shift_mod: bool,
    pub ctrl_mod: bool,
    pub alt_mod: bool,
}

impl KeyboardState {
    pub fn new() -> Result<Self> {
        let cfg = crate::config::get();
        let kb = Keyboard::new()?;

        Ok(Self {
            kb,
            l2_was_pressed: false,
            r2_was_pressed: false,
            trigger_threshold: cfg.trigger_threshold,
            shift_state: false,
            shift_mod: false,
            ctrl_mod: false,
            alt_mod: false,
        })
    }

    pub fn send_key(&mut self, key: &RawKey, events: &mut Vec<Event>) -> Result<()> {
        macro_rules! mod_press {
            () => {
                if self.shift_mod {
                    events.push(Event::SendKey(enigo::Key::Shift, enigo::Direction::Press));
                }
                if self.ctrl_mod {
                    events.push(Event::SendKey(enigo::Key::Control, enigo::Direction::Press));
                }
                if self.alt_mod {
                    events.push(Event::SendKey(enigo::Key::Alt, enigo::Direction::Press));
                }
            };
        }
        macro_rules! mod_release {
            () => {
                if self.alt_mod {
                    events.push(Event::SendKey(enigo::Key::Alt, enigo::Direction::Release));
                }
                if self.ctrl_mod {
                    events.push(Event::SendKey(
                        enigo::Key::Control,
                        enigo::Direction::Release,
                    ));
                }
                if self.shift_mod {
                    events.push(Event::SendKey(enigo::Key::Shift, enigo::Direction::Release));
                }
            };
        }
        match key {
            RawKey::Key(k) => {
                mod_press!();
                if let Some(c) = k.chars().next() {
                    events.push(Event::SendKey(
                        enigo::Key::Unicode(c),
                        enigo::Direction::Click,
                    ));
                }
                mod_release!();
            }
            RawKey::Enigo(k) => {
                mod_press!();
                events.push(Event::SendKey(*k, enigo::Direction::Click));
                mod_release!();
            }
            RawKey::Paste => {
                mod_press!();
                events.push(Event::SendKey(enigo::Key::Control, enigo::Direction::Press));
                events.push(Event::SendKey(
                    enigo::Key::Unicode('v'),
                    enigo::Direction::Click,
                ));
                events.push(Event::SendKey(
                    enigo::Key::Control,
                    enigo::Direction::Release,
                ));
                mod_release!();
            }
            _ => return Ok(()),
        }

        self.shift_state = false;
        self.shift_mod = false;
        self.ctrl_mod = false;
        self.alt_mod = false;

        Ok(())
    }

    pub fn toggle_shift(&mut self) {
        if self.shift_state || self.shift_mod {
            self.shift_state = false;
            self.shift_mod = false;
        } else {
            if self.ctrl_mod || self.alt_mod {
                self.shift_mod = !self.shift_mod
            } else {
                self.shift_state = !self.shift_state;
            }
        }
    }

    pub fn toggle_ctrl(&mut self) {
        self.ctrl_mod = !self.ctrl_mod;
    }

    pub fn toggle_alt(&mut self) {
        self.alt_mod = !self.alt_mod;
    }

    pub fn draw_ui(&mut self, ctx: &Context, ui: &mut Ui, events: &mut Vec<Event>) {
        if let Some(key) = self.kb.draw_ui(
            ctx,
            ui,
            self.shift_state,
            self.shift_mod,
            self.ctrl_mod,
            self.alt_mod,
        ) {
            match key {
                RawKey::Done => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                RawKey::Shift => {
                    self.toggle_shift();
                }
                RawKey::Menu => {
                    events.push(Event::ChangeState(StateId::Menu));
                }
                _ => {
                    self.send_key(&key, events).expect("send key");
                }
            }
        }
    }

    pub fn handle_controller_input(
        &mut self,
        ctx: &Context,
        input: &Option<Box<dyn ControllerInput>>,
        events: &mut Vec<Event>,
    ) -> Result<StateId> {
        let input = match input {
            Some(input) => input,
            None => {
                // end of inputs, reset
                self.kb.selected = (None, None);
                return Ok(StateId::Keyboard);
            }
        };

        let selected_left = self
            .kb
            .get_nearest_key_left(input.left_stick(), self.shift_state);
        let selected_right = self
            .kb
            .get_nearest_key_right(input.right_stick(), self.shift_state);

        self.kb.selected = (selected_left.clone(), selected_right.clone());

        {
            let val = input.trigger_left().unwrap_or(0);
            let pressed = val > self.trigger_threshold;

            if pressed && !self.l2_was_pressed {
                match selected_left {
                    Some(RawKey::Done) => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Some(RawKey::Menu) => return Ok(StateId::Menu),
                    Some(key) => {
                        self.send_key(&key, events)?;
                    }
                    _ => (),
                }
            }
            self.l2_was_pressed = pressed;
        }
        {
            let val = input.trigger_right().unwrap_or(0);
            let pressed = val > self.trigger_threshold;

            if pressed && !self.r2_was_pressed {
                match selected_right {
                    Some(RawKey::Done) => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Some(RawKey::Menu) => return Ok(StateId::Menu),
                    Some(key) => {
                        self.send_key(&key, events).expect("send_key");
                    }
                    _ => (),
                }
            }
            self.r2_was_pressed = pressed;
        }

        if input.face_bottom() {
            self.send_key(&RawKey::Key(" ".to_string()), events)?;
        }

        if input.face_left() {
            self.send_key(&RawKey::Enigo(enigo::Key::Backspace), events)?;
        }

        if input.face_top() {
            self.toggle_shift();
        }

        if input.stick_left() {
            self.toggle_ctrl();
        }

        if input.stick_right() {
            self.toggle_alt();
        }

        if matches!(input.dpad(), Some(Dpad::Up)) {
            return Ok(StateId::TextInput);
        }

        Ok(StateId::Keyboard)
    }
}

use crate::{
    controller::ControllerInput,
    state::{keyboard::key::RawKey, StateId},
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
}

impl KeyboardState {
    pub fn draw_ui(&mut self, ctx: &Context, ui: &mut Ui) -> StateId {
        if let Some(key) = self.kb.draw_ui(ctx, ui) {
            match key {
                RawKey::Done => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                RawKey::Shift => {
                    self.kb.toggle_shift();
                }
                RawKey::Menu => return StateId::Menu,
                _ => {
                    self.kb.send_key(&key).expect("send key");
                }
            }
        }
        StateId::Keyboard
    }

    pub fn handle_controller_input(
        &mut self,
        ctx: &Context,
        input: &Option<Box<dyn ControllerInput>>,
    ) -> Result<StateId> {
        let input = match input {
            Some(input) => input,
            None => {
                // end of inputs, reset
                self.kb.selected = (None, None);
                return Ok(StateId::Keyboard);
            }
        };

        let selected_left = self.kb.get_nearest_key_left(input.left_stick());
        let selected_right = self.kb.get_nearest_key_right(input.right_stick());

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
                        self.kb.send_key(&key)?;
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
                        self.kb.send_key(&key).expect("send_key");
                    }
                    _ => (),
                }
            }
            self.r2_was_pressed = pressed;
        }

        if input.face_bottom() {
            self.kb.send_key(&RawKey::Key(" ".to_string()))?;
        }

        if input.face_left() {
            self.kb.send_key(&RawKey::Enigo(enigo::Key::Backspace))?;
        }

        if input.face_top() {
            self.kb.toggle_shift();
        }

        if input.stick_left() {
            self.kb.toggle_ctrl();
        }

        if input.stick_right() {
            self.kb.toggle_alt();
        }

        Ok(StateId::Keyboard)
    }
}

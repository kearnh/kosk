use crate::{
    keyboard::{Keyboard, RawKey},
    ps4::{Dpad, Ps4InputData},
    state::StateId,
};
use anyhow::Result;
use egui::{Context, Ui};

pub(super) struct KeyboardState {
    pub(super) kb: Keyboard,
    pub(super) l2_was_pressed: bool,
    pub(super) r2_was_pressed: bool,
    pub(super) trigger_threshold: u8,
}

impl KeyboardState {
    pub(super) fn draw_ui(&mut self, ctx: &Context, ui: &mut Ui) -> StateId {
        if let Some(key) = self.kb.draw_ui(ui) {
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

    pub(super) fn handle_controller_input(
        &mut self,
        ctx: &Context,
        input: &Option<Ps4InputData>,
    ) -> Result<StateId> {
        let input = match input {
            Some(input) => input,
            None => {
                // end of inputs, reset
                self.kb.selected = (None, None);
                return Ok(StateId::Keyboard);
            }
        };

        let selected_left = self.kb.get_nearest_key_left(input.left);
        let selected_right = self.kb.get_nearest_key_right(input.right);

        self.kb.selected = (selected_left.clone(), selected_right.clone());

        {
            let val = input.l2.unwrap_or(0);
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
            let val = input.r2.unwrap_or(0);
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

        if input.cross {
            self.kb.send_key(&RawKey::Key(" ".to_string()))?;
        }

        if input.square {
            self.kb.send_key(&RawKey::Enigo(enigo::Key::Backspace))?;
        }

        if input.triangle {
            self.kb.toggle_shift();
        }

        if input.l3 {
            self.kb.toggle_ctrl();
        }

        if input.r3 {
            self.kb.toggle_alt();
        }

        Ok(StateId::Keyboard)
    }
}

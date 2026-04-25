use crate::{
    controller::ControllerInput,
    state::keyboard::{key::RawKey, KeyboardState},
};
use anyhow::Result;
use egui::{Context, TextEdit, Ui};

pub struct TextInputState {
    text: String,
    cursor_pos: usize,
}

impl TextInputState {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor_pos: 0,
        }
    }

    fn insert_char(&mut self, ch: char) {
        if self.cursor_pos > self.text.len() {
            self.cursor_pos = self.text.len();
        }
        self.text.insert(self.cursor_pos, ch);
        self.cursor_pos += 1;
    }

    fn backspace(&mut self) {
        if self.cursor_pos > 0 && !self.text.is_empty() {
            self.cursor_pos -= 1;
            self.text.remove(self.cursor_pos);
        }
    }

    fn move_cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    fn move_cursor_right(&mut self) {
        if self.cursor_pos < self.text.len() {
            self.cursor_pos += 1;
        }
    }

    fn submit_text(&mut self, keyboard_state: &mut KeyboardState) -> Result<()> {
        if !self.text.is_empty() {
            keyboard_state.send_text(&self.text)?;
            self.text.clear();
            self.cursor_pos = 0;
        }
        Ok(())
    }

    pub fn draw_ui(
        &mut self,
        ctx: &Context,
        ui: &mut Ui,
        keyboard_state: &mut KeyboardState,
    ) -> crate::state::StateId {
        // Draw text input box
        ui.vertical(|ui| {
            let response = ui.add(
                TextEdit::singleline(&mut self.text)
                    .desired_width(400.0)
                    .hint_text("Type here...")
            );
            
            // Draw the keyboard by calling keyboard_state's draw_ui
            // Accept that mouse clicks will send keys directly (future problem)
            let next_state = keyboard_state.draw_ui(ctx, ui);
            
            // If keyboard_state wants to change state (e.g., to Menu), respect that
            if next_state != crate::state::StateId::Keyboard {
                return next_state;
            }
        });

        // Otherwise stay in TextInput
        crate::state::StateId::TextInput
    }

    pub fn handle_controller_input(
        &mut self,
        ctx: &Context,
        input: &Option<Box<dyn ControllerInput>>,
        keyboard_state: &mut KeyboardState,
    ) -> Result<crate::state::StateId> {
        // Let keyboard_state handle controller input first
        // This updates shift state, mods, and selection
        // TODO: This may consume inputs that text input needs
        let keyboard_next_state = keyboard_state.handle_controller_input(ctx, input)?;
        
        // If keyboard_state wants to change state, respect that
        if keyboard_next_state != crate::state::StateId::Keyboard {
            return Ok(keyboard_next_state);
        }

        // Now handle text input specific logic
        let input = match input {
            Some(input) => input,
            None => return Ok(crate::state::StateId::TextInput),
        };

        // Use keyboard_state's trigger state to detect presses
        {
            let val = input.trigger_left().unwrap_or(0);
            let pressed = val > keyboard_state.trigger_threshold;

            if pressed && !keyboard_state.l2_was_pressed {
                // Insert character from left stick selection
                if let Some(RawKey::Key(ch)) = &keyboard_state.kb.selected.0 {
                    if let Some(c) = ch.chars().next() {
                        self.insert_char(c);
                    }
                }
            }
            keyboard_state.l2_was_pressed = pressed;
        }

        {
            let val = input.trigger_right().unwrap_or(0);
            let pressed = val > keyboard_state.trigger_threshold;

            if pressed && !keyboard_state.r2_was_pressed {
                self.submit_text(keyboard_state)?;
            }
            keyboard_state.r2_was_pressed = pressed;
        }

        // Handle face buttons for text editing
        // TODO: These may conflict with keyboard_state's handling
        if input.face_bottom() {
            self.insert_char(' ');
        }

        if input.face_left() {
            self.backspace();
        }

        // D-pad for cursor movement
        match input.dpad() {
            Some(crate::controller::Dpad::Left) => self.move_cursor_left(),
            Some(crate::controller::Dpad::Right) => self.move_cursor_right(),
            _ => {}
        }

        // Face right to exit back to keyboard mode
        if input.face_right() {
            return Ok(crate::state::StateId::Keyboard);
        }

        Ok(crate::state::StateId::TextInput)
    }
}

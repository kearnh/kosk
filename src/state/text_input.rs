use crate::{
    controller::{ControllerInput, Dpad},
    state::{event::Event, keyboard::KeyboardState, StateId},
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

    fn submit_text(&mut self, events: &mut Vec<Event>) {
        if !self.text.is_empty() {
            events.push(Event::SendText(self.text.to_string()));
            events.push(Event::SendKey(enigo::Key::Return, enigo::Direction::Click));
            events.push(Event::ChangeState(StateId::Keyboard));
            self.text.clear();
            self.cursor_pos = 0;
        }
    }

    fn process_events(&mut self, events: Vec<Event>, output_events: &mut Vec<Event>) {
        for event in events {
            match event {
                Event::SendKey(enigo::Key::Unicode(ch), enigo::Direction::Click) => {
                    self.insert_char(ch);
                }
                Event::SendKey(enigo::Key::Backspace, enigo::Direction::Click) => self.backspace(),
                Event::SendKey(enigo::Key::Return, enigo::Direction::Click) => {
                    self.submit_text(output_events)
                }
                Event::SendKey(_, _) => {}
                Event::SendText(_) => todo!(),
                Event::ChangeState(_) => {
                    output_events.push(event);
                }
            }
        }
    }

    pub fn draw_ui(
        &mut self,
        ctx: &Context,
        ui: &mut Ui,
        events: &mut Vec<Event>,
        keyboard_state: &mut KeyboardState,
    ) {
        // Draw text input box
        ui.vertical(|ui| {
            let _ = ui.add(
                TextEdit::singleline(&mut self.text)
                    .desired_width(400.0)
                    .interactive(false),
            );
        });

        let mut kb_events = vec![];
        keyboard_state.draw_ui(ctx, ui, &mut kb_events);
        self.process_events(kb_events, events);
    }

    pub fn handle_controller_input(
        &mut self,
        ctx: &Context,
        input: &Option<Box<dyn ControllerInput>>,
        events: &mut Vec<Event>,
        keyboard_state: &mut KeyboardState,
    ) -> Result<()> {
        let mut handled = false;
        if let Some(input) = input {
            if matches!(input.dpad(), Some(Dpad::Left)) {
                dbg!();
                self.move_cursor_left();
                handled = true;
            }
            if matches!(input.dpad(), Some(Dpad::Right)) {
                self.move_cursor_right();
                handled = true;
            }
            if matches!(input.dpad(), Some(Dpad::Up)) {
                events.push(Event::ChangeState(StateId::Keyboard));
                return Ok(());
            }
        }

        if !handled {
            let mut kb_events = vec![];
            keyboard_state.handle_controller_input(ctx, input, &mut kb_events)?;
            self.process_events(kb_events, events);
        }

        Ok(())
    }
}

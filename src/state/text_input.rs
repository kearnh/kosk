use crate::{
    config,
    controller::{ControllerInput, Dpad},
    state::{event::Event, keyboard, StateId},
};
use anyhow::Result;
use egui::{Color32, Context, FontId, TextEdit, Ui};
use std::sync::{Mutex, OnceLock};

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

    fn build_display_string(&self) -> String {
        let mut display = String::new();
        let cursor_char = '\u{258f}'; // Left eighth block character

        for (i, ch) in self.text.chars().enumerate() {
            if i == self.cursor_pos {
                display.push(cursor_char);
            }
            display.push(ch);
        }

        // If cursor is at the end
        if self.cursor_pos == self.text.len() {
            display.push(cursor_char);
        }

        display
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

    pub fn draw_ui(&mut self, ctx: &Context, ui: &mut Ui, events: &mut Vec<Event>) {
        let ti = &config::get().text_input;
        let bg = Color32::from_rgba_unmultiplied(
            ti.background_color[0],
            ti.background_color[1],
            ti.background_color[2],
            ti.background_color[3],
        );
        let fg = Color32::from_rgba_unmultiplied(
            ti.text_color[0],
            ti.text_color[1],
            ti.text_color[2],
            ti.text_color[3],
        );
        let font_size = ti.font_size.max(1.0);

        ui.vertical(|ui| {
            let mut display_text = self.build_display_string();
            let _ = ui.add(
                TextEdit::singleline(&mut display_text)
                    .background_color(bg)
                    .text_color(fg)
                    .font(FontId::proportional(font_size))
                    .desired_width(400.0)
                    .interactive(false),
            );
        });

        let mut kb_events = vec![];
        keyboard::with_mut(|keyboard_state| {
            keyboard_state.draw_ui(ctx, ui, &mut kb_events);
        });
        self.process_events(kb_events, events);
    }

    pub fn handle_controller_input(
        &mut self,
        ctx: &Context,
        input: &Option<Box<dyn ControllerInput>>,
        events: &mut Vec<Event>,
    ) -> Result<()> {
        let mut handled = false;
        if let Some(input) = input {
            if matches!(input.dpad(), Some(Dpad::Left)) {
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
            keyboard::with_mut(|keyboard_state| {
                keyboard_state.handle_controller_input(ctx, input, &mut kb_events)
            })?;
            self.process_events(kb_events, events);
        }

        Ok(())
    }
}

static TEXT_INPUT: OnceLock<Mutex<TextInputState>> = OnceLock::new();

pub fn init() -> Result<()> {
    let t = TextInputState::new();
    TEXT_INPUT
        .set(Mutex::new(t))
        .map_err(|_| anyhow::anyhow!("text input state already initialized"))?;
    Ok(())
}

pub(crate) fn with_mut<R>(f: impl FnOnce(&mut TextInputState) -> R) -> R {
    let mut guard = TEXT_INPUT
        .get()
        .expect("text input state not initialized")
        .lock()
        .unwrap();
    f(&mut guard)
}

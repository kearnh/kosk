use crate::{
    config,
    controller::{ControllerButton, ControllerInput, Dpad},
    state::{event::Event, event::EventQueue, event::EventSource, keyboard, StateId},
};
use anyhow::Result;
use egui::text::CCursor;
use egui::text_selection::text_cursor_state::{char_index_from_byte_index, cursor_rect};
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

    fn submit_text(&mut self, events: &mut EventQueue) {
        if !self.text.is_empty() {
            let n = EventSource::None;
            events.start_batch(&n);
            let _ = events.push(Event::SendText(self.text.to_string()), &n);
            let _ = events.push(
                Event::SendKey(enigo::Key::Return, enigo::Direction::Click),
                &n,
            );
            let _ = events.push(Event::ChangeState(StateId::Keyboard), &n);
            let _ = events.end_batch();
            self.text.clear();
            self.cursor_pos = 0;
        }
    }

    fn process_events(
        &mut self,
        events: Vec<(Event, EventSource)>,
        output_events: &mut EventQueue,
    ) {
        for (event, src) in events {
            match event {
                Event::SendKey(enigo::Key::Unicode(ch), enigo::Direction::Click) => {
                    self.insert_char(ch);
                }
                Event::SendKey(enigo::Key::Backspace, enigo::Direction::Click) => self.backspace(),
                Event::SendKey(enigo::Key::Return, enigo::Direction::Click) => {
                    self.submit_text(output_events)
                }
                Event::SendKey(_, _) => {}
                _ => {
                    let _ = output_events.push(event, &src);
                }
            }
        }
    }

    pub fn draw_ui(&mut self, ctx: &Context, ui: &mut Ui, events: &mut EventQueue) {
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
        let cursor_color = Color32::from_rgba_unmultiplied(
            ti.cursor_color[0],
            ti.cursor_color[1],
            ti.cursor_color[2],
            ti.cursor_color[3],
        );
        let font_size = ti.font_size.max(1.0);
        let font_id = FontId::proportional(font_size);

        ui.vertical(|ui| {
            let output = TextEdit::singleline(&mut self.text)
                .background_color(bg)
                .text_color(fg)
                .font(font_id.clone())
                .desired_width(400.0)
                .interactive(false)
                .show(ui);

            let row_height = ui.fonts_mut(|f| f.row_height(&font_id));
            let ccursor = CCursor::new(char_index_from_byte_index(
                self.text.as_str(),
                self.cursor_pos,
            ));
            let primary_cursor_rect = cursor_rect(&output.galley, &ccursor, row_height)
                .translate(output.galley_pos.to_vec2());
            let stroke_width = ui.visuals().text_cursor.stroke.width;
            ui.painter().line_segment(
                [
                    primary_cursor_rect.center_top(),
                    primary_cursor_rect.center_bottom(),
                ],
                (stroke_width, cursor_color),
            );
        });

        let mut kb_events = EventQueue::passthrough();
        keyboard::with_mut(|keyboard_state| {
            keyboard_state.draw_ui(ctx, ui, &mut kb_events);
        });
        self.process_events(kb_events.drain_pending(), events);
    }

    pub fn handle_controller_input(
        &mut self,
        input: &Option<Box<dyn ControllerInput>>,
        events: &mut EventQueue,
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
                let _ = events.push(
                    Event::ChangeState(StateId::Keyboard),
                    &EventSource::Controller(ControllerButton::Dpad(Dpad::Up)),
                );
                return Ok(());
            }
        }

        if !handled {
            let mut kb_events = EventQueue::passthrough();
            keyboard::with_mut(|keyboard_state| {
                keyboard_state.handle_controller_input(input, &mut kb_events)
            })?;
            self.process_events(kb_events.drain_pending(), events);
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

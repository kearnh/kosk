use crate::{
    config,
    controller::ControllerInput,
    state::{
        actions::load_bindings,
        event::{Event, EventQueue, EventSource},
        keyboard, StateId,
    },
};
use anyhow::Result;
use egui::text::CCursor;
use egui::text_selection::text_cursor_state::{char_index_from_byte_index, cursor_rect};
use egui::{Color32, Context, FontId, TextEdit, Ui};
use std::sync::{Mutex, OnceLock};

use crate::controller::bindings::BindingEngine;

use crate::state::text_input_action::TextInputAction;

pub struct TextInputState {
    text: String,
    cursor_pos: usize,
    kb_events: EventQueue,
    bindings: BindingEngine<TextInputAction>,
}

impl TextInputState {
    pub fn new() -> Result<Self> {
        Ok(Self {
            text: String::new(),
            cursor_pos: 0,
            kb_events: EventQueue::new(),
            bindings: load_bindings(StateId::TextInput)?,
        })
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

    fn submit_text(&mut self, events: &mut EventQueue, source: &EventSource) {
        let mut steps = Vec::new();
        if !self.text.is_empty() {
            steps.push(Event::SendText(self.text.to_string()));
            steps.push(Event::SendKey(enigo::Key::Return, enigo::Direction::Click));
            self.text.clear();
            self.cursor_pos = 0;
        }
        steps.push(Event::ChangeState(StateId::Keyboard));
        let _ = events.push_seq(steps, source);
    }

    fn process_events(&mut self, events: &mut EventQueue) {
        for (event, src) in self.kb_events.drain_pending() {
            match event {
                Event::SendKey(enigo::Key::Unicode(ch), enigo::Direction::Click) => {
                    self.insert_char(ch);
                }
                Event::SendKey(enigo::Key::Backspace, enigo::Direction::Click) => self.backspace(),
                Event::SendKey(enigo::Key::Return, enigo::Direction::Click) => {
                    self.submit_text(events, &src)
                }
                Event::SendKey(_, _) => {}
                Event::SendText(text) => {
                    for ch in text.chars() {
                        self.insert_char(ch);
                    }
                }
                _ => {
                    let _ = events.push(event, &src);
                }
            }
        }
    }

    fn do_action(
        &mut self,
        action: &TextInputAction,
        events: &mut EventQueue,
        source: &EventSource,
    ) -> Result<()> {
        use TextInputAction::*;
        match action {
            MoveCursorLeft => self.move_cursor_left(),
            MoveCursorRight => self.move_cursor_right(),
            SwitchState(state) => {
                let _ = events.push(Event::ChangeState(*state), source);
            }
        }
        Ok(())
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

        keyboard::with_mut(|keyboard_state| {
            keyboard_state.draw_ui(ctx, ui, &mut self.kb_events);
        });
        self.process_events(events);
    }

    pub fn reset_controller_input(&mut self, holdover: Option<&dyn ControllerInput>) {
        self.bindings.reset(holdover);
        keyboard::with_mut(|kb| kb.reset_controller_input(holdover));
    }

    pub fn handle_controller_input(
        &mut self,
        input: &dyn ControllerInput,
        events: &mut EventQueue,
    ) -> Result<()> {
        let fired = self.bindings.evaluate(input);
        for (binding, action) in &fired {
            let src = EventSource::Controller(binding.clone());
            self.do_action(action, events, &src)?;
        }

        if fired.is_empty() {
            keyboard::with_mut(|keyboard_state| {
                keyboard_state.handle_controller_input(input, &mut self.kb_events)
            })?;
        }
        self.kb_events.end_controller_tick();
        if fired.is_empty() {
            self.process_events(events);
        }

        Ok(())
    }

    fn reload_from_config(&mut self) -> Result<()> {
        self.bindings = load_bindings(StateId::TextInput)?;
        Ok(())
    }
}

static TEXT_INPUT: OnceLock<Mutex<TextInputState>> = OnceLock::new();

pub fn init() -> Result<()> {
    let t = TextInputState::new()?;
    TEXT_INPUT
        .set(Mutex::new(t))
        .map_err(|_| anyhow::anyhow!("text input state already initialized"))?;

    crate::config::on_changed(|| {
        let result = with_mut(|ti| ti.reload_from_config());
        if let Err(e) = result {
            eprintln!("Failed to reload text input from config: {}", e);
        }
    })?;

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

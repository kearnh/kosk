use crate::{
    config,
    controller::ControllerInput,
    state::{
        actions::load_bindings,
        completion_ui,
        event::{Event, EventQueue, EventSource},
        keyboard, StateId,
    },
};
use anyhow::Result;
use egui::text::{ByteIndex, CCursor};
use egui::text_selection::text_cursor_state::{char_index_from_byte_index, cursor_rect};
use egui::{Color32, Context, FontId, TextEdit, Ui};
use std::sync::{Mutex, OnceLock};

use crate::controller::bindings::BindingEngine;
use crate::state::text_input_action::TextInputAction;

fn clamp_cursor(text: &str, cursor: usize) -> usize {
    if cursor > text.len() {
        return text.len();
    }
    if text.is_char_boundary(cursor) {
        cursor
    } else {
        text.len()
    }
}

fn preceding_is_space(text: &str, cursor: usize) -> bool {
    let cursor = clamp_cursor(text, cursor);
    if cursor == 0 {
        return false;
    }

    text[..cursor].ends_with(' ')
}

fn insert_char_at(text: &mut String, cursor: &mut usize, ch: char) {
    *cursor = clamp_cursor(text, *cursor);
    text.insert(*cursor, ch);
    *cursor += ch.len_utf8();
}

fn backspace_at(text: &mut String, cursor: &mut usize) {
    *cursor = clamp_cursor(text, *cursor);
    if *cursor == 0 || text.is_empty() {
        return;
    }
    let prev = text
        .char_indices()
        .rev()
        .find(|(i, _)| *i < *cursor)
        .map(|(i, _)| i);
    let Some(start) = prev else {
        return;
    };
    text.replace_range(start..*cursor, "");
    *cursor = start;
}

fn move_cursor_left(text: &str, cursor: &mut usize) {
    *cursor = clamp_cursor(text, *cursor);
    if *cursor == 0 {
        return;
    }
    if let Some((i, _)) = text.char_indices().rev().find(|(i, _)| *i < *cursor) {
        *cursor = i;
    }
}

fn move_cursor_right(text: &str, cursor: &mut usize) {
    *cursor = clamp_cursor(text, *cursor);
    if *cursor >= text.len() {
        return;
    }
    if let Some(ch) = text[*cursor..].chars().next() {
        *cursor += ch.len_utf8();
    }
}

pub struct TextInputState {
    text: String,
    cursor_pos: usize,
    kb_events: EventQueue,
    bindings: BindingEngine<TextInputAction>,
    undo_accept: Option<(String, usize)>,
}

impl TextInputState {
    pub fn new() -> Result<Self> {
        Ok(Self {
            text: String::new(),
            cursor_pos: 0,
            kb_events: EventQueue::new(),
            bindings: load_bindings(StateId::TextInput)?,
            undo_accept: None,
        })
    }

    fn insert_char(&mut self, ch: char) {
        let outcome = crate::completion::with_mut(|s| {
            let s = s?;
            s.clear_suggestion_just_accepted();
            s.take_eat_accept_space(ch)
        });
        let ate = outcome.is_some() && preceding_is_space(&self.text, self.cursor_pos);

        if ate {
            backspace_at(&mut self.text, &mut self.cursor_pos);
        }

        insert_char_at(&mut self.text, &mut self.cursor_pos, ch);

        if ate && outcome.is_some_and(|o| o.space_after) {
            insert_char_at(&mut self.text, &mut self.cursor_pos, ' ');
        }

        self.refresh_completion();
    }

    fn backspace(&mut self) {
        crate::completion::with_mut(|s| {
            if let Some(s) = s {
                s.clear_eat_accept_space();
                s.clear_suggestion_just_accepted();
            }
        });
        backspace_at(&mut self.text, &mut self.cursor_pos);
        self.refresh_completion();
    }

    fn move_cursor_left(&mut self) {
        crate::completion::with_mut(|s| {
            if let Some(s) = s {
                s.note_log(crate::completion::LogEvent::Arrow, "");
            }
        });
        move_cursor_left(&self.text, &mut self.cursor_pos);
        self.refresh_completion();
    }

    fn move_cursor_right(&mut self) {
        crate::completion::with_mut(|s| {
            if let Some(s) = s {
                s.note_log(crate::completion::LogEvent::Arrow, "");
            }
        });
        move_cursor_right(&self.text, &mut self.cursor_pos);
        self.refresh_completion();
    }

    fn refresh_completion(&mut self) {
        crate::completion::with_mut(|s| {
            if let Some(s) = s {
                s.request_from_buffer(&self.text, self.cursor_pos);
            }
        });
    }

    fn completion_cycle(&mut self, forward: bool) {
        crate::completion::with_mut(|s| {
            if let Some(s) = s {
                s.cycle(forward);
            }
        });
    }

    fn completion_toggle(&mut self) {
        crate::completion::with_mut(|s| {
            if let Some(s) = s {
                s.toggle_armed();
            }
        });
        self.refresh_completion();
    }

    fn completion_accept(&mut self, index: Option<usize>) -> bool {
        let cfg = config::get().completion;
        if !cfg.enabled || !cfg.show_in_text_input {
            return false;
        }
        let Some(cand) = crate::completion::with_mut(|s| {
            let s = s?;
            if !s.armed() {
                return None;
            }
            match index {
                Some(i) => s.accept_index(i).cloned(),
                None => s.highlighted().cloned(),
            }
        }) else {
            return false;
        };
        let Some(ctx) =
            crate::completion::CompletionContext::from_buffer(&self.text, self.cursor_pos, &cfg)
        else {
            return false;
        };
        self.undo_accept = Some((self.text.clone(), self.cursor_pos));
        let (new_text, new_cursor) = crate::completion::splice(
            &self.text,
            ctx.token_range,
            &cand.text,
            cfg.insert_space_on_accept,
        );
        self.text = new_text;
        self.cursor_pos = new_cursor;
        crate::completion::with_mut(|s| {
            if let Some(s) = s {
                if s.cfg().learn_on_accept {
                    let mut words = ctx.prev_words.clone();
                    words.push(cand.text.clone());
                    s.learn(&words);
                }
                s.clear_highlight();

                if cfg.insert_space_on_accept {
                    s.arm_eat_accept_space();
                }

                s.request_from_buffer(&self.text, self.cursor_pos);
                s.arm_suggestion_just_accepted();
            }
        });
        true
    }

    fn completion_cancel(&mut self) {
        crate::completion::with_mut(|s| {
            if let Some(s) = s {
                s.clear_eat_accept_space();
                s.clear_suggestion_just_accepted();
                s.clear_highlight();
            }
        });
        if let Some((text, cursor)) = self.undo_accept.take() {
            self.text = text;
            self.cursor_pos = cursor;
            self.refresh_completion();
        }
    }

    fn submit_text(&mut self, events: &mut EventQueue, source: &EventSource) {
        crate::completion::with_mut(|s| {
            if let Some(s) = s {
                s.clear_suggestion_just_accepted();
                if s.cfg().learn_on_submit {
                    let words = crate::completion::tokens_in(&self.text, s.cfg());
                    s.learn(&words);
                }
            }
        });
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
            CycleSuggestion => self.completion_cycle(true),
            CycleSuggestionPrev => self.completion_cycle(false),
            ToggleCompletion => self.completion_toggle(),
            CancelSuggestion => self.completion_cancel(),
            AcceptSuggestion(i) => {
                let _ = self.completion_accept(*i);
            }
            Submit => self.submit_text(events, source),
        }
        Ok(())
    }

    pub fn draw_ui(&mut self, ctx: &Context, ui: &mut Ui, events: &mut EventQueue) {
        let cfg = config::get();
        let ti = &cfg.text_input;
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
        let show_chips = cfg.completion.enabled && cfg.completion.show_in_text_input;
        let placement = cfg.completion.ui.placement;
        let strip_w = keyboard::with_mut(|kb| kb.content_width());
        let token = crate::completion::CompletionContext::from_buffer(
            &self.text,
            self.cursor_pos,
            &cfg.completion,
        )
        .map(|c| c.token)
        .unwrap_or_default();
        let mut chip_click = None;

        ui.vertical(|ui| {
            if show_chips && placement == crate::completion::settings::ChipPlacement::AboveField {
                chip_click = completion_ui::draw_session_strip(ui, strip_w, &token);
            }

            let output = TextEdit::singleline(&mut self.text)
                .background_color(bg)
                .text_color(fg)
                .font(font_id.clone())
                .desired_width(strip_w.max(1.0))
                .interactive(false)
                .show(ui);

            let row_height = ui.fonts_mut(|f| f.row_height(&font_id));
            let ccursor = CCursor::new(char_index_from_byte_index(
                self.text.as_str(),
                ByteIndex(self.cursor_pos),
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

            if show_chips && placement != crate::completion::settings::ChipPlacement::AboveField {
                chip_click = completion_ui::draw_session_strip(ui, strip_w, &token);
            }
        });

        if let Some(i) = chip_click {
            let _ = self.completion_accept(Some(i));
        }

        keyboard::with_mut(|keyboard_state| {
            keyboard_state.set_feed_completion_log(false);
            if let Some(key) = keyboard_state.draw_keyboard_ui(ctx, ui, &mut self.kb_events) {
                keyboard_state
                    .send_key(&key, &mut self.kb_events, &EventSource::MouseClick)
                    .expect("send key");
            }
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
        let ctx = keyboard::with_mut(|kb| kb.when_context());
        let fired = self.bindings.evaluate(input, &ctx);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_emoji_advances_full_char() {
        let mut text = String::new();
        let mut cursor = 0;
        insert_char_at(&mut text, &mut cursor, '😀');
        assert_eq!(text, "😀");
        assert_eq!(cursor, "😀".len());
        insert_char_at(&mut text, &mut cursor, 'b');
        assert_eq!(text, "😀b");
        assert_eq!(cursor, text.len());
    }

    #[test]
    fn backspace_deletes_emoji_not_one_byte() {
        let mut text = String::from("a😀b");
        let mut cursor = text.len();
        backspace_at(&mut text, &mut cursor);
        assert_eq!(text, "a😀");
        backspace_at(&mut text, &mut cursor);
        assert_eq!(text, "a");
        assert_eq!(cursor, 1);
    }

    #[test]
    fn cursor_skips_whole_chars() {
        let text = String::from("a😀b");
        let mut cursor = text.len();
        move_cursor_left(&text, &mut cursor);
        assert_eq!(cursor, "a😀".len());
        move_cursor_left(&text, &mut cursor);
        assert_eq!(cursor, 1);
        move_cursor_left(&text, &mut cursor);
        assert_eq!(cursor, 0);
        move_cursor_right(&text, &mut cursor);
        assert_eq!(cursor, 1);
        move_cursor_right(&text, &mut cursor);
        assert_eq!(cursor, "a😀".len());
    }

    #[test]
    fn preceding_space_punct_replaces_space() {
        let mut text = String::from("hello ");
        let mut cursor = text.len();
        assert!(preceding_is_space(&text, cursor));
        backspace_at(&mut text, &mut cursor);
        insert_char_at(&mut text, &mut cursor, '.');
        insert_char_at(&mut text, &mut cursor, ' ');
        assert_eq!(text, "hello. ");
        assert_eq!(cursor, text.len());
    }

    #[test]
    fn preceding_space_slash_does_not_respace() {
        let mut text = String::from("hello ");
        let mut cursor = text.len();
        backspace_at(&mut text, &mut cursor);
        insert_char_at(&mut text, &mut cursor, '/');
        assert_eq!(text, "hello/");
        assert_eq!(cursor, text.len());
    }
}

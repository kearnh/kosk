use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::config;
use crate::controller::bindings::BindingEngine;
use crate::controller::record as input_record;
use crate::controller::record::{MappingScales, TapeHeader};
use crate::state::actions::load_bindings;
use crate::state::keyboard::layout::KeyboardLayout;
use crate::{
    controller::ControllerInput,
    controller::{ControllerBinding, ControllerButton},
    state::{
        event::{Event, EventQueue, EventSource},
        keyboard::key::RawKey,
        StateId,
    },
};
use anyhow::Result;
use egui::{Context, Ui};

mod key;
mod keyboard_action;
mod layout;
mod when;

pub use crate::state::keyboard::keyboard_action::KeyboardAction;

fn stick_side_sources(left: bool) -> [EventSource; 2] {
    if left {
        [
            EventSource::Controller(ControllerBinding::Single(ControllerButton::TriggerLeft)),
            EventSource::Controller(ControllerBinding::Single(ControllerButton::PadLeft)),
        ]
    } else {
        [
            EventSource::Controller(ControllerBinding::Single(ControllerButton::TriggerRight)),
            EventSource::Controller(ControllerBinding::Single(ControllerButton::PadRight)),
        ]
    }
}

#[derive(Default)]
pub struct KeyboardState {
    layouts: HashMap<String, KeyboardLayout>,
    current_layout: String,
    selected: (Option<RawKey>, Option<RawKey>),
    shift_state: bool,
    shift_mod: bool,
    ctrl_mod: bool,
    alt_mod: bool,
    bindings: BindingEngine<KeyboardAction>,
    last_left_stick_action: Option<Instant>,
    last_right_stick_action: Option<Instant>,
    stick_select_lock_ms: Duration,
}

impl KeyboardState {
    pub fn new() -> Result<Self> {
        let mut state: Self = Default::default();
        state.reload_from_config()?;
        state.current_layout = config::get().start_layout;
        Ok(state)
    }

    pub fn send_key(
        &mut self,
        key: &RawKey,
        events: &mut EventQueue,
        source: &EventSource,
    ) -> Result<()> {
        match key {
            RawKey::Key(c) => {
                let mut steps = Vec::new();
                if self.shift_mod {
                    steps.push(Event::SendKey(enigo::Key::Shift, enigo::Direction::Press));
                }
                if self.ctrl_mod {
                    steps.push(Event::SendKey(enigo::Key::Control, enigo::Direction::Press));
                }
                if self.alt_mod {
                    steps.push(Event::SendKey(enigo::Key::Alt, enigo::Direction::Press));
                }
                // `enigo::text` (SendText) injects Unicode and ignores held modifiers, so
                // Ctrl/Alt/Shift chords never reach the app. Virtual-key click does combine.
                // Without mods, SendText is required: SendKey does not emit uppercase letters.
                if self.ctrl_mod || self.alt_mod || self.shift_mod {
                    steps.push(Event::SendKey(
                        enigo::Key::Unicode(*c),
                        enigo::Direction::Click,
                    ));
                } else {
                    steps.push(Event::SendText(c.to_string()));
                }
                if self.alt_mod {
                    steps.push(Event::SendKey(enigo::Key::Alt, enigo::Direction::Release));
                }
                if self.ctrl_mod {
                    steps.push(Event::SendKey(
                        enigo::Key::Control,
                        enigo::Direction::Release,
                    ));
                }
                if self.shift_mod {
                    steps.push(Event::SendKey(enigo::Key::Shift, enigo::Direction::Release));
                }
                if events.push_seq(steps, source) {
                    self.shift_state = false;
                    self.shift_mod = false;
                    self.ctrl_mod = false;
                    self.alt_mod = false;
                }
            }
            RawKey::Enigo(k) => {
                let mut steps = Vec::new();
                if self.shift_mod {
                    steps.push(Event::SendKey(enigo::Key::Shift, enigo::Direction::Press));
                }
                if self.ctrl_mod {
                    steps.push(Event::SendKey(enigo::Key::Control, enigo::Direction::Press));
                }
                if self.alt_mod {
                    steps.push(Event::SendKey(enigo::Key::Alt, enigo::Direction::Press));
                }
                steps.push(Event::SendKey(*k, enigo::Direction::Click));
                if self.alt_mod {
                    steps.push(Event::SendKey(enigo::Key::Alt, enigo::Direction::Release));
                }
                if self.ctrl_mod {
                    steps.push(Event::SendKey(
                        enigo::Key::Control,
                        enigo::Direction::Release,
                    ));
                }
                if self.shift_mod {
                    steps.push(Event::SendKey(enigo::Key::Shift, enigo::Direction::Release));
                }
                if events.push_seq(steps, source) {
                    self.shift_state = false;
                    self.shift_mod = false;
                    self.ctrl_mod = false;
                    self.alt_mod = false;
                }
            }
            RawKey::Action(action) => {
                self.do_action(action, events, source)?;
            }
            RawKey::Text(text) => {
                events.push(Event::SendText(text.to_owned()), source);
            }
            _ => return Ok(()),
        }

        Ok(())
    }

    fn do_action(
        &mut self,
        action: &KeyboardAction,
        events: &mut EventQueue,
        source: &EventSource,
    ) -> Result<()> {
        use KeyboardAction::*;
        match action {
            SendKeyUnderLeftStick => {
                if let (Some(left), _) = &self.selected {
                    self.last_left_stick_action = Some(Instant::now());
                    self.send_key(&left.clone(), events, source)?;
                }
            }
            SendKeyUnderRightStick => {
                if let (_, Some(right)) = &self.selected {
                    self.last_right_stick_action = Some(Instant::now());
                    self.send_key(&right.clone(), events, source)?;
                }
            }
            SendKey(key) => {
                self.send_key(&RawKey::Key(*key), events, source)?;
            }
            SendEnigoKey(key) => {
                self.send_key(&RawKey::Enigo(*key), events, source)?;
            }
            ToggleShift => {
                let _ = events.push(Event::ToggleShift, source);
            }
            ToggleCtrl => {
                let _ = events.push(Event::ToggleCtrl, source);
            }
            ToggleAlt => {
                let _ = events.push(Event::ToggleAlt, source);
            }
            Paste => {
                let _ = events.push_seq(
                    vec![
                        Event::SendKey(enigo::Key::Control, enigo::Direction::Press),
                        Event::SendKey(enigo::Key::Unicode('v'), enigo::Direction::Click),
                        Event::SendKey(enigo::Key::Control, enigo::Direction::Release),
                    ],
                    source,
                );
            }
            SwitchState(state) => {
                let _ = events.push(Event::ChangeState(*state), source);
            }
            SwitchLayout(layout_name) => {
                if self.layouts.contains_key(layout_name) {
                    self.current_layout = layout_name.to_string();
                    // Reset selection when switching layouts
                    self.selected = (None, None);
                    input_record::session().tap_layout(layout_name);
                } else {
                    return Err(anyhow::anyhow!("Layout '{}' not found", layout_name));
                }
                return Ok(());
            }
            FlipWindowLeftRight => {
                let _ = events.push(Event::FlipWindowLeftRight, source);
            }
            FlipWindowAboveBelow => {
                let _ = events.push(Event::FlipWindowAboveBelow, source);
            }
            RotateWindow => {
                let _ = events.push(Event::RotateWindow, source);
            }
            Exit => {
                let _ = events.push(Event::Exit, source);
            }
            ToggleRecord => {
                let _ = events.push(Event::ToggleRecord, source);
            }
        }
        Ok(())
    }

    pub(crate) fn tape_header(&self) -> Result<TapeHeader> {
        let cfg = config::get();
        let mut layouts: Vec<(String, String)> = self
            .layouts
            .iter()
            .map(|(name, layout)| (name.clone(), layout.source().to_owned()))
            .collect();
        layouts.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(TapeHeader {
            version: crate::controller::record::CURRENT_TAPE_VERSION,
            current_layout: self.current_layout.clone(),
            scales: MappingScales {
                scale_x: cfg.scale_x,
                scale_y: cfg.scale_y,
                stick_scale_x: cfg.stick_scale_x,
                stick_scale_y: cfg.stick_scale_y,
            },
            config_toml: Some(config::tape_config_toml(&cfg)?),
            layouts,
        })
    }

    pub(crate) fn install_recorded_layouts(&mut self, header: &TapeHeader) -> Result<()> {
        let mut map = HashMap::new();
        for (name, toml) in &header.layouts {
            map.insert(
                name.clone(),
                KeyboardLayout::load_with_scales(
                    toml,
                    header.scales.scale_x,
                    header.scales.scale_y,
                    header.scales.stick_scale_x,
                    header.scales.stick_scale_y,
                )?,
            );
        }
        if !map.contains_key(&header.current_layout) {
            return Err(anyhow::anyhow!(
                "recorded current_layout '{}' is not in the tape header",
                header.current_layout
            ));
        }
        self.layouts = map;
        self.current_layout = header.current_layout.clone();
        self.selected = (None, None);
        Ok(())
    }

    pub(crate) fn set_current_layout(&mut self, name: &str) -> Result<()> {
        if !self.layouts.contains_key(name) {
            return Err(anyhow::anyhow!("Layout '{}' not found", name));
        }
        self.current_layout = name.to_string();
        self.selected = (None, None);
        Ok(())
    }

    pub(crate) fn toggle_shift(&mut self) {
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

    pub(crate) fn toggle_ctrl(&mut self) {
        self.ctrl_mod = !self.ctrl_mod;
    }

    pub(crate) fn toggle_alt(&mut self) {
        self.alt_mod = !self.alt_mod;
    }

    pub fn draw_ui(&mut self, ctx: &Context, ui: &mut Ui, events: &mut EventQueue) {
        if let Some(key) = self.draw_keyboard_ui(ctx, ui) {
            self.send_key(&key, events, &EventSource::MouseClick)
                .expect("send key");
        }
    }

    pub fn reset_controller_input(&mut self, holdover: Option<&dyn ControllerInput>) {
        if holdover.is_none() {
            self.selected = (None, None);
        }
        self.bindings.reset(holdover);
    }

    pub fn handle_controller_input(
        &mut self,
        input: &dyn ControllerInput,
        events: &mut EventQueue,
    ) -> Result<()> {
        let current_layout = self
            .layouts
            .get(&self.current_layout)
            .ok_or_else(|| anyhow::anyhow!("Current layout '{}' not found", self.current_layout))?;

        let prev_selected = self.selected.clone();

        let selected_left =
            current_layout.get_nearest_key_left(input.left_stick(), self.shift_state);
        let selected_right =
            current_layout.get_nearest_key_right(input.right_stick(), self.shift_state);

        let lock_left = self
            .last_left_stick_action
            .is_some_and(|t| t.elapsed() <= self.stick_select_lock_ms);
        let lock_right = self
            .last_right_stick_action
            .is_some_and(|t| t.elapsed() <= self.stick_select_lock_ms);

        let new_left = if lock_left {
            prev_selected.0.clone()
        } else {
            selected_left
        };
        let new_right = if lock_right {
            prev_selected.1.clone()
        } else {
            selected_right
        };

        if prev_selected.0 != new_left {
            events.clear_toggle_suppress(stick_side_sources(true));
        }
        if prev_selected.1 != new_right {
            events.clear_toggle_suppress(stick_side_sources(false));
        }

        self.selected = (new_left, new_right);
        if !lock_left {
            self.last_left_stick_action = None;
        }
        if !lock_right {
            self.last_right_stick_action = None;
        }

        for (binding, action) in self.bindings.evaluate(input) {
            let src = EventSource::Controller(binding);
            self.do_action(&action, events, &src)?;
        }

        Ok(())
    }

    fn draw_keyboard_ui(&mut self, ctx: &Context, ui: &mut Ui) -> Option<RawKey> {
        let session = input_record::session();
        let display_ctx = when::DisplayContext {
            shift: self.shift_state,
            recording: session.is_recording(),
            replay: session.is_replay(),
            ctrl: self.ctrl_mod,
            alt: self.alt_mod,
        };

        let current_layout = self.layouts.get_mut(&self.current_layout)?;

        let mut pressed_key: Option<RawKey> = None;

        // Set semi-transparent button styling
        let style = ui.style_mut();
        style.visuals.widgets.inactive.weak_bg_fill =
            egui::Color32::from_rgba_premultiplied(60, 60, 60, 128);
        style.visuals.widgets.inactive.bg_fill =
            egui::Color32::from_rgba_premultiplied(60, 60, 60, 128);
        style.visuals.widgets.inactive.fg_stroke.color = egui::Color32::WHITE;
        style.visuals.widgets.hovered.weak_bg_fill =
            egui::Color32::from_rgba_premultiplied(80, 80, 80, 180);
        style.visuals.widgets.hovered.bg_fill =
            egui::Color32::from_rgba_premultiplied(80, 80, 80, 180);
        style.visuals.widgets.hovered.fg_stroke.color = egui::Color32::WHITE;
        style.visuals.widgets.active.weak_bg_fill =
            egui::Color32::from_rgba_premultiplied(100, 100, 100, 200);
        style.visuals.widgets.active.bg_fill =
            egui::Color32::from_rgba_premultiplied(100, 100, 100, 200);
        style.visuals.widgets.active.fg_stroke.color = egui::Color32::WHITE;
        style.visuals.selection.bg_fill = egui::Color32::from_rgba_premultiplied(50, 100, 180, 220);
        style.visuals.selection.stroke.color = egui::Color32::WHITE;

        let pad_x = current_layout.scale_x(current_layout.pad_x);
        let pad_y = current_layout.scale_y(current_layout.pad_y);

        let capturing_centres = current_layout.captured_centres.is_none();
        let mut captured_data = Vec::new();

        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::new(pad_x, pad_y);

            let left_center = current_layout.get_nearest_key_left((0.0, 0.0), self.shift_state);
            let right_center = current_layout.get_nearest_key_right((0.0, 0.0), self.shift_state);

            for (keys, indent, height) in &*current_layout {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::Vec2::new(pad_x, pad_y);

                    ui.add_space(current_layout.scale_x(indent));

                    let row_height = current_layout.scale_y(height);

                    let mut row_centres = Vec::new();

                    for key in keys {
                        // Skip rendering for SKIP keys - just add space
                        if key.is_skip() {
                            ui.add_space(current_layout.scale_x(key.width));
                            if capturing_centres {
                                row_centres.push(None);
                            }
                            continue;
                        }

                        let appearance = key.appearance(&display_ctx);
                        let mut label = egui::RichText::new(appearance.text)
                            .size(key.font_size.unwrap_or(current_layout.font_size));
                        if let Some(c) = appearance.text_color {
                            label = label.color(c);
                        }
                        let mut button = egui::Button::new(label);

                        if let Some(fill) = appearance.button_color {
                            button = button.fill(fill);
                        } else if key.is_key(
                            self.shift_state,
                            &RawKey::Action(KeyboardAction::ToggleShift),
                        ) && self.shift_state
                        {
                            button = button.selected(true);
                        } else {
                            let current_key = key.key(self.shift_state);
                            let sel0 = self.selected.0.as_ref().is_some_and(|s| s == &current_key);
                            let sel1 = self.selected.1.as_ref().is_some_and(|s| s == &current_key);

                            if sel0 && sel1 {
                                // Purple for both
                                button = button
                                    .fill(egui::Color32::from_rgb(120, 60, 180))
                                    .selected(true);
                            } else if sel0
                                || (self.selected.0.is_none()
                                    && left_center.as_ref().is_some_and(|c| c == &current_key))
                            {
                                // Blue for left stick
                                button = button
                                    .fill(egui::Color32::from_rgb(50, 100, 180))
                                    .selected(true);
                            } else if sel1
                                || (self.selected.1.is_none()
                                    && right_center.as_ref().is_some_and(|c| c == &current_key))
                            {
                                // Green for right stick
                                button = button
                                    .fill(egui::Color32::from_rgb(50, 150, 80))
                                    .selected(true);
                            }
                        }

                        let size = egui::Vec2::new(current_layout.scale_x(key.width), row_height);
                        let response = ui.add_sized(size, button);

                        if capturing_centres {
                            row_centres.push(Some(response.rect.center()));
                        }

                        // Overlay small indicator for Ctrl/Alt on the Space key in the bottom left
                        if key.display_modifiers && (self.ctrl_mod || self.alt_mod) {
                            let mut mods = Vec::new();
                            if self.ctrl_mod {
                                mods.push("ctrl");
                            }
                            if self.shift_mod {
                                mods.push("shift");
                            }
                            if self.alt_mod {
                                mods.push("alt");
                            }
                            let mod_string = mods.join("+");
                            let rect = response.rect;
                            let font_size = current_layout.font_size * 0.6;
                            ui.painter().text(
                                rect.left_bottom() + egui::Vec2::new(4.0, -4.0),
                                egui::Align2::LEFT_BOTTOM,
                                mod_string,
                                egui::FontId::proportional(font_size),
                                egui::Color32::WHITE,
                            );
                        }

                        if response.clicked() {
                            pressed_key = Some(key.key(self.shift_state));
                        }
                    }
                    if capturing_centres {
                        captured_data.push(row_centres);
                    }
                });
            }
        });

        current_layout.draw_debug(ctx, ui);

        if capturing_centres {
            current_layout.update_geometry(captured_data);
        }

        pressed_key
    }

    fn reload_from_config(&mut self) -> Result<()> {
        let cfg = config::get();

        // Reload all layouts
        self.layouts = HashMap::new();
        for (name, path) in &cfg.layouts {
            let layout = KeyboardLayout::load_from_file(path)?;
            self.layouts.insert(name.clone(), layout);
        }

        self.bindings = load_bindings(StateId::Keyboard)?;

        self.stick_select_lock_ms = Duration::from_millis(cfg.stick_select_lock_ms);

        Ok(())
    }
}

static KEYBOARD: OnceLock<Mutex<KeyboardState>> = OnceLock::new();

pub(crate) fn with_mut<R>(f: impl FnOnce(&mut KeyboardState) -> R) -> R {
    let mut guard = KEYBOARD
        .get()
        .expect("keyboard state not initialized")
        .lock()
        .unwrap();
    f(&mut guard)
}

/// Names of currently loaded keyboard layouts (sorted), for the mappings catalog.
pub(crate) fn layout_names() -> Vec<String> {
    let Some(cell) = KEYBOARD.get() else {
        return Vec::new();
    };
    let guard = cell.lock().unwrap();
    let mut names: Vec<String> = guard.layouts.keys().cloned().collect();
    names.sort();
    names
}

pub fn init() -> Result<()> {
    let kb = KeyboardState::new()?;
    KEYBOARD
        .set(Mutex::new(kb))
        .map_err(|_| anyhow::anyhow!("keyboard state already initialized"))?;

    crate::config::on_changed(|| {
        let result = with_mut(|k| k.reload_from_config());
        if let Err(e) = result {
            eprintln!("Failed to reload keyboard from config: {}", e);
        }
    })?;

    Ok(())
}

#[cfg(test)]
mod send_key_tests {
    use super::*;
    use crate::state::event::{Event, EventQueue, EventSource};

    fn drain_char(kb: &mut KeyboardState, c: char) -> Vec<Event> {
        let mut events = EventQueue::passthrough();
        let src = EventSource::MouseClick;
        kb.send_key(&RawKey::Key(c), &mut events, &src).unwrap();
        events.drain_pending().into_iter().map(|(e, _)| e).collect()
    }

    #[test]
    fn letter_without_mods_uses_send_text() {
        let mut kb = KeyboardState::default();
        assert_eq!(drain_char(&mut kb, 'c'), vec![Event::SendText("c".into())]);
    }

    #[test]
    fn ctrl_letter_sends_control_and_virtual_key() {
        let mut kb = KeyboardState {
            ctrl_mod: true,
            ..Default::default()
        };
        assert_eq!(
            drain_char(&mut kb, 'c'),
            vec![
                Event::SendKey(enigo::Key::Control, enigo::Direction::Press),
                Event::SendKey(enigo::Key::Unicode('c'), enigo::Direction::Click),
                Event::SendKey(enigo::Key::Control, enigo::Direction::Release),
            ]
        );
        assert!(!kb.ctrl_mod);
    }

    #[test]
    fn ctrl_shift_letter_sends_modifiers_and_virtual_key() {
        let mut kb = KeyboardState {
            ctrl_mod: true,
            shift_mod: true,
            ..Default::default()
        };
        assert_eq!(
            drain_char(&mut kb, 'c'),
            vec![
                Event::SendKey(enigo::Key::Shift, enigo::Direction::Press),
                Event::SendKey(enigo::Key::Control, enigo::Direction::Press),
                Event::SendKey(enigo::Key::Unicode('c'), enigo::Direction::Click),
                Event::SendKey(enigo::Key::Control, enigo::Direction::Release),
                Event::SendKey(enigo::Key::Shift, enigo::Direction::Release),
            ]
        );
    }
}

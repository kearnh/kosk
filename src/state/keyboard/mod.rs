use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::config;
use crate::controller::ControllerButton;
use crate::state::actions::get_action;
use crate::state::keyboard::layout::KeyboardLayout;
use crate::{
    controller::ControllerInput,
    state::{
        event::{Event, EventQueue},
        keyboard::key::RawKey,
        StateId,
    },
};
use anyhow::Result;
use egui::{Context, Ui};

mod key;
mod keyboard_action;
mod layout;

pub use crate::state::keyboard::keyboard_action::KeyboardAction;

#[derive(Default)]
pub struct KeyboardState {
    layouts: HashMap<String, KeyboardLayout>,
    current_layout: String,
    selected: (Option<RawKey>, Option<RawKey>),
    shift_state: bool,
    shift_mod: bool,
    ctrl_mod: bool,
    alt_mod: bool,
    mapping: HashMap<KeyboardAction, ControllerButton>,
}

impl KeyboardState {
    pub fn new() -> Result<Self> {
        let mut state: Self = Default::default();
        state.reload_from_config()?;
        state.current_layout = config::get().start_layout;
        Ok(state)
    }

    pub fn send_key(&mut self, key: &RawKey, events: &mut EventQueue) -> Result<()> {
        match key {
            RawKey::Key(c) => {
                events.start_batch();
                if self.shift_mod {
                    events.push(Event::SendKey(enigo::Key::Shift, enigo::Direction::Press));
                }
                if self.ctrl_mod {
                    events.push(Event::SendKey(enigo::Key::Control, enigo::Direction::Press));
                }
                if self.alt_mod {
                    events.push(Event::SendKey(enigo::Key::Alt, enigo::Direction::Press));
                }
                // FIXME SendKey is not sending uppercase characters, so we use SendText instead. (which uses enigo::text instead of enigo::key)
                events.push(Event::SendText(c.to_string()));
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
                if events.end_batch() {
                    self.shift_state = false;
                    self.shift_mod = false;
                    self.ctrl_mod = false;
                    self.alt_mod = false;
                }
            }
            RawKey::Enigo(k) => {
                events.start_batch();
                if self.shift_mod {
                    events.push(Event::SendKey(enigo::Key::Shift, enigo::Direction::Press));
                }
                if self.ctrl_mod {
                    events.push(Event::SendKey(enigo::Key::Control, enigo::Direction::Press));
                }
                if self.alt_mod {
                    events.push(Event::SendKey(enigo::Key::Alt, enigo::Direction::Press));
                }
                events.push(Event::SendKey(*k, enigo::Direction::Click));
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
                if events.end_batch() {
                    self.shift_state = false;
                    self.shift_mod = false;
                    self.ctrl_mod = false;
                    self.alt_mod = false;
                }
            }
            RawKey::Action(action) => {
                self.do_action(action, events)?;
            }
            RawKey::Text(text) => {
                events.push(Event::SendText(text.to_owned()));
            }
            _ => return Ok(()),
        }

        Ok(())
    }

    fn do_action(&mut self, action: &KeyboardAction, events: &mut EventQueue) -> Result<()> {
        use KeyboardAction::*;
        match action {
            SendKeyUnderLeftStick => {
                if let (Some(left), _) = &self.selected {
                    self.send_key(&left.clone(), events)?;
                }
            }
            SendKeyUnderRightStick => {
                if let (_, Some(right)) = &self.selected {
                    self.send_key(&right.clone(), events)?;
                }
            }
            ToggleShift => self.toggle_shift(),
            ToggleCtrl => self.toggle_ctrl(),
            ToggleAlt => self.toggle_alt(),
            Paste => {
                events.start_batch();
                let _ = events.push(Event::SendKey(enigo::Key::Control, enigo::Direction::Press));
                let _ = events.push(Event::SendKey(
                    enigo::Key::Unicode('v'),
                    enigo::Direction::Click,
                ));
                let _ = events.push(Event::SendKey(
                    enigo::Key::Control,
                    enigo::Direction::Release,
                ));
                let _ = events.end_batch();
            }
            SwitchState(state) => {
                let _ = events.push(Event::ChangeState(*state));
            }
            SwitchLayout(layout_name) => {
                if self.layouts.contains_key(layout_name) {
                    self.current_layout = layout_name.to_string();
                    // Reset selection when switching layouts
                    self.selected = (None, None);
                } else {
                    return Err(anyhow::anyhow!("Layout '{}' not found", layout_name));
                }
                return Ok(());
            }
            Exit => {
                let _ = events.push(Event::Exit);
            }
        }
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

    pub fn draw_ui(&mut self, ctx: &Context, ui: &mut Ui, events: &mut EventQueue) {
        if let Some(key) = self.draw_keyboard_ui(ctx, ui) { self.send_key(&key, events).expect("send key") }
    }

    pub fn handle_controller_input(
        &mut self,
        input: &Option<Box<dyn ControllerInput>>,
        events: &mut EventQueue,
    ) -> Result<()> {
        let input = match input {
            Some(input) => input,
            None => {
                // end of inputs, reset
                self.selected = (None, None);
                return Ok(());
            }
        };

        let current_layout = self
            .layouts
            .get(&self.current_layout)
            .ok_or_else(|| anyhow::anyhow!("Current layout '{}' not found", self.current_layout))?;

        let selected_left =
            current_layout.get_nearest_key_left(input.left_stick(), self.shift_state);
        let selected_right =
            current_layout.get_nearest_key_right(input.right_stick(), self.shift_state);

        self.selected = (selected_left.clone(), selected_right.clone());

        let actions = self
            .mapping
            .iter()
            .filter(|(_, button)| button.query(input.as_ref()))
            .map(|(action, _)| action.clone())
            .collect::<Vec<_>>();
        for action in actions {
            self.do_action(&action, events)?;
        }

        Ok(())
    }

    fn draw_keyboard_ui(&mut self, ctx: &Context, ui: &mut Ui) -> Option<RawKey> {
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

                        let mut button = egui::Button::new(
                            egui::RichText::new(key.display(self.shift_state))
                                .size(key.font_size.unwrap_or(current_layout.font_size)),
                        );

                        if key.is_key(
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

        self.mapping = HashMap::new();
        if let Some(raw_mapping) = cfg.controller_map.get(&StateId::Keyboard).cloned() {
            for (action, button) in raw_mapping {
                if let Some(action) = get_action(&action) {
                    if let Some(action) = action.as_ref().as_any().downcast_ref::<KeyboardAction>()
                    {
                        self.mapping.insert(action.clone(), button);
                    }
                }
            }
        }

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

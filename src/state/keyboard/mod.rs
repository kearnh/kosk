use crate::{
    controller::{ControllerInput, Dpad},
    state::{event::Event, keyboard::key::RawKey, StateId},
};
use anyhow::Result;
use egui::{Context, Ui};
use crate::state::keyboard::layout::KeyboardLayout;

mod key;
mod layout;

// FIXME why is state spread across this struct and Keyboard?
pub struct KeyboardState {
    pub layout: KeyboardLayout,
    pub selected: (Option<RawKey>, Option<RawKey>),
    pub l2_was_pressed: bool,
    pub r2_was_pressed: bool,
    pub trigger_threshold: u8,
    pub shift_state: bool,
    pub shift_mod: bool,
    pub ctrl_mod: bool,
    pub alt_mod: bool,
}

impl KeyboardState {
    pub fn new() -> Result<Self> {
        let cfg = crate::config::get();
        let layout = KeyboardLayout::load_from_file(cfg.layout)?;

        Ok(Self {
            layout,
            selected: (None, None),
            l2_was_pressed: false,
            r2_was_pressed: false,
            trigger_threshold: cfg.trigger_threshold,
            shift_state: false,
            shift_mod: false,
            ctrl_mod: false,
            alt_mod: false,
        })
    }

    pub fn send_key(&mut self, key: &RawKey, events: &mut Vec<Event>) -> Result<()> {
        macro_rules! mod_press {
            () => {
                if self.shift_mod {
                    events.push(Event::SendKey(enigo::Key::Shift, enigo::Direction::Press));
                }
                if self.ctrl_mod {
                    events.push(Event::SendKey(enigo::Key::Control, enigo::Direction::Press));
                }
                if self.alt_mod {
                    events.push(Event::SendKey(enigo::Key::Alt, enigo::Direction::Press));
                }
            };
        }
        macro_rules! mod_release {
            () => {
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
            };
        }
        match key {
            RawKey::Key(k) => {
                mod_press!();
                if let Some(c) = k.chars().next() {
                    events.push(Event::SendKey(
                        enigo::Key::Unicode(c),
                        enigo::Direction::Click,
                    ));
                }
                mod_release!();
            }
            RawKey::Enigo(k) => {
                mod_press!();
                events.push(Event::SendKey(*k, enigo::Direction::Click));
                mod_release!();
            }
            RawKey::Paste => {
                mod_press!();
                events.push(Event::SendKey(enigo::Key::Control, enigo::Direction::Press));
                events.push(Event::SendKey(
                    enigo::Key::Unicode('v'),
                    enigo::Direction::Click,
                ));
                events.push(Event::SendKey(
                    enigo::Key::Control,
                    enigo::Direction::Release,
                ));
                mod_release!();
            }
            _ => return Ok(()),
        }

        self.shift_state = false;
        self.shift_mod = false;
        self.ctrl_mod = false;
        self.alt_mod = false;

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

    pub fn draw_ui(&mut self, ctx: &Context, ui: &mut Ui, events: &mut Vec<Event>) {
        if let Some(key) = self.draw_keyboard_ui(
            ctx,
            ui,
            self.shift_state,
            self.shift_mod,
            self.ctrl_mod,
            self.alt_mod,
        ) {
            match key {
                RawKey::Done => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                RawKey::Shift => {
                    self.toggle_shift();
                }
                RawKey::Menu => {
                    events.push(Event::ChangeState(StateId::Menu));
                }
                _ => {
                    self.send_key(&key, events).expect("send key");
                }
            }
        }
    }

    pub fn handle_controller_input(
        &mut self,
        ctx: &Context,
        input: &Option<Box<dyn ControllerInput>>,
        events: &mut Vec<Event>,
    ) -> Result<()> {
        let input = match input {
            Some(input) => input,
            None => {
                // end of inputs, reset
                self.selected = (None, None);
                return Ok(());
            }
        };

        let selected_left = self
            .layout
            .get_nearest_key_left(input.left_stick(), self.shift_state);
        let selected_right = self
            .layout
            .get_nearest_key_right(input.right_stick(), self.shift_state);

        self.selected = (selected_left.clone(), selected_right.clone());

        {
            let val = input.trigger_left().unwrap_or(0);
            let pressed = val > self.trigger_threshold;

            if pressed && !self.l2_was_pressed {
                match selected_left {
                    Some(RawKey::Done) => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Some(RawKey::Menu) => {
                        events.push(Event::ChangeState(StateId::Menu));
                    }
                    Some(key) => {
                        self.send_key(&key, events)?;
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
                    Some(RawKey::Menu) => {
                        events.push(Event::ChangeState(StateId::Menu));
                    }
                    Some(key) => {
                        self.send_key(&key, events).expect("send_key");
                    }
                    _ => (),
                }
            }
            self.r2_was_pressed = pressed;
        }

        if input.face_bottom() {
            self.send_key(&RawKey::Key(" ".to_string()), events)?;
        }

        if input.face_left() {
            self.send_key(&RawKey::Enigo(enigo::Key::Backspace), events)?;
        }

        if input.face_top() {
            self.toggle_shift();
        }

        if input.stick_left() {
            self.toggle_ctrl();
        }

        if input.stick_right() {
            self.toggle_alt();
        }

        if matches!(input.dpad(), Some(Dpad::Up)) {
            events.push(Event::ChangeState(StateId::TextInput));
        }

        Ok(())
    }

    fn draw_keyboard_ui(
        &mut self,
        ctx: &Context,
        ui: &mut Ui,
        shift_state: bool,
        shift_mod: bool,
        ctrl_mod: bool,
        alt_mod: bool,
    ) -> Option<RawKey> {
        let mut pressed_key: Option<RawKey> = None;

        // Set semi-transparent button styling
        let style = ui.style_mut();
        style.visuals.widgets.inactive.weak_bg_fill =
            egui::Color32::from_rgba_premultiplied(60, 60, 60, 128);
        style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgba_premultiplied(60, 60, 60, 128);
        style.visuals.widgets.inactive.fg_stroke.color = egui::Color32::WHITE;
        style.visuals.widgets.hovered.weak_bg_fill =
            egui::Color32::from_rgba_premultiplied(80, 80, 80, 180);
        style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgba_premultiplied(80, 80, 80, 180);
        style.visuals.widgets.hovered.fg_stroke.color = egui::Color32::WHITE;
        style.visuals.widgets.active.weak_bg_fill =
            egui::Color32::from_rgba_premultiplied(100, 100, 100, 200);
        style.visuals.widgets.active.bg_fill = egui::Color32::from_rgba_premultiplied(100, 100, 100, 200);
        style.visuals.widgets.active.fg_stroke.color = egui::Color32::WHITE;
        style.visuals.selection.bg_fill = egui::Color32::from_rgba_premultiplied(50, 100, 180, 220);
        style.visuals.selection.stroke.color = egui::Color32::WHITE;

        let pad_x = self.layout.scale_x(self.layout.pad_x);
        let pad_y = self.layout.scale_y(self.layout.pad_y);

        let capturing_centres = self.layout.captured_centres.is_none();
        let mut captured_data = Vec::new();

        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::new(pad_x, pad_y);

            let left_center = self.layout.get_nearest_key_left((0.0, 0.0), shift_state);
            let right_center = self.layout.get_nearest_key_right((0.0, 0.0), shift_state);

            for (keys, indent, height) in &self.layout {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::Vec2::new(pad_x, pad_y);

                    ui.add_space(self.layout.scale_x(indent));

                    let row_height = self.layout.scale_y(height);

                    let mut row_centres = Vec::new();

                    for key in keys {
                        // Skip rendering for SKIP keys - just add space
                        if key.is_skip() {
                            ui.add_space(self.layout.scale_x(key.width));
                            if capturing_centres {
                                row_centres.push(None);
                            }
                            continue;
                        }

                        let mut button = egui::Button::new(
                            egui::RichText::new(key.display(shift_state)).size(self.layout.font_size),
                        );

                        if key.is_key(shift_state, &RawKey::Shift) && shift_state {
                            button = button.selected(true);
                        } else {
                            let current_key = key.key(shift_state);
                            let sel0 = self.selected.0.as_ref().is_some_and(|s| s == &current_key);
                            let sel1 = self.selected.1.as_ref().is_some_and(|s| s == &current_key);

                            if sel0 && sel1 {
                                // Purple for both
                                button =
                                    button.fill(egui::Color32::from_rgb(120, 60, 180)).selected(true);
                            } else if sel0
                                || (self.selected.0.is_none()
                                    && left_center.as_ref().is_some_and(|c| c == &current_key))
                            {
                                // Blue for left stick
                                button =
                                    button.fill(egui::Color32::from_rgb(50, 100, 180)).selected(true);
                            } else if sel1
                                || (self.selected.1.is_none()
                                    && right_center.as_ref().is_some_and(|c| c == &current_key))
                            {
                                // Green for right stick
                                button = button.fill(egui::Color32::from_rgb(50, 150, 80)).selected(true);
                            }
                        }

                        let size = egui::Vec2::new(self.layout.scale_x(key.width), row_height);
                        let response = ui.add_sized(size, button);

                        if capturing_centres {
                            row_centres.push(Some(response.rect.center()));
                        }

                        // Overlay small indicator for Ctrl/Alt on the Space key in the bottom left
                        if key.is_key(shift_state, " ") && (ctrl_mod || alt_mod) {
                            let mut mods = Vec::new();
                            if ctrl_mod {
                                mods.push("ctrl");
                            }
                            if shift_mod {
                                mods.push("shift");
                            }
                            if alt_mod {
                                mods.push("alt");
                            }
                            let mod_string = mods.join("+");
                            let rect = response.rect;
                            let font_size = self.layout.font_size * 0.6;
                            ui.painter().text(
                                rect.left_bottom() + egui::Vec2::new(4.0, -4.0),
                                egui::Align2::LEFT_BOTTOM,
                                mod_string,
                                egui::FontId::proportional(font_size),
                                egui::Color32::WHITE,
                            );
                        }

                        if response.clicked() {
                            pressed_key = Some(key.key(shift_state));
                        }
                    }
                    if capturing_centres {
                        captured_data.push(row_centres);
                    }
                });
            }
        });

        self.layout.draw_debug(ctx, ui);

        if capturing_centres {
            self.layout.update_geometry(captured_data);
        }

        pressed_key
    }
}

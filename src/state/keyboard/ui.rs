use anyhow::Result;
use egui::{Button, Color32, Context, Rect, RichText, Ui, Vec2};

use crate::config;
use crate::state::keyboard::key::RawKey;
use crate::state::keyboard::layout::KeyboardLayout;

pub struct Keyboard {
    pub selected: (Option<RawKey>, Option<RawKey>),
    pub layout: KeyboardLayout,
    left_stick_center: (f32, f32),
    right_stick_center: (f32, f32),
    stick_scale_x: f32,
    stick_scale_y: f32,
}

impl Keyboard {
    pub fn new() -> Result<Self> {
        let cfg = config::get();

        let layout = KeyboardLayout::load_from_file(cfg.layout)?;

        Ok(Self {
            selected: (None, None),
            layout,
            left_stick_center: Default::default(),
            right_stick_center: Default::default(),
            stick_scale_x: cfg.stick_scale_x,
            stick_scale_y: cfg.stick_scale_y,
        })
    }

    pub fn get_nearest_key_left(&self, stick: (f32, f32), shift_state: bool) -> Option<RawKey> {
        self.get_nearest_key(
            self.left_stick_center,
            stick,
            &self.layout.left_stick_bounds,
            shift_state,
        )
    }

    pub fn get_nearest_key_right(&self, stick: (f32, f32), shift_state: bool) -> Option<RawKey> {
        self.get_nearest_key(
            self.right_stick_center,
            stick,
            &self.layout.right_stick_bounds,
            shift_state,
        )
    }

    fn get_nearest_key(
        &self,
        center: (f32, f32),
        stick: (f32, f32),
        bounds: &[Rect],
        shift_state: bool,
    ) -> Option<RawKey> {
        let (x, y) = stick;
        let dx = self.layout.scale_x(x.into()) * self.stick_scale_x;
        let dy = self.layout.scale_y(y.into()) * self.stick_scale_y;

        let mut cursor_x = center.0 + dx;
        let mut cursor_y = center.1 + dy;

        if !bounds.is_empty() {
            // Check if we are already inside any of the bounds
            let is_inside = bounds
                .iter()
                .any(|r| r.contains((cursor_x, cursor_y).into()));

            if !is_inside {
                // We are outside all bounds. Find the closest point on the closest rectangle.
                let mut closest_point = (cursor_x, cursor_y);
                let mut min_dist_sq = f32::MAX;

                for r in bounds {
                    // Clamp the cursor to the individual rectangle to find the closest point on it
                    let clamped_x = cursor_x.clamp(r.min.x, r.max.x);
                    let clamped_y = cursor_y.clamp(r.min.y, r.max.y);

                    let dist_sq = (cursor_x - clamped_x).powi(2) + (cursor_y - clamped_y).powi(2);
                    if dist_sq < min_dist_sq {
                        min_dist_sq = dist_sq;
                        closest_point = (clamped_x, clamped_y);
                    }
                }
                (cursor_x, cursor_y) = closest_point;
            }
        }

        self.layout.get_key_at(cursor_x, cursor_y, shift_state)
    }

    pub fn draw_ui(
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
            Color32::from_rgba_premultiplied(60, 60, 60, 128);
        style.visuals.widgets.inactive.bg_fill = Color32::from_rgba_premultiplied(60, 60, 60, 128);
        style.visuals.widgets.inactive.fg_stroke.color = Color32::WHITE;
        style.visuals.widgets.hovered.weak_bg_fill =
            Color32::from_rgba_premultiplied(80, 80, 80, 180);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgba_premultiplied(80, 80, 80, 180);
        style.visuals.widgets.hovered.fg_stroke.color = Color32::WHITE;
        style.visuals.widgets.active.weak_bg_fill =
            Color32::from_rgba_premultiplied(100, 100, 100, 200);
        style.visuals.widgets.active.bg_fill = Color32::from_rgba_premultiplied(100, 100, 100, 200);
        style.visuals.widgets.active.fg_stroke.color = Color32::WHITE;
        style.visuals.selection.bg_fill = Color32::from_rgba_premultiplied(50, 100, 180, 220);
        style.visuals.selection.stroke.color = Color32::WHITE;

        let pad_x = self.layout.scale_x(self.layout.pad_x);
        let pad_y = self.layout.scale_y(self.layout.pad_y);

        let capturing_centres = self.layout.captured_centres.is_none();
        let mut captured_data = Vec::new();

        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(pad_x, pad_y);

            let left_center = self.get_nearest_key_left((0.0, 0.0), shift_state);
            let right_center = self.get_nearest_key_right((0.0, 0.0), shift_state);

            for (keys, indent, height) in &self.layout {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(pad_x, pad_y);

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

                        let mut button = Button::new(
                            RichText::new(key.display(shift_state)).size(self.layout.font_size),
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
                                    button.fill(Color32::from_rgb(120, 60, 180)).selected(true);
                            } else if sel0
                                || (self.selected.0.is_none()
                                    && left_center.as_ref().is_some_and(|c| c == &current_key))
                            {
                                // Blue for left stick
                                button =
                                    button.fill(Color32::from_rgb(50, 100, 180)).selected(true);
                            } else if sel1
                                || (self.selected.1.is_none()
                                    && right_center.as_ref().is_some_and(|c| c == &current_key))
                            {
                                // Green for right stick
                                button = button.fill(Color32::from_rgb(50, 150, 80)).selected(true);
                            }
                        }

                        let size = Vec2::new(self.layout.scale_x(key.width), row_height);
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
                                rect.left_bottom() + Vec2::new(4.0, -4.0),
                                egui::Align2::LEFT_BOTTOM,
                                mod_string,
                                egui::FontId::proportional(font_size),
                                Color32::WHITE,
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

        self.layout.draw_debug(
            ctx,
            ui,
            (self.stick_scale_x, self.stick_scale_y),
            self.left_stick_center,
            self.right_stick_center,
        );

        if capturing_centres {
            self.layout.captured_centres = Some(captured_data);
            self.layout.calculate_hitboxes();

            self.left_stick_center = self.layout.get_key_center("d").unwrap_or_default();
            self.right_stick_center = self.layout.get_key_center("k").unwrap_or_default();
        }

        pressed_key
    }
}

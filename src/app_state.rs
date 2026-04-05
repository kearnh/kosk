use anyhow::Result;
use egui::{Context, Ui};
use std::path::Path;

use crate::{
    keyboard::{Keyboard, RawKey},
    ps4::{Dpad, Ps4InputData},
};

pub enum AppState {
    Keyboard {
        kb: Keyboard,
        l2_was_pressed: bool,
        r2_was_pressed: bool,
        trigger_threshold: u8,
        pos: usize,
        monitor_size: (f32, f32),
    },
}

impl AppState {
    pub fn start_state(
        layout_path: impl AsRef<Path>,
        stick_range_x: f32,
        stick_range_y: f32,
        stick_warp: f32,
        trigger_threshold: u8,
        monitor_size: (f32, f32),
    ) -> Result<Self> {
        let kb = Keyboard::new(layout_path, stick_range_x, stick_range_y, stick_warp)?;
        Ok(AppState::Keyboard {
            kb,
            l2_was_pressed: false,
            r2_was_pressed: false,
            trigger_threshold,
            pos: 0,
            monitor_size,
        })
    }

    pub fn window_size(&self) -> (f32, f32) {
        match self {
            AppState::Keyboard { kb, .. } => kb.layout.get_dimensions(),
        }
    }

    pub fn draw_ui(&mut self, ctx: &Context, ui: &mut Ui) {
        match self {
            AppState::Keyboard { kb, .. } => {
                if let Some(key) = kb.draw_ui(ui) {
                    if key == RawKey::Done {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        return;
                    }
                    if key == RawKey::Shift {
                        kb.toggle_shift();
                    } else {
                        kb.send_key(&key).expect("send key");
                    }
                }
            }
        }
    }

    pub fn handle_controller_input(
        &mut self,
        ctx: &Context,
        input: &Option<Ps4InputData>,
    ) -> Result<()> {
        match self {
            AppState::Keyboard {
                kb,
                trigger_threshold,
                l2_was_pressed,
                r2_was_pressed,
                pos,
                monitor_size,
            } => {
                let input = match input {
                    Some(input) => input,
                    None => {
                        // end of inputs, reset
                        kb.selected = (None, None);
                        return Ok(());
                    }
                };

                let selected_left = kb.get_nearest_key_left(input.left);
                let selected_right = kb.get_nearest_key_right(input.right);

                kb.selected = (selected_left.clone(), selected_right.clone());

                {
                    let val = input.l2.unwrap_or(0);
                    let pressed = val > *trigger_threshold;

                    if pressed && !*l2_was_pressed {
                        match selected_left {
                            Some(RawKey::Done) => {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                            Some(key) => {
                                kb.send_key(&key)?;
                            }
                            _ => (),
                        }
                    }
                    *l2_was_pressed = pressed;
                }
                {
                    let val = input.r2.unwrap_or(0);
                    let pressed = val > *trigger_threshold;

                    if pressed && !*r2_was_pressed {
                        match selected_right {
                            Some(RawKey::Done) => {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                            Some(key) => {
                                kb.send_key(&key).expect("send_key");
                            }
                            _ => (),
                        }
                    }
                    *r2_was_pressed = pressed;
                }

                if input.cross {
                    kb.send_key(&RawKey::Key(" ".to_string()))?;
                }

                if input.square {
                    kb.send_key(&RawKey::Backspace)?;
                }

                if input.triangle {
                    kb.toggle_shift();
                }

                if input.l3 {
                    kb.toggle_ctrl();
                }

                if input.r3 {
                    kb.toggle_alt();
                }

                if matches!(input.dpad, Some(Dpad::Down)) {
                    let (kb_width, kb_height) = { kb.layout.get_dimensions() };
                    let (size_x, size_y) = *monitor_size;
                    let xy = [
                        (0.0, 0.0),
                        (size_x - kb_width, 0.0),
                        (size_x - kb_width, size_y - kb_height),
                        (0.0, size_y - kb_height),
                    ][*pos];
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(xy.into()));
                    *pos += 1;
                    *pos %= 4;
                }
            }
        }

        Ok(())
    }
}

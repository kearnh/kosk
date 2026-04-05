use anyhow::Result;
use egui::{Context, Ui};
use std::path::Path;

use crate::{
    keyboard::{Keyboard, RawKey},
    ps4::{Dpad, Ps4InputData},
};

pub trait AppState: Send {
    fn window_size(&self) -> (f32, f32);
    fn draw_ui(&mut self, ctx: &Context, ui: &mut Ui);
    fn handle_controller_input(
        &mut self,
        ctx: &Context,
        input: &Option<Ps4InputData>,
    ) -> Result<()>;
}

pub struct KeyboardState {
    kb: Keyboard,
    l2_was_pressed: bool,
    r2_was_pressed: bool,
    trigger_threshold: u8,
    pos: usize,
    monitor_size: (f32, f32),
}

impl KeyboardState {
    pub fn new(
        layout_path: impl AsRef<Path>,
        stick_range_x: f32,
        stick_range_y: f32,
        stick_warp: f32,
        trigger_threshold: u8,
        monitor_size: (f32, f32),
    ) -> Result<Self> {
        let kb = Keyboard::new(layout_path, stick_range_x, stick_range_y, stick_warp)?;
        Ok(Self {
            kb,
            l2_was_pressed: false,
            r2_was_pressed: false,
            trigger_threshold,
            pos: 0,
            monitor_size,
        })
    }
}

impl AppState for KeyboardState {
    fn window_size(&self) -> (f32, f32) {
        self.kb.layout.get_dimensions()
    }

    fn draw_ui(&mut self, ctx: &Context, ui: &mut Ui) {
        if let Some(key) = self.kb.draw_ui(ui) {
            if key == RawKey::Done {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            if key == RawKey::Shift {
                self.kb.toggle_shift();
            } else {
                self.kb.send_key(&key).expect("send key");
            }
        }
    }

    fn handle_controller_input(
        &mut self,
        ctx: &Context,
        input: &Option<Ps4InputData>,
    ) -> Result<()> {
        let input = match input {
            Some(input) => input,
            None => {
                // end of inputs, reset
                self.kb.selected = (None, None);
                return Ok(());
            }
        };

        let selected_left = self.kb.get_nearest_key_left(input.left);
        let selected_right = self.kb.get_nearest_key_right(input.right);

        self.kb.selected = (selected_left.clone(), selected_right.clone());

        {
            let val = input.l2.unwrap_or(0);
            let pressed = val > self.trigger_threshold;

            if pressed && !self.l2_was_pressed {
                match selected_left {
                    Some(RawKey::Done) => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Some(key) => {
                        self.kb.send_key(&key)?;
                    }
                    _ => (),
                }
            }
            self.l2_was_pressed = pressed;
        }
        {
            let val = input.r2.unwrap_or(0);
            let pressed = val > self.trigger_threshold;

            if pressed && !self.r2_was_pressed {
                match selected_right {
                    Some(RawKey::Done) => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Some(key) => {
                        self.kb.send_key(&key).expect("send_key");
                    }
                    _ => (),
                }
            }
            self.r2_was_pressed = pressed;
        }

        if input.cross {
            self.kb.send_key(&RawKey::Key(" ".to_string()))?;
        }

        if input.square {
            self.kb.send_key(&RawKey::Backspace)?;
        }

        if input.triangle {
            self.kb.toggle_shift();
        }

        if input.l3 {
            self.kb.toggle_ctrl();
        }

        if input.r3 {
            self.kb.toggle_alt();
        }

        if matches!(input.dpad, Some(Dpad::Down)) {
            let (kb_width, kb_height) = { self.kb.layout.get_dimensions() };
            let (size_x, size_y) = self.monitor_size;
            let xy = [
                (0.0, 0.0),
                (size_x - kb_width, 0.0),
                (size_x - kb_width, size_y - kb_height),
                (0.0, size_y - kb_height),
            ][self.pos];
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(xy.into()));
            self.pos += 1;
            self.pos %= 4;
        }

        Ok(())
    }
}

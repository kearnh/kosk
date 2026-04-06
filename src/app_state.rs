use anyhow::Result;
use egui::{Button, Context, Ui};

use crate::{
    config,
    keyboard::{Keyboard, RawKey},
    ps4::{Dpad, Ps4InputData},
};

pub enum StateId {
    Menu,
    Keyboard,
}

pub struct AppState {
    state: StateId,
    kb: KeyboardState,
    menu: MenuState,
}

impl AppState {
    pub fn new(monitor_size: (f32, f32)) -> Result<Self> {
        let cfg = config::get();
        let kb_inner = Keyboard::new(&cfg.layout, cfg.stick_x, cfg.stick_y, cfg.stick_warp)?;
        let kb = KeyboardState {
            kb: kb_inner,
            l2_was_pressed: false,
            r2_was_pressed: false,
            trigger_threshold: cfg.trigger_threshold,
            pos: 0,
            monitor_size,
        };
        Ok(Self {
            state: StateId::Keyboard,
            kb,
            menu: MenuState,
        })
    }

    pub fn window_size(&self) -> (f32, f32) {
        self.kb.kb.layout.get_dimensions()
    }

    pub fn draw_ui(&mut self, ctx: &Context, ui: &mut Ui) {
        let id = match self.state {
            StateId::Keyboard => self.kb.draw_ui(ctx, ui),
            StateId::Menu => self.menu.draw_ui(ctx, ui),
        };
        self.state = id;
    }

    pub fn handle_controller_input(
        &mut self,
        ctx: &Context,
        input: &Option<Ps4InputData>,
    ) -> Result<()> {
        let id = match self.state {
            StateId::Keyboard => self.kb.handle_controller_input(ctx, input)?,
            StateId::Menu => self.menu.handle_controller_input(ctx, input)?,
        };
        self.state = id;
        Ok(())
    }
}

struct KeyboardState {
    kb: Keyboard,
    l2_was_pressed: bool,
    r2_was_pressed: bool,
    trigger_threshold: u8,
    pos: usize,
    monitor_size: (f32, f32),
}

impl KeyboardState {
    fn draw_ui(&mut self, ctx: &Context, ui: &mut Ui) -> StateId {
        if let Some(key) = self.kb.draw_ui(ui) {
            match key {
                RawKey::Done => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                RawKey::Shift => {
                    self.kb.toggle_shift();
                }
                RawKey::Menu => return StateId::Menu,
                _ => {
                    self.kb.send_key(&key).expect("send key");
                }
            }
        }
        StateId::Keyboard
    }

    fn handle_controller_input(
        &mut self,
        ctx: &Context,
        input: &Option<Ps4InputData>,
    ) -> Result<StateId> {
        let input = match input {
            Some(input) => input,
            None => {
                // end of inputs, reset
                self.kb.selected = (None, None);
                return Ok(StateId::Keyboard);
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
                    Some(RawKey::Menu) => return Ok(StateId::Menu),
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
                    Some(RawKey::Menu) => return Ok(StateId::Menu),
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
            self.kb.send_key(&RawKey::Enigo(enigo::Key::Backspace))?;
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

        Ok(StateId::Keyboard)
    }
}

struct MenuState;

impl MenuState {
    pub fn draw_ui(&mut self, ctx: &Context, ui: &mut Ui) -> StateId {
        if ui.add(Button::new("back")).clicked() {
            return StateId::Keyboard;
        }
        StateId::Menu
    }

    fn handle_controller_input(
        &mut self,
        ctx: &Context,
        input: &Option<Ps4InputData>,
    ) -> Result<StateId> {
        if let Some(input) = input {
            if input.cross {
                return Ok(StateId::Keyboard);
            }
        }
        Ok(StateId::Menu)
    }
}

use crate::{
    config,
    controller::bindings::BindingEngine,
    controller::record as input_record,
    controller::ControllerInput,
    state::{
        actions::load_bindings,
        event::{Event, EventQueue, EventSource},
        keyboard,
        select_layout_action::SelectLayoutAction,
        StateId,
    },
};

use anyhow::Result;
use egui::{Button, Context, Ui};
use std::sync::{Mutex, OnceLock};

pub struct SelectLayoutState {
    names: Vec<String>,
    selected: usize,
    preview: bool,
    preview_kb: Option<keyboard::KeyboardState>,
    bindings: BindingEngine<SelectLayoutAction>,
}

impl SelectLayoutState {
    pub fn new() -> Result<Self> {
        Ok(Self {
            names: Vec::new(),
            selected: 0,
            preview: false,
            preview_kb: None,
            bindings: load_bindings(StateId::SelectLayout)?.with_left_stick_dpad(),
        })
    }

    pub fn begin(&mut self) {
        self.names = keyboard::layout_names();
        let current = keyboard::current_layout_name();
        self.selected = self.names.iter().position(|n| n == &current).unwrap_or(0);
        self.preview = false;
        self.rebuild_preview_kb();
    }

    fn rebuild_preview_kb(&mut self) {
        let mut cfg = config::get();
        if let Some(name) = self.names.get(self.selected) {
            cfg.start_layout = name.clone();
        }
        match keyboard::KeyboardState::with_config(cfg) {
            Ok(kb) => self.preview_kb = Some(kb),
            Err(e) => {
                eprintln!("select layout preview: {e:#}");
                self.preview_kb = None;
            }
        }
    }

    fn sync_preview_layout(&mut self) {
        let Some(name) = self.names.get(self.selected) else {
            return;
        };
        let Some(kb) = self.preview_kb.as_mut() else {
            return;
        };
        if let Err(e) = kb.set_current_layout(name) {
            eprintln!("select layout preview: {e:#}");
        }
    }

    pub fn draw_ui(&mut self, ctx: &Context, ui: &mut Ui, events: &mut EventQueue) {
        self.names = keyboard::layout_names();
        if self.names.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.names.len() {
            self.selected = self.names.len() - 1;
        }
        self.sync_preview_layout();

        if self.names.is_empty() {
            ui.add_enabled(false, Button::new("No layouts"));
            return;
        }

        let current = keyboard::current_layout_name();
        let mut clicked: Option<usize> = None;

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading("Layouts");
                for (i, name) in self.names.iter().enumerate() {
                    let label = if *name == current {
                        format!("{name} (current)")
                    } else {
                        name.clone()
                    };
                    if ui
                        .add(Button::new(label).selected(i == self.selected))
                        .clicked()
                    {
                        clicked = Some(i);
                    }
                }
            });
            if self.preview {
                if let Some(kb) = self.preview_kb.as_mut() {
                    let _ = kb.draw_keyboard_ui(ctx, ui);
                }
            }
        });

        if let Some(i) = clicked {
            self.selected = i;
            let _ = self.do_action(
                &SelectLayoutAction::Activate,
                events,
                &EventSource::MouseClick,
            );
        }
    }

    fn do_action(
        &mut self,
        action: &SelectLayoutAction,
        events: &mut EventQueue,
        source: &EventSource,
    ) -> Result<()> {
        use SelectLayoutAction::*;
        match action {
            SelectUp => {
                if self.names.is_empty() {
                    return Ok(());
                }
                self.selected =
                    (self.selected as isize - 1).rem_euclid(self.names.len() as isize) as usize;
            }
            SelectDown => {
                if self.names.is_empty() {
                    return Ok(());
                }
                self.selected =
                    (self.selected as isize + 1).rem_euclid(self.names.len() as isize) as usize;
            }
            Activate => {
                let Some(name) = self.names.get(self.selected).cloned() else {
                    return Ok(());
                };
                match keyboard::with_mut(|kb| kb.set_current_layout(&name)) {
                    Ok(()) => {
                        input_record::session().tap_layout(&name);
                        let _ = events.push(Event::ChangeState(StateId::Keyboard), source);
                    }
                    Err(e) => eprintln!("{e:#}"),
                }
            }
            TogglePreview => {
                self.preview = !self.preview;
            }
            SwitchState(state) => {
                let _ = events.push(Event::ChangeState(*state), source);
            }
        }
        Ok(())
    }

    pub fn reset_controller_input(&mut self, holdover: Option<&dyn ControllerInput>) {
        self.bindings.reset(holdover);
    }

    pub fn handle_controller_input(
        &mut self,
        _: &Context,
        input: &dyn ControllerInput,
        events: &mut EventQueue,
    ) -> Result<()> {
        for (binding, action) in self.bindings.evaluate(input) {
            let src = EventSource::Controller(binding);
            self.do_action(&action, events, &src)?;
        }
        Ok(())
    }

    fn reload_from_config(&mut self) -> Result<()> {
        self.bindings = load_bindings(StateId::SelectLayout)?.with_left_stick_dpad();
        self.rebuild_preview_kb();
        Ok(())
    }
}

static SELECT_LAYOUT: OnceLock<Mutex<SelectLayoutState>> = OnceLock::new();

pub fn init() -> Result<()> {
    let state = SelectLayoutState::new()?;
    SELECT_LAYOUT
        .set(Mutex::new(state))
        .map_err(|_| anyhow::anyhow!("select layout state already initialized"))?;

    crate::config::on_changed(|| {
        let result = with_mut(|s| s.reload_from_config());
        if let Err(e) = result {
            eprintln!("Failed to reload select layout from config: {e}");
        }
    })?;

    Ok(())
}

pub(crate) fn with_mut<R>(f: impl FnOnce(&mut SelectLayoutState) -> R) -> R {
    let mut guard = SELECT_LAYOUT
        .get()
        .expect("select layout state not initialized")
        .lock()
        .unwrap();
    f(&mut guard)
}

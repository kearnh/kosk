use crate::{
    controller::{ControllerButton, ControllerInput},
    state::event::{Event, EventQueue, EventSource, ReturnStateResult},
};

use anyhow::Result;
use egui::{Context, TextEdit, Ui};
use std::sync::{Mutex, OnceLock};

/// Hardcoded Select Key chrome — tweak here only (not in mappings.toml).
const OK: ControllerButton = ControllerButton::FaceBottom;
const CANCEL: ControllerButton = ControllerButton::FaceRight;

pub struct SelectKeyState {
    /// Buffer shown in the stub TextEdit; prefilled by caller via `begin(initial)`.
    text: String,
    status: String,
    prev_ok: bool,
    prev_cancel: bool,
}

impl SelectKeyState {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            status: String::new(),
            prev_ok: false,
            prev_cancel: false,
        }
    }

    pub fn begin(&mut self, initial: &str) {
        self.text = initial.to_owned();
        self.status.clear();
        self.prev_ok = false;
        self.prev_cancel = false;
    }

    pub fn draw_ui(&mut self, _: &Context, ui: &mut Ui, events: &mut EventQueue) {
        ui.heading("Select Key");
        ui.label("(temporary text entry — not final)");
        ui.add(TextEdit::singleline(&mut self.text).desired_width(240.0));
        if !self.status.is_empty() {
            ui.label(&self.status);
        }
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                self.submit_ok(events, &EventSource::MouseClick);
            }
            if ui.button("Cancel").clicked() {
                let _ = events.push(
                    Event::ReturnState(ReturnStateResult::Cancelled),
                    &EventSource::MouseClick,
                );
            }
        });
        ui.label("A OK    B Cancel");
    }

    fn submit_ok(&mut self, events: &mut EventQueue, source: &EventSource) {
        let trimmed = self.text.trim();
        if trimmed.is_empty() {
            self.status = "enter a key".to_owned();
            return;
        }
        let _ = events.push(
            Event::ReturnState(ReturnStateResult::Value(trimmed.to_owned())),
            source,
        );
    }

    pub fn handle_controller_input(
        &mut self,
        input: &Option<Box<dyn ControllerInput>>,
        events: &mut EventQueue,
    ) -> Result<()> {
        let Some(input) = input.as_ref() else {
            self.prev_ok = false;
            self.prev_cancel = false;
            return Ok(());
        };

        let ok_down = OK.query(input.as_ref());
        let cancel_down = CANCEL.query(input.as_ref());

        if ok_down && !self.prev_ok {
            let binding = crate::controller::ControllerBinding::Single(OK);
            self.submit_ok(events, &EventSource::Controller(binding));
        }
        if cancel_down && !self.prev_cancel {
            let binding = crate::controller::ControllerBinding::Single(CANCEL);
            let _ = events.push(
                Event::ReturnState(ReturnStateResult::Cancelled),
                &EventSource::Controller(binding),
            );
        }

        self.prev_ok = ok_down;
        self.prev_cancel = cancel_down;
        Ok(())
    }
}

static SELECT_KEY: OnceLock<Mutex<SelectKeyState>> = OnceLock::new();

pub fn init() -> Result<()> {
    SELECT_KEY
        .set(Mutex::new(SelectKeyState::new()))
        .map_err(|_| anyhow::anyhow!("select key state already initialized"))?;
    Ok(())
}

pub(crate) fn with_mut<R>(f: impl FnOnce(&mut SelectKeyState) -> R) -> R {
    let mut guard = SELECT_KEY
        .get()
        .expect("select key state not initialized")
        .lock()
        .unwrap();
    f(&mut guard)
}

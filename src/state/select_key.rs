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

    pub fn reset_controller_input(&mut self, holdover: Option<&dyn ControllerInput>) {
        match holdover {
            None => {
                self.prev_ok = false;
                self.prev_cancel = false;
            }
            Some(input) => {
                self.prev_ok = OK.query(input);
                self.prev_cancel = CANCEL.query(input);
            }
        }
    }

    pub fn handle_controller_input(
        &mut self,
        input: &dyn ControllerInput,
        events: &mut EventQueue,
    ) -> Result<()> {
        let ok_down = OK.query(input);
        let cancel_down = CANCEL.query(input);

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[derive(Debug, Clone)]
    struct ButtonSetInput(HashSet<ControllerButton>);

    impl ControllerInput for ButtonSetInput {
        fn left_stick_raw(&self) -> (f32, f32) {
            (0.0, 0.0)
        }
        fn right_stick_raw(&self) -> (f32, f32) {
            (0.0, 0.0)
        }
        fn dpad_up(&self) -> bool {
            false
        }
        fn dpad_down(&self) -> bool {
            false
        }
        fn dpad_left(&self) -> bool {
            false
        }
        fn dpad_right(&self) -> bool {
            false
        }
        fn face_bottom(&self) -> bool {
            self.0.contains(&ControllerButton::FaceBottom)
        }
        fn face_right(&self) -> bool {
            self.0.contains(&ControllerButton::FaceRight)
        }
        fn face_top(&self) -> bool {
            false
        }
        fn face_left(&self) -> bool {
            false
        }
        fn shoulder_left(&self) -> bool {
            false
        }
        fn shoulder_right(&self) -> bool {
            false
        }
        fn stick_left(&self) -> bool {
            false
        }
        fn stick_right(&self) -> bool {
            false
        }
        fn trigger_left(&self) -> Option<u8> {
            None
        }
        fn trigger_right(&self) -> Option<u8> {
            None
        }
        fn btn_options(&self) -> bool {
            false
        }
        fn btn_share(&self) -> bool {
            false
        }
        fn btn_system(&self) -> bool {
            false
        }
        fn is_engaged(&self) -> bool {
            !self.0.is_empty()
        }
        fn box_clone(&self) -> Box<dyn ControllerInput + Send + Sync> {
            Box::new(self.clone())
        }
    }

    #[test]
    fn holdover_reset_suppresses_ok_until_repress() {
        let mut s = SelectKeyState::new();
        s.begin("");
        let held = ButtonSetInput(HashSet::from([ControllerButton::FaceBottom]));
        let empty = ButtonSetInput(HashSet::new());
        s.reset_controller_input(Some(&held));

        let mut events = EventQueue::passthrough();
        s.handle_controller_input(&held, &mut events).unwrap();
        assert!(
            events.drain_pending().is_empty(),
            "holdover must not OK on still-held A"
        );
        assert!(s.status.is_empty());

        s.handle_controller_input(&empty, &mut events).unwrap();
        s.handle_controller_input(&held, &mut events).unwrap();
        assert!(events.drain_pending().is_empty());
        assert_eq!(s.status, "enter a key");
    }
}

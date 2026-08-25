use crate::{
    controller::{ControllerBinding, ControllerButton, ControllerInput},
    state::actions::get_action,
    state::event::{Event, EventQueue, EventSource, ReturnStateResult},
    state::StateId,
};

use anyhow::Result;
use egui::{Context, TextEdit, Ui};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Mutex, OnceLock};

/// Hardcoded Select Key chrome — tweak here only (not in mappings.toml).
const OK: ControllerButton = ControllerButton::FaceBottom;
const CANCEL: ControllerButton = ControllerButton::FaceRight;

pub struct SelectKeyState {
    binding: String,
    action: String,
    /// Mode whose action catalog validates `action`; set by `begin`.
    mode: StateId,
    /// Snapshot of the caller's tab draft (action → bindings) for the reactive list.
    draft_mode: HashMap<String, Vec<ControllerBinding>>,
    /// Pill being replaced; omitted from the reactive list under the typed action.
    editing: Option<ControllerBinding>,
    status: String,
    prev_ok: bool,
    prev_cancel: bool,
    /// Set by `begin`; cleared after the first `request_focus` on the Binding field.
    request_text_focus: bool,
}

impl SelectKeyState {
    pub fn new() -> Self {
        Self {
            binding: String::new(),
            action: String::new(),
            mode: StateId::Keyboard,
            draft_mode: HashMap::new(),
            editing: None,
            status: String::new(),
            prev_ok: false,
            prev_cancel: false,
            request_text_focus: false,
        }
    }

    pub fn begin(
        &mut self,
        binding: String,
        action: String,
        mode: StateId,
        draft_mode: HashMap<String, Vec<ControllerBinding>>,
        editing: Option<ControllerBinding>,
    ) {
        self.binding = binding;
        self.action = action;
        self.mode = mode;
        self.draft_mode = draft_mode;
        self.editing = editing;
        self.status.clear();
        self.prev_ok = false;
        self.prev_cancel = false;
        self.request_text_focus = true;
    }

    pub fn draw_ui(&mut self, _: &Context, ui: &mut Ui, events: &mut EventQueue) {
        ui.heading("Select Key");
        ui.label("(temporary text entry — not final)");

        ui.label("Binding");
        let binding_resp = ui.add(TextEdit::singleline(&mut self.binding).desired_width(240.0));
        if self.request_text_focus {
            binding_resp.request_focus();
            self.request_text_focus = false;
        }

        ui.label("Action");
        ui.add(TextEdit::singleline(&mut self.action).desired_width(240.0));

        let action_key = self.action.trim();
        if !action_key.is_empty() {
            let existing: Vec<String> = self
                .draft_mode
                .get(action_key)
                .map(|bindings| {
                    bindings
                        .iter()
                        .filter(|b| self.editing.as_ref() != Some(*b))
                        .map(|b| format!("[{b}]"))
                        .collect()
                })
                .unwrap_or_default();
            if !existing.is_empty() {
                ui.label(format!("existing: {}", existing.join(" ")));
            }
        }

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
        let binding = self.binding.trim();
        if binding.is_empty() {
            self.status = "enter a binding".to_owned();
            return;
        }
        let candidate = match ControllerBinding::from_str(binding) {
            Ok(candidate) => candidate,
            Err(err) => {
                self.status = err;
                return;
            }
        };
        let action = self.action.trim();
        if action.is_empty() {
            self.status = "enter an action".to_owned();
            return;
        }
        if get_action(self.mode, action).is_none() {
            self.status = format!("invalid action: {action}");
            return;
        }
        let _ = events.push(
            Event::ReturnState(ReturnStateResult::SelectKey {
                binding: candidate.to_string(),
                action: action.to_owned(),
            }),
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

    fn begin_empty(s: &mut SelectKeyState) {
        s.begin(
            String::new(),
            String::new(),
            StateId::Keyboard,
            HashMap::new(),
            None,
        );
    }

    #[test]
    fn holdover_reset_suppresses_ok_until_repress() {
        let mut s = SelectKeyState::new();
        begin_empty(&mut s);
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
        assert_eq!(s.status, "enter a binding");
    }

    #[test]
    fn submit_rejects_unparseable_binding() {
        let mut s = SelectKeyState::new();
        s.begin(
            "not-a-button".to_owned(),
            "toggleShift".to_owned(),
            StateId::Keyboard,
            HashMap::new(),
            None,
        );
        let mut events = EventQueue::passthrough();
        s.submit_ok(&mut events, &EventSource::MouseClick);
        assert!(!s.status.is_empty());
        assert!(events.drain_pending().is_empty());
    }

    #[test]
    fn submit_rejects_empty_action() {
        let mut s = SelectKeyState::new();
        s.begin(
            "faceTop".to_owned(),
            "   ".to_owned(),
            StateId::Keyboard,
            HashMap::new(),
            None,
        );
        let mut events = EventQueue::passthrough();
        s.submit_ok(&mut events, &EventSource::MouseClick);
        assert_eq!(s.status, "enter an action");
        assert!(events.drain_pending().is_empty());
    }

    #[test]
    fn submit_rejects_bare_send_key_action() {
        let mut s = SelectKeyState::new();
        s.begin(
            "faceTop".to_owned(),
            "sendKey.".to_owned(),
            StateId::Keyboard,
            HashMap::new(),
            None,
        );
        let mut events = EventQueue::passthrough();
        s.submit_ok(&mut events, &EventSource::MouseClick);
        assert_eq!(s.status, "invalid action: sendKey.");
        assert!(events.drain_pending().is_empty());
    }

    #[test]
    fn submit_ok_returns_canonical_binding_and_trimmed_action() {
        let mut s = SelectKeyState::new();
        s.begin(
            " options + faceTop ".to_owned(),
            " toggleShift ".to_owned(),
            StateId::Keyboard,
            HashMap::new(),
            None,
        );
        let mut events = EventQueue::passthrough();
        s.submit_ok(&mut events, &EventSource::MouseClick);
        let drained = events.drain_pending();
        assert_eq!(drained.len(), 1);
        assert_eq!(
            drained[0].0,
            Event::ReturnState(ReturnStateResult::SelectKey {
                binding: "options + faceTop".to_owned(),
                action: "toggleShift".to_owned(),
            })
        );
    }
}

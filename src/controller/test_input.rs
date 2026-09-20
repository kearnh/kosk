//! Shared `ControllerInput` fakes for unit tests.

use std::collections::HashSet;

use crate::controller::{ControllerButton, ControllerInput, ControllerKind};

#[derive(Debug, Clone)]
pub struct ButtonSetInput(pub HashSet<ControllerButton>);

impl ControllerInput for ButtonSetInput {
    fn left_stick_raw(&self) -> (f32, f32) {
        (0.0, 0.0)
    }
    fn right_stick_raw(&self) -> (f32, f32) {
        (0.0, 0.0)
    }
    fn trigger_left(&self) -> Option<u8> {
        None
    }
    fn trigger_right(&self) -> Option<u8> {
        None
    }
    fn query(&self, button: ControllerButton) -> bool {
        self.0.contains(&button)
    }
    fn is_engaged(&self) -> bool {
        !self.0.is_empty()
    }
    fn family(&self) -> ControllerKind {
        ControllerKind::Sc2
    }
    fn box_clone(&self) -> Box<dyn ControllerInput + Send + Sync> {
        Box::new(self.clone())
    }
}

#[derive(Debug, Clone)]
pub struct AnalogInput {
    pub stick: (f32, f32),
    pub buttons: HashSet<ControllerButton>,
}

impl ControllerInput for AnalogInput {
    fn left_stick_raw(&self) -> (f32, f32) {
        self.stick
    }
    fn right_stick_raw(&self) -> (f32, f32) {
        (0.0, 0.0)
    }
    fn trigger_left(&self) -> Option<u8> {
        None
    }
    fn trigger_right(&self) -> Option<u8> {
        None
    }
    fn query(&self, button: ControllerButton) -> bool {
        self.buttons.contains(&button)
    }
    fn is_engaged(&self) -> bool {
        true
    }
    fn family(&self) -> ControllerKind {
        ControllerKind::Sc2
    }
    fn box_clone(&self) -> Box<dyn ControllerInput + Send + Sync> {
        Box::new(self.clone())
    }
}

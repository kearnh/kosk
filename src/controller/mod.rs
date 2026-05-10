use crate::config;

pub mod ps4;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Dpad {
    Up,
    Down,
    Left,
    Right,
    UpRight,
    UpLeft,
    DownRight,
    DownLeft,
}

impl std::fmt::Display for Dpad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Dpad::Up => "↑",
            Dpad::UpRight => "↗",
            Dpad::Right => "→",
            Dpad::DownRight => "↘",
            Dpad::Down => "↓",
            Dpad::DownLeft => "↙",
            Dpad::Left => "←",
            Dpad::UpLeft => "↖",
        };
        write!(f, "{}", s)
    }
}

fn warp((mut x, mut y): (f32, f32), warp: f32) -> (f32, f32) {
    if warp > 0.0 {
        let u2 = x * x;
        let v2 = y * y;
        let offset = (u2 + v2).sqrt();
        if offset > 0.001 {
            // Determine how much to scale based on the warp factor
            // At warp=1.0, this pushes the circle out to fill the square corners.
            let scale = (offset / (x.abs().max(y.abs()))).powf(warp);
            x *= scale;
            y *= scale;
        }
    }

    // Clip to square bounds
    x = x.clamp(-1.0, 1.0);
    y = y.clamp(-1.0, 1.0);

    (x, y)
}

#[allow(unused)]
pub trait ControllerInput {
    fn left_stick_raw(&self) -> (f32, f32);
    fn right_stick_raw(&self) -> (f32, f32);

    // FIXME can we cache stick_warp somehow so we don't have to constantly lock config mutex?
    fn left_stick(&self) -> (f32, f32) {
        warp(self.left_stick_raw(), config::get().stick_warp)
    }
    fn right_stick(&self) -> (f32, f32) {
        warp(self.right_stick_raw(), config::get().stick_warp)
    }

    fn dpad(&self) -> Option<Dpad>;
    fn face_bottom(&self) -> bool;
    fn face_right(&self) -> bool;
    fn face_top(&self) -> bool;
    fn face_left(&self) -> bool;
    fn shoulder_left(&self) -> bool;
    fn shoulder_right(&self) -> bool;
    fn stick_left(&self) -> bool;
    fn stick_right(&self) -> bool;
    fn trigger_left(&self) -> Option<u8>;
    fn trigger_right(&self) -> Option<u8>;
    fn btn_options(&self) -> bool;
    fn btn_share(&self) -> bool;
    fn btn_system(&self) -> bool;

    fn box_clone(&self) -> Box<dyn ControllerInput + Send + Sync>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ControllerButton {
    Dpad(Dpad),
    FaceBottom,
    FaceRight,
    FaceLeft,
    FaceTop,
    ShoulderLeft,
    ShoulderRight,
    StickLeft,
    StickRight,
    TriggerLeft { threshold: u8 },
    TriggerRight { threshold: u8 },
    Options,
    Share,
    System,
}

impl ControllerButton {
    pub(crate) fn query(&self, input: &dyn ControllerInput) -> bool {
        match self {
            ControllerButton::Dpad(dpad) => input.dpad().map(|d| d == *dpad).unwrap_or(false),
            ControllerButton::FaceBottom => input.face_bottom(),
            ControllerButton::FaceRight => input.face_right(),
            ControllerButton::FaceLeft => input.face_left(),
            ControllerButton::FaceTop => input.face_top(),
            ControllerButton::ShoulderLeft => input.shoulder_left(),
            ControllerButton::ShoulderRight => input.shoulder_right(),
            ControllerButton::StickLeft => input.stick_left(),
            ControllerButton::StickRight => input.stick_right(),
            ControllerButton::TriggerLeft { threshold } => input
                .trigger_left()
                .map(|t| t >= *threshold)
                .unwrap_or(false),
            ControllerButton::TriggerRight { threshold } => input
                .trigger_right()
                .map(|t| t >= *threshold)
                .unwrap_or(false),
            ControllerButton::Options => input.btn_options(),
            ControllerButton::Share => input.btn_share(),
            ControllerButton::System => input.btn_system(),
        }
    }
}

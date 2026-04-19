pub mod ps4;

#[derive(Clone, Debug, PartialEq, Eq)]
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

impl ToString for Dpad {
    fn to_string(&self) -> String {
        match self {
            Dpad::Up => "↑",
            Dpad::UpRight => "↗",
            Dpad::Right => "→",
            Dpad::DownRight => "↘",
            Dpad::Down => "↓",
            Dpad::DownLeft => "↙",
            Dpad::Left => "←",
            Dpad::UpLeft => "↖",
        }
        .to_string()
    }
}

#[allow(unused)]
pub trait ControllerInput {
    fn left_stick(&self) -> (i32, i32);
    fn right_stick(&self) -> (i32, i32);
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

#[allow(unused)]
enum ControllerButton {
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
    #[allow(unused)]
    fn query(&self, input: Box<dyn ControllerInput>) -> bool {
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

#[allow(unused)]
pub struct ControllerMap {
    input: Box<dyn ControllerInput>,
}

impl ControllerMap {}

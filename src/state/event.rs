use crate::state::StateId;

pub enum Event {
    SendKey(enigo::Key, enigo::Direction),
    SendText(String),
    ChangeState(StateId),
    Exit,
}

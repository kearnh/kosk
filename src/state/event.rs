pub enum Event {
    SendKey(enigo::Key, enigo::Direction),
    SendText(String),
}

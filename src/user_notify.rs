//! Messages meant for the person using kosk.
//!
//! This is the single channel for those messages. It prints to stderr today.
//! A later change should also show them in the UI. Call this instead of
//! printing such a message directly, so that change can find every one.

/// Tell the user something. Stderr for now; the UI will show these later.
pub(crate) fn notify_user(message: &str) {
    eprintln!("{message}");
}

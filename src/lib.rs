//! Library surface for the kosk app, tests, and auxiliary binaries
//! (e.g. `completion_dev`, `sc2_test`).
pub mod completion;
pub mod config;
mod config_overlay;
pub mod controller;
pub mod debug;
pub mod platform;
pub mod state;
pub(crate) mod ui;
pub use ui::controller_glyph;
mod user_notify;
pub mod when;

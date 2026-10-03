//! OS-specific helpers. Completion talks to these through traits, not Win32.

pub mod foreground;
pub mod open_file;
pub(crate) mod password_field;

pub use foreground::{FixedForeground, ForegroundExe, OsForeground};
pub use open_file::{foreground_window_handle, open_in_default_app};

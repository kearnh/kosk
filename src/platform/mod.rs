//! OS-specific helpers. Completion talks to these through traits, not Win32.

pub mod foreground;

pub use foreground::{FixedForeground, ForegroundExe, OsForeground};

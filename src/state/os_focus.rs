//! Temporary OS foreground focus for text entry.
//!
//! The overlay runs with `WS_EX_NOACTIVATE` so it never steals focus. While a
//! text-entry mode is active, `main` drops that style and nudges the window to
//! the foreground so the OS delivers keyboard input to egui `TextEdit`s. The
//! previous foreground window is restored best-effort when the mode is left.

use crate::state::StateId;

/// Modes that need OS keyboard focus for `TextEdit` input.
pub fn text_entry_focus_wanted(current: StateId) -> bool {
    current == StateId::SelectKey
}

#[cfg(target_os = "windows")]
mod imp {
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SetActiveWindow, SetFocus};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
    };
    /// Restores the foreground window that was active when text entry began.
    pub struct OsFocusGuard {
        previous: isize,
    }

    impl OsFocusGuard {
        pub fn activate_for_text_entry(kosk_hwnd: isize) -> Self {
            let previous = unsafe { GetForegroundWindow() };
            if previous != kosk_hwnd {
                // SetForegroundWindow is foreground-locked for background
                // processes; attaching to the foreground thread's input queue
                // temporarily grants us the right.
                unsafe {
                    let fg_thread = GetWindowThreadProcessId(previous, std::ptr::null_mut());
                    let cur_thread = GetCurrentThreadId();
                    let attached = fg_thread != 0
                        && fg_thread != cur_thread
                        && AttachThreadInput(cur_thread, fg_thread, 1) != 0;
                    let _ = BringWindowToTop(kosk_hwnd);
                    let _ = SetForegroundWindow(kosk_hwnd);
                    SetActiveWindow(kosk_hwnd);
                    SetFocus(kosk_hwnd);
                    if attached {
                        AttachThreadInput(cur_thread, fg_thread, 0);
                    }
                }
            }
            Self { previous }
        }

        /// Best-effort restore; no-op if the old window is gone.
        pub fn restore(self) {
            if self.previous != 0 {
                unsafe {
                    let _ = SetForegroundWindow(self.previous);
                }
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    pub struct OsFocusGuard;

    impl OsFocusGuard {
        pub fn activate_for_text_entry(_kosk_hwnd: isize) -> Self {
            Self
        }

        pub fn restore(self) {}
    }
}

pub use imp::OsFocusGuard;

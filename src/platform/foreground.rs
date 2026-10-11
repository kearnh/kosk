//! Foreground process basename for app-type completion.

pub trait ForegroundExe: Send + Sync {
    fn basename(&self) -> Option<String>;
}

/// Live Windows query. Other targets always return `None` (catch-all type).
pub struct OsForeground;

impl ForegroundExe for OsForeground {
    fn basename(&self) -> Option<String> {
        foreground_exe_basename()
    }
}

/// Tests and `completion_dev --exe`.
pub struct FixedForeground(pub Option<String>);

impl ForegroundExe for FixedForeground {
    fn basename(&self) -> Option<String> {
        self.0.clone()
    }
}

#[derive(Default)]
struct ForegroundSnapshot {
    window: Option<isize>,
    basename: Option<String>,
}

impl ForegroundSnapshot {
    fn update(&mut self, window: Option<isize>, foreground: &dyn ForegroundExe) -> Option<String> {
        if self.window == window {
            return self.basename.clone();
        }
        self.window = window;
        self.basename = window.and_then(|_| foreground.basename());
        self.basename.clone()
    }
}

/// Cache the executable query until foreground focus changes.
pub(crate) fn current_basename() -> Option<String> {
    use std::sync::{Mutex, OnceLock};

    static SNAPSHOT: OnceLock<Mutex<ForegroundSnapshot>> = OnceLock::new();
    SNAPSHOT
        .get_or_init(|| Mutex::new(ForegroundSnapshot::default()))
        .lock()
        .unwrap()
        .update(super::foreground_window_handle(), &OsForeground)
}

fn foreground_exe_basename() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        windows_basename()
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

#[cfg(target_os = "windows")]
fn windows_basename() -> Option<String> {
    use std::path::Path;

    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const IMAGE_NAME_CAP: u32 = 1024;

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut buf = [0u16; IMAGE_NAME_CAP as usize];
        let mut size = IMAGE_NAME_CAP;
        let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size);
        CloseHandle(handle);
        if ok == 0 || size == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..size as usize]);
        Path::new(&path)
            .file_name()
            .and_then(|s| s.to_str())
            .map(str::to_string)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_refreshes_on_focus_change_and_clears_without_focus() {
        let mut snapshot = ForegroundSnapshot::default();
        let factorio = FixedForeground(Some("factorio.exe".into()));
        let editor = FixedForeground(Some("notepad.exe".into()));
        assert_eq!(
            snapshot.update(Some(1), &factorio).as_deref(),
            Some("factorio.exe")
        );
        assert_eq!(
            snapshot.update(Some(1), &editor).as_deref(),
            Some("factorio.exe")
        );
        assert_eq!(
            snapshot.update(Some(2), &editor).as_deref(),
            Some("notepad.exe")
        );
        assert_eq!(snapshot.update(None, &factorio), None);
        assert_eq!(snapshot.update(Some(3), &FixedForeground(None)), None);
    }
}

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
        if hwnd == 0 {
            return None;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle == 0 {
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

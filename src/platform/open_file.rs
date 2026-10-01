//! Open a file in its default application.

use std::path::Path;

use anyhow::Result;

/// Open `path` the way Explorer would on double-click.
pub fn open_in_default_app(path: &Path) -> Result<()> {
    open_path(path)
}

#[cfg(target_os = "windows")]
fn open_path(path: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWDEFAULT;

    const SHELL_EXECUTE_MAX_ERROR_CODE: isize = 32;

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    let operation = wide(std::ffi::OsStr::new("open"));
    let file = wide(path.as_os_str());
    unsafe {
        let result = ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWDEFAULT,
        ) as isize;
        if result <= SHELL_EXECUTE_MAX_ERROR_CODE {
            anyhow::bail!("ShellExecuteW failed with code {}", result);
        }
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn open_path(path: &Path) -> Result<()> {
    anyhow::bail!("opening {} is not supported here", path.display())
}

/// Foreground window handle. `None` when it cannot be read (tests, headless).
pub fn foreground_window_handle() -> Option<isize> {
    foreground_handle()
}

#[cfg(target_os = "windows")]
fn foreground_handle() -> Option<isize> {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            None
        } else {
            Some(hwnd as isize)
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn foreground_handle() -> Option<isize> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_fails_to_open() {
        assert!(open_path(Path::new("C:\\nonexistent-kosk-dir-9f3a\\missing.html")).is_err());
    }
}

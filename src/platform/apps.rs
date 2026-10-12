pub(crate) fn running_apps() -> Vec<(String, String)> {
    let mut apps = enumerate();
    apps.sort_by_key(|(_, executable)| executable.to_ascii_lowercase());
    apps.dedup_by(|a, b| a.1.eq_ignore_ascii_case(&b.1));
    apps
}

#[cfg(not(target_os = "windows"))]
fn enumerate() -> Vec<(String, String)> {
    Vec::new()
}

#[cfg(target_os = "windows")]
fn enumerate() -> Vec<(String, String)> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HWND, LPARAM},
        System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
        },
        UI::WindowsAndMessaging::{
            EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
        },
    };
    const WINDOW_TITLE_CAPACITY: usize = 1024;
    const EXECUTABLE_PATH_CAPACITY: usize = 32768;

    unsafe extern "system" fn visit(window: HWND, context: LPARAM) -> i32 {
        unsafe {
            if IsWindowVisible(window) == 0 {
                return 1;
            }
            let mut process = 0;
            GetWindowThreadProcessId(window, &mut process);
            if process == std::process::id() {
                return 1;
            }
            let mut title = [0u16; WINDOW_TITLE_CAPACITY];
            let length = GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32);
            if length == 0 {
                return 1;
            }
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process);
            if handle.is_null() {
                return 1;
            }
            let mut path = vec![0u16; EXECUTABLE_PATH_CAPACITY];
            let mut size = path.len() as u32;
            let success = QueryFullProcessImageNameW(handle, 0, path.as_mut_ptr(), &mut size);
            CloseHandle(handle);
            if success == 0 {
                return 1;
            }
            let path = String::from_utf16_lossy(&path[..size as usize]);
            let Some(executable) = std::path::Path::new(&path)
                .file_name()
                .and_then(|name| name.to_str())
            else {
                return 1;
            };
            let title = String::from_utf16_lossy(&title[..length as usize]);
            let apps = &mut *(context as *mut Vec<(String, String)>);
            apps.push((format!("{executable} — {title}"), executable.into()));
            1
        }
    }
    let mut apps = Vec::new();
    unsafe {
        EnumWindows(Some(visit), &mut apps as *mut _ as LPARAM);
    }
    apps
}

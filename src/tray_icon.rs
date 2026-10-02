use anyhow::{bail, Result};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIcon, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon,
    DestroyMenu, DestroyWindow, GetCursorPos, GetForegroundWindow, PostMessageW, RegisterClassW,
    RegisterWindowMessageW, SetForegroundWindow, TrackPopupMenu, HICON, MF_STRING, MSG,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_CANCELMODE, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP,
    WNDCLASSW, WS_EX_TOOLWINDOW,
};

const TRAY_CALLBACK: u32 = WM_APP + 1;
const TRAY_EVENT: u32 = WM_APP + 2;
const RESTORE_TRAY_EVENT: u32 = WM_APP + 3;
const TRAY_ID: u32 = 1;
const TOGGLE_COMMAND: usize = 1;
const QUIT_COMMAND: usize = 2;
const ICON_SIZE: usize = 32;
const BLUE: [u8; 3] = [240, 130, 40]; // BGRA order.

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn taskbar_created_message() -> u32 {
    static MESSAGE: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *MESSAGE.get_or_init(|| unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) })
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let event = if message == TRAY_CALLBACK {
        Some(TRAY_EVENT)
    } else if message != 0 && message == taskbar_created_message() {
        Some(RESTORE_TRAY_EVENT)
    } else {
        None
    };
    if let Some(event) = event {
        unsafe { PostMessageW(hwnd, event, wparam, lparam) };
        return 0;
    }

    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn blue_k_icon() -> Result<HICON> {
    let mut pixels = [0u8; ICON_SIZE * ICON_SIZE * 4];
    let mut mask = [0xffu8; ICON_SIZE * ICON_SIZE / 8];
    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let stem = (6..12).contains(&x) && (4..28).contains(&y);
            let diagonal = (4..28).contains(&y)
                && (11..27).contains(&x)
                && (x as i32 - 11 - (y as i32 - 15).abs()).abs() <= 3;
            if !stem && !diagonal {
                continue;
            }

            let index = (y * ICON_SIZE + x) * 4;
            pixels[index..index + 3].copy_from_slice(&BLUE);
            pixels[index + 3] = u8::MAX;
            mask[y * ICON_SIZE / 8 + x / 8] &= !(0x80 >> (x % 8));
        }
    }
    let icon = unsafe {
        CreateIcon(
            GetModuleHandleW(std::ptr::null()),
            ICON_SIZE as i32,
            ICON_SIZE as i32,
            1,
            32,
            mask.as_ptr(),
            pixels.as_ptr(),
        )
    };
    if icon.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(icon)
}

pub(super) enum TrayAction {
    Toggle,
    Quit,
}

pub(super) struct MenuDismissal(isize);

impl MenuDismissal {
    pub(super) fn dismiss(&self) {
        unsafe {
            PostMessageW(self.0 as HWND, WM_CANCELMODE, 0, 0);
        }
    }
}

pub(super) struct TrayIcon {
    hwnd: HWND,
    icon: HICON,
}

impl TrayIcon {
    pub(super) fn menu_dismissal(&self) -> MenuDismissal {
        MenuDismissal(self.hwnd as isize)
    }

    pub(super) fn new() -> Result<Self> {
        let class = wide("KOSK tray");
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let window_class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        if unsafe { RegisterClassW(&window_class) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class.as_ptr(),
                class.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            )
        };
        if hwnd.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let icon = match blue_k_icon() {
            Ok(icon) => icon,
            Err(error) => {
                unsafe {
                    DestroyWindow(hwnd);
                }
                return Err(error);
            }
        };
        let tray = Self { hwnd, icon };
        tray.add()?;
        Ok(tray)
    }

    fn data(&self) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: TRAY_ID,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: TRAY_CALLBACK,
            hIcon: self.icon,
            ..Default::default()
        };
        for (dest, value) in data.szTip.iter_mut().zip(wide("KOSK")) {
            *dest = value;
        }
        data
    }

    fn add(&self) -> Result<()> {
        if unsafe { Shell_NotifyIconW(NIM_ADD, &self.data()) } == 0 {
            bail!("Windows could not add KOSK to the notification area");
        }
        Ok(())
    }

    pub(super) fn handle_message(&self, message: &MSG, visible: bool) -> Option<TrayAction> {
        if message.hwnd != self.hwnd {
            return None;
        }
        if message.message == RESTORE_TRAY_EVENT {
            if let Err(error) = self.add() {
                eprintln!("restore tray icon: {error:#}");
            }
            return None;
        }
        if message.message != TRAY_EVENT {
            return None;
        }
        match message.lParam as u32 {
            WM_LBUTTONUP => Some(TrayAction::Toggle),
            WM_RBUTTONUP => self.menu(visible),
            _ => None,
        }
    }

    fn menu(&self, visible: bool) -> Option<TrayAction> {
        unsafe {
            let menu = CreatePopupMenu();
            if menu.is_null() {
                return None;
            }
            AppendMenuW(
                menu,
                MF_STRING,
                TOGGLE_COMMAND,
                wide(if visible { "Hide KOSK" } else { "Show KOSK" }).as_ptr(),
            );
            AppendMenuW(menu, MF_STRING, QUIT_COMMAND, wide("Quit KOSK").as_ptr());
            let mut position = Default::default();
            GetCursorPos(&mut position);
            let previous = GetForegroundWindow();
            SetForegroundWindow(self.hwnd);
            let command = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                position.x,
                position.y,
                0,
                self.hwnd,
                std::ptr::null(),
            );
            PostMessageW(self.hwnd, WM_NULL, 0, 0);
            DestroyMenu(menu);
            if GetForegroundWindow() == self.hwnd {
                SetForegroundWindow(previous);
            }
            match command as usize {
                TOGGLE_COMMAND => Some(TrayAction::Toggle),
                QUIT_COMMAND => Some(TrayAction::Quit),
                _ => None,
            }
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &self.data());
            DestroyIcon(self.icon);
            DestroyWindow(self.hwnd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{PeekMessageW, SendMessageW, PM_REMOVE};

    #[test]
    fn sent_shell_messages_reach_the_event_queue() {
        let class = wide("KOSK tray callback test");
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let window_class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        unsafe { RegisterClassW(&window_class) };
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class.as_ptr(),
                class.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            )
        };
        assert!(!hwnd.is_null());

        let taskbar_created = unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) };
        let mut forwarded = Vec::new();
        for (message, expected, event) in [
            (TRAY_CALLBACK, TRAY_EVENT, WM_RBUTTONUP as LPARAM),
            (taskbar_created, RESTORE_TRAY_EVENT, 0),
        ] {
            unsafe { SendMessageW(hwnd, message, TRAY_ID as WPARAM, event) };
            let mut queued = MSG::default();
            let received =
                unsafe { PeekMessageW(&mut queued, hwnd, expected, expected, PM_REMOVE) };
            forwarded.push((received, queued.wParam, queued.lParam));
        }
        unsafe { DestroyWindow(hwnd) };

        assert_eq!(
            forwarded,
            vec![
                (1, TRAY_ID as WPARAM, WM_RBUTTONUP as LPARAM),
                (1, TRAY_ID as WPARAM, 0)
            ]
        );
    }
}

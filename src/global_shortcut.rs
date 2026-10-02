use anyhow::{bail, Result};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
    VK_F1, VK_F24,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, MSG, PM_REMOVE, WM_HOTKEY,
};

#[path = "tray_icon.rs"]
mod tray_icon;

const SHORTCUT_ID: i32 = 1;
const MESSAGE_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shortcut {
    modifiers: u32,
    key: u32,
}

impl Shortcut {
    fn parse(value: &str) -> Result<Option<Self>> {
        if value.trim().is_empty() {
            return Ok(None);
        }

        let mut parts = value.split('+').map(str::trim).peekable();
        let mut modifiers = MOD_NOREPEAT;
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                let number = part
                    .to_ascii_uppercase()
                    .strip_prefix('F')
                    .and_then(|number| number.parse::<u32>().ok());
                let Some(number) =
                    number.filter(|number| (1..=u32::from(VK_F24 - VK_F1) + 1).contains(number))
                else {
                    bail!("Use F1-F24, optionally with Ctrl, Alt, Shift, or Win.");
                };
                return Ok(Some(Self {
                    modifiers,
                    key: u32::from(VK_F1) + number - 1,
                }));
            }

            let modifier = match part.to_ascii_lowercase().as_str() {
                "ctrl" => MOD_CONTROL,
                "alt" => MOD_ALT,
                "shift" => MOD_SHIFT,
                "win" => MOD_WIN,
                _ => bail!("Unknown shortcut modifier: {part}."),
            };
            if modifiers & modifier != 0 {
                bail!("Repeated shortcut modifier: {part}.");
            }
            modifiers |= modifier;
        }
        unreachable!()
    }
}

struct Registration;

impl Registration {
    fn new(shortcut: Shortcut) -> Result<Self> {
        if unsafe {
            RegisterHotKey(
                std::ptr::null_mut(),
                SHORTCUT_ID,
                shortcut.modifiers,
                shortcut.key,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Self)
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        unsafe {
            UnregisterHotKey(std::ptr::null_mut(), SHORTCUT_ID);
        }
    }
}

pub(super) struct GlobalShortcut {
    stopping: Arc<AtomicBool>,
    menu_dismissal: Arc<Mutex<Option<tray_icon::MenuDismissal>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl GlobalShortcut {
    pub(super) fn start(
        on_toggle: impl Fn() + Send + 'static,
        on_show: impl Fn() + Send + 'static,
        is_visible: impl Fn() -> bool + Send + 'static,
        on_quit: impl Fn() + Send + 'static,
    ) -> Result<Self> {
        let stopping = Arc::new(AtomicBool::new(false));
        let stop = stopping.clone();
        let menu_dismissal = Arc::new(Mutex::new(None));
        let menu = menu_dismissal.clone();
        let requested = Arc::new(Mutex::new(kosk::config::get().show_hide_shortcut));
        let updated = requested.clone();
        kosk::config::on_changed(move || {
            *updated.lock().unwrap() = kosk::config::get().show_hide_shortcut;
        })?;
        let thread = std::thread::Builder::new().name("show-hide-shortcut".into()).spawn(move || {
            let tray = match tray_icon::TrayIcon::new() {
                Ok(tray) => {
                    *menu.lock().unwrap() = Some(tray.menu_dismissal());
                    Some(tray)
                }
                Err(error) => {
                    kosk::user_notify::notify(kosk::user_notify::Notice {
                        key: kosk::user_notify::NoticeKey::TrayIcon,
                        severity: kosk::user_notify::Severity::Warning,
                        title: "Tray icon unavailable",
                        body: format!("Could not create the tray icon: {error}. Use the show/hide shortcut to reopen KOSK."),
                        action: None,
                        wide: false,
                        immediate: true,
                    });
                    None
                }
            };
            let mut configured = None;
            let mut registration = None;
            while !stop.load(Ordering::Relaxed) {
                let value = requested.lock().unwrap().clone();
                if configured.as_ref() != Some(&value) {
                    registration.take();
                    // Drop queued presses belonging to the previous shortcut.
                    let mut message = MSG::default();
                    while unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), WM_HOTKEY, WM_HOTKEY, PM_REMOVE) } != 0 {}
                    let result = Shortcut::parse(&value).and_then(|shortcut| shortcut.map(Registration::new).transpose());
                    match result {
                        Ok(active) => {
                            if active.is_none() && tray.is_none() { on_show(); }
                            registration = active;
                        }
                        Err(error) => {
                            on_show();
                            kosk::user_notify::notify(kosk::user_notify::Notice {
                                key: kosk::user_notify::NoticeKey::ShowHideShortcut,
                                severity: kosk::user_notify::Severity::Warning,
                                title: "Show/hide shortcut unavailable",
                                body: format!("Could not register {value:?}: {error}. KOSK stays visible. Change the show/hide shortcut in your settings."),
                                action: None,
                                wide: false,
                                immediate: true,
                            });
                        }
                    }
                    configured = Some(value);
                }

                let mut message = MSG::default();
                while unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
                    if registration.is_some() && message.wParam == SHORTCUT_ID as usize
                        && message.message == WM_HOTKEY {
                        on_toggle();
                    }
                    if let Some(tray) = &tray {
                        match tray.handle_message(&message, is_visible()) {
                            Some(tray_icon::TrayAction::Toggle) => on_toggle(),
                            Some(tray_icon::TrayAction::Quit) => on_quit(),
                            None => {}
                        }
                    }
                    unsafe { DispatchMessageW(&message); }
                }
                std::thread::sleep(MESSAGE_POLL_INTERVAL);
            }
        })?;
        Ok(Self {
            stopping,
            menu_dismissal,
            thread: Some(thread),
        })
    }
}

impl Drop for GlobalShortcut {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Relaxed);
        if let Some(menu) = self.menu_dismissal.lock().unwrap().as_ref() {
            menu.dismiss();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shortcuts_and_disabled_value() {
        assert_eq!(Shortcut::parse(" ").unwrap(), None);
        assert_eq!(
            Shortcut::parse("F3").unwrap(),
            Some(Shortcut {
                modifiers: MOD_NOREPEAT,
                key: u32::from(VK_F1) + 2
            })
        );
        assert_eq!(
            Shortcut::parse("ctrl + ALT + Shift + win + f24").unwrap(),
            Some(Shortcut {
                modifiers: MOD_NOREPEAT | MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_WIN,
                key: u32::from(VK_F24)
            })
        );
    }

    #[test]
    fn rejects_invalid_shortcuts() {
        for value in [
            "F0",
            "F25",
            "F3+",
            "Ctrl",
            "Ctrl+Ctrl+F3",
            "Meta+F3",
            "+F3",
            "F3+F4",
        ] {
            assert!(Shortcut::parse(value).is_err(), "{value}");
        }
    }
}

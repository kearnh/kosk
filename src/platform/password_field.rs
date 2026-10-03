use std::sync::{Arc, Mutex};
#[cfg(any(target_os = "windows", test))]
use std::time::Duration;
use std::time::Instant;

#[cfg(any(target_os = "windows", test))]
const FIELD_RESULT_LIFETIME: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldKind {
    Password,
    Ordinary,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FieldTarget {
    pub(crate) window: Option<isize>,
    pub(crate) element: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FieldObservation {
    pub(crate) kind: FieldKind,
    pub(crate) target: FieldTarget,
    pub(crate) generation: u64,
    pub(crate) observed_at: Instant,
}

impl FieldObservation {
    pub(crate) fn unknown() -> Self {
        Self {
            kind: FieldKind::Unknown,
            target: FieldTarget {
                window: None,
                element: 0,
            },
            generation: 0,
            observed_at: Instant::now(),
        }
    }

    #[cfg(any(target_os = "windows", test))]
    fn refreshed(mut self, window: Option<isize>, generation: u64) -> Self {
        if self.target.window != window {
            self.kind = FieldKind::Unknown;
            self.target = FieldTarget { window, element: 0 };
            self.generation = generation;
        } else if self.kind != FieldKind::Password
            && (self.generation != generation || self.observed_at.elapsed() > FIELD_RESULT_LIFETIME)
        {
            self.kind = FieldKind::Unknown;
            self.generation = generation;
        }
        self
    }
}

pub(crate) trait FieldDetector: Send + Sync {
    fn observation(&self) -> FieldObservation;
}

pub(crate) struct OsFieldDetector {
    observation: Arc<Mutex<FieldObservation>>,
    #[cfg(target_os = "windows")]
    worker: windows_detection::Worker,
}

impl OsFieldDetector {
    pub(crate) fn start(notify: Arc<dyn Fn() + Send + Sync>) -> Self {
        let observation = Arc::new(Mutex::new(FieldObservation::unknown()));
        #[cfg(target_os = "windows")]
        let worker = windows_detection::Worker::start(Arc::clone(&observation), notify);
        #[cfg(not(target_os = "windows"))]
        let _ = notify;
        Self {
            observation,
            #[cfg(target_os = "windows")]
            worker,
        }
    }
}

impl FieldDetector for OsFieldDetector {
    fn observation(&self) -> FieldObservation {
        let observation = *self.observation.lock().unwrap();
        #[cfg(target_os = "windows")]
        return observation.refreshed(super::foreground_window_handle(), self.worker.generation());
        #[cfg(not(target_os = "windows"))]
        observation
    }
}

#[cfg(test)]
pub(crate) struct FixedFieldDetector(pub(crate) Mutex<FieldObservation>);

#[cfg(test)]
impl FieldDetector for FixedFieldDetector {
    fn observation(&self) -> FieldObservation {
        *self.0.lock().unwrap()
    }
}

#[cfg(target_os = "windows")]
mod windows_detection {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::mpsc::{self, SyncSender};
    use std::thread::JoinHandle;
    use windows::core::{implement, Interface, Ref};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Accessibility::*;

    const FIELD_REFRESH_INTERVAL: Duration = Duration::from_millis(100);
    const PROVIDER_TIMEOUT_MILLISECONDS: u32 = 250;

    #[implement(
        IUIAutomationFocusChangedEventHandler,
        IUIAutomationPropertyChangedEventHandler
    )]
    struct Changed {
        generation: Arc<AtomicU64>,
        wake: SyncSender<()>,
        notify: Arc<dyn Fn() + Send + Sync>,
    }

    impl Changed {
        fn changed(&self) {
            self.generation.fetch_add(1, Ordering::AcqRel);
            let _ = self.wake.try_send(());
            (self.notify)();
        }
    }

    impl IUIAutomationFocusChangedEventHandler_Impl for Changed_Impl {
        fn HandleFocusChangedEvent(
            &self,
            _: Ref<IUIAutomationElement>,
        ) -> windows::core::Result<()> {
            self.changed();
            Ok(())
        }
    }

    impl IUIAutomationPropertyChangedEventHandler_Impl for Changed_Impl {
        fn HandlePropertyChangedEvent(
            &self,
            _: Ref<IUIAutomationElement>,
            _: UIA_PROPERTY_ID,
            _: &VARIANT,
        ) -> windows::core::Result<()> {
            self.changed();
            Ok(())
        }
    }

    pub(super) struct Worker {
        generation: Arc<AtomicU64>,
        stop: Arc<AtomicBool>,
        wake: SyncSender<()>,
        join: Option<JoinHandle<()>>,
    }

    impl Worker {
        pub(super) fn start(
            observation: Arc<Mutex<FieldObservation>>,
            notify: Arc<dyn Fn() + Send + Sync>,
        ) -> Self {
            let generation = Arc::new(AtomicU64::new(0));
            let stop = Arc::new(AtomicBool::new(false));
            let (wake, rx) = mpsc::sync_channel(1);
            let changed = Changed {
                generation: Arc::clone(&generation),
                wake: wake.clone(),
                notify: Arc::clone(&notify),
            };
            let stopped = Arc::clone(&stop);
            let counter = Arc::clone(&generation);
            let join = std::thread::Builder::new()
                .name("kosk-password-field".into())
                .spawn(move || {
                    if unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_err() {
                        return;
                    }
                    let _ = unsafe { run(observation, changed, counter, stopped, rx, notify) };
                    unsafe { CoUninitialize() };
                })
                .ok();
            Self {
                generation,
                stop,
                wake,
                join,
            }
        }

        pub(super) fn generation(&self) -> u64 {
            self.generation.load(Ordering::Acquire)
        }
    }

    impl Drop for Worker {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            let _ = self.wake.try_send(());
            // Provider calls run off the UI thread; shutdown must not wait for them.
            self.join.take();
        }
    }

    unsafe fn run(
        observation: Arc<Mutex<FieldObservation>>,
        changed: Changed,
        counter: Arc<AtomicU64>,
        stop: Arc<AtomicBool>,
        rx: mpsc::Receiver<()>,
        notify: Arc<dyn Fn() + Send + Sync>,
    ) -> windows::core::Result<()> {
        let automation: IUIAutomation2 =
            unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)? };
        unsafe {
            automation.SetConnectionTimeout(PROVIDER_TIMEOUT_MILLISECONDS)?;
            automation.SetTransactionTimeout(PROVIDER_TIMEOUT_MILLISECONDS)?;
        }
        let focus: IUIAutomationFocusChangedEventHandler = changed.into();
        let property: IUIAutomationPropertyChangedEventHandler = focus.cast()?;
        let _ = unsafe { automation.AddFocusChangedEventHandler(None, &focus) };
        let mut previous: Option<IUIAutomationElement> = None;
        let mut element_id = 0;
        while !stop.load(Ordering::Acquire) {
            let mut generation = counter.load(Ordering::Acquire);
            let window = super::super::foreground_window_handle();
            let element = unsafe { automation.GetFocusedElement() }.ok();
            let element = element.filter(|element| {
                let Some(window) = window else { return false };
                let mut process = 0;
                unsafe {
                    windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
                        window as _,
                        &mut process,
                    )
                };
                unsafe { element.CurrentProcessId() }.is_ok_and(|id| id as u32 == process)
            });
            let same = match (&previous, &element) {
                (Some(a), Some(b)) => unsafe { automation.CompareElements(a, b) }
                    .ok()
                    .map(|same| same.as_bool()),
                (_, None) => Some(true),
                _ => Some(false),
            };
            if same == Some(false) {
                generation = counter.fetch_add(1, Ordering::AcqRel) + 1;
                element_id += 1;
                if let Some(previous) = &previous {
                    let _ = unsafe {
                        automation.RemovePropertyChangedEventHandler(previous, &property)
                    };
                }
                if let Some(element) = &element {
                    let _ = unsafe {
                        automation.AddPropertyChangedEventHandlerNativeArray(
                            element,
                            TreeScope_Element,
                            None,
                            &property,
                            &[UIA_IsPasswordPropertyId],
                        )
                    };
                }
            }
            let kind = element
                .as_ref()
                .filter(|_| same.is_some())
                .and_then(|element| unsafe {
                    element
                        .GetCurrentPropertyValueEx(UIA_IsPasswordPropertyId, true)
                        .ok()
                })
                .and_then(|value| bool::try_from(&value).ok())
                .map_or(FieldKind::Unknown, |password| {
                    if password {
                        FieldKind::Password
                    } else {
                        FieldKind::Ordinary
                    }
                });
            if generation == counter.load(Ordering::Acquire)
                && window == super::super::foreground_window_handle()
            {
                let mut current = observation.lock().unwrap();
                let target = FieldTarget {
                    window,
                    element: element_id,
                };
                let kind = if kind == FieldKind::Unknown
                    && current.target == target
                    && current.kind == FieldKind::Password
                {
                    FieldKind::Password
                } else {
                    kind
                };
                let changed = current.target != target
                    || current.kind != kind
                    || current.generation != generation;
                *current = FieldObservation {
                    kind,
                    target,
                    generation,
                    observed_at: Instant::now(),
                };
                drop(current);
                if changed {
                    notify();
                }
            }
            if element.is_some() && same.is_some() {
                previous = element;
            }
            let _ = rx.recv_timeout(FIELD_REFRESH_INTERVAL);
        }
        let _ = unsafe { automation.RemoveAllEventHandlers() };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_ordinary_results_become_unknown_but_passwords_remain_protected() {
        let mut observation = FieldObservation::unknown();
        observation.target.window = Some(1);
        observation.kind = FieldKind::Ordinary;
        observation.observed_at -= FIELD_RESULT_LIFETIME + Duration::from_millis(1);
        assert_eq!(observation.refreshed(Some(1), 0).kind, FieldKind::Unknown);
        observation.kind = FieldKind::Password;
        assert_eq!(observation.refreshed(Some(1), 1).kind, FieldKind::Password);
        assert_eq!(observation.refreshed(Some(2), 1).kind, FieldKind::Unknown);
        observation.kind = FieldKind::Ordinary;
        observation.observed_at = Instant::now();
        assert_eq!(observation.refreshed(Some(1), 1).kind, FieldKind::Unknown);
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "requires an interactive Windows desktop; opens temporary test controls"]
    fn live_native_password_field_transitions() {
        let script = r#"
Add-Type -AssemblyName System.Windows.Forms
$form = New-Object System.Windows.Forms.Form
$form.Text = 'KOSK password detection test'
$form.TopMost = $true
$plain = New-Object System.Windows.Forms.TextBox
$plain.Name = 'ordinary'
$plain.Top = 20
$secret = New-Object System.Windows.Forms.TextBox
$secret.Name = 'password'
$secret.Top = 60
$secret.UseSystemPasswordChar = $true
$form.Controls.AddRange(@($plain, $secret))
$timer = New-Object System.Windows.Forms.Timer
$timer.Interval = 2000
$script:phase = 0
$timer.Add_Tick({
    $script:phase++
    switch ($script:phase) {
        1 { $secret.Focus() }
        2 { $secret.UseSystemPasswordChar = $false }
        3 { $plain.Focus() }
        4 { $form.Close() }
    }
})
$form.Add_Shown({ $form.Activate(); $plain.Focus(); $timer.Start() })
[System.Windows.Forms.Application]::Run($form)
$timer.Dispose()
$form.Dispose()
"#;
        run_live_field_fixture(script);
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "requires an interactive Windows desktop; opens a temporary browser control"]
    fn live_browser_password_field_transitions() {
        let script = r#"
Add-Type -AssemblyName System.Windows.Forms
$form = New-Object System.Windows.Forms.Form
$form.Text = 'KOSK browser password detection test'
$form.TopMost = $true
$browser = New-Object System.Windows.Forms.WebBrowser
$browser.Dock = 'Fill'
$browser.DocumentText = '<html><body><input id="ordinary"><input id="password" type="password"><input id="later"></body></html>'
$form.Controls.Add($browser)
$timer = New-Object System.Windows.Forms.Timer
$timer.Interval = 2000
$script:phase = 0
$timer.Add_Tick({
    $script:phase++
    switch ($script:phase) {
        1 { $browser.Document.GetElementById('password').Focus() }
        2 { $browser.Document.GetElementById('ordinary').Focus() }
        3 { $browser.Document.GetElementById('later').Focus() }
        4 { $form.Close() }
    }
})
$browser.Add_DocumentCompleted({ $form.Activate(); $browser.Document.GetElementById('ordinary').Focus(); $timer.Start() })
[System.Windows.Forms.Application]::Run($form)
$timer.Dispose()
$form.Dispose()
"#;
        run_live_field_fixture(script);
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "requires Microsoft Edge and an interactive Windows desktop; opens an isolated test window"]
    fn live_edge_password_field_transitions() {
        use std::process::Command;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetWindowThreadProcessId, PostMessageW, WM_CLOSE,
        };
        let fixture =
            std::env::temp_dir().join(format!("kosk-edge-field-{}.html", std::process::id()));
        let profile =
            std::env::temp_dir().join(format!("kosk-edge-field-profile-{}", std::process::id()));
        std::fs::write(
            &fixture,
            r#"<!doctype html><title>KOSK password field test</title>
<label>Ordinary<input id="ordinary" autofocus></label>
<label>Password<input id="password" type="password"></label>
<script>
setTimeout(() => document.getElementById('password').focus(), 2500);
setTimeout(() => document.getElementById('ordinary').focus(), 5000);
</script>"#,
        )
        .unwrap();
        let mut child =
            Command::new(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe")
                .args([
                    "--no-first-run",
                    "--no-default-browser-check",
                    "--disable-background-mode",
                    "--force-renderer-accessibility",
                ])
                .arg(format!("--user-data-dir={}", profile.display()))
                .arg(format!(
                    "--app=file:///{}",
                    fixture.to_string_lossy().replace('\\', "/")
                ))
                .spawn()
                .unwrap();
        let detector = OsFieldDetector::start(Arc::new(|| {}));
        let deadline = Instant::now() + Duration::from_secs(12);
        let mut ordinary = false;
        let mut password = false;
        let mut cleared = false;
        let mut window = None;
        while Instant::now() < deadline {
            let observation = detector.observation();
            if let Some(handle) = observation.target.window {
                let mut process = 0;
                unsafe { GetWindowThreadProcessId(handle as _, &mut process) };
                if process == child.id() {
                    window = Some(handle);
                    if observation.kind == FieldKind::Ordinary {
                        ordinary = true;
                        cleared |= password;
                    }
                    password |= observation.kind == FieldKind::Password;
                }
            }
            if cleared {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if let Some(handle) = window {
            unsafe { PostMessageW(handle as _, WM_CLOSE, 0, 0) };
        }
        let shutdown = Instant::now() + Duration::from_secs(3);
        while Instant::now() < shutdown && child.try_wait().unwrap().is_none() {
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_file(fixture);
        assert!(ordinary, "Edge ordinary field not detected");
        assert!(password, "Edge password field not detected");
        assert!(cleared, "Edge ordinary field remained protected");
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "requires an interactive Windows desktop; opens a temporary terminal"]
    fn live_terminal_remains_available_for_filtered_learning() {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        use windows_sys::Win32::System::Threading::CREATE_NEW_CONSOLE;
        let previous = crate::platform::foreground_window_handle();
        let mut child = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-Command",
                "Write-Host 'KOSK terminal field detection test'; Start-Sleep -Seconds 6",
            ])
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn()
            .unwrap();
        let detector = OsFieldDetector::start(Arc::new(|| {}));
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut observed = false;
        while Instant::now() < deadline {
            let observation = detector.observation();
            if observation.target.window.is_some() && observation.target.window != previous {
                assert_ne!(observation.kind, FieldKind::Password);
                observed = true;
            }
            if child.try_wait().unwrap().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
        assert!(observed, "terminal did not receive foreground focus");
    }

    #[cfg(target_os = "windows")]
    fn run_live_field_fixture(script: &str) {
        use std::process::Command;
        let mut child = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-STA",
                "-WindowStyle",
                "Hidden",
                "-Command",
                script,
            ])
            .spawn()
            .unwrap();
        let detector = OsFieldDetector::start(Arc::new(|| {}));
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut ordinary = false;
        let mut password = false;
        let mut cleared = false;
        let mut last_target = None;
        let mut moved = false;
        while Instant::now() < deadline {
            let observation = detector.observation();
            if observation.kind == FieldKind::Ordinary {
                ordinary = true;
                if password {
                    cleared = true;
                }
            }
            if observation.kind == FieldKind::Password {
                password = true;
                last_target = Some(observation.target);
            }
            if cleared && last_target.is_some_and(|target| target != observation.target) {
                moved = true;
            }
            if child.try_wait().unwrap().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
        assert!(ordinary, "ordinary field not detected");
        assert!(password, "password field not detected");
        assert!(cleared, "password property change not detected");
        assert!(moved, "same-window focus change not detected");
    }
}

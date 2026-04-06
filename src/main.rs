#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_state;
mod config;
mod keyboard;
mod ps4;

use std::sync::{Arc, Mutex};

use anyhow::Result;
use eframe::{wgpu::rwh::HasWindowHandle, CreationContext};
use hidapi::{HidApi, HidDevice};

use crate::app_state::AppState;

const PS4_VID: u16 = 0x054c;
const PS4_PID: u16 = 0x09cc;
fn find_device(hid: &HidApi, (vid, pid): (u16, u16)) -> Option<HidDevice> {
    for device in hid.device_list() {
        if device.vendor_id() == vid && device.product_id() == pid {
            if let Ok(dev) = device.open_device(hid) {
                return Some(dev);
            }
        }
    }
    None
}

struct App {
    state: Arc<Mutex<AppState>>,
    window_setup_done: bool,
}

impl App {
    fn new(cc: &CreationContext<'_>, state: Arc<Mutex<AppState>>) -> Self {
        // Configure fonts for Unicode support
        let mut fonts = egui::FontDefinitions::default();

        // On Windows, use Segoe UI Symbol and Emoji as fallbacks for unicode characters
        #[cfg(target_os = "windows")]
        {
            let mut font_added = false;
            if let Ok(font_data) = std::fs::read("C:\\Windows\\Fonts\\seguisym.ttf") {
                fonts.font_data.insert(
                    "SegoeUISymbol".to_owned(),
                    egui::FontData::from_owned(font_data).into(),
                );
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .push("SegoeUISymbol".to_owned());
                font_added = true;
            }
            if let Ok(font_data) = std::fs::read("C:\\Windows\\Fonts\\seguiemj.ttf") {
                fonts.font_data.insert(
                    "SegoeUIEmoji".to_owned(),
                    egui::FontData::from_owned(font_data).into(),
                );
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .push("SegoeUIEmoji".to_owned());
                font_added = true;
            }

            if font_added {
                cc.egui_ctx.set_fonts(fonts);
            }
        }

        // Resize viewport to fit state reported size
        let (width, height) = {
            let s = state.lock().unwrap();
            s.window_size()
        };

        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::Vec2::new(
                width + 10.0,
                height + 10.0,
            )));
        if let Some(size) = cc.egui_ctx.input(|i| i.viewport().monitor_size) {
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::OuterPosition(
                    (size.x - width, size.y - height).into(),
                ));
        }

        Self {
            state,
            window_setup_done: false,
        }
    }
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // Semi-transparent dark background so users know there's a window
        // RGBA: slightly dark with ~30% opacity
        [0.08, 0.08, 0.08, 0.3]
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals {
            window_fill: egui::Color32::TRANSPARENT,
            panel_fill: egui::Color32::from_rgba_premultiplied(20, 20, 20, 100), // Semi-transparent dark background
            ..Default::default()
        });

        // Setup window styles (non-transparent parts)
        if let Ok(h) = frame.window_handle() {
            use eframe::wgpu::rwh::RawWindowHandle::*;
            match h.as_raw() {
                Win32(h) => {
                    use windows::Win32::Foundation::{COLORREF, HWND};
                    use windows::Win32::Graphics::Dwm::{
                        DwmEnableBlurBehindWindow, DWM_BB_ENABLE, DWM_BLURBEHIND,
                    };
                    use windows::Win32::UI::WindowsAndMessaging::{
                        GetWindowLongPtrW, SetLayeredWindowAttributes, SetWindowLongPtrW,
                        GWL_EXSTYLE, LWA_ALPHA, WS_EX_LAYERED, WS_EX_NOACTIVATE,
                    };
                    let hwnd = HWND(h.hwnd.get() as _);
                    unsafe {
                        // Set WS_EX_NOACTIVATE and WS_EX_LAYERED every frame (can be reset by system)
                        let current_ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                        let new_ex_style = current_ex_style
                            | (WS_EX_NOACTIVATE.0 as isize)
                            | (WS_EX_LAYERED.0 as isize);

                        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_ex_style);

                        // Set layered window attributes for alpha transparency
                        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);

                        // Enable DWM blur behind for transparency (only once)
                        if !self.window_setup_done {
                            let bb = DWM_BLURBEHIND {
                                dwFlags: DWM_BB_ENABLE,
                                fEnable: true.into(),
                                hRgnBlur: Default::default(),
                                fTransitionOnMaximized: false.into(),
                            };
                            let _ = DwmEnableBlurBehindWindow(hwnd, &bb);
                            self.window_setup_done = true;
                        }
                    }
                }
                _ => {}
            }
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::same(5)))
            .show(ctx, |ui| {
                let mut s = self.state.lock().unwrap();
                s.draw_ui(ctx, ui);
            });
    }
}

fn main() -> Result<()> {
    config::init();

    // Box<dyn 'app + FnOnce(&CreationContext<'_>) -> Result<Box<dyn 'app + App>, DynError>>;
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_transparent(true)
            .with_active(false)
            .with_always_on_top()
            .with_decorations(false)
            .with_resizable(false)
            .with_inner_size([520.0, 250.0]), // Initial size, will be resized by App::new()
        renderer: eframe::Renderer::Glow,
        centered: true,
        ..Default::default()
    };

    // Spawn a thread to echo keyboard input from stdin
    #[cfg(debug_assertions)]
    std::thread::spawn(|| {
        use std::io::{self, BufRead};
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            if let Ok(text) = line {
                println!("[Echo] {}", text);
            }
        }
    });

    eframe::run_native(
        "KOSK",
        native_options,
        Box::new(|cc| {
            let ctx = cc.egui_ctx.clone();

            let state = {
                let monitor_size = ctx
                    .input(|i| i.viewport().monitor_size)
                    .ok_or(anyhow::anyhow!("could not get monitor size"))?;

                let state = AppState::new((monitor_size.x, monitor_size.y))?;

                Arc::new(Mutex::new(state))
            };

            let state_clone = state.clone();
            std::thread::spawn(move || -> Result<()> {
                loop {
                    let device = loop {
                        let hid = HidApi::new()?;
                        match find_device(&hid, (PS4_VID, PS4_PID)) {
                            Some(device) => break device,
                            None => {
                                std::thread::sleep(std::time::Duration::from_millis(500));
                                continue;
                            }
                        }
                    };

                    let ps4 = ps4::Ps4Device::new(device);
                    for input in ps4 {
                        let mut s = state_clone.lock().unwrap();
                        if let Err(e) = s.handle_controller_input(&ctx, &input) {
                            eprintln!("warn: error from controller input handler: {}", e);
                        };
                        ctx.request_repaint();
                    }
                }
            });
            Ok(Box::new(App::new(cc, state)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {}", e))?;

    Ok(())
}

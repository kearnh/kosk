// #![windows_subsystem = "windows"]

mod ps4;
mod qwerty_keyboard;

use std::sync::mpsc::Receiver;

use anyhow::{bail, Result};
use eframe::{wgpu::rwh::HasWindowHandle, CreationContext};
use enigo::{Enigo, Keyboard as _};
use hidapi::{HidApi, HidDevice};
use image::ImageReader;
use qwerty_keyboard::qwerty_keyboard;

use crate::qwerty_keyboard::{Highlight, Keyboard, RawKey};

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

enum StateUpdate {
    Highlight0(Option<Highlight>),
    Highlight1(Option<Highlight>),
}

struct App {
    kb: Keyboard,
    enigo: Enigo,
    window_setup_done: bool,
    rx: Receiver<StateUpdate>,
}

impl App {
    fn new(cc: &CreationContext<'_>, rx: Receiver<StateUpdate>) -> Self {
        let kb =
            Keyboard::with_layout_file("keyboard_layout.toml").unwrap_or_else(|_| Keyboard::new());

        // Calculate window size based on keyboard layout
        let (kb_width, kb_height) = kb.layout.calculate_size();
        // let margin = 10.0 * 2.0; // inner margin on both sides
        // let window_width = kb_width + margin;
        // let window_height = kb_height + margin;

        // Resize viewport to fit keyboard
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::Vec2::new(
                kb_width + 10.0,
                kb_height + 10.0,
            )));

        Self {
            kb,
            enigo: Enigo::new(&Default::default()).unwrap(),
            window_setup_done: false,
            rx,
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
        while let Ok(x) = self.rx.try_recv() {
            use StateUpdate::*;
            match x {
                Highlight0(h) => self.kb.highlight.0 = h,
                Highlight1(h) => self.kb.highlight.1 = h,
            }
        }

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
                if let Some(key) = qwerty_keyboard(ui, &self.kb) {
                    if key == RawKey::Shift {
                        self.kb.toggle_shift();
                    } else {
                        let _ = key.send(&mut self.enigo);
                        // Auto-disable shift after typing a character (like mobile keyboards)
                        if self.kb.shift_state {
                            self.kb.shift_state = false;
                        }
                    }
                }
            });
    }
}

fn main() -> Result<()> {
    println!(
        "Looking for PS4 controller (VID:{:04x}, PID:{:04x})...",
        PS4_VID, PS4_PID
    );

    // let keymap = ImageReader::open("map.png")?.decode()?;

    // for input in &ps4 {
    //     let (left_x, left_y, right_x, right_y) = input.get_sticks();
    //     println!("({}, {}) : ({}, {})", left_x, left_y, right_x, right_y);
    // }

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

    eframe::run_native(
        "KOSK",
        native_options,
        Box::new(|cc| {
            let ctx = cc.egui_ctx.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || -> Result<()> {
                let hid = HidApi::new()?;

                let device = match find_device(&hid, (PS4_VID, PS4_PID)) {
                    Some(device) => device,
                    None => bail!("PS4 controller not found."),
                };

                let ps4 = ps4::Ps4Device::new(device);

                for input in &ps4 {
                    ctx.request_repaint();
                }

                Ok(())
            });
            Ok(Box::new(App::new(cc, rx)))
        }),
    )
    .unwrap();

    Ok(())
}

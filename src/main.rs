// #![windows_subsystem = "windows"]

mod keyboard;
mod ps4;

use std::sync::{mpsc::Receiver, Arc, RwLock};

use anyhow::Result;
use clap::Parser;
use eframe::{wgpu::rwh::HasWindowHandle, CreationContext};
use enigo::Enigo;
use hidapi::{HidApi, HidDevice};
use keyboard::draw_ui;

use crate::{
    keyboard::{Keyboard, RawKey},
    ps4::Dpad,
};

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

#[derive(Parser, Debug)]
#[command(name = "kosk")]
#[command(about = "Keyboard On-Screen for Kontroller", long_about = None)]
struct Args {
    /// Path to keyboard layout TOML file
    #[arg(short, long)]
    layout: Option<String>,

    /// Sensitivity/range multiplier for the horizontal stick axis
    #[arg(long, default_value_t = 3.0)]
    stick_x: f32,

    /// Sensitivity/range multiplier for the vertical stick axis
    #[arg(long, default_value_t = 2.5)]
    stick_y: f32,

    /// Stick warp factor (0.0 = circle, 1.0 = square)
    #[arg(long, default_value_t = 1.0)]
    stick_warp: f32,

    /// Trigger threshold for key press (0-255)
    #[arg(long, default_value_t = 40)]
    trigger_threshold: u8,
}

enum AppMessage {
    SelectLeft(Option<RawKey>),
    SelectRight(Option<RawKey>),
    Unselect,
    Done,
}

struct App {
    kb: Arc<RwLock<Keyboard>>,
    enigo: Enigo,
    window_setup_done: bool,
    rx: Receiver<AppMessage>,
}

impl App {
    fn new(cc: &CreationContext<'_>, kb: Arc<RwLock<Keyboard>>, rx: Receiver<AppMessage>) -> Self {
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

        // Calculate window size based on keyboard layout
        let (kb_width, kb_height) = {
            let kb_lock = kb.read().unwrap();
            kb_lock.layout.get_dimensions()
        };

        // Resize viewport to fit keyboard
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::Vec2::new(
                kb_width + 10.0,
                kb_height + 10.0,
            )));
        if let Some(size) = cc.egui_ctx.input(|i| i.viewport().monitor_size) {
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::OuterPosition(
                    (size.x - kb_width, size.y - kb_height).into(),
                ));
        }

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
            use AppMessage::*;
            let mut kb = self.kb.write().unwrap();
            match x {
                SelectLeft(h) => kb.selected.0 = h,
                SelectRight(h) => kb.selected.1 = h,
                Unselect => kb.selected = (None, None),
                Done => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
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
                let kb_read = self.kb.read().unwrap();
                if let Some(key) = draw_ui(ui, &kb_read) {
                    drop(kb_read); // Release read lock before acquiring write lock
                    let mut kb = self.kb.write().unwrap();

                    if key == RawKey::Done {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }

                    if key == RawKey::Shift {
                        kb.toggle_shift();
                    } else {
                        let _ = kb.send_key(&mut self.enigo, &key);
                    }
                }
            });
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    println!(
        "Looking for PS4 controller (VID:{:04x}, PID:{:04x})...",
        PS4_VID, PS4_PID
    );

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

    let kb = {
        let k = if let Some(layout_path) = args.layout {
            Keyboard::with_layout_file(&layout_path, args.stick_x, args.stick_y, args.stick_warp)?
        } else {
            Keyboard::new(args.stick_x, args.stick_y, args.stick_warp)
        };
        Arc::new(RwLock::new(k))
    };

    // Spawn a thread to echo keyboard input from stdin
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
            let (tx, rx) = std::sync::mpsc::channel();
            let kb_clone = kb.clone();
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

                    let mut enigo = Enigo::new(&Default::default())?;

                    let ps4 = ps4::Ps4Device::new(device);
                    let mut l2_was_pressed = false;
                    let mut r2_was_pressed = false;
                    let trigger_threshold = args.trigger_threshold;

                    let mut pos = 0usize;

                    for input in ps4 {
                        let input = match input {
                            Some(input) => input,
                            None => {
                                // no input, reset selected
                                let _ = tx.send(AppMessage::Unselect);
                                continue;
                            }
                        };

                        let kb = kb_clone.read().unwrap();

                        let overlapping_left = kb.get_nearest_key_left(input.left);
                        let selected_left = overlapping_left.clone();

                        let overlapping_right = kb.get_nearest_key_right(input.right);
                        let selected_right = overlapping_right.clone();

                        drop(kb);

                        let _ = tx.send(AppMessage::SelectLeft(selected_left.clone()));
                        let _ = tx.send(AppMessage::SelectRight(selected_right.clone()));

                        let mut stick_press =
                            |input: &Option<u8>,
                             was_pressed: &mut bool,
                             kb: Arc<RwLock<Keyboard>>,
                             key: &Option<RawKey>| {
                                let val = input.unwrap_or(0);
                                let pressed = val > trigger_threshold;

                                if pressed && !*was_pressed {
                                    match key {
                                        Some(RawKey::Done) => {
                                            let _ = tx.send(AppMessage::Done);
                                        }
                                        Some(key) => {
                                            let mut kb_write = kb.write().unwrap();
                                            let _ = kb_write.send_key(&mut enigo, key);
                                        }
                                        _ => (),
                                    }
                                }
                                *was_pressed = pressed;
                            };
                        stick_press(
                            &input.l2,
                            &mut l2_was_pressed,
                            kb_clone.clone(),
                            &selected_left,
                        );
                        stick_press(
                            &input.r2,
                            &mut r2_was_pressed,
                            kb_clone.clone(),
                            &selected_right,
                        );

                        if input.cross {
                            let mut kb_write = kb_clone.write().unwrap();
                            kb_write.send_key(&mut enigo, &RawKey::Key(" ".to_string()))?;
                        }

                        if input.square {
                            let mut kb_write = kb_clone.write().unwrap();
                            kb_write.send_key(&mut enigo, &RawKey::Backspace)?;
                        }

                        if input.triangle {
                            let mut kb_write = kb_clone.write().unwrap();
                            kb_write.toggle_shift();
                        }

                        if input.l3 {
                            let mut kb_write = kb_clone.write().unwrap();
                            kb_write.toggle_ctrl();
                        }

                        if input.r3 {
                            let mut kb_write = kb_clone.write().unwrap();
                            kb_write.toggle_alt();
                        }

                        if matches!(input.dpad, Some(Dpad::Down)) {
                            let (kb_width, kb_height) = {
                                let kb_lock = kb_clone.read().unwrap();
                                kb_lock.layout.get_dimensions()
                            };
                            if let Some(size) = ctx.input(|i| i.viewport().monitor_size) {
                                let xy = [
                                    (0.0, 0.0),
                                    (size.x - kb_width, 0.0),
                                    (size.x - kb_width, size.y - kb_height),
                                    (0.0, size.y - kb_height),
                                ][pos];
                                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(
                                    xy.into(),
                                ));
                                pos += 1;
                                pos %= 4;
                            }
                        }

                        ctx.request_repaint();
                    }
                }
            });
            Ok(Box::new(App::new(cc, kb, rx)))
        }),
    )
    .unwrap();

    Ok(())
}

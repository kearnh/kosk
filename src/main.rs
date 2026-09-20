#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::Result;
use eframe::CreationContext;
use egui::Vec2;
use kosk::config;
use kosk::controller;
use kosk::debug;
use kosk::state::{
    os_focus::{text_entry_focus_wanted, OsFocusGuard},
    AppState, StateId,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::sync::{Arc, Mutex};

struct App {
    state: Arc<Mutex<AppState>>,
    window_setup_done: bool,
    size: Vec2,
    min_size: Vec2,
    /// Last applied outer top-left; kept across content-driven resizes.
    last_outer: Option<egui::Pos2>,
    /// Resolved overlay alpha for the current mode (updated each frame).
    current_opacity: f32,
    /// Held while a text-entry mode has OS foreground focus.
    os_focus_guard: Option<OsFocusGuard>,
}

impl App {
    fn new(cc: &CreationContext<'_>, state: Arc<Mutex<AppState>>) -> Self {
        // Configure fonts: Phosphor icons always; Segoe fallbacks on Windows when present.
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor_icons::add_fonts(&mut fonts);

        // On Windows, use Segoe UI Symbol and Emoji as fallbacks for unicode characters
        #[cfg(target_os = "windows")]
        {
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
            }
        }

        cc.egui_ctx.set_fonts(fonts);

        egui_extras::install_image_loaders(&cc.egui_ctx);

        debug::register(&cc.egui_ctx);

        Self {
            state,
            window_setup_done: false,
            size: Vec2::ZERO,
            min_size: Vec2::ZERO,
            last_outer: None,
            current_opacity: 1.0,
            os_focus_guard: None,
        }
    }
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.08, 0.08, 0.08, self.current_opacity]
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let cfg = config::get();
        let is_transparent = cfg.transparent;
        let state = self.state.lock().unwrap().current_state();
        {
            self.current_opacity = if !is_transparent {
                1.0
            } else {
                let raw = match state {
                    StateId::Keyboard | StateId::TextInput | StateId::MoveWindow => {
                        cfg.keyboard_opacity
                    }
                    StateId::Menu
                    | StateId::Mappings
                    | StateId::SelectKey
                    | StateId::SelectLayout => cfg.ui_opacity,
                };
                raw.clamp(0.0, 1.0)
            };
        }
        let opacity = self.current_opacity;

        ctx.set_visuals(egui::Visuals {
            window_fill: if is_transparent {
                egui::Color32::TRANSPARENT
            } else {
                egui::Color32::from_rgb(20, 20, 20)
            },
            panel_fill: if is_transparent {
                egui::Color32::from_rgba_unmultiplied(20, 20, 20, (opacity * 255.0).round() as u8)
            } else {
                egui::Color32::from_rgb(20, 20, 20)
            },
            ..Default::default()
        });

        // Setup window styles (non-transparent parts)
        if let Ok(h) = frame.window_handle() {
            if let RawWindowHandle::Win32(h) = h.as_raw() {
                use windows_sys::Win32::Foundation::{COLORREF, HWND};
                use windows_sys::Win32::Graphics::Dwm::{
                    DwmEnableBlurBehindWindow, DWM_BB_ENABLE, DWM_BLURBEHIND,
                };
                use windows_sys::Win32::UI::WindowsAndMessaging::{
                    GetWindowLongPtrW, SetLayeredWindowAttributes, SetWindowLongPtrW, GWL_EXSTYLE,
                    LWA_ALPHA, WS_EX_LAYERED, WS_EX_NOACTIVATE,
                };
                let hwnd = h.hwnd.get() as HWND;
                // Keep the overlay non-activating (can be reset by system), except while
                // a text-entry mode needs OS keyboard input.
                let wants_text_entry = text_entry_focus_wanted(state);
                unsafe {
                    let current_ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
                    let mut new_ex_style = if wants_text_entry {
                        current_ex_style & !(WS_EX_NOACTIVATE as isize)
                    } else {
                        current_ex_style | (WS_EX_NOACTIVATE as isize)
                    };

                    if is_transparent {
                        new_ex_style |= WS_EX_LAYERED as isize;
                    }

                    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_ex_style);

                    if is_transparent {
                        // Set layered window attributes for alpha transparency
                        let _ = SetLayeredWindowAttributes(hwnd, 0 as COLORREF, 255, LWA_ALPHA);
                    }

                    // Enable DWM blur behind for transparency (only once)
                    if !self.window_setup_done {
                        if is_transparent {
                            let bb = DWM_BLURBEHIND {
                                dwFlags: DWM_BB_ENABLE,
                                fEnable: 1,
                                hRgnBlur: 0,
                                fTransitionOnMaximized: 0,
                            };
                            let _ = DwmEnableBlurBehindWindow(hwnd, &bb);
                        }
                        self.window_setup_done = true;
                    }
                }
                if wants_text_entry {
                    if self.os_focus_guard.is_none() {
                        self.os_focus_guard = Some(OsFocusGuard::activate_for_text_entry(hwnd));
                    }
                } else if let Some(guard) = self.os_focus_guard.take() {
                    guard.restore();
                }
            }
        }

        // Measure first; OuterPosition is applied after size so content-driven
        // resizes can keep the previous top edge instead of re-resolving corners.
        let mut size = Vec2::ZERO;

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::same(5)))
            .show(ui, |ui| {
                let inner = egui::Frame::NONE.show(ui, |ui| {
                    let mut s = self.state.lock().unwrap();
                    s.draw_ui(&ctx, ui);
                });
                size = inner.response.rect.size() + [10.0, 10.0].into();
            });

        // FIXME min_size will change if layout changes?
        if self.min_size == Vec2::ZERO {
            self.min_size = size;
        }
        size = Vec2 {
            x: self.min_size.x.max(size.x),
            y: self.min_size.y.max(size.y),
        };

        let size_changed = size != self.size;
        if size_changed {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
            self.size = size;
        }

        let monitor_size = ctx.input(|i| {
            i.viewport()
                .monitor_size
                .unwrap_or_else(|| egui::Vec2::new(1920.0, 1080.0))
        });
        {
            let mut s = self.state.lock().unwrap();
            s.set_monitor_size((monitor_size.x, monitor_size.y));
        }

        let outer = if size_changed {
            let kept = self.last_outer.unwrap_or_else(|| {
                let mut s = self.state.lock().unwrap();
                let (x, y) = s.get_position(
                    egui::Rect::from_min_size(egui::Pos2::ZERO, size),
                    ctx.pixels_per_point(),
                );
                egui::Pos2::new(x, y)
            });
            let max_x = (monitor_size.x - size.x).max(0.0);
            let max_y = (monitor_size.y - size.y).max(0.0);
            egui::Pos2::new(kept.x.clamp(0.0, max_x), kept.y.clamp(0.0, max_y))
        } else {
            let mut s = self.state.lock().unwrap();
            let (x, y) = s.get_position(
                egui::Rect::from_min_size(egui::Pos2::ZERO, size),
                ctx.pixels_per_point(),
            );
            egui::Pos2::new(x, y)
        };
        self.last_outer = Some(outer);
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(outer));
    }
}

fn main() -> Result<()> {
    config::init()?;
    if config::mcp_controller_mode() {
        controller::control_server::spawn(config::mcp_controller_bind())?;
    }

    // Box<dyn 'app + FnOnce(&CreationContext<'_>) -> Result<Box<dyn 'app + App>, DynError>>;
    #[cfg(feature = "wgpu")]
    let renderer = eframe::Renderer::Wgpu;
    #[cfg(not(feature = "wgpu"))]
    let renderer = eframe::Renderer::Glow;

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_transparent(config::get().transparent)
            .with_active(false)
            .with_always_on_top()
            .with_decorations(false)
            .with_resizable(false)
            .with_inner_size([520.0, 250.0]), // Initial size, will be resized by App::new()
        renderer,
        centered: true,
        ..Default::default()
    };

    // Spawn a thread to echo keyboard input from stdin
    #[cfg(debug_assertions)]
    std::thread::spawn(|| {
        use std::io::{self, BufRead};
        let stdin = io::stdin();
        for text in stdin.lock().lines().map_while(Result::ok) {
            println!("[Echo] {}", text);
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

            config::on_changed(move || {
                ctx.request_repaint();
            })?;

            let ctx = cc.egui_ctx.clone();
            let state_clone = state.clone();
            std::thread::spawn(move || -> Result<()> {
                loop {
                    match controller::find_device() {
                        Some(device) => {
                            let is_replay = device.is_replay();
                            if let Some(header) = device.replay_header() {
                                let header = header.clone();
                                let mut s = state_clone.lock().unwrap();
                                if let Err(e) = s.apply_replay_header(&header) {
                                    eprintln!("replay: failed to install layouts: {e:#}");
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                    break;
                                }
                            }
                            for input in device {
                                let mut s = state_clone.lock().unwrap();
                                let result = match &input {
                                    None => {
                                        controller::record::session().tap_input(&input);
                                        s.reset_controller_input(&ctx)
                                    }
                                    Some(snap) if !snap.is_engaged() => {
                                        s.note_battery(snap.as_ref());
                                        Ok(())
                                    }
                                    Some(snap) => {
                                        controller::record::session().tap_input(&input);
                                        s.handle_controller_input(&ctx, snap.as_ref())
                                    }
                                };
                                if let Err(e) = result {
                                    eprintln!("warn: error from controller input handler: {}", e);
                                }
                                ctx.request_repaint();
                            }
                            if is_replay {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                break;
                            }
                        }
                        None => {
                            if config::preferred_is_replay() {
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(500));
                        }
                    }
                }
                Ok(())
            });
            Ok(Box::new(App::new(cc, state)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{:#}", e))?;

    Ok(())
}

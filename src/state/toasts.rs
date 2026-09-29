//! Toast presentation: satellite-window view, docking, and tip actions.
//!
//! The queue, timing, and dismiss edges live in [`crate::user_notify`]. This
//! module shapes the visible card, docks the satellite window beside the
//! keyboard, and owns notice side effects (unsupported-button scan, tip flag,
//! setup guide).

use std::collections::HashSet;

use egui::{Color32, Pos2, RichText, Vec2};

use crate::controller::{ControllerBinding, ControllerButton, ControllerKind};
use crate::ui::controller_glyph::{self, GlyphFamily};
use crate::user_notify::{self, NoticeAction, NoticeKey, Severity};

pub const CARD_WIDTH: f32 = 320.0;
pub const CARD_HEIGHT: f32 = 120.0;
pub const TIP_WIDTH: f32 = 400.0;
pub const TIP_HEIGHT: f32 = 156.0;
pub const DOCK_GAP: f32 = 8.0;
pub const CARD_RADIUS: f32 = 8.0;
pub const ACCENT_WIDTH: f32 = 3.0;
pub const CARD_MARGIN: f32 = 12.0;
pub const TITLE_SIZE: f32 = 15.0;
pub const BODY_SIZE: f32 = 13.0;
pub const HINT_SIZE: f32 = 11.0;
pub const ICON_SIZE: f32 = 20.0;
pub const GLYPH_SIZE: f32 = 18.0;
pub const FADE_IN_MS: f32 = 150.0;
pub const FADE_OUT_MS: f32 = 120.0;
pub const SLIDE_PX: f32 = 6.0;

/// Window title for the satellite viewport. The Win32 styler finds it by this.
pub const SATELLITE_TITLE: &str = "kosk notice";

/// What the satellite window draws this frame.
#[derive(Debug, Clone)]
pub struct ToastView {
    pub title: String,
    pub body: String,
    pub severity: Severity,
    pub wide: bool,
    pub is_tip: bool,
    pub counter: Option<String>,
    pub more: usize,
    pub hint_visible: bool,
    pub family: GlyphFamily,
    /// 0 = just appeared, 1 = fully in. Driven by frame time in the caller.
    pub appear: f32,
}

pub fn snapshot_for_ui() -> Option<ToastView> {
    let snap = user_notify::snapshot()?;
    let counter = if snap.total > 1 {
        Some(format!("{} / {}", snap.index, snap.total))
    } else {
        None
    };

    Some(ToastView {
        title: snap.title,
        body: snap.body,
        severity: snap.severity,
        wide: snap.wide,
        is_tip: snap.action == Some(NoticeAction::OpenNextWordGuide),
        counter,
        more: snap.more,
        hint_visible: snap.hint_visible,
        family: GlyphFamily::from_kind(snap.family),
        appear: snap.appear,
    })
}

pub fn card_size(view: &ToastView) -> Vec2 {
    if view.wide {
        Vec2::new(TIP_WIDTH, TIP_HEIGHT)
    } else {
        Vec2::new(CARD_WIDTH, CARD_HEIGHT)
    }
}

/// Dock the toast beside the keyboard, on the side facing the screen interior.
pub fn dock_position(kb_outer: Pos2, kb_size: Vec2, toast_size: Vec2, monitor: Vec2) -> Pos2 {
    let below = kb_outer.y + kb_size.y + DOCK_GAP;
    let above = kb_outer.y - DOCK_GAP - toast_size.y;
    let kb_mid_y = kb_outer.y + kb_size.y / 2.0;
    let fits = |y: f32| y >= 0.0 && y + toast_size.y <= monitor.y;

    let mut y = if kb_mid_y >= monitor.y / 2.0 {
        above
    } else {
        below
    };
    if !fits(y) {
        let other = if y == above { below } else { above };
        if fits(other) {
            y = other;
        }
    }

    let kb_mid_x = kb_outer.x + kb_size.x / 2.0;
    let x = if kb_mid_x >= monitor.x / 2.0 {
        kb_outer.x + kb_size.x - toast_size.x
    } else {
        kb_outer.x
    };

    Pos2::new(
        x.clamp(0.0, (monitor.x - toast_size.x).max(0.0)),
        y.clamp(0.0, (monitor.y - toast_size.y).max(0.0)),
    )
}

pub fn viewport_builder(pos: Pos2, size: Vec2) -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title(SATELLITE_TITLE)
        .with_transparent(true)
        .with_decorations(false)
        .with_always_on_top()
        .with_taskbar(false)
        .with_active(false)
        .with_mouse_passthrough(true)
        .with_resizable(false)
        .with_has_shadow(false)
        .with_inner_size(size)
        .with_position(pos)
}

fn accent_color(severity: Severity, cfg: &crate::config::Config) -> Color32 {
    let battery = &cfg.battery;
    let appearance = &cfg.theme().notifications;
    let rgba = match severity {
        Severity::Info => battery.charging.unwrap_or(appearance.info_color),
        Severity::Warning => battery.low.unwrap_or(appearance.warning_color),
        Severity::Error => battery.empty.unwrap_or(appearance.error_color),
    };
    crate::theme::color(rgba)
}

fn icon_name(severity: Severity, is_tip: bool) -> &'static str {
    if is_tip {
        return "lightbulb";
    }

    match severity {
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Error => "warning-octagon",
    }
}

fn icon_glyph(name: &str) -> &str {
    egui_phosphor_icons::Icon::from_name(name).map_or("?", |icon| icon.as_str())
}

/// Draw the card into the satellite viewport. Fixed size; content is capped.
pub fn draw_satellite(ui: &mut egui::Ui, view: &ToastView) {
    let cfg = crate::config::try_get().unwrap_or_default();
    let appearance = &cfg.theme().notifications;
    *ui.visuals_mut() = cfg.theme().visuals();
    let muted_text = crate::theme::color(appearance.muted_text_color);
    let ctx = ui.ctx().clone();
    let size = card_size(view);
    let accent = accent_color(view.severity, &cfg);
    let appear = view.appear.clamp(0.0, 1.0);
    let slide = (1.0 - appear) * SLIDE_PX;
    let [r, g, b, a] = appearance.background_color;
    let fill = Color32::from_rgba_unmultiplied(r, g, b, (a as f32 * appear) as u8);

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(Color32::TRANSPARENT))
        .show(ui, |ui| {
            ui.set_width(size.x);
            ui.set_height(size.y);

            let card = egui::Frame::NONE
                .fill(fill)
                .stroke(egui::Stroke::new(
                    appearance.border_width,
                    crate::theme::color(appearance.border_color),
                ))
                .corner_radius(appearance.corner_radius)
                .inner_margin(CARD_MARGIN)
                .show(ui, |ui| {
                    // Fill the window. A shorter card leaves a transparent band
                    // that DWM paints as the bottom of a larger window.
                    ui.set_min_size(Vec2::new(
                        size.x - CARD_MARGIN * 2.0,
                        size.y - CARD_MARGIN * 2.0,
                    ));
                    ui.set_width(size.x - CARD_MARGIN * 2.0 + slide);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(icon_glyph(icon_name(view.severity, view.is_tip)))
                                    .font(egui::FontId::new(
                                        ICON_SIZE,
                                        egui::FontFamily::Name("phosphor-regular".into()),
                                    ))
                                    .color(accent),
                            );
                            ui.label(RichText::new(&view.title).size(TITLE_SIZE).strong());
                            if let Some(counter) = &view.counter {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            RichText::new(counter)
                                                .size(HINT_SIZE)
                                                .color(muted_text),
                                        );
                                    },
                                );
                            }
                        });

                        ui.add_space(4.0);
                        ui.label(RichText::new(&view.body).size(BODY_SIZE));

                        if view.is_tip {
                            ui.add_space(8.0);
                            tip_footer(ui, view, muted_text);
                        } else {
                            ui.add_space(6.0);
                            let hint = RichText::new("Press any button to dismiss")
                                .size(HINT_SIZE)
                                .color(if view.hint_visible {
                                    muted_text
                                } else {
                                    Color32::TRANSPARENT
                                });
                            ui.label(hint);
                        }

                        if view.more > 0 {
                            ui.label(
                                RichText::new(format!("And {} more", view.more))
                                    .size(HINT_SIZE)
                                    .color(muted_text),
                            );
                        }
                    });
                });

            let rect = card.response.rect;
            ui.painter().rect_filled(
                egui::Rect::from_min_size(
                    rect.min
                        + Vec2::new(
                            CARD_MARGIN / 2.0,
                            appearance.corner_radius.min(rect.height() / 2.0),
                        ),
                    Vec2::new(
                        ACCENT_WIDTH,
                        (rect.height() - appearance.corner_radius * 2.0).max(0.0),
                    ),
                ),
                ACCENT_WIDTH / 2.0,
                accent,
            );
        });

    ctx.request_repaint();
}

fn tip_footer(ui: &mut egui::Ui, view: &ToastView, muted_text: Color32) {
    ui.horizontal(|ui| {
        controller_glyph::show(ui, view.family, ControllerButton::FaceBottom, GLYPH_SIZE);
        ui.label(RichText::new("Open setup guide").size(HINT_SIZE).strong());
        ui.label(RichText::new("·").size(HINT_SIZE).color(muted_text));
        ui.label(
            RichText::new("Any other button: dismiss")
                .size(HINT_SIZE)
                .color(if view.hint_visible {
                    muted_text
                } else {
                    Color32::TRANSPARENT
                }),
        );
    });
}

/// Buttons a DualShock 4 never reports. Mirrors the always-false arm of
/// [`crate::controller::ps4`] input queries.
const PS4_UNSUPPORTED: [ControllerButton; 7] = [
    ControllerButton::PadLeft,
    ControllerButton::PadRight,
    ControllerButton::L4,
    ControllerButton::L5,
    ControllerButton::R4,
    ControllerButton::R5,
    ControllerButton::QuickAccess,
];

fn unsupported_for(kind: ControllerKind) -> &'static [ControllerButton] {
    match kind {
        ControllerKind::Ps4 => &PS4_UNSUPPORTED,
        ControllerKind::Sc2 | ControllerKind::Replay => &[],
    }
}

/// Warn once when the active mappings use buttons the connected controller
/// does not have. Grouped into a single notice with a count.
pub(crate) fn check_unsupported(kind: ControllerKind) {
    let unsupported = unsupported_for(kind);
    if unsupported.is_empty() {
        return;
    }

    let mut hit = HashSet::new();
    for mode in crate::config::get().controller_map.values() {
        for binding in mode.keys() {
            match binding {
                ControllerBinding::Single(button) => {
                    if unsupported.contains(button) {
                        hit.insert(*button);
                    }
                }
                ControllerBinding::Chord { leader, follower } => {
                    for button in [leader, follower] {
                        if unsupported.contains(button) {
                            hit.insert(*button);
                        }
                    }
                }
            }
        }
    }

    if !hit.is_empty() {
        user_notify::notify(user_notify::Notice::unsupported_buttons(hit.len()));
    }
}

/// Mark the setup tip shown in memory and on disk. In-memory first, so the
/// config reload from the write cannot re-show it.
pub(crate) fn acknowledge_tip() {
    let mut live = crate::config::get();
    if live.tips.completion_next_word_setup_shown {
        return;
    }

    live.tips.completion_next_word_setup_shown = true;
    crate::config::replace_live(live.clone());
    if let Err(e) = crate::config::persist_settings(&live, |cfg| {
        cfg.tips.completion_next_word_setup_shown = true;
    }) {
        eprintln!("tips: persist shown flag: {e:#}");
        user_notify::notify(user_notify::Notice::settings_save_failed());
    }
}

const GUIDE_SOURCE: &str = include_str!("../../assets/guides/next-word-setup.html");

/// Write the embedded setup guide beside the user config with this machine's
/// folders filled in, then open it in the default browser.
pub(crate) fn open_setup_guide() -> anyhow::Result<()> {
    let Some(dir) = crate::config::config_dir() else {
        anyhow::bail!("config directory is unknown");
    };
    let cfg = crate::config::get();
    let model_dir = if cfg.completion.ngram.model_dir.is_absolute() {
        cfg.completion.ngram.model_dir.clone()
    } else {
        dir.join(&cfg.completion.ngram.model_dir)
    };

    let html = GUIDE_SOURCE
        .replace("{{MODEL_DIR}}", &model_dir.display().to_string())
        .replace("{{CONFIG_DIR}}", &dir.display().to_string());
    if html.contains("{{") {
        anyhow::bail!("setup guide has unfilled placeholders");
    }

    let path = dir.join("next-word-setup.html");
    std::fs::write(&path, html).map_err(|e| anyhow::anyhow!("write {}: {e}", path.display()))?;
    crate::platform::open_in_default_app(&path)
}

/// True when the dismissing press should open the setup guide.
pub(crate) fn is_tip_acknowledge(key: &NoticeKey) -> bool {
    *key == NoticeKey::NextWordSetup
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_icons_resolve() {
        for name in ["lightbulb", "info", "warning", "warning-octagon"] {
            assert!(
                egui_phosphor_icons::Icon::from_name(name).is_some(),
                "missing phosphor icon {name}"
            );
        }
    }

    #[test]
    fn dock_bottom_right_goes_above() {
        let pos = dock_position(
            Pos2::new(1400.0, 800.0),
            Vec2::new(520.0, 250.0),
            Vec2::new(CARD_WIDTH, CARD_HEIGHT),
            Vec2::new(1920.0, 1080.0),
        );
        assert_eq!(
            pos,
            Pos2::new(1400.0 + 520.0 - CARD_WIDTH, 800.0 - 8.0 - CARD_HEIGHT)
        );
    }

    #[test]
    fn dock_top_left_goes_below() {
        let pos = dock_position(
            Pos2::new(0.0, 0.0),
            Vec2::new(520.0, 250.0),
            Vec2::new(CARD_WIDTH, CARD_HEIGHT),
            Vec2::new(1920.0, 1080.0),
        );
        assert_eq!(pos, Pos2::new(0.0, 250.0 + 8.0));
    }

    #[test]
    fn dock_falls_back_when_no_room_above() {
        let pos = dock_position(
            Pos2::new(1400.0, 40.0),
            Vec2::new(520.0, 250.0),
            Vec2::new(CARD_WIDTH, CARD_HEIGHT),
            Vec2::new(1920.0, 1080.0),
        );
        assert_eq!(
            pos,
            Pos2::new(1400.0 + 520.0 - CARD_WIDTH, 40.0 + 250.0 + 8.0)
        );
    }

    #[test]
    fn dock_clamps_to_monitor() {
        let pos = dock_position(
            Pos2::new(1800.0, 1000.0),
            Vec2::new(520.0, 250.0),
            Vec2::new(TIP_WIDTH, TIP_HEIGHT),
            Vec2::new(1920.0, 1080.0),
        );
        assert!(pos.x + TIP_WIDTH <= 1920.0);
        assert!(pos.y + TIP_HEIGHT <= 1080.0);
        assert!(pos.x >= 0.0 && pos.y >= 0.0);
    }

    #[test]
    fn satellite_builder_never_takes_focus() {
        let builder = viewport_builder(Pos2::ZERO, Vec2::new(CARD_WIDTH, CARD_HEIGHT));
        assert_eq!(builder.active, Some(false));
        assert_eq!(builder.mouse_passthrough, Some(true));
        assert_eq!(builder.decorations, Some(false));
        assert_eq!(builder.transparent, Some(true));
        assert_eq!(builder.has_shadow, Some(false));
        assert_eq!(builder.taskbar, Some(false));
        assert_eq!(builder.resizable, Some(false));
        assert!(matches!(
            builder.window_level,
            Some(egui::WindowLevel::AlwaysOnTop)
        ));
    }

    #[test]
    fn card_renders_without_panic() {
        for view in [
            ToastView {
                title: "Couldn't record".to_owned(),
                body: "Recording could not start. Check the save location in settings.".to_owned(),
                severity: Severity::Error,
                wide: false,
                is_tip: false,
                counter: Some("1 / 2".to_owned()),
                more: 1,
                hint_visible: true,
                family: GlyphFamily::Sc2,
                appear: 1.0,
            },
            ToastView {
                title: "Better next-word suggestions".to_owned(),
                body: "kosk can guess your next word after a space.\nThis needs a one-time setup.\nWithout it, common words only.".to_owned(),
                severity: Severity::Info,
                wide: true,
                is_tip: true,
                counter: None,
                more: 0,
                hint_visible: false,
                family: GlyphFamily::Ps4,
                appear: 0.5,
            },
        ] {
            let ctx = egui::Context::default();
            let mut fonts = egui::FontDefinitions::default();
            egui_phosphor_icons::add_fonts(&mut fonts);
            ctx.set_fonts(fonts);
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                draw_satellite(ui, &view);
            });
            output.textures_delta.clear();
        }
    }

    #[test]
    fn unsupported_only_on_ps4() {
        assert!(!unsupported_for(ControllerKind::Sc2).contains(&ControllerButton::L4));
        assert!(unsupported_for(ControllerKind::Ps4).contains(&ControllerButton::L4));
        assert!(unsupported_for(ControllerKind::Ps4).contains(&ControllerButton::QuickAccess));
        assert!(!unsupported_for(ControllerKind::Ps4).contains(&ControllerButton::FaceBottom));
    }

    #[test]
    fn guide_has_placeholders() {
        assert!(GUIDE_SOURCE.contains("{{MODEL_DIR}}"));
        assert!(GUIDE_SOURCE.contains("{{CONFIG_DIR}}"));
    }
}

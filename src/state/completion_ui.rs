use crate::completion::settings::{ArmedDotPlacement, ChipLabel, ChipWidth, CompletionUiConfig};
use crate::completion::Candidate;
use egui::{Color32, FontId, Pos2, Sense, Stroke, Ui, Vec2};

fn rgba(c: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

pub struct StripOutcome {
    pub clicked: Option<usize>,
}

#[allow(clippy::too_many_arguments)]
pub fn draw_strip(
    ui: &mut Ui,
    content_width: f32,
    ui_cfg: &CompletionUiConfig,
    candidates: &[Candidate],
    highlight: Option<usize>,
    armed: bool,
    typed_token: &str,
    slots: usize,
) -> StripOutcome {
    let columns = ui_cfg.columns.max(1);
    let rows = ui_cfg.rows.max(1);
    let slots = slots.min(columns * rows);
    let font = FontId::proportional(ui_cfg.font_size.max(1.0));
    let row_h = ui.fonts_mut(|f| f.row_height(&font)) + ui_cfg.padding_y * 2.0;
    let gap = ui_cfg.gap;
    let dot_d = ui_cfg.armed_dot_radius * 2.0;
    let mut clicked = None;

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(gap, gap);
        for row in 0..rows {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(gap, 0.0);
                let leading_dot = ui_cfg.armed_dot
                    && ui_cfg.armed_dot_placement == ArmedDotPlacement::ChipsLeading
                    && row == 0;
                let trailing_dot = ui_cfg.armed_dot
                    && ui_cfg.armed_dot_placement == ArmedDotPlacement::ChipsTrailing
                    && row == 0;

                if leading_dot {
                    draw_dot(ui, armed, ui_cfg, row_h, dot_d);
                }

                let dots = if ui_cfg.armed_dot && row == 0 {
                    dot_d + gap
                } else {
                    0.0
                };
                let inner_w = (content_width - dots).max(ui_cfg.min_chip_width);
                let chip_w = match ui_cfg.chip_width {
                    ChipWidth::Fill => {
                        let g = gap * (columns.saturating_sub(1) as f32);
                        ((inner_w - g) / columns as f32).max(ui_cfg.min_chip_width)
                    }
                    ChipWidth::Hug => ui_cfg.max_chip_width,
                };

                for col in 0..columns {
                    let i = row * columns + col;
                    if i >= slots && ui_cfg.reserve_slots {
                        draw_empty(ui, chip_w, row_h, ui_cfg);
                        continue;
                    }
                    if i >= slots {
                        continue;
                    }
                    if let Some(c) = candidates.get(i) {
                        if draw_chip(
                            ui,
                            c,
                            i,
                            highlight == Some(i),
                            chip_w,
                            row_h,
                            ui_cfg,
                            &font,
                            typed_token,
                        ) {
                            clicked = Some(i);
                        }
                    } else if ui_cfg.reserve_slots {
                        draw_empty(ui, chip_w, row_h, ui_cfg);
                    }
                }

                if trailing_dot {
                    draw_dot(ui, armed, ui_cfg, row_h, dot_d);
                }
            });
        }
    });

    StripOutcome { clicked }
}

pub fn draw_session_strip(ui: &mut Ui, content_width: f32, token: &str) -> Option<usize> {
    crate::completion::with_mut(|sess| {
        let s = sess?;
        let ui_cfg = s.cfg().ui.clone();
        let slots = s.visible_slots().min(s.cfg().max_suggestions);
        let armed = s.armed();
        let candidates: Vec<Candidate> = if armed {
            s.candidates().to_vec()
        } else {
            Vec::new()
        };
        let highlight = if armed { s.highlight() } else { None };
        draw_strip(
            ui,
            content_width,
            &ui_cfg,
            &candidates,
            highlight,
            armed,
            token,
            slots,
        )
        .clicked
    })
}

fn draw_dot(ui: &mut Ui, armed: bool, cfg: &CompletionUiConfig, row_h: f32, d: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(d.max(8.0), row_h), Sense::hover());
    let color = if armed {
        rgba(cfg.armed_color)
    } else {
        rgba(cfg.disarmed_color)
    };
    ui.painter()
        .circle_filled(rect.center(), cfg.armed_dot_radius.max(2.0), color);
}

fn draw_empty(ui: &mut Ui, w: f32, h: f32, cfg: &CompletionUiConfig) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    ui.painter()
        .rect_filled(rect, cfg.corner_radius, rgba(cfg.empty_slot_background));
}

#[allow(clippy::too_many_arguments)]
fn draw_chip(
    ui: &mut Ui,
    cand: &Candidate,
    index: usize,
    selected: bool,
    w: f32,
    h: f32,
    cfg: &CompletionUiConfig,
    font: &FontId,
    token: &str,
) -> bool {
    let id = egui::Id::new(format!("suggestion_{index}"));
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    let resp = ui.interact(rect, id, Sense::click());
    let bg = if selected {
        rgba(cfg.selected_background_color)
    } else {
        rgba(cfg.background_color)
    };
    let fg = if selected {
        rgba(cfg.selected_text_color)
    } else {
        rgba(cfg.text_color)
    };
    ui.painter().rect_filled(rect, cfg.corner_radius, bg);
    if selected && cfg.selected_outline_width > 0.0 {
        ui.painter().rect_stroke(
            rect,
            cfg.corner_radius,
            Stroke::new(cfg.selected_outline_width, fg),
            egui::StrokeKind::Inside,
        );
    }

    let label = match cfg.label {
        ChipLabel::Full => cand.text.clone(),
        ChipLabel::Remainder => crate::completion::remainder(token, &cand.text),
    };
    let text_pos = Pos2::new(rect.left() + cfg.padding_x, rect.center().y);
    if cfg.dim_typed_prefix && cfg.label == ChipLabel::Full && !token.is_empty() {
        let rest = crate::completion::remainder(token, &cand.text);
        let prefix_w = ui.fonts_mut(|f| {
            f.layout_no_wrap(token.to_string(), font.clone(), fg)
                .size()
                .x
        });
        let dim = Color32::from_rgba_unmultiplied(fg.r(), fg.g(), fg.b(), 140);
        ui.painter().text(
            text_pos,
            egui::Align2::LEFT_CENTER,
            token,
            font.clone(),
            dim,
        );
        ui.painter().text(
            Pos2::new(text_pos.x + prefix_w, text_pos.y),
            egui::Align2::LEFT_CENTER,
            rest,
            font.clone(),
            fg,
        );
    } else {
        ui.painter()
            .text(text_pos, egui::Align2::LEFT_CENTER, label, font.clone(), fg);
    }

    if cfg.show_debug_scores {
        ui.painter().text(
            Pos2::new(rect.right() - 4.0, rect.top() + 2.0),
            egui::Align2::RIGHT_TOP,
            format!("{:.2}", cand.score),
            FontId::proportional((font.size * 0.6).max(8.0)),
            fg,
        );
    }

    resp.clicked()
}

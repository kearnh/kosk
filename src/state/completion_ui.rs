use crate::completion::settings::{ArmedDotPlacement, ChipLabel, ChipWidth, CompletionUiConfig};
use crate::completion::{Candidate, Source};
use egui::{Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};

const CHIP_OVERFLOW_MARK: &str = "..";

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

struct FittedText {
    text: String,
    kept: usize,
}

fn text_width(ui: &Ui, text: &str, font: &FontId) -> f32 {
    ui.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(text.to_string(), font.clone(), Color32::PLACEHOLDER)
            .size()
            .x
    })
}

fn chip_text_clip(rect: Rect, padding_x: f32) -> Rect {
    let left = rect.left() + padding_x;
    let right = (rect.right() - padding_x).max(left);

    Rect::from_min_max(Pos2::new(left, rect.top()), Pos2::new(right, rect.bottom()))
}

fn truncate_to_width(text: &str, max_width: f32, width_of: impl Fn(&str) -> f32) -> FittedText {
    let count = text.chars().count();
    if max_width <= 0.0 {
        return FittedText {
            text: String::new(),
            kept: 0,
        };
    }
    if width_of(text) <= max_width {
        return FittedText {
            text: text.to_string(),
            kept: count,
        };
    }

    if width_of(CHIP_OVERFLOW_MARK) <= max_width {
        let kept = longest_prefix_len(text, max_width, &width_of, CHIP_OVERFLOW_MARK);

        return FittedText {
            text: prefix_plus(text, kept, CHIP_OVERFLOW_MARK),
            kept,
        };
    }

    let kept = longest_prefix_len(text, max_width, &width_of, "");

    FittedText {
        text: text.chars().take(kept).collect(),
        kept,
    }
}

fn longest_prefix_len(
    text: &str,
    max_width: f32,
    width_of: &impl Fn(&str) -> f32,
    suffix: &str,
) -> usize {
    let mut low = 0;
    let mut high = text.chars().count();
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if width_of(&prefix_plus(text, mid, suffix)) <= max_width {
            low = mid;
        } else {
            high = mid - 1;
        }
    }

    low
}

fn prefix_plus(text: &str, n: usize, suffix: &str) -> String {
    let mut out: String = text.chars().take(n).collect();
    out.push_str(suffix);
    out
}

fn split_dim_prefix(fitted: &FittedText, token: &str) -> (String, String) {
    let token_chars = token.chars().count();
    if fitted.kept <= token_chars {
        return (fitted.text.clone(), String::new());
    }

    let dim: String = fitted.text.chars().take(token_chars).collect();
    let bright: String = fitted.text.chars().skip(token_chars).collect();
    (dim, bright)
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

    let current_word = cand.source == Source::CurrentWord;
    let label = if current_word {
        cand.text.clone()
    } else {
        match cfg.label {
            ChipLabel::Full => cand.text.clone(),
            ChipLabel::Remainder => {
                if crate::completion::is_case_insensitive_prefix(token, &cand.text) {
                    crate::completion::remainder(token, &cand.text)
                } else {
                    cand.text.clone()
                }
            }
        }
    };
    let text_clip = chip_text_clip(rect, cfg.padding_x);
    let painter = ui.painter().with_clip_rect(text_clip);
    let text_pos = Pos2::new(rect.left() + cfg.padding_x, rect.center().y);
    let mark_w = if current_word {
        let mark = "+ ";
        let w = text_width(ui, mark, font);
        painter.text(
            text_pos,
            egui::Align2::LEFT_CENTER,
            mark,
            font.clone(),
            rgba(cfg.new_word_mark_color),
        );
        w
    } else {
        0.0
    };
    let word_pos = Pos2::new(text_pos.x + mark_w, text_pos.y);
    let dim_prefix = !current_word
        && cfg.dim_typed_prefix
        && cfg.label == ChipLabel::Full
        && !token.is_empty()
        && crate::completion::is_case_insensitive_prefix(token, &cand.text);
    let display = if dim_prefix {
        let rest = crate::completion::remainder(token, &cand.text);
        format!("{token}{rest}")
    } else {
        label
    };
    let available = (text_clip.right() - word_pos.x).max(0.0);
    let fitted = truncate_to_width(&display, available, |sample| text_width(ui, sample, font));
    if dim_prefix {
        let (dim_text, bright_text) = split_dim_prefix(&fitted, token);
        let dim = Color32::from_rgba_unmultiplied(fg.r(), fg.g(), fg.b(), 140);
        let prefix_w = text_width(ui, &dim_text, font);
        painter.text(
            word_pos,
            egui::Align2::LEFT_CENTER,
            dim_text,
            font.clone(),
            dim,
        );
        painter.text(
            Pos2::new(word_pos.x + prefix_w, word_pos.y),
            egui::Align2::LEFT_CENTER,
            bright_text,
            font.clone(),
            fg,
        );
    } else {
        painter.text(
            word_pos,
            egui::Align2::LEFT_CENTER,
            fitted.text,
            font.clone(),
            fg,
        );
    }

    if cfg.show_debug_scores && !current_word {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn char_width(text: &str) -> f32 {
        text.chars().count() as f32
    }

    #[test]
    fn short_label_is_unchanged() {
        let fitted = truncate_to_width("hi", 10.0, char_width);
        assert_eq!(fitted.text, "hi");
        assert_eq!(fitted.kept, 2);
    }

    #[test]
    fn exact_fit_is_unchanged() {
        let fitted = truncate_to_width("hello", 5.0, char_width);
        assert_eq!(fitted.text, "hello");
        assert_eq!(fitted.kept, 5);
    }

    #[test]
    fn long_label_ends_with_dots_inside_width() {
        let fitted = truncate_to_width("inconsequential", 8.0, char_width);
        assert_eq!(fitted.text, "incons..");
        assert_eq!(fitted.kept, 6);
        assert!(char_width(&fitted.text) <= 8.0);
    }

    #[test]
    fn uses_measured_width() {
        let width = |text: &str| {
            text.chars()
                .map(|c| if c == 'i' { 1.0 } else { 10.0 })
                .sum::<f32>()
        };
        let fitted = truncate_to_width("inconsequential", 21.0, width);
        assert_eq!(fitted.text, "i..");
        assert_eq!(fitted.kept, 1);
        assert!(width(&fitted.text) <= 21.0);
    }

    #[test]
    fn prefix_only_when_dots_do_not_fit() {
        let fitted = truncate_to_width("inconsequential", 1.0, char_width);
        assert_eq!(fitted.text, "i");
        assert_eq!(fitted.kept, 1);
    }

    #[test]
    fn zero_width_is_empty() {
        let fitted = truncate_to_width("inconsequential", 0.0, char_width);
        assert_eq!(fitted.text, "");
        assert_eq!(fitted.kept, 0);
    }

    #[test]
    fn dim_split_keeps_typed_prefix_when_remainder_is_cut() {
        let fitted = FittedText {
            text: "incons..".into(),
            kept: 6,
        };
        let (dim, bright) = split_dim_prefix(&fitted, "inc");
        assert_eq!(dim, "inc");
        assert_eq!(bright, "ons..");
    }

    #[test]
    fn dim_split_includes_dots_when_cut_inside_prefix() {
        let fitted = FittedText {
            text: "in..".into(),
            kept: 2,
        };
        let (dim, bright) = split_dim_prefix(&fitted, "incon");
        assert_eq!(dim, "in..");
        assert_eq!(bright, "");
    }
}

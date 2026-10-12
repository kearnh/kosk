use crate::controller::ControllerButton;
use crate::theme::MenusTheme;
use crate::ui::controller_glyph::{self, GlyphFamily};
use crate::ui::fonts::{font_id, FontRole};
use egui::{Align2, Frame, Margin, RichText, Sense, Ui, Vec2};

const LIST_WIDTH: f32 = 320.0;
const ROW_HEIGHT: f32 = 28.0;
const ROW_FONT: f32 = 14.0;
const ROW_PAD: f32 = 8.0;
const EDGE_INSET: i8 = 16;
const TOP_INSET: i8 = 12;
const HINT_GLYPH: f32 = 24.0;
const HINT_GAP: f32 = 10.0;
const FOOTER_HEIGHT: f32 = 64.0;

pub(crate) fn width_with_side_panel(panel_width: f32, gap: f32) -> f32 {
    LIST_WIDTH + panel_width + gap + 2.0 * f32::from(EDGE_INSET)
}

pub(crate) fn page(ui: &mut Ui, action_height: f32) -> (Ui, Ui) {
    let rect = ui.available_rect_before_wrap();
    let split = (rect.bottom() - FOOTER_HEIGHT - action_height).max(rect.top());
    let body = egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), split));
    let footer = egui::Rect::from_min_max(egui::pos2(rect.left(), split), rect.max);
    let layout = egui::Layout::top_down(egui::Align::Min);
    let body = ui.new_child(egui::UiBuilder::new().max_rect(body).layout(layout));
    let footer = ui.new_child(egui::UiBuilder::new().max_rect(footer).layout(layout));
    ui.allocate_rect(rect, Sense::hover());
    (body, footer)
}

pub(crate) fn frame() -> Frame {
    Frame::NONE.inner_margin(Margin {
        left: EDGE_INSET,
        right: EDGE_INSET,
        top: TOP_INSET,
        bottom: 0,
    })
}

pub(crate) fn heading(ui: &mut Ui, title: &str, appearance: &MenusTheme) {
    ui.set_width(LIST_WIDTH);
    ui.label(
        RichText::new(title)
            .heading()
            .color(crate::theme::color(appearance.heading_color)),
    );
    ui.separator();
}

pub(crate) fn row(
    ui: &mut Ui,
    label: &str,
    value: Option<&str>,
    selected: bool,
    appearance: &MenusTheme,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(LIST_WIDTH, ROW_HEIGHT), Sense::click());
    if selected {
        if let Some(corners) = &appearance.selection {
            corners.paint(
                ui.painter(),
                rect,
                Some(ui.visuals().selection.bg_fill),
                ui.visuals().selection.stroke,
            );
        } else {
            ui.painter().rect_filled(
                rect,
                ui.visuals().widgets.inactive.corner_radius,
                ui.visuals().selection.bg_fill,
            );
        }
    }
    let color = if selected {
        appearance
            .selected_text_color
            .map(crate::theme::color)
            .unwrap_or_else(|| ui.visuals().text_color())
    } else {
        ui.visuals().text_color()
    };
    ui.painter().text(
        rect.left_center() + Vec2::new(ROW_PAD, 0.0),
        Align2::LEFT_CENTER,
        label,
        font_id(ui, FontRole::Menu, ROW_FONT),
        color,
    );
    if let Some(value) = value {
        ui.painter().text(
            rect.right_center() - Vec2::new(ROW_PAD, 0.0),
            Align2::RIGHT_CENTER,
            value,
            font_id(ui, FontRole::Menu, ROW_FONT),
            color,
        );
    }
    response
}

pub(crate) fn hints<'a>(
    ui: &mut Ui,
    family: GlyphFamily,
    hints: impl IntoIterator<Item = (Vec<ControllerButton>, &'a str)>,
) {
    let hints: Vec<_> = hints
        .into_iter()
        .filter(|(buttons, _)| !buttons.is_empty())
        .collect();
    if hints.is_empty() {
        return;
    }

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (index, (buttons, label)) in hints.iter().enumerate() {
            if index > 0 {
                ui.add_space(HINT_GAP);
            }
            for button in buttons {
                controller_glyph::show(ui, family, *button, HINT_GLYPH);
            }
            ui.label(*label);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_heading_has_compact_top_spacing() {
        let mut output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
            let menu = frame().show(ui, |ui| {
                heading(ui, "Settings", &MenusTheme::default());
                ui.min_rect()
            });

            assert_eq!(
                menu.inner.min - menu.response.rect.min,
                Vec2::new(16.0, 12.0)
            );
        });
        output.textures_delta.clear();
    }
}

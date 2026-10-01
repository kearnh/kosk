use crate::controller::ControllerButton;
use crate::theme::MenusTheme;
use crate::ui::controller_glyph::{self, GlyphFamily};
use egui::{Align2, FontId, Frame, Margin, RichText, Sense, Ui, Vec2};

const LIST_WIDTH: f32 = 320.0;
const ROW_HEIGHT: f32 = 28.0;
const ROW_FONT: f32 = 14.0;
const ROW_PAD: f32 = 8.0;
const EDGE_INSET: i8 = 16;
const HINT_GLYPH: f32 = 16.0;
const HINT_GAP: f32 = 10.0;

pub(crate) fn frame() -> Frame {
    Frame::NONE.inner_margin(Margin {
        left: EDGE_INSET,
        right: EDGE_INSET,
        top: EDGE_INSET,
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
        ui.painter().rect_filled(
            rect,
            ui.visuals().widgets.inactive.corner_radius,
            ui.visuals().selection.bg_fill,
        );
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
        FontId::proportional(ROW_FONT),
        color,
    );
    if let Some(value) = value {
        ui.painter().text(
            rect.right_center() - Vec2::new(ROW_PAD, 0.0),
            Align2::RIGHT_CENTER,
            value,
            FontId::proportional(ROW_FONT),
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

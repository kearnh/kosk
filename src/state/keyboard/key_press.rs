use egui::{Color32, CornerRadius, Painter, Rect, Stroke, StrokeKind};

const MAXIMUM_KEY_PRESS_INSET: f32 = 2.0;
const MAXIMUM_KEY_PRESS_INSET_FRACTION: f32 = 0.25;
const KEY_PRESS_OUTLINE_WIDTH: f32 = 2.0;
const KEY_PRESS_FLASH_WIDTH: f32 = 1.0;

pub(super) struct KeyPressFeedback {
    background: Color32,
    pulse: Color32,
    strength: f32,
}

impl KeyPressFeedback {
    pub(super) fn new(background: Color32, pulse: Color32, remaining: f32) -> Self {
        let strength = remaining.clamp(0.0, 1.0).powi(2);
        Self {
            background,
            pulse: pulse.gamma_multiply(strength),
            strength,
        }
    }

    pub(super) fn background(&self) -> Color32 {
        self.background
    }

    pub(super) fn rect(&self, rect: Rect) -> Rect {
        let inset = (MAXIMUM_KEY_PRESS_INSET * self.strength)
            .min(rect.width().min(rect.height()) * MAXIMUM_KEY_PRESS_INSET_FRACTION);
        rect.shrink(inset)
    }

    pub(super) fn draw(
        &self,
        painter: &Painter,
        rect: Rect,
        corner_radius: CornerRadius,
        text_color: Color32,
    ) {
        if self.strength == 0.0 {
            return;
        }

        painter.rect_stroke(
            rect,
            corner_radius,
            Stroke::new(
                KEY_PRESS_OUTLINE_WIDTH,
                text_color.gamma_multiply(self.strength),
            ),
            StrokeKind::Inside,
        );
        if rect.width().min(rect.height()) <= KEY_PRESS_OUTLINE_WIDTH * 2.0 {
            return;
        }

        painter.rect_stroke(
            rect.shrink(KEY_PRESS_OUTLINE_WIDTH),
            corner_radius,
            Stroke::new(KEY_PRESS_FLASH_WIDTH, self.pulse),
            StrokeKind::Inside,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{color, Theme};

    #[test]
    fn matching_key_and_flash_colors_still_show_a_press() {
        let theme = Theme::parse(include_str!("../../../themes/cyberpunk-2077.toml")).unwrap();
        let background = color(theme.keyboard.right_selection_color);
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(30.0, 32.0));
        let feedback =
            KeyPressFeedback::new(background, color(theme.keyboard.key_press_color), 1.0);
        assert!(feedback.background() != background || feedback.rect(rect) != rect);
    }

    #[test]
    fn press_fades_out_without_changing_key_fill_or_selection_geometry() {
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(30.0, 32.0));
        let background = Color32::YELLOW;
        let mut previous_width = rect.width();
        for remaining in [0.0, 0.25, 0.5, 1.0] {
            let feedback = KeyPressFeedback::new(background, Color32::WHITE, remaining);
            let pressed_rect = feedback.rect(rect);
            assert_eq!(feedback.background(), background);
            assert_eq!(pressed_rect.center(), rect.center());
            assert!(pressed_rect.width() <= previous_width);
            assert!(rect.contains_rect(pressed_rect));
            if remaining == 0.0 {
                assert_eq!(pressed_rect, rect);
            }
            previous_width = pressed_rect.width();
        }
    }

    #[test]
    fn matching_flash_color_keeps_a_visible_text_contrast_outline() {
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(30.0, 32.0));
        let ctx = egui::Context::default();
        for remaining in [0.0, 0.5, 1.0] {
            let feedback = KeyPressFeedback::new(Color32::YELLOW, Color32::YELLOW, remaining);
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                feedback.draw(
                    ui.painter(),
                    feedback.rect(rect),
                    CornerRadius::ZERO,
                    Color32::BLACK,
                );
            });
            output.textures_delta.clear();
            let outlines: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect) => Some(rect),
                    _ => None,
                })
                .collect();
            if remaining == 0.0 {
                assert!(outlines.is_empty());
                continue;
            }
            assert_eq!(outlines.len(), 2);
            assert_eq!(outlines[0].stroke.color.r(), 0);
            assert!(outlines[0].stroke.color.a() > 0);
            assert!(outlines[1].stroke.color.r() > 0);
        }
    }
}

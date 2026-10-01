use egui::{epaint::Mesh, Color32, Painter, Pos2, Shape, Stroke, Vec2};

use crate::config::{CursorAppearance, StickPadCursors};
use crate::controller::StickSide;

const FADE_SEGMENTS: u32 = 64;
const CONTRAST_EDGE_WIDTH: f32 = 1.5;
const LIGHT_CURSOR_INTENSITY: f32 = 0.5;

pub(super) fn draw(painter: &Painter, center: Pos2, side: StickSide, settings: &StickPadCursors) {
    let (color, fill_color) = match side {
        StickSide::Left => (settings.left_color, settings.left_fill_color),
        StickSide::Right => (settings.right_color, settings.right_fill_color),
    };
    let Some(shape) = cursor_shape(center, color, fill_color, settings) else {
        return;
    };
    let cursor_color = crate::theme::color(color).gamma_multiply(settings.opacity.clamp(0.0, 1.0));
    if cursor_color.a() > 0 {
        let opaque_color = Color32::from_rgb(color[0], color[1], color[2]);
        let edge = if opaque_color.intensity() >= LIGHT_CURSOR_INTENSITY {
            Color32::BLACK
        } else {
            Color32::WHITE
        }
        .gamma_multiply(f32::from(cursor_color.a()) / f32::from(u8::MAX));
        painter.circle_stroke(
            center,
            settings.radius + CONTRAST_EDGE_WIDTH * 0.5,
            Stroke::new(CONTRAST_EDGE_WIDTH, edge),
        );
    }
    painter.add(shape);
}

fn cursor_shape(
    center: Pos2,
    color: [u8; 4],
    fill_color: Option<[u8; 4]>,
    settings: &StickPadCursors,
) -> Option<Shape> {
    if !settings.radius.is_finite()
        || !settings.opacity.is_finite()
        || !settings.ring_thickness.is_finite()
    {
        return None;
    }

    let radius = settings.radius.max(0.0);
    let opacity = settings.opacity.clamp(0.0, 1.0);
    let fill_color = crate::theme::color(fill_color.unwrap_or(color));
    let color = crate::theme::color(color).gamma_multiply(opacity);
    if radius == 0.0 || opacity == 0.0 {
        return None;
    }

    let shape = match settings.appearance {
        CursorAppearance::Solid => Shape::circle_filled(center, radius, color),
        CursorAppearance::Ring => {
            let thickness = settings.ring_thickness.clamp(0.0, radius);
            Shape::Circle(egui::epaint::CircleShape {
                center,
                radius: radius - thickness * 0.5,
                fill: fill_color
                    .gamma_multiply(settings.ring_fill_opacity.clamp(0.0, 1.0) * opacity),
                stroke: Stroke::new(thickness, color),
            })
        }
        CursorAppearance::Fade => {
            let mut mesh = Mesh::default();
            mesh.colored_vertex(center, color);
            for segment in 0..FADE_SEGMENTS {
                let angle = std::f32::consts::TAU * segment as f32 / FADE_SEGMENTS as f32;
                mesh.colored_vertex(center + Vec2::angled(angle) * radius, Color32::TRANSPARENT);
                mesh.add_triangle(0, segment + 1, (segment + 1) % FADE_SEGMENTS + 1);
            }
            Shape::mesh(mesh)
        }
    };

    Some(shape)
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLOR: [u8; 4] = [0, 255, 0, 255];

    fn painted_cursor(settings: &StickPadCursors) -> Vec<Shape> {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            draw(
                ui.painter(),
                Pos2::new(30.0, 30.0),
                StickSide::Left,
                settings,
            );
        });
        output.textures_delta.clear();
        output.shapes.into_iter().map(|shape| shape.shape).collect()
    }

    #[test]
    fn matching_highlight_colors_still_have_a_contrasting_cursor_edge() {
        for appearance in [
            CursorAppearance::Solid,
            CursorAppearance::Ring,
            CursorAppearance::Fade,
        ] {
            let settings = StickPadCursors {
                appearance,
                left_color: COLOR,
                ..Default::default()
            };
            assert!(
                painted_cursor(&settings).iter().any(|shape| matches!(
                    shape,
                    Shape::Circle(circle)
                        if circle.stroke.width > 0.0
                            && circle.stroke.color.a() > 0
                            && circle.stroke.color != crate::theme::color(COLOR)
                )),
                "{appearance:?}: cursor needs an edge distinct from its fill"
            );
        }
    }

    #[test]
    fn cursor_edge_respects_opacity_and_transparent_outlines() {
        for (opacity, expected_alpha) in [(0.25, 32), (0.5, 64), (1.0, 128)] {
            let settings = StickPadCursors {
                left_color: [0, 255, 0, 128],
                opacity,
                ..Default::default()
            };
            let shapes = painted_cursor(&settings);
            let Shape::Circle(edge) = &shapes[0] else {
                panic!("expected cursor edge");
            };
            assert_eq!(edge.stroke.color.a(), expected_alpha);
        }
        let settings = StickPadCursors {
            appearance: CursorAppearance::Ring,
            left_color: [0; 4],
            left_fill_color: Some(COLOR),
            ..Default::default()
        };
        let shapes = painted_cursor(&settings);
        assert_eq!(shapes.len(), 1);
        let Shape::Circle(circle) = &shapes[0] else {
            panic!("expected filled ring");
        };
        assert_eq!(circle.stroke.color, Color32::TRANSPARENT);
        assert!(circle.fill.a() > 0);
        assert!(painted_cursor(&StickPadCursors {
            opacity: 0.0,
            ..settings
        })
        .is_empty());
    }

    #[test]
    fn solid_uses_configured_radius_and_opacity() {
        let settings = StickPadCursors {
            radius: 12.0,
            opacity: 0.5,
            ..Default::default()
        };
        let Some(Shape::Circle(circle)) = cursor_shape(Pos2::ZERO, COLOR, None, &settings) else {
            panic!("expected circle");
        };
        assert_eq!(circle.radius, 12.0);
        assert_eq!(circle.fill.a(), 128);
    }

    #[test]
    fn ring_has_translucent_fill_and_keeps_outer_radius() {
        let settings = StickPadCursors {
            appearance: CursorAppearance::Ring,
            radius: 12.0,
            ring_thickness: 3.0,
            ..Default::default()
        };
        let Some(Shape::Circle(circle)) = cursor_shape(Pos2::ZERO, COLOR, None, &settings) else {
            panic!("expected circle");
        };
        assert!(circle.fill.a() > 0);
        assert!(circle.fill.a() < circle.stroke.color.a());
        assert_eq!(circle.stroke.width, 3.0);
        assert_eq!(circle.radius + circle.stroke.width * 0.5, 12.0);
    }

    #[test]
    fn ring_fill_opacity_combines_with_overall_opacity() {
        for (fill_opacity, expected_alpha) in [(0.0, 0), (0.25, 32), (1.0, 128)] {
            let settings = StickPadCursors {
                appearance: CursorAppearance::Ring,
                opacity: 0.5,
                ring_fill_opacity: fill_opacity,
                ..Default::default()
            };
            let Some(Shape::Circle(circle)) = cursor_shape(Pos2::ZERO, COLOR, None, &settings)
            else {
                panic!("expected circle");
            };
            assert_eq!(circle.fill.a(), expected_alpha);
            assert_eq!(circle.stroke.color.a(), 128);
        }
    }

    #[test]
    fn ring_fill_color_is_independent_and_overall_opacity_scales_both() {
        const FILL_COLOR: [u8; 4] = [255, 0, 0, 128];
        for (opacity, outline_alpha, fill_alpha) in
            [(0.25, 64, 32), (0.5, 128, 64), (1.0, 255, 128)]
        {
            let settings = StickPadCursors {
                appearance: CursorAppearance::Ring,
                opacity,
                ring_fill_opacity: 1.0,
                ..Default::default()
            };
            let Some(Shape::Circle(circle)) =
                cursor_shape(Pos2::ZERO, COLOR, Some(FILL_COLOR), &settings)
            else {
                panic!("expected circle");
            };
            assert_eq!(circle.fill.a(), fill_alpha);
            assert_eq!(circle.stroke.color.a(), outline_alpha);
            assert!(circle.fill.r() > 0);
            assert_eq!(circle.fill.g(), 0);
            assert_eq!(circle.stroke.color.r(), 0);
            assert!(circle.stroke.color.g() > 0);
        }
    }

    #[test]
    fn transparent_outline_preserves_independent_fill() {
        let settings = StickPadCursors {
            appearance: CursorAppearance::Ring,
            ..Default::default()
        };
        let Some(Shape::Circle(circle)) = cursor_shape(Pos2::ZERO, [0; 4], Some(COLOR), &settings)
        else {
            panic!("expected circle");
        };
        assert_eq!(circle.stroke.color, Color32::TRANSPARENT);
        assert!(circle.fill.a() > 0);
    }

    #[test]
    fn fade_has_opaque_center_and_transparent_edge() {
        let settings = StickPadCursors {
            appearance: CursorAppearance::Fade,
            ..Default::default()
        };
        let Some(Shape::Mesh(mesh)) = cursor_shape(Pos2::ZERO, COLOR, None, &settings) else {
            panic!("expected mesh");
        };
        assert!(mesh.is_valid());
        assert_eq!(mesh.vertices[0].color.a(), 255);
        assert!(mesh.vertices[1..]
            .iter()
            .all(|vertex| vertex.color == Color32::TRANSPARENT));
    }

    #[test]
    fn invalid_or_invisible_cursors_are_not_painted() {
        for settings in [
            StickPadCursors {
                radius: f32::NAN,
                ..Default::default()
            },
            StickPadCursors {
                radius: -1.0,
                ..Default::default()
            },
            StickPadCursors {
                opacity: 0.0,
                ..Default::default()
            },
        ] {
            assert!(cursor_shape(Pos2::ZERO, COLOR, None, &settings).is_none());
        }
    }
}

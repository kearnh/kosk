use egui::{epaint::Mesh, Color32, Painter, Pos2, Shape, Stroke, Vec2};

use crate::config::{CursorAppearance, StickPadCursors};

const FADE_SEGMENTS: u32 = 64;

pub(super) fn draw(painter: &Painter, center: Pos2, color: [u8; 4], settings: &StickPadCursors) {
    if let Some(shape) = cursor_shape(center, color, settings) {
        painter.add(shape);
    }
}

fn cursor_shape(center: Pos2, color: [u8; 4], settings: &StickPadCursors) -> Option<Shape> {
    if !settings.radius.is_finite()
        || !settings.opacity.is_finite()
        || !settings.ring_thickness.is_finite()
    {
        return None;
    }

    let radius = settings.radius.max(0.0);
    let color = crate::theme::color(color).gamma_multiply(settings.opacity.clamp(0.0, 1.0));
    if radius == 0.0 || color == Color32::TRANSPARENT {
        return None;
    }

    let shape = match settings.appearance {
        CursorAppearance::Solid => Shape::circle_filled(center, radius, color),
        CursorAppearance::Ring => {
            let thickness = settings.ring_thickness.clamp(0.0, radius);
            Shape::circle_stroke(
                center,
                radius - thickness * 0.5,
                Stroke::new(thickness, color),
            )
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

    #[test]
    fn solid_uses_configured_radius_and_opacity() {
        let settings = StickPadCursors {
            radius: 12.0,
            opacity: 0.5,
            ..Default::default()
        };
        let Some(Shape::Circle(circle)) = cursor_shape(Pos2::ZERO, COLOR, &settings) else {
            panic!("expected circle");
        };
        assert_eq!(circle.radius, 12.0);
        assert_eq!(circle.fill.a(), 128);
    }

    #[test]
    fn ring_keeps_center_transparent_and_outer_radius() {
        let settings = StickPadCursors {
            appearance: CursorAppearance::Ring,
            radius: 12.0,
            ring_thickness: 3.0,
            ..Default::default()
        };
        let Some(Shape::Circle(circle)) = cursor_shape(Pos2::ZERO, COLOR, &settings) else {
            panic!("expected circle");
        };
        assert_eq!(circle.fill, Color32::TRANSPARENT);
        assert_eq!(circle.stroke.width, 3.0);
        assert_eq!(circle.radius + circle.stroke.width * 0.5, 12.0);
    }

    #[test]
    fn fade_has_opaque_center_and_transparent_edge() {
        let settings = StickPadCursors {
            appearance: CursorAppearance::Fade,
            ..Default::default()
        };
        let Some(Shape::Mesh(mesh)) = cursor_shape(Pos2::ZERO, COLOR, &settings) else {
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
            assert!(cursor_shape(Pos2::ZERO, COLOR, &settings).is_none());
        }
    }
}

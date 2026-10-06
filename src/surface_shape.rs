use anyhow::{bail, Result};
use egui::{Color32, Mesh, Pos2, Rect, Stroke};
use serde::{Deserialize, Serialize};

const CURVE_SEGMENTS: usize = 12;

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum CornerStyle {
    #[default]
    Square,
    Rounded,
    Beveled,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
struct RoundedCorners {
    corner_radius: f32,
    top_left: Option<f32>,
    top_right: Option<f32>,
    bottom_right: Option<f32>,
    bottom_left: Option<f32>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
struct BeveledCorners {
    corner_size: f32,
    top_left: Option<f32>,
    top_right: Option<f32>,
    bottom_right: Option<f32>,
    bottom_left: Option<f32>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct SurfaceCorners {
    corner_style: CornerStyle,
    rounded_corners: Option<RoundedCorners>,
    beveled_corners: Option<BeveledCorners>,
}

impl SurfaceCorners {
    fn sizes(&self) -> [f32; 4] {
        match self.corner_style {
            CornerStyle::Rounded => self.rounded_corners.as_ref().map(|c| {
                [c.top_left, c.top_right, c.bottom_right, c.bottom_left]
                    .map(|value| value.unwrap_or(c.corner_radius))
            }),
            CornerStyle::Beveled => self.beveled_corners.as_ref().map(|c| {
                [c.top_left, c.top_right, c.bottom_right, c.bottom_left]
                    .map(|value| value.unwrap_or(c.corner_size))
            }),
            CornerStyle::Square => None,
        }
        .unwrap_or([0.0; 4])
    }

    pub(crate) fn validate(&self) -> Result<()> {
        if (self.rounded_corners.is_some() && self.corner_style != CornerStyle::Rounded)
            || (self.beveled_corners.is_some() && self.corner_style != CornerStyle::Beveled)
        {
            bail!("corner parameters must match corner_style");
        }
        let mut values = self.sizes().to_vec();
        if let Some(corners) = &self.rounded_corners {
            values.push(corners.corner_radius);
        }
        if let Some(corners) = &self.beveled_corners {
            values.push(corners.corner_size);
        }
        if values
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            bail!("corner sizes must be finite and nonnegative");
        }
        Ok(())
    }

    pub(crate) fn content_margin(&self) -> egui::Margin {
        let size = self.sizes().into_iter().fold(0.0, f32::max);
        egui::Margin::same((size * 0.5).ceil().clamp(5.0, i8::MAX as f32) as i8)
    }

    pub(crate) fn outline(&self, rect: Rect) -> Vec<Pos2> {
        if !rect.is_positive() {
            return Vec::new();
        }
        let [tl, tr, br, bl] = self.sizes();
        let scale = [
            rect.width() / (tl + tr),
            rect.width() / (bl + br),
            rect.height() / (tl + bl),
            rect.height() / (tr + br),
        ]
        .into_iter()
        .fold(1.0, f32::min);
        let sizes = [tl, tr, br, bl].map(|size| size * scale);
        let corners = [
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
        ];
        let offsets = [
            egui::vec2(1.0, 1.0),
            egui::vec2(-1.0, 1.0),
            egui::vec2(-1.0, -1.0),
            egui::vec2(1.0, -1.0),
        ];
        let mut points = Vec::new();
        for (index, ((corner, offset), radius)) in
            corners.into_iter().zip(offsets).zip(sizes).enumerate()
        {
            if radius == 0.0 {
                points.push(corner);
                continue;
            }
            let center = corner + offset * radius;
            let segments = if self.corner_style == CornerStyle::Rounded {
                CURVE_SEGMENTS
            } else {
                1
            };
            let start = std::f32::consts::PI + index as f32 * std::f32::consts::FRAC_PI_2;
            for step in 0..=segments {
                let angle = start + step as f32 / segments as f32 * std::f32::consts::FRAC_PI_2;
                points.push(center + egui::vec2(angle.cos(), angle.sin()) * radius);
            }
        }
        points.dedup_by(|a, b| a.distance_sq(*b) < 0.0001);
        if points.len() > 1 && points[0].distance_sq(*points.last().unwrap()) < 0.0001 {
            points.pop();
        }
        points
    }

    pub(crate) fn paint(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        fill: Option<Color32>,
        stroke: Stroke,
    ) {
        let points = self.outline(rect);
        if points.len() < 3 {
            return;
        }
        if let Some(fill) = fill {
            painter.add(egui::Shape::convex_polygon(
                points.clone(),
                fill,
                Stroke::NONE,
            ));
        }
        if stroke.width <= 0.0 || stroke.color == Color32::TRANSPARENT {
            return;
        }
        let Some(inner) = inset(&points, stroke.width) else {
            painter.add(egui::Shape::convex_polygon(
                points,
                stroke.color,
                Stroke::NONE,
            ));
            return;
        };
        let count = points.len() as u32;
        let mut mesh = Mesh::default();
        for point in points.iter().chain(&inner) {
            mesh.colored_vertex(*point, stroke.color);
        }
        for index in 0..count {
            let next = (index + 1) % count;
            mesh.add_triangle(index, next, next + count);
            mesh.add_triangle(index, next + count, index + count);
        }
        painter.add(mesh);
    }
}

fn cross(a: egui::Vec2, b: egui::Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}

fn contains(points: &[Pos2], point: Pos2) -> bool {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .all(|(a, b)| cross(*b - *a, point - *a) >= -0.001)
}

fn inset(points: &[Pos2], width: f32) -> Option<Vec<Pos2>> {
    let mut inner = Vec::with_capacity(points.len());
    for index in 0..points.len() {
        let previous = points[(index + points.len() - 1) % points.len()];
        let current = points[index];
        let next = points[(index + 1) % points.len()];
        let incoming = (current - previous).normalized();
        let outgoing = (next - current).normalized();
        let first = current + egui::vec2(-incoming.y, incoming.x) * width;
        let second = current + egui::vec2(-outgoing.y, outgoing.x) * width;
        let divisor = cross(incoming, outgoing);
        let point = if divisor.abs() < 0.00001 {
            first
        } else {
            first + incoming * (cross(second - first, outgoing) / divisor)
        };
        if !contains(points, point) {
            return None;
        }
        inner.push(point);
    }
    let area: f32 = inner
        .iter()
        .zip(inner.iter().cycle().skip(1))
        .map(|(a, b)| a.x * b.y - a.y * b.x)
        .sum();
    (area > 0.0).then_some(inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cuts(text: &str) -> SurfaceCorners {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn asymmetric_cuts_clamp_and_leave_content_inside() {
        let corners =
            cuts("corner_style = 'beveled'\n[beveled_corners]\ncorner_size = 12\nbottom_left = 0");
        corners.validate().unwrap();
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(100.0, 60.0));
        let outline = corners.outline(rect);
        assert_eq!(outline.len(), 7);
        assert!(!contains(&outline, egui::pos2(1.0, 1.0)));
        assert!(contains(&outline, egui::pos2(1.0, 59.0)));
        assert!(contains(&outline, rect.center()));
        let tiny = corners.outline(Rect::from_min_size(Pos2::ZERO, egui::vec2(6.0, 4.0)));
        assert!(tiny.iter().all(|point| point.x >= -0.001
            && point.x <= 6.001
            && point.y >= -0.001
            && point.y <= 4.001));
    }

    #[test]
    fn diagonal_borders_have_constant_perpendicular_width() {
        let corners = cuts("corner_style = 'beveled'\n[beveled_corners]\ncorner_size = 12");
        let outline = corners.outline(Rect::from_min_size(Pos2::ZERO, egui::vec2(100.0, 60.0)));
        let inner = inset(&outline, 3.0).unwrap();
        for index in 0..outline.len() {
            let edge = (outline[(index + 1) % outline.len()] - outline[index]).normalized();
            assert!((cross(edge, inner[index] - outline[index]) - 3.0).abs() < 0.001);
        }
        assert!(inset(&outline, 100.0).is_none());
    }

    #[test]
    fn square_round_zero_and_invalid_parameters() {
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(100.0, 60.0));
        assert_eq!(SurfaceCorners::default().outline(rect).len(), 4);
        assert_eq!(cuts("corner_style = 'beveled'").outline(rect).len(), 4);
        let round = cuts("corner_style = 'rounded'\n[rounded_corners]\ncorner_radius = 10");
        assert!(round.outline(rect).len() > 8);
        assert!(!contains(&round.outline(rect), Pos2::ZERO));
        for text in [
            "corner_style='beveled'\n[rounded_corners]\ncorner_radius=4",
            "corner_style='square'\n[beveled_corners]\ncorner_size=4",
            "corner_style='beveled'\n[beveled_corners]\ncorner_size=-1",
            "corner_style='rounded'\n[rounded_corners]\ncorner_radius=nan",
        ] {
            assert!(cuts(text).validate().is_err());
        }
        assert!(toml::from_str::<SurfaceCorners>("corner_style='unknown'").is_err());
    }
}

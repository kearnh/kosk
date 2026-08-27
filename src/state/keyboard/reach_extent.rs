use std::cmp::Ordering;

use egui::Pos2;

use crate::controller::pad_origin::stretch_stick;
use crate::controller::virtual_ctl::StickSide;
use crate::controller::warp;

use super::layout::KeyboardLayout;

const STICK_SAMPLE_COUNT: usize = 64;
const SQUARE_GRID_SIZE: usize = 33;
const ORIGIN_GRID_SIZE: usize = 9;
const SAFE_ORIGIN_LIMIT: f32 = 0.9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputDomain {
    UnitCircle,
    UnitSquare,
}

#[derive(Debug, Clone)]
pub(crate) struct ReachEnvelope {
    pub fill_hull: Vec<Pos2>,
    pub outline: Vec<Pos2>,
}

pub(crate) fn compute_stick_envelope(
    layout: &KeyboardLayout,
    side: StickSide,
    stick_warp: f32,
) -> Option<ReachEnvelope> {
    layout.captured_centres.as_ref()?;

    let points = sample_domain(InputDomain::UnitCircle)
        .into_iter()
        .map(|raw| {
            let warped = warp(raw, stick_warp);
            cursor_point(layout, side, warped)
        })
        .collect();

    Some(build_envelope(layout, side, points))
}

pub(crate) fn compute_pad_envelope(
    layout: &KeyboardLayout,
    side: StickSide,
    stick_warp: f32,
    stretch_k: f32,
    stretch_max_gain: f32,
) -> Option<ReachEnvelope> {
    compute_pad_envelope_with_origin_range(
        layout,
        side,
        stick_warp,
        stretch_k,
        stretch_max_gain,
        -1.0,
        1.0,
    )
}

pub(crate) fn compute_safe_pad_envelope(
    layout: &KeyboardLayout,
    side: StickSide,
    stick_warp: f32,
    stretch_k: f32,
    stretch_max_gain: f32,
) -> Option<ReachEnvelope> {
    compute_pad_envelope_with_origin_range(
        layout,
        side,
        stick_warp,
        stretch_k,
        stretch_max_gain,
        -SAFE_ORIGIN_LIMIT,
        SAFE_ORIGIN_LIMIT,
    )
}

fn compute_pad_envelope_with_origin_range(
    layout: &KeyboardLayout,
    side: StickSide,
    stick_warp: f32,
    stretch_k: f32,
    stretch_max_gain: f32,
    origin_min: f32,
    origin_max: f32,
) -> Option<ReachEnvelope> {
    layout.captured_centres.as_ref()?;

    let origins = if stretch_k == 0.0 {
        vec![(0.0, 0.0)]
    } else {
        sample_grid(origin_min, origin_max)
    };
    let raw_samples = sample_domain(InputDomain::UnitSquare);
    let mut points = Vec::with_capacity(origins.len() * raw_samples.len());

    for origin in origins {
        for raw in &raw_samples {
            let stretched = stretch_stick(*raw, origin, stretch_k, stretch_max_gain);
            let warped = warp(stretched, stick_warp);
            points.push(cursor_point(layout, side, warped));
        }
    }

    Some(build_envelope(layout, side, points))
}

fn cursor_point(layout: &KeyboardLayout, side: StickSide, stick: (f32, f32)) -> Pos2 {
    let (x, y) = match side {
        StickSide::Left => layout.stick_to_cursor_left(stick),
        StickSide::Right => layout.stick_to_cursor_right(stick),
    };
    Pos2::new(x, y)
}

fn build_envelope(layout: &KeyboardLayout, side: StickSide, points: Vec<Pos2>) -> ReachEnvelope {
    let fill_hull = convex_hull(points);
    let mut outline = fill_hull.clone();
    let rest = layout.stick_center(side);
    outline.sort_by(|a, b| {
        let angle_a = (a.y - rest.1).atan2(a.x - rest.0);
        let angle_b = (b.y - rest.1).atan2(b.x - rest.0);
        angle_a.partial_cmp(&angle_b).unwrap_or(Ordering::Equal)
    });
    ReachEnvelope { fill_hull, outline }
}

fn sample_domain(domain: InputDomain) -> Vec<(f32, f32)> {
    match domain {
        InputDomain::UnitCircle => {
            let mut points = Vec::with_capacity(STICK_SAMPLE_COUNT + 1);
            for index in 0..STICK_SAMPLE_COUNT {
                let angle = std::f32::consts::TAU * index as f32 / STICK_SAMPLE_COUNT as f32;
                points.push((angle.cos(), angle.sin()));
            }
            points.push((0.0, 0.0));
            points
        }
        InputDomain::UnitSquare => {
            let step = 2.0 / (SQUARE_GRID_SIZE as f32 - 1.0);
            let values = (0..SQUARE_GRID_SIZE)
                .map(|index| -1.0 + step * index as f32)
                .collect::<Vec<_>>();
            let mut points = Vec::with_capacity(SQUARE_GRID_SIZE * SQUARE_GRID_SIZE);
            for &x in &values {
                for &y in &values {
                    if x != 0.0 || y != 0.0 {
                        points.push((x, y));
                    }
                }
            }
            points.push((0.0, 0.0));
            points
        }
    }
}

fn sample_grid(min: f32, max: f32) -> Vec<(f32, f32)> {
    let step = (max - min) / (ORIGIN_GRID_SIZE as f32 - 1.0);
    let values: Vec<f32> = (0..ORIGIN_GRID_SIZE)
        .map(|index| min + step * index as f32)
        .collect();
    values
        .iter()
        .flat_map(|&x| values.iter().map(move |&y| (x, y)))
        .collect()
}

fn convex_hull(mut points: Vec<Pos2>) -> Vec<Pos2> {
    points.sort_by(|a, b| {
        a.x.partial_cmp(&b.x)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.y.partial_cmp(&b.y).unwrap_or(Ordering::Equal))
    });
    points.dedup_by(|a, b| a.x == b.x && a.y == b.y);
    if points.len() <= 2 {
        return points;
    }

    let mut lower = Vec::new();
    for point in &points {
        while lower.len() >= 2
            && cross(lower[lower.len() - 2], lower[lower.len() - 1], *point) <= 0.0
        {
            lower.pop();
        }
        lower.push(*point);
    }

    let mut upper = Vec::new();
    for point in points.iter().rev() {
        while upper.len() >= 2
            && cross(upper[upper.len() - 2], upper[upper.len() - 1], *point) <= 0.0
        {
            upper.pop();
        }
        upper.push(*point);
    }

    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

fn cross(origin: Pos2, a: Pos2, b: Pos2) -> f32 {
    (a.x - origin.x) * (b.y - origin.y) - (a.y - origin.y) * (b.x - origin.x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_layout() -> KeyboardLayout {
        let toml = r#"
stick_rest_left = [0, 0]
stick_rest_right = [0, 0]
[[rows]]
indent = 0.0
keys = [{ key = "a" }]
"#;
        let mut layout = KeyboardLayout::load_with_scales(toml, 1.0, 1.0, 1.0, 1.0).unwrap();
        layout.update_geometry(vec![vec![Some(Pos2::new(10.0, 20.0))]]);
        layout
    }

    fn bounded_test_layout() -> KeyboardLayout {
        let toml = r#"
stick_rest_left = [0, 0]
stick_rest_right = [0, 0]
[stick_bounds]
left = [{ min = { x = -0.5, y = -0.5 }, max = { x = 0.5, y = 0.5 } }]
right = [{ min = { x = -0.5, y = -0.5 }, max = { x = 0.5, y = 0.5 } }]
[[rows]]
indent = 0.0
keys = [{ key = "a" }]
"#;
        let mut layout = KeyboardLayout::load_with_scales(toml, 1.0, 1.0, 1.0, 1.0).unwrap();
        layout.update_geometry(vec![vec![Some(Pos2::new(10.0, 20.0))]]);
        layout
    }

    #[test]
    fn circle_samples_include_center_and_unit_radius() {
        let points = sample_domain(InputDomain::UnitCircle);
        assert_eq!(points.len(), STICK_SAMPLE_COUNT + 1);
        assert_eq!(points.last(), Some(&(0.0, 0.0)));
        assert!(points[..STICK_SAMPLE_COUNT]
            .iter()
            .all(|(x, y)| ((x * x + y * y) - 1.0).abs() < 1e-5));
    }

    #[test]
    fn square_samples_include_center_and_edges() {
        let points = sample_domain(InputDomain::UnitSquare);
        assert_eq!(points.len(), SQUARE_GRID_SIZE * SQUARE_GRID_SIZE);
        assert_eq!(points.last(), Some(&(0.0, 0.0)));
        assert!(points[..points.len() - 1]
            .iter()
            .all(|(x, y)| x.abs() <= 1.0 && y.abs() <= 1.0));
        assert!(points.contains(&(-1.0, -1.0)));
        assert!(points.contains(&(1.0, -1.0)));
        assert!(points.contains(&(1.0, 1.0)));
        assert!(points.contains(&(-1.0, 1.0)));
    }

    #[test]
    fn square_samples_cover_the_interior() {
        let points = sample_domain(InputDomain::UnitSquare);
        assert!(points
            .iter()
            .any(|(x, y)| x.abs() < 1.0 && y.abs() < 1.0 && (*x, *y) != (0.0, 0.0)));
    }

    #[test]
    fn convex_hull_discards_interior_points() {
        let hull = convex_hull(vec![
            Pos2::new(-1.0, -1.0),
            Pos2::new(1.0, -1.0),
            Pos2::new(1.0, 1.0),
            Pos2::new(-1.0, 1.0),
            Pos2::new(0.0, 0.0),
        ]);
        assert_eq!(hull.len(), 4);
    }

    #[test]
    fn envelope_outline_contains_only_hull_vertices() {
        let layout = test_layout();
        let envelope = build_envelope(
            &layout,
            StickSide::Left,
            vec![
                Pos2::new(9.0, 19.0),
                Pos2::new(11.0, 19.0),
                Pos2::new(11.0, 21.0),
                Pos2::new(9.0, 21.0),
                Pos2::new(10.0, 20.0),
            ],
        );
        assert_eq!(envelope.outline.len(), envelope.fill_hull.len());
    }

    #[test]
    fn pad_envelope_ignores_layout_bounds() {
        let layout = bounded_test_layout();
        let envelope = compute_pad_envelope(&layout, StickSide::Left, 0.0, 0.0, 1.5).unwrap();
        assert!(envelope.fill_hull.contains(&Pos2::new(11.0, 21.0)));
    }

    #[test]
    fn stick_envelope_follows_warped_domain() {
        let layout = test_layout();
        let circle = compute_stick_envelope(&layout, StickSide::Left, 0.0).unwrap();
        assert!(circle
            .fill_hull
            .iter()
            .all(|point| (point.x - 10.0).abs() <= 1.0 + 1e-5));
        assert!(circle
            .fill_hull
            .iter()
            .all(|point| (point.y - 20.0).abs() <= 1.0 + 1e-5));

        let square = compute_stick_envelope(&layout, StickSide::Left, 1.0).unwrap();
        assert!(square
            .fill_hull
            .iter()
            .any(|point| (point.x - 9.0).abs() < 1e-5 && (point.y - 19.0).abs() < 1e-5));
        assert!(square
            .fill_hull
            .iter()
            .any(|point| (point.x - 11.0).abs() < 1e-5 && (point.y - 21.0).abs() < 1e-5));
    }

    #[test]
    fn zero_stretch_pad_matches_square_stick_domain() {
        let layout = test_layout();
        let stick = compute_stick_envelope(&layout, StickSide::Right, 1.0).unwrap();
        let pad = compute_pad_envelope(&layout, StickSide::Right, 1.0, 0.0, 1.5).unwrap();
        assert_eq!(stick.fill_hull, pad.fill_hull);
    }

    #[test]
    fn safe_origin_grid_is_inside_full_origin_grid() {
        let full = sample_grid(-1.0, 1.0);
        let safe = sample_grid(-SAFE_ORIGIN_LIMIT, SAFE_ORIGIN_LIMIT);
        assert!(safe
            .iter()
            .all(|point| full.contains(point) || point.0.abs() < 1.0));
        assert!(safe.iter().all(|(x, y)| {
            (-SAFE_ORIGIN_LIMIT..=SAFE_ORIGIN_LIMIT).contains(x)
                && (-SAFE_ORIGIN_LIMIT..=SAFE_ORIGIN_LIMIT).contains(y)
        }));
    }
}

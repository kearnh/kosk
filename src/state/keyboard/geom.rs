//! Pure stick/hit helpers shared by live layout and MCP geometry snapshots.

use serde::Serialize;

/// Circle/ellipse key hit region (live layout + MCP snapshot).
#[derive(Debug, Clone, Serialize)]
pub enum KeyHitBox {
    Circle { x: f32, y: f32, r: f32 },
    Ellipse { x: f32, y: f32, rx: f32, ry: f32 },
}

impl KeyHitBox {
    /// Rim-fraction score when `(x,y)` is inside (`0` at centre, `1` at rim).
    pub fn contains(&self, x: f32, y: f32) -> Option<f32> {
        match self {
            KeyHitBox::Circle {
                x: kx,
                y: ky,
                r: kr,
            } => circle_score(x, y, *kx, *ky, *kr),
            KeyHitBox::Ellipse {
                x: kx,
                y: ky,
                rx,
                ry,
            } => ellipse_score(x, y, *kx, *ky, *rx, *ry),
        }
    }
}

pub(crate) fn circle_score(x: f32, y: f32, kx: f32, ky: f32, r: f32) -> Option<f32> {
    let distance_sq = (x - kx).powi(2) + (y - ky).powi(2);
    let r_sq = r * r;
    if distance_sq > r_sq {
        return None;
    }
    if r_sq <= f32::EPSILON {
        return Some(0.0);
    }
    Some(distance_sq / r_sq)
}

pub(crate) fn ellipse_score(x: f32, y: f32, kx: f32, ky: f32, rx: f32, ry: f32) -> Option<f32> {
    let dx = x - kx;
    let dy = y - ky;
    let val = (dx * dx) / (rx * rx) + (dy * dy) / (ry * ry);
    if val <= 1.0 {
        Some(val)
    } else {
        None
    }
}

/// Map stick (−1..1) to screen cursor given rest point and scales.
pub(crate) fn stick_to_cursor(
    rest: (f32, f32),
    scale_x: f32,
    scale_y: f32,
    stick_scale_x: f32,
    stick_scale_y: f32,
    stick: (f32, f32),
) -> (f32, f32) {
    let (rx, ry) = rest;
    let (sx, sy) = stick;
    (
        rx + sx * scale_x * stick_scale_x,
        ry + sy * scale_y * stick_scale_y,
    )
}

/// Clamp cursor to the nearest AABB if outside all bounds. Empty → unchanged.
pub(crate) fn clamp_cursor_to_aabbs(
    cursor: (f32, f32),
    bounds: &[(f32, f32, f32, f32)],
) -> (f32, f32) {
    if bounds.is_empty() {
        return cursor;
    }

    let (x, y) = cursor;
    let inside = bounds
        .iter()
        .any(|&(min_x, min_y, max_x, max_y)| x >= min_x && x <= max_x && y >= min_y && y <= max_y);
    if inside {
        return cursor;
    }

    let mut closest = cursor;
    let mut min_dist_sq = f32::MAX;
    for &(min_x, min_y, max_x, max_y) in bounds {
        let clamped_x = x.clamp(min_x, max_x);
        let clamped_y = y.clamp(min_y, max_y);
        let dist_sq = (x - clamped_x).powi(2) + (y - clamped_y).powi(2);
        if dist_sq < min_dist_sq {
            min_dist_sq = dist_sq;
            closest = (clamped_x, clamped_y);
        }
    }
    closest
}

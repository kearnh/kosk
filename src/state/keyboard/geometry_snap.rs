//! Immutable keyboard geometry snapshot for MCP / control-server queries.
//!
//! Published from the UI path after first-draw centre capture. Control threads
//! must read this session only — never `keyboard::with_mut`.

use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;

use crate::controller::virtual_ctl::StickSide;
use crate::state::keyboard::key::RawKey;
use crate::state::keyboard::keyboard_action::KeyboardAction;
use crate::state::keyboard::layout::KeyboardLayout;
use crate::state::StateId;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct SnapRect {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

#[derive(Debug, Clone, Serialize)]
pub enum SnapHitBox {
    Circle { x: f32, y: f32, r: f32 },
    Ellipse { x: f32, y: f32, rx: f32, ry: f32 },
}

impl SnapHitBox {
    /// Distance-like score when `(x,y)` is inside, matching live `HitBox::contains`.
    pub fn contains(&self, x: f32, y: f32) -> Option<f32> {
        match self {
            SnapHitBox::Circle {
                x: kx,
                y: ky,
                r: kr,
            } => {
                let distance_sq = (x - kx).powi(2) + (y - ky).powi(2);
                if distance_sq <= kr.powi(2) {
                    Some(distance_sq)
                } else {
                    None
                }
            }
            SnapHitBox::Ellipse {
                x: kx,
                y: ky,
                rx,
                ry,
            } => {
                let dx = x - kx;
                let dy = y - ky;
                let val = (dx * dx) / (rx * rx) + (dy * dy) / (ry * ry);
                if val <= 1.0 {
                    Some(val)
                } else {
                    None
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SnapKey {
    pub row: usize,
    pub col: usize,
    pub id: String,
    pub selectable: bool,
    pub centre: Option<(f32, f32)>,
    pub hitbox: Option<SnapHitBox>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct StickAabb {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct GeometrySnapshot {
    pub layout: String,
    pub revision: u64,
    pub scale_x: f32,
    pub scale_y: f32,
    pub stick_scale_x: f32,
    pub stick_scale_y: f32,
    pub left_rest: (f32, f32),
    pub right_rest: (f32, f32),
    pub left_bounds: Vec<SnapRect>,
    pub right_bounds: Vec<SnapRect>,
    pub keys: Vec<SnapKey>,
}

pub fn session() -> Arc<Mutex<Option<GeometrySnapshot>>> {
    static SESSION: OnceLock<Arc<Mutex<Option<GeometrySnapshot>>>> = OnceLock::new();
    SESSION.get_or_init(|| Arc::new(Mutex::new(None))).clone()
}

/// Publish a fresh snapshot from a layout that already has captured centres.
/// Callers must not touch `session().lock()` for publish/clear themselves.
pub fn publish(layout_name: &str, layout: &KeyboardLayout) {
    if layout.captured_centres.is_none() {
        clear();
        return;
    }
    let cell = session();
    let mut guard = cell.lock().expect("geometry snapshot lock poisoned");
    let revision = guard.as_ref().map(|p| p.revision + 1).unwrap_or(1);
    *guard = Some(layout.export_geometry(layout_name, revision));
}

/// Drop the published snapshot (idempotent).
pub fn clear() {
    let cell = session();
    let mut guard = cell.lock().expect("geometry snapshot lock poisoned");
    *guard = None;
}

fn to_camel(pascal: &str) -> String {
    let mut chars = pascal.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_lowercase().chain(chars).collect(),
    }
}

fn state_wire_id(state: StateId) -> String {
    let pascal = serde_plain::to_string(&state).unwrap_or_else(|_| format!("{state:?}"));
    to_camel(&pascal)
}

fn action_wire_id(action: &KeyboardAction) -> String {
    use KeyboardAction::*;
    match action {
        SendKeyUnderLeftStick => "sendKeyUnderLeftStick".into(),
        SendKeyUnderRightStick => "sendKeyUnderRightStick".into(),
        SendKey(c) => {
            let payload = match *c {
                ' ' => "space".to_owned(),
                '\n' => "enter".to_owned(),
                '\t' => "tab".to_owned(),
                other => other.to_string(),
            };
            format!("sendKey.{payload}")
        }
        SendEnigoKey(k) => {
            let payload = match k {
                enigo::Key::Backspace => "backspace".to_owned(),
                enigo::Key::Delete => "delete".to_owned(),
                enigo::Key::LeftArrow => "left".to_owned(),
                enigo::Key::RightArrow => "right".to_owned(),
                enigo::Key::UpArrow => "up".to_owned(),
                enigo::Key::DownArrow => "down".to_owned(),
                other => serde_plain::to_string(other).unwrap_or_else(|_| format!("{other:?}")),
            };
            format!("sendKey.{payload}")
        }
        ToggleShift => "toggleShift".into(),
        ToggleCtrl => "toggleCtrl".into(),
        ToggleAlt => "toggleAlt".into(),
        Paste => "paste".into(),
        SwitchState(s) => format!("switchState.{}", state_wire_id(*s)),
        SwitchLayout(name) => format!("switchLayout.{name}"),
        FlipWindowLeftRight => "flipWindowLeftRight".into(),
        FlipWindowAboveBelow => "flipWindowAboveBelow".into(),
        RotateWindow => "rotateWindow".into(),
        Exit => "exit".into(),
        ToggleRecord => "toggleRecord".into(),
    }
}

/// Stable wire id for agents; `None` for `Skip` (omit from snapshot keys).
pub(crate) fn wire_id(key: &RawKey) -> Option<String> {
    match key {
        RawKey::Skip => None,
        RawKey::Key(c) => Some(c.to_string()),
        RawKey::Text(s) => Some(s.clone()),
        RawKey::Enigo(k) => Some(serde_plain::to_string(k).unwrap_or_else(|_| format!("{k:?}"))),
        RawKey::Action(a) => Some(action_wire_id(a)),
    }
}

impl GeometrySnapshot {
    fn rest(&self, side: StickSide) -> (f32, f32) {
        match side {
            StickSide::Left => self.left_rest,
            StickSide::Right => self.right_rest,
        }
    }

    fn bounds(&self, side: StickSide) -> &[SnapRect] {
        match side {
            StickSide::Left => &self.left_bounds,
            StickSide::Right => &self.right_bounds,
        }
    }

    /// Unclamped inverse of `stick_to_cursor_*`.
    pub fn stick_for_centre_raw(&self, side: StickSide, cx: f32, cy: f32) -> (f32, f32) {
        let (rx, ry) = self.rest(side);
        let sx = (cx - rx) / (self.scale_x * self.stick_scale_x);
        let sy = (cy - ry) / (self.scale_y * self.stick_scale_y);
        (sx, sy)
    }

    pub fn stick_for_centre(&self, side: StickSide, cx: f32, cy: f32) -> (f32, f32) {
        let (sx, sy) = self.stick_for_centre_raw(side, cx, cy);
        (sx.clamp(-1.0, 1.0), sy.clamp(-1.0, 1.0))
    }

    pub fn stick_to_cursor(&self, side: StickSide, stick: (f32, f32)) -> (f32, f32) {
        let (rx, ry) = self.rest(side);
        let (sx, sy) = stick;
        (
            rx + sx * self.scale_x * self.stick_scale_x,
            ry + sy * self.scale_y * self.stick_scale_y,
        )
    }

    pub fn stick_aabb_for_hitbox(&self, side: StickSide, hb: &SnapHitBox) -> Option<StickAabb> {
        let (min_cx, max_cx, min_cy, max_cy) = match hb {
            SnapHitBox::Circle { x, y, r } => (x - r, x + r, y - r, y + r),
            SnapHitBox::Ellipse { x, y, rx, ry } => (x - rx, x + rx, y - ry, y + ry),
        };
        let corners = [
            (min_cx, min_cy),
            (max_cx, min_cy),
            (min_cx, max_cy),
            (max_cx, max_cy),
        ];
        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut min_y = f32::MAX;
        let mut max_y = f32::MIN;
        for (cx, cy) in corners {
            let (sx, sy) = self.stick_for_centre_raw(side, cx, cy);
            min_x = min_x.min(sx);
            max_x = max_x.max(sx);
            min_y = min_y.min(sy);
            max_y = max_y.max(sy);
        }
        min_x = min_x.max(-1.0);
        max_x = max_x.min(1.0);
        min_y = min_y.max(-1.0);
        max_y = max_y.min(1.0);
        if min_x > max_x || min_y > max_y {
            None
        } else {
            Some(StickAabb {
                min_x,
                max_x,
                min_y,
                max_y,
            })
        }
    }

    pub fn lookup_key(
        &self,
        key: &str,
        row: Option<usize>,
        col: Option<usize>,
    ) -> Option<&SnapKey> {
        match (row, col) {
            (Some(r), Some(c)) => self.keys.iter().find(|k| k.row == r && k.col == c),
            (None, None) => self.keys.iter().find(|k| k.id == key),
            // Partial row/col alone is not enough — require id match as well.
            (Some(r), None) => self.keys.iter().find(|k| k.id == key && k.row == r),
            (None, Some(c)) => self.keys.iter().find(|k| k.id == key && k.col == c),
        }
    }

    fn clamp_cursor_to_bounds(x: f32, y: f32, bounds: &[SnapRect]) -> (f32, f32) {
        if bounds.is_empty() {
            return (x, y);
        }
        let inside = bounds
            .iter()
            .any(|r| x >= r.min_x && x <= r.max_x && y >= r.min_y && y <= r.max_y);
        if inside {
            return (x, y);
        }
        let mut closest = (x, y);
        let mut min_dist_sq = f32::MAX;
        for r in bounds {
            let clamped_x = x.clamp(r.min_x, r.max_x);
            let clamped_y = y.clamp(r.min_y, r.max_y);
            let dist_sq = (x - clamped_x).powi(2) + (y - clamped_y).powi(2);
            if dist_sq < min_dist_sq {
                min_dist_sq = dist_sq;
                closest = (clamped_x, clamped_y);
            }
        }
        closest
    }

    pub fn nearest_key(&self, side: StickSide, stick: (f32, f32)) -> Option<&SnapKey> {
        let (cx, cy) = self.stick_to_cursor(side, stick);
        let (cursor_x, cursor_y) = Self::clamp_cursor_to_bounds(cx, cy, self.bounds(side));
        let mut best: Option<(f32, &SnapKey)> = None;
        for key in &self.keys {
            let Some(hb) = key.hitbox.as_ref() else {
                continue;
            };
            if let Some(score) = hb.contains(cursor_x, cursor_y) {
                if best.as_ref().is_none_or(|(s, _)| score < *s) {
                    best = Some((score, key));
                }
            }
        }
        best.map(|(_, k)| k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Pos2;

    fn sample_snap() -> GeometrySnapshot {
        GeometrySnapshot {
            layout: "main".into(),
            revision: 1,
            scale_x: 30.0,
            scale_y: 32.0,
            stick_scale_x: 3.0,
            stick_scale_y: 2.5,
            left_rest: (100.0, 100.0),
            right_rest: (300.0, 100.0),
            left_bounds: vec![SnapRect {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 200.0,
                max_y: 200.0,
            }],
            right_bounds: Vec::new(),
            keys: vec![
                SnapKey {
                    row: 0,
                    col: 1,
                    id: "d".into(),
                    selectable: true,
                    centre: Some((100.0, 100.0)),
                    hitbox: Some(SnapHitBox::Circle {
                        x: 100.0,
                        y: 100.0,
                        r: 33.75,
                    }),
                },
                SnapKey {
                    row: 0,
                    col: 2,
                    id: "e".into(),
                    selectable: true,
                    centre: Some((190.0, 100.0)),
                    hitbox: Some(SnapHitBox::Circle {
                        x: 190.0,
                        y: 100.0,
                        r: 33.75,
                    }),
                },
            ],
        }
    }

    #[test]
    fn rest_inverse_is_zero_and_aabb_contains_centre() {
        let snap = sample_snap();
        let (sx, sy) = snap.stick_for_centre(StickSide::Left, snap.left_rest.0, snap.left_rest.1);
        assert!((sx).abs() < 1e-5 && (sy).abs() < 1e-5);

        let e = snap.lookup_key("e", None, None).expect("e");
        let (ex, ey) =
            snap.stick_for_centre(StickSide::Left, e.centre.unwrap().0, e.centre.unwrap().1);
        let nearest = snap.nearest_key(StickSide::Left, (ex, ey));
        assert_eq!(nearest.map(|k| k.id.as_str()), Some("e"));

        let aabb = snap
            .stick_aabb_for_hitbox(StickSide::Left, e.hitbox.as_ref().unwrap())
            .expect("aabb");
        assert!(aabb.min_x <= ex && ex <= aabb.max_x);
        assert!(aabb.min_y <= ey && ey <= aabb.max_y);
    }

    #[test]
    fn publish_export_clear_roundtrip() {
        let toml = r#"
[[rows]]
indent = 0.0
items = [
  { key = "d", width = 1.0 },
  { key = "e", width = 1.0 },
]
"#;
        let mut layout = KeyboardLayout::load_with_scales(toml, 30.0, 32.0, 3.0, 2.5).unwrap();
        layout.update_geometry(vec![vec![
            Some(Pos2::new(100.0, 100.0)),
            Some(Pos2::new(190.0, 100.0)),
        ]]);
        publish("main", &layout);
        let cell = session();
        {
            let guard = cell.lock().unwrap();
            let snap = guard.as_ref().expect("published");
            assert_eq!(snap.layout, "main");
            assert!(snap.revision >= 1);
            assert!(snap.lookup_key("d", None, None).is_some());
            assert!(snap.lookup_key("e", None, None).is_some());
        }
        clear();
        assert!(cell.lock().unwrap().is_none());
    }
}

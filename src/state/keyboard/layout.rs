use std::{collections::HashMap, fs, path::Path};

use anyhow::Result;
use egui::{Color32, Context, Pos2, Rect, Ui, Vec2};
use serde::Deserialize;

use crate::{
    config,
    debug::DebugPlugin,
    state::keyboard::key::{Key, RawKey},
};

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct UnscaledPixelUnitY(f32);

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct UnscaledPixelUnitX(f32);

impl From<f32> for UnscaledPixelUnitX {
    fn from(value: f32) -> Self {
        Self(value)
    }
}

impl From<f32> for UnscaledPixelUnitY {
    fn from(value: f32) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct KeyButton {
    display: Option<Key<String>>,
    key: Key<RawKey>,
    pub pos: (usize, usize),
    pub width: UnscaledPixelUnitX,
}

impl KeyButton {
    pub fn display(&self, shifted: bool) -> String {
        if let Some(d) = &self.display {
            d.get(shifted)
        } else {
            self.key.get(shifted).to_string()
        }
    }

    pub fn is_skip(&self) -> bool {
        self.key.normal == RawKey::Skip
    }

    pub fn is_key<K: ?Sized>(&self, shifted: bool, key: &K) -> bool
    where
        RawKey: PartialEq<K>,
    {
        self.key.get(shifted) == *key
    }

    pub fn key(&self, shifted: bool) -> RawKey {
        self.key.get(shifted)
    }
}

#[derive(Debug, Default, Clone, Deserialize)]
struct StickBounds {
    left: Vec<Rect>,
    right: Vec<Rect>,
}

#[derive(Debug, Clone, Deserialize)]
struct KeyboardLayoutFile {
    keys: Vec<KeyButton>,
    row_indents: Vec<f32>,
    #[serde(default)]
    row_heights: Vec<f32>,
    #[serde(default = "default_pad_x")]
    pad_x: f32,
    #[serde(default = "default_pad_y")]
    pad_y: f32,
    #[serde(default = "default_font_size")]
    font_size: f32,
    #[serde(default)]
    stick_bounds: StickBounds,
}

fn default_pad_x() -> f32 {
    2.0
}
fn default_pad_y() -> f32 {
    2.0
}
fn default_font_size() -> f32 {
    18.0
}

#[derive(Debug)]
enum HitBox {
    Circle { x: f32, y: f32, r: f32 },
    Ellipse { x: f32, y: f32, rx: f32, ry: f32 },
}

impl HitBox {
    fn contains(&self, x: f32, y: f32) -> Option<f32> {
        match self {
            HitBox::Circle {
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
            HitBox::Ellipse {
                x: kx,
                y: ky,
                rx,
                ry,
            } => {
                let dx = x - kx;
                let dy = y - ky;
                let val = (dx * dx) / (rx * rx) + (dy * dy) / (ry * ry);
                if val <= 1.0 {
                    Some(val) // Return a value proportional to distance for the nearest-key logic
                } else {
                    None
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct KeyboardLayout {
    row_indents: Vec<UnscaledPixelUnitX>,
    row_heights: Vec<UnscaledPixelUnitY>,
    rows: Vec<Vec<KeyButton>>,

    // Layout constants
    pub font_size: f32,
    scale_x: f32,
    scale_y: f32,
    pub pad_x: UnscaledPixelUnitX,
    pub pad_y: UnscaledPixelUnitY,

    // Stick scaling and centers
    stick_scale_x: f32,
    stick_scale_y: f32,
    left_stick_center: (f32, f32),
    right_stick_center: (f32, f32),

    // (x, y, r) where (x, y) is center of key, and r is a radius. This will not overlap exactly
    // with a key button, and key circles may overlap each other. Key selection for highlight will
    // use closest center, will key press will return all overlapping keys to allow typo resistance,
    // i.e. may decide on key press based on engligh word etc.
    key_hit_boxes: Vec<Vec<Option<HitBox>>>,

    pub left_stick_bounds: Vec<Rect>,
    pub right_stick_bounds: Vec<Rect>,

    pub captured_centres: Option<Vec<Vec<Option<Pos2>>>>,
}

impl KeyboardLayout {
    fn load(toml: &str) -> Result<Self> {
        let layout: KeyboardLayoutFile = toml::from_str(toml)?;

        // Group keys by row
        let mut row_map: HashMap<usize, Vec<KeyButton>> = HashMap::new();
        for key in &layout.keys {
            row_map.entry(key.pos.0).or_default().push(key.clone());
        }

        // Find max row number
        let max_row = layout.keys.iter().map(|k| k.pos.0).max().unwrap_or(0);

        // Check that row_indents length matches number of rows
        if layout.row_indents.len() != max_row + 1 {
            anyhow::bail!(
                "row_indents length ({}) doesn't match number of rows ({})",
                layout.row_indents.len(),
                max_row + 1
            );
        }

        // Check for duplicate (row, col) pairs (SKIP keys are allowed to duplicate)
        let mut seen = std::collections::HashSet::new();
        for key in &layout.keys {
            if !seen.insert(key.pos) {
                anyhow::bail!("Duplicate key at row {}, col {}", key.pos.0, key.pos.1);
            }
        }

        // Sort by row number and store
        let mut sorted_rows: Vec<_> = row_map.into_iter().collect();
        sorted_rows.sort_by_key(|(row_num, _)| *row_num);

        // Process each row: sort by col and insert spacers for gaps
        let rows = sorted_rows
            .into_iter()
            .map(|(_, mut keys)| {
                // Sort by column
                keys.sort_by_key(|k| k.pos.1);

                // Insert spacers for gaps
                let mut result = Vec::new();
                let mut expected_col = 0;

                for key in keys {
                    // Insert spacer(s) for gap
                    while expected_col < key.pos.1 {
                        result.push(KeyButton {
                            display: None,
                            key: Key {
                                normal: RawKey::Skip,
                                shift: None,
                            },
                            pos: (key.pos.0, expected_col),
                            width: 1.0.into(),
                        });
                        expected_col += 1;
                    }

                    result.push(key.clone());
                    expected_col += 1;
                }

                result
            })
            .collect();

        let cfg = config::get();

        let scale: Vec2 = (cfg.scale_x, cfg.scale_y).into();

        let layout = KeyboardLayout {
            row_indents: layout.row_indents.into_iter().map(Into::into).collect(),
            row_heights: layout.row_heights.into_iter().map(Into::into).collect(),
            rows,
            font_size: layout.font_size,
            scale_x: cfg.scale_x,
            scale_y: cfg.scale_y,
            pad_x: layout.pad_x.into(),
            pad_y: layout.pad_y.into(),
            stick_scale_x: cfg.stick_scale_x,
            stick_scale_y: cfg.stick_scale_y,
            left_stick_center: (0.0, 0.0),
            right_stick_center: (0.0, 0.0),
            key_hit_boxes: Default::default(),
            left_stick_bounds: layout
                .stick_bounds
                .left
                .into_iter()
                .map(|r| {
                    r.translate(r.center().to_vec2() * scale)
                        .scale_from_center2(scale)
                })
                .collect(),
            right_stick_bounds: layout
                .stick_bounds
                .right
                .into_iter()
                .map(|r| {
                    r.translate(r.center().to_vec2() * scale)
                        .scale_from_center2(scale)
                })
                .collect(),
            captured_centres: None,
        };

        Ok(layout)
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::load(&fs::read_to_string(path)?)
    }

    pub fn scale_x(&self, val: UnscaledPixelUnitX) -> f32 {
        self.scale_x * val.0
    }

    pub fn scale_y(&self, val: UnscaledPixelUnitY) -> f32 {
        self.scale_y * val.0
    }

    pub fn get_key_center(&self, key_name: &str) -> Option<(f32, f32)> {
        for (row_idx, row) in self.rows.iter().enumerate() {
            for (col_idx, key_button) in row.iter().enumerate() {
                if key_button.key.normal == key_name {
                    if let Some(centres) = &self.captured_centres {
                        let pos = centres[row_idx][col_idx]?;
                        return Some((pos.x, pos.y));
                    }
                }
            }
        }
        None
    }

    pub fn calculate_hitboxes(&mut self) {
        let centres = match &self.captured_centres {
            Some(c) => c,
            None => return,
        };

        let mut key_hit_boxes = Vec::new();

        for (row_idx, row) in self.rows.iter().enumerate() {
            let mut hitboxes_row = Vec::new();
            if row.is_empty() {
                key_hit_boxes.push(hitboxes_row);
                continue;
            }

            let height = self.row_heights.get(row_idx).copied().unwrap_or(1.0.into());
            let row_height = self.scale_y(height);

            for (col_idx, key) in row.iter().enumerate() {
                if let Some(Some(pos)) = centres.get(row_idx).and_then(|r| r.get(col_idx)) {
                    let center_x = pos.x;
                    let center_y = pos.y;

                    if key.key.normal != RawKey::Skip
                        && key.key.normal != RawKey::Shift
                        && key.key.normal != " "
                    {
                        let width = self.scale_x(key.width);
                        if width / row_height >= 1.2 {
                            hitboxes_row.push(Some(HitBox::Ellipse {
                                x: center_x,
                                y: center_y,
                                rx: (width / 2.0) * std::f32::consts::SQRT_2,
                                ry: (row_height / 2.0) * std::f32::consts::SQRT_2,
                            }));
                        } else {
                            let radius = self.scale_x * 1.125;
                            hitboxes_row.push(Some(HitBox::Circle {
                                x: center_x,
                                y: center_y,
                                r: radius,
                            }));
                        }
                    } else {
                        hitboxes_row.push(None);
                    }
                } else {
                    hitboxes_row.push(None);
                }
            }

            key_hit_boxes.push(hitboxes_row);
        }

        self.key_hit_boxes = key_hit_boxes;
    }

    pub fn update_geometry(&mut self, captured_centres: Vec<Vec<Option<Pos2>>>) {
        self.captured_centres = Some(captured_centres);
        self.calculate_hitboxes();
        self.left_stick_center = self.get_key_center("d").unwrap_or_default();
        self.right_stick_center = self.get_key_center("k").unwrap_or_default();
    }

    pub fn stick_to_cursor_left(&self, stick: (f32, f32)) -> (f32, f32) {
        let (x, y) = stick;
        let dx = self.scale_x(x.into()) * self.stick_scale_x;
        let dy = self.scale_y(y.into()) * self.stick_scale_y;
        (self.left_stick_center.0 + dx, self.left_stick_center.1 + dy)
    }

    pub fn stick_to_cursor_right(&self, stick: (f32, f32)) -> (f32, f32) {
        let (x, y) = stick;
        let dx = self.scale_x(x.into()) * self.stick_scale_x;
        let dy = self.scale_y(y.into()) * self.stick_scale_y;
        (
            self.right_stick_center.0 + dx,
            self.right_stick_center.1 + dy,
        )
    }

    fn get_nearest_key_with_bounds(
        &self,
        cursor: (f32, f32),
        bounds: &[Rect],
        shift_state: bool,
    ) -> Option<RawKey> {
        let (x, y) = cursor;
        let mut cursor_x = x;
        let mut cursor_y = y;
        if !bounds.is_empty() {
            // Check if we are already inside any of the bounds
            let is_inside = bounds
                .iter()
                .any(|r| r.contains((cursor_x, cursor_y).into()));
            if !is_inside {
                // We are outside all bounds. Find the closest point on the closest rectangle.
                let mut closest_point = (cursor_x, cursor_y);
                let mut min_dist_sq = f32::MAX;
                for r in bounds {
                    // Clamp the cursor to the individual rectangle to find the closest point on it
                    let clamped_x = cursor_x.clamp(r.min.x, r.max.x);
                    let clamped_y = cursor_y.clamp(r.min.y, r.max.y);
                    let dist_sq = (cursor_x - clamped_x).powi(2) + (cursor_y - clamped_y).powi(2);
                    if dist_sq < min_dist_sq {
                        min_dist_sq = dist_sq;
                        closest_point = (clamped_x, clamped_y);
                    }
                }
                (cursor_x, cursor_y) = closest_point;
            }
        }
        self.get_key_at(cursor_x, cursor_y, shift_state)
    }

    pub fn get_nearest_key_left(&self, stick: (f32, f32), shift_state: bool) -> Option<RawKey> {
        let cursor = self.stick_to_cursor_left(stick);
        self.get_nearest_key_with_bounds(cursor, &self.left_stick_bounds, shift_state)
    }

    pub fn get_nearest_key_right(&self, stick: (f32, f32), shift_state: bool) -> Option<RawKey> {
        let cursor = self.stick_to_cursor_right(stick);
        self.get_nearest_key_with_bounds(cursor, &self.right_stick_bounds, shift_state)
    }

    pub fn get_key_at(&self, x: f32, y: f32, shifted: bool) -> Option<RawKey> {
        let mut candidate = (f32::MAX, None);
        for (row_idx, row) in self.key_hit_boxes.iter().enumerate() {
            for (col_idx, h) in row.iter().enumerate() {
                if let Some(h) = h {
                    if let Some(d) = h.contains(x, y) {
                        if d < candidate.0 {
                            let key_button = &self.rows[row_idx][col_idx];
                            candidate = (d, Some(key_button.key.get(shifted)));
                        }
                    }
                }
            }
        }
        candidate.1
    }

    pub fn draw_debug(&self, ctx: &Context, ui: &mut Ui) {
        if let Some(debug) = config::get().debug {
            let painter = ui.painter();

            if debug.show_stick_cursors {
                let d_lock = ctx.plugin::<DebugPlugin>();
                let d = d_lock.lock();
                if let Some(input) = &d.controller_input {
                    let (x, y) = input.left_stick();
                    let (cursor_x, cursor_y) = self.stick_to_cursor_left((x, y));
                    painter.circle_filled(
                        [cursor_x, cursor_y].into(),
                        8.0,
                        Color32::from_rgb(0, 0, 255),
                    );

                    let (x, y) = input.right_stick();
                    let (cursor_x, cursor_y) = self.stick_to_cursor_right((x, y));
                    painter.circle_filled(
                        [cursor_x, cursor_y].into(),
                        8.0,
                        Color32::from_rgb(0, 255, 0),
                    );
                }
            }

            if debug.show_stick_bounds {
                // Draw left stick bounds in blue
                for r in &self.left_stick_bounds {
                    painter.rect_stroke(
                        *r,
                        egui::CornerRadius::default(),
                        egui::Stroke::new(1.0, Color32::from_rgba_premultiplied(0, 0, 255, 255)),
                        egui::StrokeKind::Middle,
                    );
                }

                // Draw right stick bounds in green
                for r in &self.right_stick_bounds {
                    painter.rect_stroke(
                        *r,
                        egui::CornerRadius::default(),
                        egui::Stroke::new(1.0, Color32::from_rgba_premultiplied(0, 255, 0, 255)),
                        egui::StrokeKind::Middle,
                    );
                }
            }
            if debug.show_hitboxes {
                for row in &self.key_hit_boxes {
                    for hitbox in row.iter().flatten() {
                        match hitbox {
                            HitBox::Circle { x, y, r } => {
                                painter.circle_stroke(
                                    [*x, *y].into(),
                                    *r,
                                    egui::Stroke::new(
                                        1.0,
                                        Color32::from_rgba_premultiplied(0, 192, 255, 128),
                                    ),
                                );
                            }
                            HitBox::Ellipse { x, y, rx, ry } => {
                                painter.add(egui::epaint::EllipseShape::stroke(
                                    [*x, *y].into(),
                                    Vec2::new(*rx, *ry),
                                    egui::Stroke::new(
                                        1.0,
                                        Color32::from_rgba_premultiplied(0, 192, 255, 128),
                                    ),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
}

impl<'a> IntoIterator for &'a KeyboardLayout {
    type Item = (&'a Vec<KeyButton>, UnscaledPixelUnitX, UnscaledPixelUnitY);

    type IntoIter = std::iter::Map<
        std::iter::Zip<
            std::iter::Zip<
                std::slice::Iter<'a, Vec<KeyButton>>,
                std::slice::Iter<'a, UnscaledPixelUnitX>,
            >,
            std::slice::Iter<'a, UnscaledPixelUnitY>,
        >,
        fn(
            (
                (&'a Vec<KeyButton>, &'a UnscaledPixelUnitX),
                &'a UnscaledPixelUnitY,
            ),
        ) -> Self::Item,
    >;

    fn into_iter(self) -> Self::IntoIter {
        self.rows
            .iter()
            .zip(self.row_indents.iter())
            .zip(self.row_heights.iter())
            .map(|((row, &indent), &height)| (row, indent, height))
    }
}

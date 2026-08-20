use std::fmt;
use std::{fs, path::Path};

use anyhow::Result;
use egui::{Color32, Context, Pos2, Rect, Ui, Vec2};
use serde::de::{self, SeqAccess, Visitor};
use serde::Deserialize;

use crate::{
    config,
    debug::DebugPlugin,
    state::keyboard::key::{Key, RawKey},
    state::keyboard::when::{DisplayContext, WhenExpr},
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

#[derive(Debug, Clone, Copy)]
struct RgbaColor(Color32);

impl From<RgbaColor> for Color32 {
    fn from(c: RgbaColor) -> Self {
        c.0
    }
}

impl<'de> Deserialize<'de> for RgbaColor {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ColorVisitor;

        impl<'de> Visitor<'de> for ColorVisitor {
            type Value = RgbaColor;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an [r, g, b] or [r, g, b, a] array of 0-255")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let r: u8 = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let g: u8 = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                let b: u8 = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(2, &self))?;
                let a: u8 = seq.next_element()?.unwrap_or(255);
                if seq.next_element::<u8>()?.is_some() {
                    return Err(de::Error::invalid_length(5, &self));
                }
                Ok(RgbaColor(Color32::from_rgba_unmultiplied(r, g, b, a)))
            }
        }

        deserializer.deserialize_seq(ColorVisitor)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct DisplayRule {
    text: String,
    #[serde(default)]
    when: Option<WhenExpr>,
    #[serde(default, alias = "button_colour")]
    button_color: Option<RgbaColor>,
    #[serde(default, alias = "text_colour")]
    text_color: Option<RgbaColor>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum KeyDisplay {
    Constant(String),
    Rules(Vec<DisplayRule>),
    Shifted(Key<String>),
}

#[derive(Debug, Clone)]
pub struct KeyAppearance {
    pub text: String,
    pub button_color: Option<Color32>,
    pub text_color: Option<Color32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KeyButton {
    #[serde(default)]
    display: Option<KeyDisplay>,
    key: Key<RawKey>,

    #[serde(default = "default_selectable")]
    selectable: bool,

    #[serde(default)]
    pub display_modifiers: bool,

    #[serde(default = "default_key_width_unit")]
    pub width: UnscaledPixelUnitX,
    pub font_size: Option<f32>,
}

fn default_selectable() -> bool {
    true
}

fn default_key_width_unit() -> UnscaledPixelUnitX {
    1.0.into()
}

impl KeyButton {
    fn fallback_text(&self, shifted: bool) -> String {
        match self.key.get(shifted) {
            RawKey::Key(c) => c.to_string(),
            _ => "?".to_string(), // no way to display this, user should define a display for it
        }
    }

    pub fn appearance(&self, ctx: &DisplayContext) -> KeyAppearance {
        match &self.display {
            None => KeyAppearance {
                text: self.fallback_text(ctx.shift),
                button_color: None,
                text_color: None,
            },
            Some(KeyDisplay::Constant(s)) => KeyAppearance {
                text: s.clone(),
                button_color: None,
                text_color: None,
            },
            Some(KeyDisplay::Shifted(d)) => KeyAppearance {
                text: d.get(ctx.shift),
                button_color: None,
                text_color: None,
            },
            Some(KeyDisplay::Rules(rules)) => {
                for rule in rules {
                    if rule.when.as_ref().is_none_or(|w| w.eval(ctx)) {
                        return KeyAppearance {
                            text: rule.text.clone(),
                            button_color: rule.button_color.map(Into::into),
                            text_color: rule.text_color.map(Into::into),
                        };
                    }
                }
                KeyAppearance {
                    text: self.fallback_text(ctx.shift),
                    button_color: None,
                    text_color: None,
                }
            }
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

fn default_row_height_unit() -> UnscaledPixelUnitY {
    1.0.into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct KeyboardRow {
    pub indent: UnscaledPixelUnitX,
    #[serde(default = "default_row_height_unit")]
    pub height: UnscaledPixelUnitY,
    pub keys: Vec<KeyButton>,
}

#[derive(Debug, Default, Clone, Deserialize)]
struct StickBounds {
    left: Vec<Rect>,
    right: Vec<Rect>,
}

#[derive(Debug, Clone, Deserialize)]
struct KeyboardLayoutFile {
    rows: Vec<KeyboardRow>,
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
    pub rows: Vec<KeyboardRow>,

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

    key_hit_boxes: Vec<Vec<Option<HitBox>>>,

    pub left_stick_bounds: Vec<Rect>,
    pub right_stick_bounds: Vec<Rect>,

    pub captured_centres: Option<Vec<Vec<Option<Pos2>>>>,

    /// Original TOML, for input-tape headers.
    source: String,
}

impl KeyboardLayout {
    fn load(toml: &str) -> Result<Self> {
        let cfg = config::get();
        Self::load_with_scales(
            toml,
            cfg.scale_x,
            cfg.scale_y,
            cfg.stick_scale_x,
            cfg.stick_scale_y,
        )
    }

    pub(crate) fn load_with_scales(
        toml: &str,
        scale_x: f32,
        scale_y: f32,
        stick_scale_x: f32,
        stick_scale_y: f32,
    ) -> Result<Self> {
        let parsed: KeyboardLayoutFile = toml::from_str(toml)?;

        let KeyboardLayoutFile {
            rows,
            pad_x,
            pad_y,
            font_size,
            stick_bounds,
        } = parsed;

        let scale: Vec2 = (scale_x, scale_y).into();

        let layout = KeyboardLayout {
            rows,
            font_size,
            scale_x,
            scale_y,
            pad_x: pad_x.into(),
            pad_y: pad_y.into(),
            stick_scale_x,
            stick_scale_y,
            left_stick_center: (0.0, 0.0),
            right_stick_center: (0.0, 0.0),
            key_hit_boxes: Default::default(),
            left_stick_bounds: stick_bounds
                .left
                .into_iter()
                .map(|r| {
                    r.translate(r.center().to_vec2() * scale)
                        .scale_from_center2(scale)
                })
                .collect(),
            right_stick_bounds: stick_bounds
                .right
                .into_iter()
                .map(|r| {
                    r.translate(r.center().to_vec2() * scale)
                        .scale_from_center2(scale)
                })
                .collect(),
            captured_centres: None,
            source: toml.to_owned(),
        };

        Ok(layout)
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::load(&fs::read_to_string(path)?)
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub fn scale_x(&self, val: UnscaledPixelUnitX) -> f32 {
        self.scale_x * val.0
    }

    pub fn scale_y(&self, val: UnscaledPixelUnitY) -> f32 {
        self.scale_y * val.0
    }

    pub fn get_key_center(&self, key: &RawKey) -> Option<(f32, f32)> {
        for (row_idx, row) in self.rows.iter().enumerate() {
            for (col_idx, key_button) in row.keys.iter().enumerate() {
                if key_button.key.normal == *key {
                    if let Some(centres) = &self.captured_centres {
                        let pos = centres[row_idx][col_idx]?;
                        return Some((pos.x, pos.y));
                    }
                }
            }
        }
        None
    }

    fn calculate_hitboxes(&mut self) {
        let centres = match &self.captured_centres {
            Some(c) => c,
            None => return,
        };

        let mut key_hit_boxes = Vec::new();

        for (row_idx, row) in self.rows.iter().enumerate() {
            let mut hitboxes_row = Vec::new();
            if row.keys.is_empty() {
                key_hit_boxes.push(hitboxes_row);
                continue;
            }

            let row_height = self.scale_y(row.height);

            for (col_idx, key) in row.keys.iter().enumerate() {
                if let Some(Some(pos)) = centres.get(row_idx).and_then(|r| r.get(col_idx)) {
                    let center_x = pos.x;
                    let center_y = pos.y;

                    if key.key.normal != RawKey::Skip && key.selectable {
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
        // FIXME make centers configurable
        self.left_stick_center = self.get_key_center(&RawKey::Key('d')).unwrap_or_default();
        self.right_stick_center = self.get_key_center(&RawKey::Key('k')).unwrap_or_default();
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
                            let key_button = &self.rows[row_idx].keys[col_idx];
                            candidate = (d, Some(key_button.key.get(shifted)));
                        }
                    }
                }
            }
        }
        candidate.1
    }

    pub fn draw_debug(&self, ctx: &Context, _: &mut Ui) {
        if let Some(debug) = config::get().debug {
            let painter = ctx.debug_painter();

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

    type IntoIter =
        std::iter::Map<std::slice::Iter<'a, KeyboardRow>, fn(&'a KeyboardRow) -> Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        fn project(row: &KeyboardRow) -> (&Vec<KeyButton>, UnscaledPixelUnitX, UnscaledPixelUnitY) {
            (&row.keys, row.indent, row.height)
        }
        self.rows.iter().map(project as fn(_) -> _)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parses_repo_layout_toml() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        for path in ["qwerty.toml", "old_steam_controller_kb.toml"] {
            let s = fs::read_to_string(dir.join(path)).unwrap();
            let _: KeyboardLayoutFile = toml::from_str(&s).unwrap();
        }
    }

    #[test]
    fn display_string_shifted_and_rules() {
        let constant: KeyButton = toml::from_str(
            r#"
            key = "exit"
            display = "Done"
            "#,
        )
        .unwrap();
        let shifted: KeyButton = toml::from_str(
            r#"
            key = { normal = "1", shift = "!" }
            display = { normal = "one", shift = "bang" }
            "#,
        )
        .unwrap();
        let rules: KeyButton = toml::from_str(
            r#"
            key = "toggleRecord"
            display = [
                { text = "stop", when = "recording", button_color = [220, 40, 40, 255], text_color = [255, 255, 255] },
                { text = "rec" },
            ]
            "#,
        )
        .unwrap();

        let off = DisplayContext::default();
        let on = DisplayContext {
            recording: true,
            ..DisplayContext::default()
        };
        let shifted_on = DisplayContext {
            shift: true,
            ..DisplayContext::default()
        };

        assert_eq!(constant.appearance(&off).text, "Done");
        assert_eq!(shifted.appearance(&off).text, "one");
        assert_eq!(shifted.appearance(&shifted_on).text, "bang");
        assert_eq!(rules.appearance(&off).text, "rec");
        let stop = rules.appearance(&on);
        assert_eq!(stop.text, "stop");
        assert_eq!(
            stop.button_color,
            Some(Color32::from_rgba_unmultiplied(220, 40, 40, 255))
        );
        assert_eq!(
            stop.text_color,
            Some(Color32::from_rgba_unmultiplied(255, 255, 255, 255))
        );
    }

    #[test]
    fn display_colour_aliases_and_first_match() {
        let key: KeyButton = toml::from_str(
            r#"
            key = "a"
            display = [
                { text = "A", when = "shift && recording", button_colour = [1, 2, 3], text_colour = [4, 5, 6, 7] },
                { text = "rec", when = "recording" },
                { text = "a" },
            ]
            "#,
        )
        .unwrap();

        let rec = DisplayContext {
            recording: true,
            ..DisplayContext::default()
        };
        let both = DisplayContext {
            shift: true,
            recording: true,
            ..DisplayContext::default()
        };

        assert_eq!(key.appearance(&DisplayContext::default()).text, "a");
        assert_eq!(key.appearance(&rec).text, "rec");
        let a = key.appearance(&both);
        assert_eq!(a.text, "A");
        assert_eq!(
            a.button_color,
            Some(Color32::from_rgba_unmultiplied(1, 2, 3, 255))
        );
        assert_eq!(
            a.text_color,
            Some(Color32::from_rgba_unmultiplied(4, 5, 6, 7))
        );
    }

    #[test]
    fn display_rules_fallback_when_none_match() {
        let key: KeyButton = toml::from_str(
            r#"
            key = "q"
            display = [
                { text = "stop", when = "recording" },
            ]
            "#,
        )
        .unwrap();
        assert_eq!(key.appearance(&DisplayContext::default()).text, "q");
        let shifted = DisplayContext {
            shift: true,
            ..DisplayContext::default()
        };
        assert_eq!(key.appearance(&shifted).text, "Q");
    }
}

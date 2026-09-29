use std::collections::HashMap;
use std::fmt;
use std::{fs, path::Path};

use anyhow::Result;
use egui::{Color32, Context, Painter, Pos2, Rect, Shape, Stroke, Ui, Vec2};
use serde::de::{self, SeqAccess, Visitor};
use serde::Deserialize;

use crate::{
    config,
    controller::{ControllerInput, ControllerKind, StickSide},
    debug::DebugPlugin,
    state::keyboard::key::{Key, RawKey},
    state::keyboard::when::{DisplayContext, WhenExpr},
};

use super::geom::{self, KeyHitBox};
use super::reach_extent::ReachEnvelope;

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
    #[serde(default)]
    pub align: ItemAlign,
}

fn default_selectable() -> bool {
    true
}

fn default_key_width_unit() -> UnscaledPixelUnitX {
    1.0.into()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemAlign {
    #[default]
    Left,
    Right,
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

#[derive(Debug, Clone, Deserialize)]
pub struct BatteryItem {
    #[serde(default = "default_key_width_unit")]
    pub width: UnscaledPixelUnitX,
    pub font_size: Option<f32>,
    #[serde(default)]
    pub align: ItemAlign,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectedControllerItem {
    #[serde(default = "default_key_width_unit")]
    pub width: UnscaledPixelUnitX,
    pub font_size: Option<f32>,
    #[serde(default)]
    pub align: ItemAlign,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum TypedRowItem {
    Key(KeyButton),
    Battery(BatteryItem),
    #[serde(rename = "connectedController")]
    ConnectedController(ConnectedControllerItem),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum RowItemDe {
    Typed(TypedRowItem),
    Key(KeyButton),
}

#[derive(Debug, Clone)]
pub enum RowItem {
    Key(KeyButton),
    Battery(BatteryItem),
    ConnectedController(ConnectedControllerItem),
}

impl<'de> Deserialize<'de> for RowItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match RowItemDe::deserialize(deserializer)? {
            RowItemDe::Typed(TypedRowItem::Key(k)) | RowItemDe::Key(k) => Self::Key(k),
            RowItemDe::Typed(TypedRowItem::Battery(b)) => Self::Battery(b),
            RowItemDe::Typed(TypedRowItem::ConnectedController(c)) => Self::ConnectedController(c),
        })
    }
}

impl RowItem {
    pub fn as_key(&self) -> Option<&KeyButton> {
        match self {
            Self::Key(k) => Some(k),
            Self::Battery(_) | Self::ConnectedController(_) => None,
        }
    }

    pub fn width(&self) -> UnscaledPixelUnitX {
        match self {
            Self::Key(k) => k.width,
            Self::Battery(b) => b.width,
            Self::ConnectedController(c) => c.width,
        }
    }

    pub fn align(&self) -> ItemAlign {
        match self {
            Self::Key(k) => k.align,
            Self::Battery(b) => b.align,
            Self::ConnectedController(c) => c.align,
        }
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
    #[serde(alias = "keys")]
    pub items: Vec<RowItem>,
}

#[derive(Debug, Default, Clone, Deserialize)]
struct StickBounds {
    left: Vec<Rect>,
    right: Vec<Rect>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum StickRestTable {
    Keys { row: usize, column: usize },
    Position { x: f32, y: f32 },
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(untagged)]
enum StickRest {
    Keys((usize, usize)),
    Table(StickRestTable),
}

impl StickRest {
    fn key_position(&self) -> Option<(usize, usize)> {
        match self {
            Self::Keys(position) => Some(*position),
            Self::Table(StickRestTable::Keys { row, column }) => Some((*row, *column)),
            Self::Table(StickRestTable::Position { .. }) => None,
        }
    }
}

#[derive(Debug)]
enum ReachCache {
    Stick {
        left: ReachEnvelope,
        right: ReachEnvelope,
    },
    Pad {
        left_all: ReachEnvelope,
        right_all: ReachEnvelope,
        left_safe: ReachEnvelope,
        right_safe: ReachEnvelope,
    },
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
    /// Rest centre for the left stick/pad.
    #[serde(default)]
    stick_rest_left: Option<StickRest>,
    /// Rest centre for the right stick/pad.
    #[serde(default)]
    stick_rest_right: Option<StickRest>,
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

fn validate_stick_rest(rows: &[KeyboardRow], rest: Option<&StickRest>, field: &str) -> Result<()> {
    let Some((row, col)) = rest.and_then(StickRest::key_position) else {
        return Ok(());
    };
    let Some(r) = rows.get(row) else {
        anyhow::bail!("{field}: row {row} out of range ({} rows)", rows.len());
    };
    let Some(item) = r.items.get(col) else {
        anyhow::bail!(
            "{field}: column {col} out of range (row {row} has {} items)",
            r.items.len()
        );
    };
    let Some(key) = item.as_key() else {
        anyhow::bail!("{field}: [{row}, {col}] points at a non-key item");
    };
    if key.is_skip() {
        anyhow::bail!("{field}: [{row}, {col}] points at a Skip key");
    }
    Ok(())
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

    // Aim ranges. `load_with_scales` copies one pair onto every profile.
    sc2_pad_scale: (f32, f32),
    sc2_stick_scale: (f32, f32),
    ps4_stick_scale: (f32, f32),
    aim_kind: ControllerKind,
    left_stick_center: (f32, f32),
    right_stick_center: (f32, f32),
    stick_rest_left: Option<StickRest>,
    stick_rest_right: Option<StickRest>,

    key_hit_boxes: Vec<Vec<Option<KeyHitBox>>>,

    pub left_stick_bounds: Vec<Rect>,
    pub right_stick_bounds: Vec<Rect>,
    left_stick_bounds_local: Vec<Rect>,
    right_stick_bounds_local: Vec<Rect>,
    geometry_shift: Vec2,

    pub captured_centres: Option<Vec<Vec<Option<Pos2>>>>,
    cursor_bias_left: (f32, f32),
    cursor_bias_right: (f32, f32),
    compensate_rest_on_capture: bool,

    reach_cache: Option<ReachCache>,

    /// Original TOML, for input-tape headers.
    source: String,
}

fn scale_stick_bounds(rects: Vec<Rect>, scale: Vec2) -> Vec<Rect> {
    rects
        .into_iter()
        .map(|r| {
            r.translate(r.center().to_vec2() * scale)
                .scale_from_center2(scale)
        })
        .collect()
}

fn point_in_rects(pos: Pos2, rects: &[Rect]) -> bool {
    rects.iter().any(|r| r.contains(pos))
}

fn first_centre(centres: &[Vec<Option<Pos2>>]) -> Option<Pos2> {
    first_centre_cell(centres).map(|(_, _, p)| p)
}

fn first_centre_cell(centres: &[Vec<Option<Pos2>>]) -> Option<(usize, usize, Pos2)> {
    for (ri, row) in centres.iter().enumerate() {
        for (ci, p) in row.iter().enumerate() {
            if let Some(pos) = p {
                return Some((ri, ci, *pos));
            }
        }
    }
    None
}

pub(crate) fn clamp_stick_cursor(cursor: (f32, f32), bounds: &[Rect]) -> (f32, f32) {
    let aabbs: Vec<_> = bounds
        .iter()
        .map(|r| (r.min.x, r.min.y, r.max.x, r.max.y))
        .collect();
    geom::clamp_cursor_to_aabbs(cursor, &aabbs)
}

impl KeyboardLayout {
    fn load(toml: &str) -> Result<Self> {
        let cfg = config::get();
        Self::load_with_profiles(
            toml,
            cfg.scale_x,
            cfg.scale_y,
            (cfg.sc2.pad.scale_x, cfg.sc2.pad.scale_y),
            (cfg.sc2.stick.scale_x, cfg.sc2.stick.scale_y),
            (cfg.ps4.stick.scale_x, cfg.ps4.stick.scale_y),
        )
    }

    /// One range for every profile. Tests use this.
    #[cfg(test)]
    pub(crate) fn load_with_scales(
        toml: &str,
        scale_x: f32,
        scale_y: f32,
        stick_scale_x: f32,
        stick_scale_y: f32,
    ) -> Result<Self> {
        let pair = (stick_scale_x, stick_scale_y);
        Self::load_with_profiles(toml, scale_x, scale_y, pair, pair, pair)
    }

    pub(crate) fn load_with_profiles(
        toml: &str,
        scale_x: f32,
        scale_y: f32,
        sc2_pad_scale: (f32, f32),
        sc2_stick_scale: (f32, f32),
        ps4_stick_scale: (f32, f32),
    ) -> Result<Self> {
        let parsed: KeyboardLayoutFile = toml::from_str(toml)?;

        let KeyboardLayoutFile {
            rows,
            pad_x,
            pad_y,
            font_size,
            stick_bounds,
            stick_rest_left,
            stick_rest_right,
        } = parsed;

        validate_stick_rest(&rows, stick_rest_left.as_ref(), "stick_rest_left")?;
        validate_stick_rest(&rows, stick_rest_right.as_ref(), "stick_rest_right")?;

        let scale: Vec2 = (scale_x, scale_y).into();
        let left_stick_bounds_local = scale_stick_bounds(stick_bounds.left, scale);
        let right_stick_bounds_local = scale_stick_bounds(stick_bounds.right, scale);

        let layout = KeyboardLayout {
            rows,
            font_size,
            scale_x,
            scale_y,
            pad_x: pad_x.into(),
            pad_y: pad_y.into(),
            sc2_pad_scale,
            sc2_stick_scale,
            ps4_stick_scale,
            aim_kind: ControllerKind::Sc2,
            left_stick_center: (0.0, 0.0),
            right_stick_center: (0.0, 0.0),
            stick_rest_left,
            stick_rest_right,
            key_hit_boxes: Default::default(),
            left_stick_bounds: left_stick_bounds_local.clone(),
            right_stick_bounds: right_stick_bounds_local.clone(),
            left_stick_bounds_local,
            right_stick_bounds_local,
            geometry_shift: Vec2::ZERO,
            captured_centres: None,
            cursor_bias_left: (0.0, 0.0),
            cursor_bias_right: (0.0, 0.0),
            compensate_rest_on_capture: false,
            reach_cache: None,
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

    pub(crate) fn cluster_width<'a>(
        &self,
        items: impl IntoIterator<Item = &'a RowItem>,
        pad_px: f32,
    ) -> f32 {
        let mut iter = items.into_iter();
        let Some(first) = iter.next() else {
            return 0.0;
        };
        iter.fold(self.scale_x(first.width()), |w, item| {
            w + pad_px + self.scale_x(item.width())
        })
    }

    /// Widest left-aligned row (indent + LTR items). RTL items sit against this edge.
    pub(crate) fn left_content_width(&self, pad_px: f32) -> f32 {
        self.rows
            .iter()
            .map(|row| {
                self.scale_x(row.indent)
                    + self.cluster_width(
                        row.items
                            .iter()
                            .filter(|item| item.align() == ItemAlign::Left),
                        pad_px,
                    )
            })
            .fold(0.0, f32::max)
    }

    /// Space after LTR so an RTL cluster of `right_w` ends at `content_width`.
    pub(crate) fn rtl_leading_gap(content_width: f32, left_used: f32, right_w: f32) -> f32 {
        (content_width - left_used - right_w).max(0.0)
    }

    pub fn get_key_center(&self, key: &RawKey) -> Option<(f32, f32)> {
        for (row_idx, row) in self.rows.iter().enumerate() {
            for (col_idx, item) in row.items.iter().enumerate() {
                let Some(key_button) = item.as_key() else {
                    continue;
                };
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
            if row.items.is_empty() {
                key_hit_boxes.push(hitboxes_row);
                continue;
            }

            let row_height = self.scale_y(row.height);

            for (col_idx, item) in row.items.iter().enumerate() {
                if let Some(Some(pos)) = centres.get(row_idx).and_then(|r| r.get(col_idx)) {
                    let center_x = pos.x;
                    let center_y = pos.y;

                    let selectable_key = item
                        .as_key()
                        .filter(|k| k.key.normal != RawKey::Skip && k.selectable);
                    if let Some(key) = selectable_key {
                        let width = self.scale_x(key.width);
                        if width / row_height >= 1.2 {
                            hitboxes_row.push(Some(KeyHitBox::Ellipse {
                                x: center_x,
                                y: center_y,
                                rx: (width / 2.0) * std::f32::consts::SQRT_2,
                                ry: (row_height / 2.0) * std::f32::consts::SQRT_2,
                            }));
                        } else {
                            let radius = self.scale_x * 1.125;
                            hitboxes_row.push(Some(KeyHitBox::Circle {
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
        if let Some((ri, ci, new)) = first_centre_cell(&captured_centres) {
            let old = self
                .captured_centres
                .as_ref()
                .and_then(|c| first_centre(c))
                .or_else(|| self.layout_local_centre(ri, ci));
            if let Some(old) = old {
                let delta = new - old;
                if self.captured_centres.is_none() {
                    // Replace provisional shift copied from another layout.
                    self.geometry_shift = delta;
                } else {
                    self.geometry_shift += delta;
                }
                self.refresh_shifted_bounds();
            }
        }

        self.reach_cache = None;
        self.captured_centres = Some(captured_centres);
        self.calculate_hitboxes();
        let old_left = self.left_stick_center;
        let old_right = self.right_stick_center;
        self.left_stick_center = self
            .centre_at_rest(self.stick_rest_left.as_ref())
            .unwrap_or_default();
        self.right_stick_center = self
            .centre_at_rest(self.stick_rest_right.as_ref())
            .unwrap_or_default();
        if self.compensate_rest_on_capture {
            self.cursor_bias_left.0 += old_left.0 - self.left_stick_center.0;
            self.cursor_bias_left.1 += old_left.1 - self.left_stick_center.1;
            self.cursor_bias_right.0 += old_right.0 - self.right_stick_center.0;
            self.cursor_bias_right.1 += old_right.1 - self.right_stick_center.1;
            self.compensate_rest_on_capture = false;
        }
        self.reach_cache = self.build_reach_cache();
    }

    pub(crate) fn typo_neighbors(&self) -> HashMap<char, Vec<char>> {
        const K: usize = 6;
        let Some(centres) = &self.captured_centres else {
            return HashMap::new();
        };

        let bounds_empty = self.left_stick_bounds.is_empty() && self.right_stick_bounds.is_empty();
        let mut letters: Vec<(char, Pos2, bool, bool)> = Vec::new();

        for (ri, row) in self.rows.iter().enumerate() {
            for (ci, item) in row.items.iter().enumerate() {
                let Some(key) = item.as_key() else {
                    continue;
                };
                if !key.selectable {
                    continue;
                }
                let RawKey::Key(ch) = key.key.get(false) else {
                    continue;
                };
                let Some(Some(pos)) = centres.get(ri).and_then(|r| r.get(ci)) else {
                    continue;
                };
                let folded = ch.to_lowercase().next().unwrap_or(ch);
                if !folded.is_alphanumeric() && folded != '\'' && folded != '_' {
                    continue;
                }
                let left = bounds_empty || point_in_rects(*pos, &self.left_stick_bounds);
                let right = bounds_empty || point_in_rects(*pos, &self.right_stick_bounds);
                if !left && !right {
                    letters.push((folded, *pos, true, true));
                } else {
                    letters.push((folded, *pos, left, right));
                }
            }
        }

        let mut map: HashMap<char, Vec<char>> = HashMap::new();
        for (i, (ch, pos, left, right)) in letters.iter().enumerate() {
            let mut dists: Vec<(f32, char)> = letters
                .iter()
                .enumerate()
                .filter(|(j, (other, _, ol, or))| {
                    *j != i && *other != *ch && ((*left && *ol) || (*right && *or))
                })
                .map(|(_, (other, op, _, _))| (pos.distance(*op), *other))
                .collect();
            dists.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut neigh = Vec::new();
            for (_, nch) in dists.into_iter().take(K) {
                if !neigh.contains(&nch) {
                    neigh.push(nch);
                }
            }
            map.entry(*ch).or_default().extend(neigh);
        }
        map
    }

    fn layout_local_centre(&self, row: usize, col: usize) -> Option<Pos2> {
        let pad_x = self.scale_x(self.pad_x);
        let pad_y = self.scale_y(self.pad_y);
        let mut y = 0.0;
        for (ri, row_data) in self.rows.iter().enumerate() {
            let h = self.scale_y(row_data.height);
            if ri != row {
                y += h + pad_y;
                continue;
            }
            let mut x = self.scale_x(row_data.indent);
            for (ci, item) in row_data.items.iter().enumerate() {
                let w = self.scale_x(item.width());
                if ci == col {
                    return Some(Pos2::new(x + w * 0.5, y + h * 0.5));
                }
                x += w + pad_x;
            }
            return None;
        }
        None
    }

    fn refresh_shifted_bounds(&mut self) {
        let shift = self.geometry_shift;
        self.left_stick_bounds = self
            .left_stick_bounds_local
            .iter()
            .copied()
            .map(|r| r.translate(shift))
            .collect();
        self.right_stick_bounds = self
            .right_stick_bounds_local
            .iter()
            .copied()
            .map(|r| r.translate(shift))
            .collect();
    }

    fn centre_at_rest(&self, rest: Option<&StickRest>) -> Option<(f32, f32)> {
        let rest = rest?;
        if let Some((row, col)) = rest.key_position() {
            let centres = self.captured_centres.as_ref()?;
            let pos = centres.get(row)?.get(col).copied()??;
            return Some((pos.x, pos.y));
        }

        let StickRest::Table(StickRestTable::Position { x, y }) = rest else {
            return None;
        };
        Some((
            self.scale_x((*x).into()) + self.geometry_shift.x,
            self.scale_y((*y).into()) + self.geometry_shift.y,
        ))
    }

    /// Reset centre/hitbox/rest fields. Call sites must use this (or
    /// `KeyboardState::on_layouts_changed`) â€” do not open-code `captured_centres = None`.
    pub fn clear_captured_geometry(&mut self) {
        self.captured_centres = None;
        self.key_hit_boxes.clear();
        self.left_stick_center = (0.0, 0.0);
        self.right_stick_center = (0.0, 0.0);
        self.geometry_shift = Vec2::ZERO;
        self.refresh_shifted_bounds();
        self.reach_cache = None;
        self.clear_cursor_bias();
    }

    pub(crate) fn stick_center(&self, side: StickSide) -> (f32, f32) {
        match side {
            StickSide::Left => self.left_stick_center,
            StickSide::Right => self.right_stick_center,
        }
    }

    /// Bias analog mapping by the previous layout's rest. Recapture on next draw.
    pub(crate) fn continue_origin_from(&mut self, from: &Self) {
        let left = from.mapping_rest(StickSide::Left);
        let right = from.mapping_rest(StickSide::Right);
        self.clear_captured_geometry();
        self.cursor_bias_left = left;
        self.cursor_bias_right = right;
        self.compensate_rest_on_capture = true;
    }

    pub(crate) fn clear_cursor_bias(&mut self) {
        self.cursor_bias_left = (0.0, 0.0);
        self.cursor_bias_right = (0.0, 0.0);
        self.compensate_rest_on_capture = false;
    }

    fn build_reach_cache(&self) -> Option<ReachCache> {
        let cfg = config::try_get()?;
        let overlay = cfg.debug.reach_overlay;
        let stick = cfg.aim(self.aim_kind, config::AimSurface::Stick);
        let pad = cfg.aim(ControllerKind::Sc2, config::AimSurface::Pad);

        match overlay {
            config::ReachOverlay::None => None,
            config::ReachOverlay::Stick => Some(ReachCache::Stick {
                left: super::reach_extent::compute_stick_envelope(
                    self,
                    StickSide::Left,
                    stick.warp,
                )?,
                right: super::reach_extent::compute_stick_envelope(
                    self,
                    StickSide::Right,
                    stick.warp,
                )?,
            }),
            config::ReachOverlay::Pad => Some(ReachCache::Pad {
                left_all: super::reach_extent::compute_pad_envelope(
                    self,
                    StickSide::Left,
                    pad.warp,
                    cfg.sc2.pad_origin_relative,
                    cfg.sc2.pad_origin_stretch,
                    cfg.sc2.pad_origin_stretch_max_gain,
                )?,
                right_all: super::reach_extent::compute_pad_envelope(
                    self,
                    StickSide::Right,
                    pad.warp,
                    cfg.sc2.pad_origin_relative,
                    cfg.sc2.pad_origin_stretch,
                    cfg.sc2.pad_origin_stretch_max_gain,
                )?,
                left_safe: super::reach_extent::compute_safe_pad_envelope(
                    self,
                    StickSide::Left,
                    pad.warp,
                    cfg.sc2.pad_origin_relative,
                    cfg.sc2.pad_origin_stretch,
                    cfg.sc2.pad_origin_stretch_max_gain,
                )?,
                right_safe: super::reach_extent::compute_safe_pad_envelope(
                    self,
                    StickSide::Right,
                    pad.warp,
                    cfg.sc2.pad_origin_relative,
                    cfg.sc2.pad_origin_stretch,
                    cfg.sc2.pad_origin_stretch_max_gain,
                )?,
            }),
        }
    }

    /// Copy centres/hitboxes/ids into an immutable snapshot for MCP queries.
    pub fn export_geometry(
        &self,
        layout_name: &str,
        revision: u64,
    ) -> crate::state::keyboard::geometry_snap::GeometrySnapshot {
        use crate::state::keyboard::geometry_snap::{wire_id, GeometrySnapshot, SnapKey, SnapRect};

        let centres = self.captured_centres.as_ref();
        let mut keys = Vec::new();
        for (row_idx, row) in self.rows.iter().enumerate() {
            for (col_idx, item) in row.items.iter().enumerate() {
                let Some(key_button) = item.as_key() else {
                    continue;
                };
                let Some(id) = wire_id(&key_button.key.normal) else {
                    continue; // Skip
                };
                let centre = centres
                    .and_then(|c| c.get(row_idx))
                    .and_then(|r| r.get(col_idx))
                    .and_then(|p| p.map(|pos| (pos.x, pos.y)));
                let hitbox = self
                    .key_hit_boxes
                    .get(row_idx)
                    .and_then(|r| r.get(col_idx))
                    .and_then(|h| h.clone());
                keys.push(SnapKey {
                    row: row_idx,
                    col: col_idx,
                    id,
                    selectable: key_button.selectable,
                    centre,
                    hitbox,
                });
            }
        }

        let (stick_scale_x, stick_scale_y) = self.aim_scale(self.aim_kind, false);
        let (pad_scale_x, pad_scale_y) = if self.aim_kind == ControllerKind::Ps4 {
            (None, None)
        } else {
            let (x, y) = self.aim_scale(ControllerKind::Sc2, true);
            (Some(x), Some(y))
        };

        let rect_to_snap = |r: &egui::Rect| SnapRect {
            min_x: r.min.x,
            min_y: r.min.y,
            max_x: r.max.x,
            max_y: r.max.y,
        };

        GeometrySnapshot {
            layout: layout_name.to_owned(),
            revision,
            scale_x: self.scale_x,
            scale_y: self.scale_y,
            stick_scale_x,
            stick_scale_y,
            pad_scale_x,
            pad_scale_y,
            left_rest: self.mapping_rest(StickSide::Left),
            right_rest: self.mapping_rest(StickSide::Right),
            left_bounds: self.left_stick_bounds.iter().map(rect_to_snap).collect(),
            right_bounds: self.right_stick_bounds.iter().map(rect_to_snap).collect(),
            keys,
        }
    }

    fn mapping_rest(&self, side: StickSide) -> (f32, f32) {
        let (rx, ry) = self.stick_center(side);
        let (bx, by) = match side {
            StickSide::Left => self.cursor_bias_left,
            StickSide::Right => self.cursor_bias_right,
        };
        (rx + bx, ry + by)
    }

    /// Remember which controller's ranges the reach overlay and geometry snapshot use.
    /// Returns whether the kind changed.
    pub(crate) fn set_aim_kind(&mut self, kind: ControllerKind) -> bool {
        let kind = config::resolved_controller(kind);
        if self.aim_kind == kind {
            return false;
        }
        self.aim_kind = kind;
        self.reach_cache = None;
        if self.captured_centres.is_some() {
            self.reach_cache = self.build_reach_cache();
        }
        true
    }

    pub(crate) fn aim_kind_for_reach(&self) -> ControllerKind {
        self.aim_kind
    }

    pub(crate) fn aim_scale(&self, kind: ControllerKind, pad: bool) -> (f32, f32) {
        match config::resolved_controller(kind) {
            ControllerKind::Ps4 => self.ps4_stick_scale,
            _ if pad => self.sc2_pad_scale,
            _ => self.sc2_stick_scale,
        }
    }

    pub fn stick_to_cursor(&self, side: StickSide, stick: (f32, f32)) -> (f32, f32) {
        let (sx, sy) = self.aim_scale(ControllerKind::Sc2, false);
        self.stick_to_cursor_scaled(side, stick, sx, sy)
    }

    pub(crate) fn stick_to_cursor_scaled(
        &self,
        side: StickSide,
        stick: (f32, f32),
        aim_scale_x: f32,
        aim_scale_y: f32,
    ) -> (f32, f32) {
        geom::stick_to_cursor(
            self.mapping_rest(side),
            self.scale_x,
            self.scale_y,
            aim_scale_x,
            aim_scale_y,
            stick,
        )
    }

    pub fn get_nearest_key(
        &self,
        side: StickSide,
        stick: (f32, f32),
        shift_state: bool,
    ) -> Option<RawKey> {
        self.key_at_cell(self.nearest_cell(side, stick, None, 1.0)?, shift_state)
    }

    pub fn get_key_at(&self, x: f32, y: f32, shifted: bool) -> Option<RawKey> {
        self.key_at_cell(self.pick_cell_at(x, y, None, 1.0)?, shifted)
    }

    pub(crate) fn key_at_cell(&self, cell: (usize, usize), shifted: bool) -> Option<RawKey> {
        self.rows
            .get(cell.0)?
            .items
            .get(cell.1)?
            .as_key()
            .map(|key_button| key_button.key.get(shifted))
    }

    fn pick_cell_at(
        &self,
        x: f32,
        y: f32,
        sticky: Option<(usize, usize)>,
        k: f32,
    ) -> Option<(usize, usize)> {
        let k = k.max(1.0);
        let mut best: Option<(f32, usize, usize)> = None;
        let mut sticky_score = None;

        for (row_idx, row) in self.key_hit_boxes.iter().enumerate() {
            for (col_idx, h) in row.iter().enumerate() {
                let Some(h) = h else {
                    continue;
                };
                let Some(score) = h.contains(x, y) else {
                    continue;
                };

                if sticky == Some((row_idx, col_idx)) {
                    sticky_score = Some(score);
                }

                if best.is_none_or(|(best_score, _, _)| score < best_score) {
                    best = Some((score, row_idx, col_idx));
                }
            }
        }

        let (other_score, row, col) = best?;

        if let Some(s_sticky) = sticky_score {
            if s_sticky <= other_score * k {
                return sticky;
            }
        }

        Some((row, col))
    }

    fn stick_bounds(&self, side: StickSide) -> &[Rect] {
        match side {
            StickSide::Left => &self.left_stick_bounds,
            StickSide::Right => &self.right_stick_bounds,
        }
    }

    pub(crate) fn cell_at_pixel(
        &self,
        side: StickSide,
        pixel: (f32, f32),
    ) -> Option<(usize, usize)> {
        let (x, y) = clamp_stick_cursor(pixel, self.stick_bounds(side));
        self.pick_cell_at(x, y, None, 1.0)
    }

    pub(crate) fn nearest_cell(
        &self,
        side: StickSide,
        stick: (f32, f32),
        sticky: Option<(usize, usize)>,
        k: f32,
    ) -> Option<(usize, usize)> {
        let (sx, sy) = self.aim_scale(ControllerKind::Sc2, false);
        self.nearest_cell_scaled(side, stick, sticky, k, sx, sy)
    }

    pub(crate) fn nearest_cell_scaled(
        &self,
        side: StickSide,
        stick: (f32, f32),
        sticky: Option<(usize, usize)>,
        k: f32,
        aim_scale_x: f32,
        aim_scale_y: f32,
    ) -> Option<(usize, usize)> {
        let cursor = self.stick_to_cursor_scaled(side, stick, aim_scale_x, aim_scale_y);
        let (x, y) = clamp_stick_cursor(cursor, self.stick_bounds(side));
        self.pick_cell_at(x, y, sticky, k)
    }

    pub fn draw_debug(&self, ctx: &Context, _: &mut Ui) {
        {
            let debug = config::get().debug;
            let painter = ctx.debug_painter();

            if debug.show_stick_cursors {
                let d_lock = ctx.plugin::<DebugPlugin>();
                let d = d_lock.lock();
                let input = d.controller_input.as_deref();
                let (cursor_x, cursor_y) = debug_cursor(self, input, StickSide::Left);
                painter.circle_filled(
                    [cursor_x, cursor_y].into(),
                    8.0,
                    Color32::from_rgb(0, 0, 255),
                );

                let (cursor_x, cursor_y) = debug_cursor(self, input, StickSide::Right);
                painter.circle_filled(
                    [cursor_x, cursor_y].into(),
                    8.0,
                    Color32::from_rgb(0, 255, 0),
                );
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
                            KeyHitBox::Circle { x, y, r } => {
                                painter.circle_stroke(
                                    [*x, *y].into(),
                                    *r,
                                    egui::Stroke::new(
                                        1.0,
                                        Color32::from_rgba_premultiplied(0, 192, 255, 128),
                                    ),
                                );
                            }
                            KeyHitBox::Ellipse { x, y, rx, ry } => {
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

            if debug.reach_overlay != config::ReachOverlay::None {
                if let Some(cache) = &self.reach_cache {
                    match cache {
                        ReachCache::Stick { left, right } => {
                            draw_reach_envelope(&painter, left, side_color(StickSide::Left), 64);
                            draw_reach_envelope(&painter, right, side_color(StickSide::Right), 64);
                        }
                        ReachCache::Pad {
                            left_all,
                            right_all,
                            left_safe,
                            right_safe,
                        } => {
                            let gray = reach_color(Color32::from_gray(210), 45);
                            draw_reach_envelope(&painter, left_all, gray, 80);
                            draw_reach_envelope(&painter, right_all, gray, 80);
                            draw_dashed_reach_envelope(&painter, left_safe, StickSide::Left);
                            draw_dashed_reach_envelope(&painter, right_safe, StickSide::Right);
                        }
                    }
                }
            }
        }
    }
}

fn debug_cursor(
    layout: &KeyboardLayout,
    input: Option<&(dyn ControllerInput + Send + Sync)>,
    side: StickSide,
) -> (f32, f32) {
    let Some(input) = input else {
        return layout.stick_to_cursor(side, (0.0, 0.0));
    };
    let (stick, pad) = match side {
        StickSide::Left => match input.left_pad() {
            Some(p) => (p, true),
            None => (input.left_stick(), false),
        },
        StickSide::Right => match input.right_pad() {
            Some(p) => (p, true),
            None => (input.right_stick(), false),
        },
    };
    let (sx, sy) = layout.aim_scale(input.family(), pad);
    layout.stick_to_cursor_scaled(side, stick, sx, sy)
}

fn side_color(side: StickSide) -> Color32 {
    match side {
        StickSide::Left => Color32::from_rgb(0, 0, 255),
        StickSide::Right => Color32::from_rgb(0, 255, 0),
    }
}

fn reach_color(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

fn draw_reach_envelope(
    painter: &Painter,
    envelope: &ReachEnvelope,
    color: Color32,
    fill_alpha: u8,
) {
    if envelope.fill_hull.len() >= 3 {
        painter.add(Shape::convex_polygon(
            envelope.fill_hull.clone(),
            reach_color(color, fill_alpha),
            Stroke::NONE,
        ));
    }

    painter.add(Shape::closed_line(
        envelope.outline.clone(),
        Stroke::new(1.0, reach_color(color, 180)),
    ));
}

fn draw_dashed_reach_envelope(painter: &Painter, envelope: &ReachEnvelope, side: StickSide) {
    let color = side_color(side);
    if envelope.fill_hull.len() >= 3 {
        painter.add(Shape::convex_polygon(
            envelope.fill_hull.clone(),
            reach_color(color, 38),
            Stroke::NONE,
        ));
    }

    let stroke = Stroke::new(1.0, reach_color(color, 180));
    for (index, pair) in envelope.outline.windows(2).enumerate() {
        if index % 2 == 0 {
            painter.line_segment([pair[0], pair[1]], stroke);
        }
    }
    if envelope.outline.len() > 1 && (envelope.outline.len() - 1).is_multiple_of(2) {
        painter.line_segment(
            [*envelope.outline.last().unwrap(), envelope.outline[0]],
            stroke,
        );
    }
}

impl<'a> IntoIterator for &'a KeyboardLayout {
    type Item = (&'a Vec<RowItem>, UnscaledPixelUnitX, UnscaledPixelUnitY);

    type IntoIter =
        std::iter::Map<std::slice::Iter<'a, KeyboardRow>, fn(&'a KeyboardRow) -> Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        fn project(row: &KeyboardRow) -> (&Vec<RowItem>, UnscaledPixelUnitX, UnscaledPixelUnitY) {
            (&row.items, row.indent, row.height)
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
        for path in ["qwerty.toml", "old_sc.toml", "old_sc_symbols.toml"] {
            let s = fs::read_to_string(dir.join(path)).unwrap();
            let _: KeyboardLayoutFile = toml::from_str(&s).unwrap();
            KeyboardLayout::load_with_scales(&s, 30.0, 32.0, 3.0, 2.5).unwrap();
        }
    }

    #[test]
    fn old_sc_and_symbols_share_home_row_rest_slots() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let main: KeyboardLayoutFile =
            toml::from_str(&fs::read_to_string(dir.join("old_sc.toml")).unwrap()).unwrap();
        let symbols: KeyboardLayoutFile =
            toml::from_str(&fs::read_to_string(dir.join("old_sc_symbols.toml")).unwrap()).unwrap();
        assert_eq!(
            main.stick_rest_left
                .as_ref()
                .and_then(StickRest::key_position),
            symbols
                .stick_rest_left
                .as_ref()
                .and_then(StickRest::key_position),
        );
        assert_eq!(
            main.stick_rest_right
                .as_ref()
                .and_then(StickRest::key_position),
            symbols
                .stick_rest_right
                .as_ref()
                .and_then(StickRest::key_position),
        );
    }

    #[test]
    fn translucent_reach_fill_premultiplies_rgb() {
        let fill = reach_color(Color32::from_gray(210), 45);
        assert!(fill.r() <= fill.a());
        assert!(fill.g() <= fill.a());
        assert!(fill.b() <= fill.a());
    }

    #[test]
    fn stick_rest_rejects_skip_and_oob() {
        let skip = r#"
stick_rest_left = [0, 0]
[[rows]]
indent = 0.0
items = [ { key = "Skip" } ]
"#;
        let err = KeyboardLayout::load_with_scales(skip, 1.0, 1.0, 1.0, 1.0)
            .unwrap_err()
            .to_string();
        assert!(err.contains("Skip"), "{err}");

        let oob = r#"
stick_rest_right = [0, 9]
[[rows]]
indent = 0.0
items = [ { key = "a" } ]
"#;
        let err = KeyboardLayout::load_with_scales(oob, 1.0, 1.0, 1.0, 1.0)
            .unwrap_err()
            .to_string();
        assert!(err.contains("out of range"), "{err}");
    }

    #[test]
    fn stick_rest_uses_row_col_centre() {
        let toml = r#"
stick_rest_left = [0, 1]
stick_rest_right = [0, 0]
[[rows]]
indent = 0.0
items = [
  { key = "a", width = 1.0 },
  { key = "=", width = 1.0 },
]
"#;
        let mut layout = KeyboardLayout::load_with_scales(toml, 30.0, 32.0, 3.0, 2.5).unwrap();
        layout.update_geometry(vec![vec![
            Some(Pos2::new(10.0, 20.0)),
            Some(Pos2::new(30.0, 40.0)),
        ]]);
        assert_eq!(layout.left_stick_center, (30.0, 40.0));
        assert_eq!(layout.right_stick_center, (10.0, 20.0));
    }

    fn two_key_layout(rest_col: usize) -> KeyboardLayout {
        let toml = format!(
            r#"
stick_rest_left = [0, {rest_col}]
stick_rest_right = [0, {rest_col}]
[stick_bounds]
left = [{{ min = {{ x = 0.0, y = 0.0 }}, max = {{ x = 4.0, y = 2.0 }} }}]
right = [{{ min = {{ x = 0.0, y = 0.0 }}, max = {{ x = 4.0, y = 2.0 }} }}]
[[rows]]
indent = 0.0
items = [
  {{ key = "a", width = 1.0 }},
  {{ key = "b", width = 1.0 }},
]
"#
        );
        let mut layout = KeyboardLayout::load_with_scales(&toml, 30.0, 32.0, 3.0, 2.5).unwrap();
        layout.update_geometry(vec![vec![
            Some(Pos2::new(10.0, 20.0)),
            Some(Pos2::new(80.0, 20.0)),
        ]]);
        layout
    }

    #[test]
    fn origin_continuity_keeps_analog_cursor_pixel() {
        let analog = (0.4, -0.2);
        let from = two_key_layout(0);
        let mut to = two_key_layout(1);
        let old = from.stick_to_cursor(StickSide::Left, analog);
        to.continue_origin_from(&from);
        let new = to.stick_to_cursor(StickSide::Left, analog);
        assert!(
            (old.0 - new.0).abs() < 0.01 && (old.1 - new.1).abs() < 0.01,
            "old={old:?} new={new:?}"
        );
    }

    #[test]
    fn layout_switch_discards_geometry_then_recaptures() {
        let analog = (0.4, -0.2);
        let from = two_key_layout(0);
        let mut to = two_key_layout(1);
        let old = from.stick_to_cursor(StickSide::Left, analog);

        to.continue_origin_from(&from);

        assert!(to.captured_centres.is_none());
        assert_eq!(to.geometry_shift, Vec2::ZERO);
        let held = to.stick_to_cursor(StickSide::Left, analog);
        assert!(
            (old.0 - held.0).abs() < 0.01 && (old.1 - held.1).abs() < 0.01,
            "old={old:?} held={held:?}"
        );

        let centres = vec![vec![
            Some(Pos2::new(10.0, 100.0)),
            Some(Pos2::new(80.0, 100.0)),
        ]];
        to.update_geometry(centres.clone());

        let mut cold = two_key_layout(1);
        cold.clear_captured_geometry();
        cold.update_geometry(centres);

        assert_eq!(to.left_stick_bounds, cold.left_stick_bounds);
        assert_eq!(to.right_stick_bounds, cold.right_stick_bounds);
        assert_eq!(to.geometry_shift, cold.geometry_shift);

        let after = to.stick_to_cursor(StickSide::Left, analog);
        assert!(
            (old.0 - after.0).abs() < 0.01 && (old.1 - after.1).abs() < 0.01,
            "old={old:?} after={after:?}"
        );
    }

    #[test]
    fn origin_continuity_survives_first_capture() {
        let analog = (0.4, -0.2);
        let from = two_key_layout(0);
        let toml = r#"
stick_rest_left = [0, 1]
stick_rest_right = [0, 1]
[[rows]]
indent = 0.0
items = [
  { key = "a", width = 1.0 },
  { key = "b", width = 1.0 },
]
"#;
        let mut to = KeyboardLayout::load_with_scales(toml, 30.0, 32.0, 3.0, 2.5).unwrap();
        to.continue_origin_from(&from);
        let held = to.stick_to_cursor(StickSide::Left, analog);
        to.update_geometry(vec![vec![
            Some(Pos2::new(10.0, 20.0)),
            Some(Pos2::new(80.0, 20.0)),
        ]]);
        let after = to.stick_to_cursor(StickSide::Left, analog);
        assert!(
            (held.0 - after.0).abs() < 0.01 && (held.1 - after.1).abs() < 0.01,
            "held={held:?} after={after:?}"
        );
    }

    #[test]
    fn stick_rest_supports_keys_and_unscaled_position() {
        let toml = r#"
stick_rest_left = { type = "position", x = 2.5, y = 3.5 }
stick_rest_right = { type = "keys", row = 0, column = 1 }
[[rows]]
indent = 0.0
items = [
  { key = "a", width = 1.0 },
  { key = "=", width = 1.0 },
]
"#;
        let mut layout = KeyboardLayout::load_with_scales(toml, 30.0, 32.0, 3.0, 2.5).unwrap();
        layout.update_geometry(vec![vec![
            Some(Pos2::new(10.0, 20.0)),
            Some(Pos2::new(30.0, 40.0)),
        ]]);
        let shift = Pos2::new(10.0, 20.0) - Pos2::new(15.0, 16.0);
        assert_eq!(layout.left_stick_center, (75.0 + shift.x, 112.0 + shift.y));
        assert_eq!(layout.right_stick_center, (30.0, 40.0));
    }

    #[test]
    fn cursor_defaults_to_configured_rest_without_input() {
        let toml = r#"
stick_rest_left = [0, 0]
stick_rest_right = [0, 0]
[[rows]]
indent = 0.0
items = [{ key = "a" }]
"#;
        let mut layout = KeyboardLayout::load_with_scales(toml, 1.0, 1.0, 1.0, 1.0).unwrap();
        layout.update_geometry(vec![vec![Some(Pos2::new(10.0, 20.0))]]);
        assert_eq!(debug_cursor(&layout, None, StickSide::Left), (10.0, 20.0));
        assert_eq!(debug_cursor(&layout, None, StickSide::Right), (10.0, 20.0));
    }

    #[test]
    fn typo_neighbors_knn_same_row() {
        let toml = r#"
[[rows]]
indent = 0.0
items = [{ key = "q" }, { key = "w" }, { key = "e" }]
"#;
        let mut layout = KeyboardLayout::load_with_scales(toml, 1.0, 1.0, 1.0, 1.0).unwrap();
        layout.update_geometry(vec![vec![
            Some(Pos2::new(0.0, 0.0)),
            Some(Pos2::new(10.0, 0.0)),
            Some(Pos2::new(20.0, 0.0)),
        ]]);
        let n = layout.typo_neighbors();
        assert!(n.get(&'q').unwrap().contains(&'w'));
        assert!(n.get(&'w').unwrap().contains(&'q'));
        assert!(n.get(&'w').unwrap().contains(&'e'));
        assert_eq!(n.get(&'q').unwrap()[0], 'w');
    }

    #[test]
    fn recapture_translates_debug_cursor_bounds_and_position_rest() {
        let toml = r#"
stick_rest_left = { type = "position", x = 1.0, y = 2.0 }
stick_rest_right = [0, 0]
[stick_bounds]
left = [{ min = { x = 0.0, y = 0.0 }, max = { x = 2.0, y = 2.0 } }]
right = [{ min = { x = 2.0, y = 0.0 }, max = { x = 4.0, y = 2.0 } }]
[[rows]]
indent = 0.0
items = [{ key = "a" }]
"#;
        let mut layout = KeyboardLayout::load_with_scales(toml, 10.0, 10.0, 1.0, 1.0).unwrap();
        layout.update_geometry(vec![vec![Some(Pos2::new(5.0, 10.0))]]);

        let left_bounds = layout.left_stick_bounds.clone();
        let right_bounds = layout.right_stick_bounds.clone();
        let position_rest = layout.left_stick_center;
        assert_eq!(debug_cursor(&layout, None, StickSide::Right), (5.0, 10.0));

        layout.update_geometry(vec![vec![Some(Pos2::new(5.0, 50.0))]]);

        let dy = 40.0;
        assert_eq!(debug_cursor(&layout, None, StickSide::Right), (5.0, 50.0));
        assert_eq!(
            layout.left_stick_center,
            (position_rest.0, position_rest.1 + dy)
        );
        assert_eq!(layout.left_stick_bounds[0].min.y, left_bounds[0].min.y + dy);
        assert_eq!(layout.left_stick_bounds[0].max.y, left_bounds[0].max.y + dy);
        assert_eq!(layout.left_stick_bounds[0].min.x, left_bounds[0].min.x);
        assert_eq!(
            layout.right_stick_bounds[0].min.y,
            right_bounds[0].min.y + dy
        );
    }

    #[test]
    fn first_capture_shifts_bounds_so_bottom_row_stays_reachable() {
        let toml = r#"
pad_x = 0.0
pad_y = 0.0
stick_rest_left = [0, 0]
[stick_bounds]
left = [{ min = { x = 0.0, y = 0.0 }, max = { x = 1.0, y = 2.0 } }]
right = []
[[rows]]
indent = 0.0
height = 1.0
items = [{ key = "a", width = 1.0 }]
[[rows]]
indent = 0.0
height = 1.0
items = [{ key = "z", width = 1.0 }]
"#;
        let mut layout = KeyboardLayout::load_with_scales(toml, 10.0, 10.0, 1.0, 1.0).unwrap();
        let chrome = 80.0;
        layout.update_geometry(vec![
            vec![Some(Pos2::new(5.0, 5.0 + chrome))],
            vec![Some(Pos2::new(5.0, 15.0 + chrome))],
        ]);
        assert_eq!(
            layout.get_nearest_key(StickSide::Left, (0.0, 1.0), false),
            Some(RawKey::Key('z'))
        );
    }

    #[test]
    fn clear_geometry_restores_unshifted_bounds() {
        let toml = r#"
stick_rest_left = [0, 0]
[stick_bounds]
left = [{ min = { x = 0.0, y = 0.0 }, max = { x = 2.0, y = 2.0 } }]
right = []
[[rows]]
indent = 0.0
items = [{ key = "a" }]
"#;
        let mut layout = KeyboardLayout::load_with_scales(toml, 10.0, 10.0, 1.0, 1.0).unwrap();
        let original = layout.left_stick_bounds.clone();
        layout.update_geometry(vec![vec![Some(Pos2::new(5.0, 10.0))]]);
        layout.update_geometry(vec![vec![Some(Pos2::new(5.0, 50.0))]]);
        layout.clear_captured_geometry();
        assert_eq!(layout.left_stick_bounds, original);
        assert_eq!(layout.left_stick_center, (0.0, 0.0));
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

    #[test]
    fn items_default_to_key_and_keys_alias() {
        let items: KeyboardLayoutFile = toml::from_str(
            r#"
[[rows]]
indent = 0.0
[[rows.items]]
key = "a"
[[rows.items]]
type = "battery"
width = 0.5
"#,
        )
        .unwrap();
        assert!(matches!(items.rows[0].items[0], RowItem::Key(_)));
        assert!(matches!(items.rows[0].items[1], RowItem::Battery(_)));

        let alias: KeyboardLayoutFile = toml::from_str(
            r#"
[[rows]]
indent = 0.0
[[rows.keys]]
key = "b"
"#,
        )
        .unwrap();
        assert!(matches!(alias.rows[0].items[0], RowItem::Key(_)));
    }

    #[test]
    fn item_align_defaults_left_and_parses_right() {
        let file: KeyboardLayoutFile = toml::from_str(
            r#"
[[rows]]
indent = 0.0
[[rows.items]]
key = "a"
[[rows.items]]
type = "battery"
align = "right"
"#,
        )
        .unwrap();
        assert_eq!(file.rows[0].items[0].align(), ItemAlign::Left);
        assert_eq!(file.rows[0].items[1].align(), ItemAlign::Right);
    }

    #[test]
    fn connected_controller_item_reserves_width_without_becoming_a_key() {
        let file: KeyboardLayoutFile = toml::from_str(
            r#"
[[rows]]
indent = 0.0
items = [
  { type = "battery", align = "right", width = 2.0 },
  { type = "connectedController", align = "right", width = 2.0 },
]
"#,
        )
        .unwrap();

        let item = &file.rows[0].items[1];
        assert!(matches!(item, RowItem::ConnectedController(_)));
        assert!(item.as_key().is_none());
        assert_eq!(item.align(), ItemAlign::Right);
        assert_eq!(item.width().0, file.rows[0].items[0].width().0);
    }

    #[test]
    fn left_content_width_ignores_right_aligned_items() {
        let toml = r#"
pad_x = 0.1
[[rows]]
indent = 0.0
items = [
  { key = "a", width = 1.0 },
  { key = "b", width = 1.0 },
]
[[rows]]
indent = 0.0
items = [
  { key = "c", width = 0.5 },
  { type = "battery", align = "right", width = 9.0 },
]
"#;
        let layout = KeyboardLayout::load_with_scales(toml, 10.0, 10.0, 1.0, 1.0).unwrap();
        let pad = layout.scale_x(layout.pad_x);
        assert!((pad - 1.0).abs() < 1e-5);
        assert!((layout.left_content_width(pad) - 21.0).abs() < 1e-4);

        let left_used = layout.scale_x(layout.rows[1].indent)
            + layout.cluster_width(
                layout.rows[1]
                    .items
                    .iter()
                    .filter(|item| item.align() == ItemAlign::Left),
                pad,
            );
        let right_w = layout.cluster_width(
            layout.rows[1]
                .items
                .iter()
                .filter(|item| item.align() == ItemAlign::Right),
            pad,
        );
        assert!((left_used - 5.0).abs() < 1e-4);
        assert!((right_w - 90.0).abs() < 1e-4);
        assert_eq!(
            KeyboardLayout::rtl_leading_gap(21.0, left_used, right_w),
            0.0
        );
        assert!((KeyboardLayout::rtl_leading_gap(21.0, 5.0, 8.0) - 8.0).abs() < 1e-4);
    }

    #[test]
    fn stick_rest_rejects_battery() {
        let toml = r#"
stick_rest_left = [0, 1]
[[rows]]
indent = 0.0
items = [
  { key = "a" },
  { type = "battery" },
]
"#;
        let err = KeyboardLayout::load_with_scales(toml, 1.0, 1.0, 1.0, 1.0)
            .unwrap_err()
            .to_string();
        assert!(err.contains("non-key"), "{err}");
    }

    fn two_letter_circles() -> KeyboardLayout {
        let toml = r#"
[[rows]]
indent = 0.0
height = 1.0
items = [
  { key = "a", width = 1.0 },
  { key = "s", width = 1.0 },
]
"#;
        let mut layout = KeyboardLayout::load_with_scales(toml, 10.0, 10.0, 1.0, 1.0).unwrap();
        layout.update_geometry(vec![vec![
            Some(Pos2::new(0.0, 0.0)),
            Some(Pos2::new(20.0, 0.0)),
        ]]);
        layout
    }

    fn letter_and_wide() -> KeyboardLayout {
        let toml = r#"
[[rows]]
indent = 0.0
height = 1.0
items = [
  { key = "a", width = 1.0 },
  { key = "b", width = 2.0 },
]
"#;
        let mut layout = KeyboardLayout::load_with_scales(toml, 10.0, 10.0, 1.0, 1.0).unwrap();
        layout.update_geometry(vec![vec![
            Some(Pos2::new(0.0, 0.0)),
            Some(Pos2::new(15.0, 0.0)),
        ]]);
        layout
    }

    #[test]
    fn overlapping_letter_beats_wide_key_when_nearer_rim() {
        let layout = letter_and_wide();
        assert_eq!(
            layout.get_key_at(2.0, 0.0, false),
            Some(RawKey::Key('a')),
            "letter rim-fraction is smaller than the wide key's"
        );
    }

    #[test]
    fn sticky_holds_across_midpoint_noise() {
        let layout = two_letter_circles();
        let a = (0, 0);
        let s = (0, 1);
        let noisy = (10.5, 0.0);
        assert_eq!(
            layout.pick_cell_at(noisy.0, noisy.1, Some(a), 1.25),
            Some(a)
        );
        assert_eq!(layout.pick_cell_at(noisy.0, noisy.1, Some(a), 1.0), Some(s));
        assert_eq!(layout.pick_cell_at(noisy.0, noisy.1, None, 1.25), Some(s));
    }

    #[test]
    fn sticky_yields_when_neighbor_is_clearly_nearer() {
        let layout = two_letter_circles();
        assert_eq!(
            layout.pick_cell_at(10.8, 0.0, Some((0, 0)), 1.25),
            Some((0, 1))
        );
    }

    #[test]
    fn sticky_does_not_keep_a_key_after_leaving_its_hitbox() {
        let layout = two_letter_circles();
        assert_eq!(layout.pick_cell_at(10.0, 50.0, Some((0, 0)), 1.25), None);
    }

    #[test]
    fn sticky_survives_a_brief_pulse_toward_the_neighbor() {
        let layout = two_letter_circles();
        let a = (0, 0);
        assert_eq!(layout.pick_cell_at(5.0, 0.0, Some(a), 1.25), Some(a));
        assert_eq!(layout.pick_cell_at(10.5, 0.0, Some(a), 1.25), Some(a));
        assert_eq!(layout.pick_cell_at(5.0, 0.0, Some(a), 1.25), Some(a));
    }
}

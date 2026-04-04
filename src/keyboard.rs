use anyhow::Result;
use egui::{Button, Color32, RichText, Ui, Vec2};
use enigo::{Enigo, Keyboard as _};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::{collections::HashMap, hint::unreachable_unchecked};

fn warp(mut x: f32, mut y: f32, warp: f32) -> (f32, f32) {
    if warp > 0.0 {
        let u2 = x * x;
        let v2 = y * y;
        let offset = (u2 + v2).sqrt();
        if offset > 0.001 {
            // Determine how much to scale based on the warp factor
            // At warp=1.0, this pushes the circle out to fill the square corners.
            let scale = (offset / (x.abs().max(y.abs()))).powf(warp);
            x *= scale;
            y *= scale;
        }
    }

    // Clip to square bounds
    x = x.clamp(-1.0, 1.0);
    y = y.clamp(-1.0, 1.0);

    (x, y)
}

#[derive(PartialEq, Eq, Debug, Clone, Deserialize)]
#[serde(try_from = "String")]
pub enum RawKey {
    Key(String),
    Enter,
    Skip,
    Shift,
    Ctrl,
    Alt,
    Backspace,
    Tab,
    Paste,
    Done,
}

impl TryFrom<String> for RawKey {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.to_uppercase().as_str() {
            "ENTER" => Ok(RawKey::Enter),
            "SKIP" => Ok(RawKey::Skip),
            "SHIFT" => Ok(RawKey::Shift),
            "CTRL" => Ok(RawKey::Ctrl),
            "ALT" => Ok(RawKey::Alt),
            "BACKSPACE" => Ok(RawKey::Backspace),
            "TAB" => Ok(RawKey::Tab),
            "PASTE" => Ok(RawKey::Paste),
            "DONE" => Ok(RawKey::Done),
            _ => Ok(RawKey::Key(value)),
        }
    }
}

impl PartialEq<str> for RawKey {
    fn eq(&self, other: &str) -> bool {
        match self {
            RawKey::Key(k) => k == other,
            _ => false,
        }
    }
}

impl PartialEq<RawKey> for str {
    fn eq(&self, other: &RawKey) -> bool {
        other == self
    }
}

impl PartialEq<&str> for RawKey {
    fn eq(&self, other: &&str) -> bool {
        self == *other
    }
}

impl ToString for RawKey {
    fn to_string(&self) -> String {
        match self {
            RawKey::Key(k) => k.clone(),
            RawKey::Enter => "Enter".to_string(),
            RawKey::Skip => unsafe { unreachable_unchecked() },
            RawKey::Shift => "Shift".to_string(),
            RawKey::Ctrl => "Ctrl".to_string(),
            RawKey::Alt => "Alt".to_string(),
            RawKey::Backspace => "Backspace".to_string(),
            RawKey::Tab => "Tab".to_string(),
            RawKey::Paste => "Paste".to_string(),
            RawKey::Done => "Done".to_string(),
        }
    }
}

trait ToUpper {
    fn to_upper(&self) -> Self;
}

impl ToUpper for RawKey {
    fn to_upper(&self) -> Self {
        match self {
            RawKey::Key(k) => RawKey::Key(k.to_uppercase()),
            _ => self.clone(),
        }
    }
}

impl ToUpper for String {
    fn to_upper(&self) -> Self {
        self.to_uppercase()
    }
}

#[derive(Debug, Clone)]
struct Key<T> {
    normal: T,
    shift: Option<T>,
}

impl<T: ToString + Clone + ToUpper> Key<T> {
    fn display(&self, shifted: bool) -> String {
        self.get(shifted).to_string()
    }

    fn get(&self, shifted: bool) -> T {
        if shifted {
            if let Some(k) = &self.shift {
                k.clone()
            } else {
                self.normal.to_upper()
            }
        } else {
            self.normal.clone()
        }
    }
}

// Helper struct for deserializing Key from table format
#[derive(Deserialize)]
struct KeyTable<T> {
    normal: T,
    shift: T,
}

impl<'de, T> Deserialize<'de> for Key<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, MapAccess, Visitor};
        use std::fmt;
        use std::marker::PhantomData;

        struct KeyVisitor<T>(PhantomData<T>);

        impl<'de, T> Visitor<'de> for KeyVisitor<T>
        where
            T: Deserialize<'de>,
        {
            type Value = Key<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string or a table with 'normal' and 'shift' fields")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                // Deserialize the string as T
                let normal = T::deserialize(de::value::StrDeserializer::new(value))?;
                Ok(Key {
                    normal,
                    shift: None,
                })
            }

            fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                // Deserialize as a table with normal and shift fields
                let table = KeyTable::<T>::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(Key {
                    normal: table.normal,
                    shift: Some(table.shift),
                })
            }
        }

        deserializer.deserialize_any(KeyVisitor(PhantomData))
    }
}

#[derive(Debug, Clone, Deserialize)]
struct KeyButton {
    display: Option<Key<String>>,
    key: Key<RawKey>,
    pos: Pos,
    width: f32, // Width multiplier (1.0 = normal, 5.0 = space bar)
    #[serde(default)]
    radius_mult: Option<f32>,
}

#[derive(Debug, Clone, Deserialize)]
struct KeyboardLayoutFile {
    keys: Vec<KeyButton>,
    row_indents: Vec<f32>, // Indent in pixels for each row
    #[serde(default)]
    row_heights: Vec<f32>, // Height multiplier for each row
    #[serde(default = "default_button_unit_width")]
    button_unit_width: f32,
    #[serde(default = "default_button_unit_height")]
    button_unit_height: f32,
    #[serde(default = "default_unit_spacing_x")]
    unit_spacing_x: f32,
    #[serde(default = "default_unit_spacing_y")]
    unit_spacing_y: f32,
    #[serde(default = "default_radius_mult")]
    radius_mult: f32,
    #[serde(default = "default_font_size")]
    font_size: f32,
}

fn default_button_unit_width() -> f32 {
    40.0
}
fn default_button_unit_height() -> f32 {
    40.0
}
fn default_unit_spacing_x() -> f32 {
    2.0
}
fn default_unit_spacing_y() -> f32 {
    2.0
}
fn default_radius_mult() -> f32 {
    1.125
}
fn default_font_size() -> f32 {
    18.0
}

#[derive(Debug)]
pub struct KeyboardLayout {
    row_indents: Vec<f32>, // Indent in pixels for each row
    row_heights: Vec<f32>, // Height multiplier for each row
    rows: Vec<Vec<KeyButton>>,
    dims: (f32, f32),

    // Layout constants
    pub font_size: f32,
    button_unit_width: f32,
    button_unit_height: f32,
    unit_spacing_x: f32,
    unit_spacing_y: f32,
    radius_mult: f32,

    // (x, y, r) where (x, y) is center of key, and r is a radius. This will not overlap exactly
    // with a key button, and key circles may overlap each other. Key selection for highlight will
    // use closest center, will key press will return all overlapping keys to allow typo resistance,
    // i.e. may decide on key press based on engligh word etc.
    key_hit_boxes: Vec<Vec<Option<(f32, f32, f32)>>>,
}

impl KeyboardLayout {
    fn load(toml: &str) -> Result<Self> {
        let layout: KeyboardLayoutFile = toml::from_str(toml)?;

        // Group keys by row
        let mut row_map: HashMap<usize, Vec<KeyButton>> = HashMap::new();
        for key in &layout.keys {
            row_map
                .entry(key.pos.0)
                .or_insert_with(Vec::new)
                .push(key.clone());
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
                            width: 1.0,
                            radius_mult: None,
                        });
                        expected_col += 1;
                    }

                    result.push(key.clone());
                    expected_col += 1;
                }

                result
            })
            .collect();

        let mut layout = KeyboardLayout {
            row_indents: layout.row_indents,
            row_heights: layout.row_heights,
            rows,
            dims: Default::default(),
            font_size: layout.font_size,
            button_unit_width: layout.button_unit_width,
            button_unit_height: layout.button_unit_height,
            unit_spacing_x: layout.unit_spacing_x,
            unit_spacing_y: layout.unit_spacing_y,
            radius_mult: layout.radius_mult,
            key_hit_boxes: Default::default(),
        };

        layout.calculate_geometry();
        Ok(layout)
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::load(&fs::read_to_string(path)?)
    }

    pub fn get_dimensions(&self) -> (f32, f32) {
        self.dims
    }

    pub fn get_key_center(&self, key_name: &str) -> Option<(f32, f32)> {
        for (row_idx, row) in self.rows.iter().enumerate() {
            for (col_idx, key_button) in row.iter().enumerate() {
                if key_button.key.normal == key_name {
                    if let Some(h) = self.key_hit_boxes[row_idx][col_idx] {
                        return Some((h.0, h.1));
                    }
                }
            }
        }
        None
    }

    fn calculate_geometry(&mut self) {
        let mut key_hit_boxes = Vec::new();
        let num_rows = self.rows.len();

        // Calculate height: sum of (row_height * button_unit_height) + spacing
        let mut total_height = 0.0;
        let mut row_tops = Vec::with_capacity(num_rows);

        for i in 0..num_rows {
            row_tops.push(total_height);
            let height_mult = self.row_heights.get(i).copied().unwrap_or(1.0);
            total_height += height_mult * self.button_unit_height;
            if i < num_rows - 1 {
                total_height += self.unit_spacing_y;
            }
        }

        // Find the row with the most total width (considering key widths and indents)
        let mut max_width: f32 = 0.0;

        for (row_idx, row) in self.rows.iter().enumerate() {
            let mut hitboxes_row = Vec::new();
            if row.is_empty() {
                key_hit_boxes.push(hitboxes_row);
                continue;
            }

            let height_mult = self.row_heights.get(row_idx).copied().unwrap_or(1.0);
            let row_height = height_mult * self.button_unit_height;

            let indent = self.row_indents.get(row_idx).copied().unwrap_or(0.0);
            let mut current_x = indent;
            let center_y = row_tops[row_idx] + (row_height / 2.0);

            for key in row {
                let width = self.button_unit_width * key.width;
                let center_x = current_x + (width / 2.0);

                if key.key.normal != RawKey::Skip
                    && key.key.normal != RawKey::Shift
                    && key.key.normal != RawKey::Tab
                    && key.key.normal != " "
                {
                    // Target radius for imprecise stick input
                    let mult = key.radius_mult.unwrap_or(self.radius_mult);
                    let radius = self.button_unit_width * mult;
                    hitboxes_row.push(Some((center_x, center_y, radius)));
                } else {
                    hitboxes_row.push(None);
                }

                current_x += width + self.unit_spacing_x;
            }

            let row_width_total = current_x - self.unit_spacing_x;
            max_width = max_width.max(row_width_total);
            key_hit_boxes.push(hitboxes_row);
        }

        self.dims = (max_width, total_height);
        self.key_hit_boxes = key_hit_boxes;
    }
}

pub type Pos = (usize, usize);

pub struct Keyboard {
    pub selected: (Option<RawKey>, Option<RawKey>),
    pub layout: KeyboardLayout,
    // sends alternative key
    shift_state: bool,
    // adds shift modifier, different to shift_state in that it can be combined with other modifiers
    shift_mod: bool,
    ctrl_mod: bool,
    alt_mod: bool,
    pub(crate) left_stick_center: (f32, f32),
    right_stick_center: (f32, f32),
    pub(crate) stick_range_x: f32,
    pub(crate) stick_range_y: f32,
    pub(crate) stick_warp: f32,
    left_selectable_bounds: (f32, f32, f32, f32),
    right_selectable_bounds: (f32, f32, f32, f32),
}

impl Keyboard {
    pub fn new(
        layout_path: impl AsRef<Path>,
        stick_range_x: f32,
        stick_range_y: f32,
        stick_warp: f32,
    ) -> Result<Self> {
        let layout = KeyboardLayout::load_from_file(layout_path)?;
        let (w, h) = layout.get_dimensions();
        let left_stick_center = layout.get_key_center("d").unwrap_or((w * 0.25, h * 0.5));
        let right_stick_center = layout.get_key_center("k").unwrap_or((w * 0.75, h * 0.5));

        let left_bounds = Self::calculate_reachable_bounds(
            &layout,
            left_stick_center,
            stick_range_x,
            stick_range_y,
        );
        let right_bounds = Self::calculate_reachable_bounds(
            &layout,
            right_stick_center,
            stick_range_x,
            stick_range_y,
        );

        Ok(Self {
            selected: (None, None),
            layout,
            shift_state: false,
            shift_mod: false,
            ctrl_mod: false,
            alt_mod: false,
            left_stick_center,
            right_stick_center,
            stick_range_x,
            stick_range_y,
            stick_warp,
            left_selectable_bounds: left_bounds,
            right_selectable_bounds: right_bounds,
        })
    }

    fn calculate_reachable_bounds(
        layout: &KeyboardLayout,
        center: (f32, f32),
        range_x: f32,
        range_y: f32,
    ) -> (f32, f32, f32, f32) {
        let max_dx = layout.button_unit_width * range_x;
        let max_dy = layout.button_unit_height * range_y;
        let max_reach_sq = max_dx * max_dx + max_dy * max_dy;

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        let mut found = false;

        for row in &layout.key_hit_boxes {
            for h in row {
                if let Some((kx, ky, kr)) = *h {
                    let dist_sq = (kx - center.0).powi(2) + (ky - center.1).powi(2);
                    // A key is reachable if its center is within the stick's max reach
                    if dist_sq <= max_reach_sq {
                        min_x = min_x.min(kx - kr);
                        max_x = max_x.max(kx + kr);
                        min_y = min_y.min(ky - kr);
                        max_y =
                            max_y.max(ky + kr - layout.button_unit_height * 0.707 /* HACK */);
                        found = true;
                    }
                }
            }
        }

        if found {
            (min_x, min_y, max_x, max_y)
        } else {
            (center.0, center.1, center.0, center.1)
        }
    }

    pub fn send_key(&mut self, enigo: &mut Enigo, key: &RawKey) -> Result<()> {
        macro_rules! mod_press {
            () => {
                if self.shift_mod {
                    enigo.key(enigo::Key::Shift, enigo::Direction::Press)?;
                }
                if self.ctrl_mod {
                    enigo.key(enigo::Key::Control, enigo::Direction::Press)?;
                }
                if self.alt_mod {
                    enigo.key(enigo::Key::Alt, enigo::Direction::Press)?;
                }
            };
        }
        macro_rules! mod_release {
            () => {
                if self.alt_mod {
                    enigo.key(enigo::Key::Alt, enigo::Direction::Release)?;
                }
                if self.ctrl_mod {
                    enigo.key(enigo::Key::Control, enigo::Direction::Release)?;
                }
                if self.shift_mod {
                    enigo.key(enigo::Key::Shift, enigo::Direction::Release)?;
                }
            };
        }
        match key {
            RawKey::Key(k) => {
                mod_press!();
                if k.len() == 1 {
                    enigo.key(
                        enigo::Key::Unicode(k.chars().nth(0).unwrap()),
                        enigo::Direction::Click,
                    )?;
                } else {
                    enigo.text(&k)?;
                }
                mod_release!();
            }
            RawKey::Enter => {
                mod_press!();
                enigo.key(enigo::Key::Return, enigo::Direction::Click)?;
                mod_release!();
            }
            RawKey::Backspace => {
                mod_press!();
                enigo.key(enigo::Key::Backspace, enigo::Direction::Click)?;
                mod_release!();
            }
            RawKey::Tab => {
                mod_press!();
                enigo.key(enigo::Key::Tab, enigo::Direction::Click)?;
                mod_release!();
            }
            RawKey::Paste => {
                mod_press!();
                enigo.key(enigo::Key::Control, enigo::Direction::Press)?;
                enigo.key(enigo::Key::Unicode('v'), enigo::Direction::Click)?;
                enigo.key(enigo::Key::Control, enigo::Direction::Release)?;
                mod_release!();
            }
            _ => return Ok(()),
        }

        self.shift_state = false;
        self.shift_mod = false;
        self.ctrl_mod = false;
        self.alt_mod = false;

        Ok(())
    }

    pub fn get_nearest_key_left(&self, stick: (i32, i32)) -> Option<RawKey> {
        self.get_nearest_key(self.left_stick_center, stick, self.left_selectable_bounds)
    }

    pub fn get_nearest_key_right(&self, stick: (i32, i32)) -> Option<RawKey> {
        self.get_nearest_key(self.right_stick_center, stick, self.right_selectable_bounds)
    }

    fn get_nearest_key(
        &self,
        center: (f32, f32),
        stick: (i32, i32),
        bounds: (f32, f32, f32, f32),
    ) -> Option<RawKey> {
        // Normalise stick input to [-1.0, 1.0]
        let x = stick.0 as f32 / 128.0;
        let y = stick.1 as f32 / 128.0;

        let (x, y) = warp(x, y, self.stick_warp);

        // Map stick to pixel offset
        let range_x = self.layout.button_unit_width * self.stick_range_x;
        let range_y = self.layout.button_unit_height * self.stick_range_y;
        let dx = x * range_x;
        let dy = y * range_y;

        let mut cursor_x = center.0 + dx;
        let mut cursor_y = center.1 + dy;

        // Clamp cursor to the specific bounds for this stick
        let (min_x, min_y, max_x, max_y) = bounds;
        cursor_x = cursor_x.clamp(min_x, max_x);
        cursor_y = cursor_y.clamp(min_y, max_y);

        let mut candidate = (f32::MAX, None);

        for (row_idx, row) in self.layout.key_hit_boxes.iter().enumerate() {
            for (col_idx, h) in row.iter().enumerate() {
                if let Some((kx, ky, kr)) = *h {
                    let distance_sq = (cursor_x - kx).powi(2) + (cursor_y - ky).powi(2);
                    if distance_sq <= kr.powi(2) && distance_sq < candidate.0 {
                        let key_button = &self.layout.rows[row_idx][col_idx];
                        candidate = (distance_sq, Some(key_button.key.get(self.shift_state)));
                    }
                }
            }
        }

        candidate.1
    }

    pub fn toggle_shift(&mut self) {
        if self.shift_state || self.shift_mod {
            self.shift_state = false;
            self.shift_mod = false;
        } else {
            if self.ctrl_mod || self.alt_mod {
                self.shift_mod = !self.shift_mod
            } else {
                self.shift_state = !self.shift_state;
            }
        }
    }

    pub fn toggle_ctrl(&mut self) {
        self.ctrl_mod = !self.ctrl_mod;
    }

    pub fn toggle_alt(&mut self) {
        self.alt_mod = !self.alt_mod;
    }

    pub fn draw_ui(&self, ui: &mut Ui) -> Option<RawKey> {
        let mut pressed_key: Option<RawKey> = None;

        // Set semi-transparent button styling
        let style = ui.style_mut();
        style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgba_premultiplied(60, 60, 60, 128);
        style.visuals.widgets.inactive.bg_fill = Color32::from_rgba_premultiplied(60, 60, 60, 128);
        style.visuals.widgets.inactive.fg_stroke.color = Color32::WHITE;
        style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgba_premultiplied(80, 80, 80, 180);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgba_premultiplied(80, 80, 80, 180);
        style.visuals.widgets.hovered.fg_stroke.color = Color32::WHITE;
        style.visuals.widgets.active.weak_bg_fill =
            Color32::from_rgba_premultiplied(100, 100, 100, 200);
        style.visuals.widgets.active.bg_fill = Color32::from_rgba_premultiplied(100, 100, 100, 200);
        style.visuals.widgets.active.fg_stroke.color = Color32::WHITE;
        style.visuals.selection.bg_fill = Color32::from_rgba_premultiplied(50, 100, 180, 220);
        style.visuals.selection.stroke.color = Color32::WHITE;

        let button_size = Vec2::new(self.layout.button_unit_width, self.layout.button_unit_height);

        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing =
                Vec2::new(self.layout.unit_spacing_x, self.layout.unit_spacing_y);

            // FIXME can this not be done once up-front
            let left_center = self.get_nearest_key_left((0, 0));
            let right_center = self.get_nearest_key_right((0, 0));

            for (row_idx, keys) in self.layout.rows.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing =
                        Vec2::new(self.layout.unit_spacing_x, self.layout.unit_spacing_y);

                    // Add indent for this row
                    let indent = self.layout.row_indents.get(row_idx).copied().unwrap_or(0.0);
                    ui.add_space(indent);

                    let height_mult = self.layout.row_heights.get(row_idx).copied().unwrap_or(1.0);
                    let row_height = self.layout.button_unit_height * height_mult;

                    for key in keys {
                        // Skip rendering for SKIP keys - just add space
                        if key.key.normal == RawKey::Skip {
                            ui.add_space(button_size.x * key.width);
                            continue;
                        }

                        // Determine what to display based on shift state
                        let display_label = if let Some(d) = &key.display {
                            d.get(self.shift_state)
                        } else {
                            key.key.display(self.shift_state)
                        };

                        let button_text = RichText::new(&display_label).size(self.layout.font_size);
                        let mut button = Button::new(button_text);

                        if key.key.normal == RawKey::Shift && self.shift_state {
                            button = button.selected(true);
                        } else {
                            let current_key = key.key.get(self.shift_state);
                            let sel0 = self.selected.0.as_ref().is_some_and(|s| s == &current_key);
                            let sel1 = self.selected.1.as_ref().is_some_and(|s| s == &current_key);

                            if sel0 && sel1 {
                                // Purple for both
                                button = button.fill(Color32::from_rgb(120, 60, 180)).selected(true);
                            } else if sel0
                                || (self.selected.0.is_none()
                                    && left_center.as_ref().is_some_and(|c| c == &current_key))
                            {
                                // Blue for left stick
                                button = button.fill(Color32::from_rgb(50, 100, 180)).selected(true);
                            } else if sel1
                                || (self.selected.1.is_none()
                                    && right_center.as_ref().is_some_and(|c| c == &current_key))
                            {
                                // Green for right stick
                                button = button.fill(Color32::from_rgb(50, 150, 80)).selected(true);
                            }
                        }

                        let size = Vec2::new(button_size.x * key.width, row_height);
                        let response = ui.add_sized(size, button);

                        // Overlay small indicator for Ctrl/Alt on the Space key in the bottom left
                        if key.key.normal == " " && (self.ctrl_mod || self.alt_mod) {
                            let mut mods = Vec::new();
                            if self.ctrl_mod {
                                mods.push("ctrl");
                            }
                            if self.shift_mod {
                                mods.push("shift");
                            }
                            if self.alt_mod {
                                mods.push("alt");
                            }
                            let mod_string = mods.join("+");
                            let rect = response.rect;
                            let font_size = self.layout.font_size * 0.6;
                            ui.painter().text(
                                rect.left_bottom() + Vec2::new(4.0, -4.0),
                                egui::Align2::LEFT_BOTTOM,
                                mod_string,
                                egui::FontId::proportional(font_size),
                                Color32::WHITE,
                            );
                        }

                        if response.clicked() {
                            pressed_key = Some(key.key.get(self.shift_state));
                        }
                    }
                });
            }

            if DEBUG {
                let painter = ui.painter();

                let (x0, y0, x1, y1) = self.left_selectable_bounds;
                let r = egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1));
                painter.rect_stroke(
                    r,
                    egui::CornerRadius::default(),
                    egui::Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 255)),
                    egui::StrokeKind::Middle,
                );

                for row in &self.layout.key_hit_boxes {
                    for h in row {
                        if let Some((x, y, r)) = *h {
                            painter.circle_stroke(
                                (x, y).into(),
                                r,
                                egui::Stroke::new(
                                    1.0,
                                    Color32::from_rgba_premultiplied(0, 192, 255, 128),
                                ),
                            );
                        }
                    }
                }
            }
        });

        pressed_key
    }
}

const DEBUG: bool = false;

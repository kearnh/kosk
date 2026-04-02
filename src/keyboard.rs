use anyhow::Result;
use egui::{Button, Color32, Ui, Vec2};
use enigo::{Enigo, Keyboard as _};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::{collections::HashMap, hint::unreachable_unchecked};

#[derive(PartialEq, Eq, Debug, Clone, Deserialize)]
#[serde(try_from = "String")]
pub enum RawKey {
    Key(String),
    Enter,
    Skip,
    Shift,
}

impl TryFrom<String> for RawKey {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.to_uppercase().as_str() {
            "ENTER" => Ok(RawKey::Enter),
            "SKIP" => Ok(RawKey::Skip),
            "SHIFT" => Ok(RawKey::Shift),
            _ => Ok(RawKey::Key(value)),
        }
    }
}

impl RawKey {
    pub fn send(self, enigo: &mut Enigo) -> Result<()> {
        match self {
            RawKey::Key(k) => enigo.text(&k)?,
            RawKey::Enter => enigo.key(enigo::Key::Return, enigo::Direction::Click)?,
            _ => (),
        }
        Ok(())
    }
}

impl ToString for RawKey {
    fn to_string(&self) -> String {
        match self {
            RawKey::Key(k) => k.clone(),
            RawKey::Enter => "Enter".to_string(),
            RawKey::Skip => unsafe { unreachable_unchecked() },
            RawKey::Shift => "Shift".to_string(),
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
    pos: KeyPos,
    width: f32, // Width multiplier (1.0 = normal, 5.0 = space bar)
}

#[derive(Debug)]
pub struct KeyboardLayout {
    row_indents: Vec<f32>, // Indent in pixels for each row
    rows: Vec<Vec<KeyButton>>,
    dims: (f32, f32),

    // (x, y, r) where (x, y) is center of key, and r is a radius. This will not overlap exactly
    // with a key button, and key circles may overlap each other. Key selection for highlight will
    // use closest center, will key press will return all overlapping keys to allow typo resistance,
    // i.e. may decide on key press based on engligh word etc.
    key_pos: Vec<Vec<(f32, f32, f32)>>,
}

impl KeyboardLayout {
    fn load(toml: &str) -> Result<Self> {
        #[derive(Debug, Clone, Deserialize)]
        struct KeyboardLayoutFile {
            keys: Vec<KeyButton>,
            row_indents: Vec<f32>, // Indent in pixels for each row
        }
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
            rows,
            dims: Default::default(),
            key_pos: Default::default(),
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

    // FIXME calculate key_pos
    fn calculate_geometry(&mut self) {
        let num_rows = self.rows.len();

        // Find the row with the most total width (considering key widths and indents)
        let mut max_width: f32 = 0.0;
        for (row_idx, row) in self.rows.iter().enumerate() {
            if row.is_empty() {
                continue;
            }

            let indent = self.row_indents.get(row_idx).copied().unwrap_or(0.0);
            let mut row_width = indent;

            for (i, key) in row.iter().enumerate() {
                row_width += BUTTON_UNIT_WIDTH * key.width;
                if i < row.len() - 1 {
                    row_width += UNIT_SPACING_X;
                }
            }

            max_width = max_width.max(row_width);
        }

        // Calculate height: num_rows * button_height + (num_rows - 1) * spacing
        let height =
            (num_rows as f32) * BUTTON_UNIT_HEIGHT + ((num_rows - 1) as f32) * UNIT_SPACING_Y;

        self.dims = (max_width, height);
    }
}

pub type KeyPos = (usize, usize);

pub struct Keyboard {
    pub selected: (Option<KeyPos>, Option<KeyPos>),
    pub layout: KeyboardLayout,
    pub shift_state: bool,
}

impl Keyboard {
    pub fn new() -> Self {
        let default = include_str!("default.toml");
        Self {
            selected: (None, None),
            layout: KeyboardLayout::load(&default).expect("Failed to load default layout"),
            shift_state: false,
        }
    }

    pub fn with_layout_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let layout = KeyboardLayout::load_from_file(path)?;
        Ok(Self {
            selected: (None, None),
            layout,
            shift_state: false,
        })
    }

    pub fn toggle_shift(&mut self) {
        self.shift_state = !self.shift_state;
    }

    fn is_selected(&self, pos: KeyPos) -> bool {
        self.selected.0.is_some_and(|s| s == pos) || self.selected.1.is_some_and(|s| s == pos)
    }
}

// FIXME should be part of layout
const BUTTON_UNIT_WIDTH: f32 = 40.0;
const BUTTON_UNIT_HEIGHT: f32 = 40.0;
const UNIT_SPACING_X: f32 = 2.0;
const UNIT_SPACING_Y: f32 = 2.0;

/// Creates a QWERTY keyboard UI in egui
/// Returns the key that was pressed, if any
pub fn draw_ui(ui: &mut Ui, kb: &Keyboard) -> Option<RawKey> {
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

    let button_size = Vec2::new(BUTTON_UNIT_WIDTH, BUTTON_UNIT_HEIGHT);

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(UNIT_SPACING_X, UNIT_SPACING_Y);

        for (row_idx, keys) in kb.layout.rows.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(UNIT_SPACING_X, UNIT_SPACING_Y);

                // Add indent for this row
                let indent = kb.layout.row_indents.get(row_idx).copied().unwrap_or(0.0);
                ui.add_space(indent);

                for key in keys {
                    // Skip rendering for SKIP keys - just add space
                    if key.key.normal == RawKey::Skip {
                        ui.add_space(button_size.x * key.width);
                        continue;
                    }

                    // Determine what to display based on shift state
                    let display_label = if let Some(d) = &key.display {
                        d.get(kb.shift_state)
                    } else {
                        key.key.display(kb.shift_state)
                    };

                    let mut button = Button::new(&display_label);

                    if key.key.normal == RawKey::Shift && kb.shift_state {
                        button = button.selected(true);
                    } else if kb.is_selected(key.pos) {
                        button = button.selected(true);
                    }

                    let size = Vec2::new(button_size.x * key.width, button_size.y);
                    if ui.add_sized(size, button).clicked() {
                        pressed_key = Some(key.key.get(kb.shift_state));
                    }
                }
            });
        }
    });

    pressed_key
}

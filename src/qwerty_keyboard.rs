use std::collections::HashMap;
use std::fs;
use std::path::Path;

use egui::{pos2, Button, Color32, Pos2, Ui, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Key {
    pub key: String,
    pub shift_key: Option<String>, // None means use uppercase of key
    // pub x: f32,                    // Normalized position for stick mapping (-1.0 to 1.0)
    // pub y: f32,                    // Normalized position for stick mapping (-1.0 to 1.0)
    pub row: usize,
    pub col: usize,
    pub width: f32, // Width multiplier (1.0 = normal, 5.0 = space bar)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyboardLayout {
    pub keys: Vec<Key>,
    pub row_indents: Vec<f32>, // Indent in pixels for each row
    #[serde(skip)]
    rows: Vec<Vec<Key>>,
}

impl KeyboardLayout {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let contents = fs::read_to_string(path)?;
        let mut layout: KeyboardLayout = toml::from_str(&contents)?;
        layout.validate()?;
        layout.compute_rows();
        Ok(layout)
    }

    fn validate(&self) -> anyhow::Result<()> {
        // Find max row number
        let max_row = self.keys.iter().map(|k| k.row).max().unwrap_or(0);

        // Check that row_indents length matches number of rows
        if self.row_indents.len() != max_row + 1 {
            anyhow::bail!(
                "row_indents length ({}) doesn't match number of rows ({})",
                self.row_indents.len(),
                max_row + 1
            );
        }

        // Check for duplicate (row, col) pairs
        let mut seen = std::collections::HashSet::new();
        for key in &self.keys {
            let coord = (key.row, key.col);
            if !seen.insert(coord) {
                anyhow::bail!("Duplicate key at row {}, col {}", key.row, key.col);
            }
        }

        Ok(())
    }

    pub fn default_qwerty() -> Self {
        let mut keys = Vec::new();

        // Row 0 - Numbers (no indent)
        let row0 = [
            ("1", "!"),
            ("2", "@"),
            ("3", "#"),
            ("4", "$"),
            ("5", "%"),
            ("6", "^"),
            ("7", "&"),
            ("8", "*"),
            ("9", "("),
            ("0", ")"),
        ];
        for (col, (normal, shifted)) in row0.iter().enumerate() {
            keys.push(Key {
                key: normal.to_string(),
                shift_key: Some(shifted.to_string()),
                row: 0,
                col,
                width: 1.0,
            });
        }

        // Row 1 - qwertyuiop (0.25 key indent)
        let row1 = ["q", "w", "e", "r", "t", "y", "u", "i", "o", "p"];
        for (col, label) in row1.iter().enumerate() {
            keys.push(Key {
                key: label.to_string(),
                shift_key: None,
                row: 1,
                col,
                width: 1.0,
            });
        }

        // Row 2 - asdfghjkl (0.5 key indent)
        let row2 = ["a", "s", "d", "f", "g", "h", "j", "k", "l"];
        for (col, label) in row2.iter().enumerate() {
            keys.push(Key {
                key: label.to_string(),
                shift_key: None,
                row: 2,
                col,
                width: 1.0,
            });
        }

        // Shift key
        keys.push(Key {
            key: "SHIFT".to_string(),
            shift_key: Some("SHIFT".to_string()),
            row: 3,
            col: 0,
            width: 1.5,
        });

        // Row 3 - zxcvbnm + Shift (0.75 key indent)
        let row3 = ["z", "x", "c", "v", "b", "n", "m"];
        for (col, label) in row3.iter().enumerate() {
            keys.push(Key {
                key: label.to_string(),
                shift_key: None,
                row: 3,
                col: col + 1,
                width: 1.0,
            });
        }

        // Row 4 - Space bar (centered)
        keys.push(Key {
            key: " ".to_string(),
            shift_key: Some(" ".to_string()),
            row: 4,
            col: 0,
            width: 5.0,
        });

        let row_indents = vec![15.0, 26.25, 37.5, 0.0, 115.0];

        let mut layout = KeyboardLayout {
            keys,
            row_indents,
            rows: Vec::new(),
        };
        layout.compute_rows();
        layout
    }

    fn compute_rows(&mut self) {
        // Group keys by row
        let mut row_map: HashMap<usize, Vec<Key>> = HashMap::new();
        for key in &self.keys {
            row_map
                .entry(key.row)
                .or_insert_with(Vec::new)
                .push(key.clone());
        }

        // Sort by row number and store
        let mut sorted_rows: Vec<_> = row_map.into_iter().collect();
        sorted_rows.sort_by_key(|(row_num, _)| *row_num);

        self.rows = sorted_rows.into_iter().map(|(_, keys)| keys).collect();
    }

    pub fn rows(&self) -> &[Vec<Key>] {
        &self.rows
    }

    /// Calculate the approximate size needed to display this keyboard layout
    /// Returns (width, height) in pixels
    pub fn calculate_size(&self) -> (f32, f32) {
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
                row_width += BUTTON_SIZE * key.width;
                if i < row.len() - 1 {
                    row_width += SPACING;
                }
            }

            max_width = max_width.max(row_width);
        }

        // Calculate height: num_rows * button_height + (num_rows - 1) * spacing
        let height = (num_rows as f32) * BUTTON_SIZE + ((num_rows - 1) as f32) * SPACING;

        (max_width, height)
    }
}

pub struct Keyboard {
    pub highlight: (&'static str, &'static str),
    pub layout: KeyboardLayout,
    pub shift_state: bool,
}

impl Keyboard {
    pub fn new() -> Self {
        Self {
            highlight: ("", ""),
            layout: KeyboardLayout::default_qwerty(),
            shift_state: false,
        }
    }

    pub fn with_layout_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let layout = KeyboardLayout::load_from_file(path)?;
        Ok(Self {
            highlight: ("", ""),
            layout,
            shift_state: false,
        })
    }

    pub fn toggle_shift(&mut self) {
        self.shift_state = !self.shift_state;
    }

    pub fn is_shift(&self) -> bool {
        self.shift_state
    }
}

pub const BUTTON_SIZE: f32 = 40.0;
pub const SPACING: f32 = 5.0;

/// Creates a QWERTY keyboard UI in egui
/// Returns the key that was pressed, if any
pub fn qwerty_keyboard(ui: &mut Ui, kb: &Keyboard) -> Option<String> {
    let mut pressed_key: Option<String> = None;

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

    let button_size = Vec2::new(BUTTON_SIZE, BUTTON_SIZE);
    let spacing = SPACING;

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(spacing, spacing);

        for (row_idx, keys) in kb.layout.rows().iter().enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(spacing, spacing);

                // Add indent for this row
                let indent = kb.layout.row_indents.get(row_idx).copied().unwrap_or(0.0);
                ui.add_space(indent);

                for key in keys {
                    // Determine what to display based on shift state
                    let display_label = if kb.shift_state {
                        key.shift_key
                            .clone()
                            .unwrap_or_else(|| key.key.to_uppercase())
                    } else {
                        key.key.clone()
                    };

                    let mut button = Button::new(&display_label);

                    // Highlight shift key when shift is active
                    if key.key == "SHIFT" && kb.shift_state {
                        button = button.selected(true);
                    } else if kb.highlight.0 == key.key || kb.highlight.1 == key.key {
                        button = button.selected(true);
                    }

                    let size = Vec2::new(button_size.x * key.width, button_size.y);
                    if ui.add_sized(size, button).clicked() {
                        if key.key == "SHIFT" {
                            // Return SHIFT as a special key press
                            pressed_key = Some("SHIFT".to_string());
                        } else {
                            pressed_key = Some(if kb.shift_state {
                                key.shift_key
                                    .clone()
                                    .unwrap_or_else(|| key.key.to_uppercase())
                            } else {
                                key.key.clone()
                            });
                        }
                    }
                }
            });
        }
    });

    pressed_key
}

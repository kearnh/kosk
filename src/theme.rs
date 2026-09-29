use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Context, Result};
use egui::{Color32, Stroke, Visuals};
use serde::{Deserialize, Serialize};

pub(crate) const DEFAULT_THEME_NAME: &str = "default";

pub(crate) fn color(rgba: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3])
}

macro_rules! section {
    ($name:ident { $($field:ident: $ty:ty = $default:expr),* $(,)? }) => {
        #[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
        #[serde(default, deny_unknown_fields)]
        pub(crate) struct $name { $(pub(crate) $field: $ty),* }

        impl Default for $name {
            fn default() -> Self { Self { $($field: $default),* } }
        }
    };
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct WidgetTheme {
    pub(crate) background_color: [u8; 4],
    pub(crate) weak_background_color: [u8; 4],
    pub(crate) text_color: [u8; 4],
    pub(crate) border_color: [u8; 4],
    pub(crate) border_width: f32,
    pub(crate) corner_radius: f32,
}

impl Default for WidgetTheme {
    fn default() -> Self {
        Self::from_visuals(&Visuals::default().widgets.inactive)
    }
}

impl WidgetTheme {
    fn from_visuals(visuals: &egui::style::WidgetVisuals) -> Self {
        Self {
            background_color: visuals.bg_fill.to_srgba_unmultiplied(),
            weak_background_color: visuals.weak_bg_fill.to_srgba_unmultiplied(),
            text_color: visuals.fg_stroke.color.to_srgba_unmultiplied(),
            border_color: visuals.bg_stroke.color.to_srgba_unmultiplied(),
            border_width: visuals.bg_stroke.width,
            corner_radius: visuals.corner_radius.nw as f32,
        }
    }

    fn keyboard(visuals: &egui::style::WidgetVisuals, fill: Color32) -> Self {
        Self {
            background_color: fill.to_srgba_unmultiplied(),
            weak_background_color: fill.to_srgba_unmultiplied(),
            text_color: Color32::WHITE.to_srgba_unmultiplied(),
            ..Self::from_visuals(visuals)
        }
    }

    pub(crate) fn apply(&self, visuals: &mut egui::style::WidgetVisuals) {
        visuals.bg_fill = color(self.background_color);
        visuals.weak_bg_fill = color(self.weak_background_color);
        visuals.fg_stroke.color = color(self.text_color);
        visuals.bg_stroke = Stroke::new(self.border_width, color(self.border_color));
        visuals.corner_radius = self.corner_radius.into();
    }

    fn validate(&self) -> Result<()> {
        validate_size("border_width", self.border_width)?;
        validate_radius(self.corner_radius)
    }
}

section!(Theme {
    background_color: [u8; 4] = [20, 20, 20, 255],
    keyboard_opacity: f32 = 0.7,
    ui_opacity: f32 = 1.0,
    text_color: [u8; 4] = Visuals::default().text_color().to_srgba_unmultiplied(),
    muted_text_color: [u8; 4] = Visuals::default().weak_text_color().to_srgba_unmultiplied(),
    selection_background_color: [u8; 4] =
        Visuals::default().selection.bg_fill.to_srgba_unmultiplied(),
    selection_border_color: [u8; 4] = Visuals::default()
        .selection
        .stroke
        .color
        .to_srgba_unmultiplied(),
    selection_border_width: f32 = Visuals::default().selection.stroke.width,
    window_border_color: [u8; 4] = Visuals::default()
        .window_stroke
        .color
        .to_srgba_unmultiplied(),
    window_border_width: f32 = Visuals::default().window_stroke.width,
    window_corner_radius: f32 = Visuals::default().window_corner_radius.nw as f32,
    noninteractive: WidgetTheme =
        WidgetTheme::from_visuals(&Visuals::default().widgets.noninteractive),
    inactive: WidgetTheme = WidgetTheme::from_visuals(&Visuals::default().widgets.inactive),
    hovered: WidgetTheme = WidgetTheme::from_visuals(&Visuals::default().widgets.hovered),
    active: WidgetTheme = WidgetTheme::from_visuals(&Visuals::default().widgets.active),
    open: WidgetTheme = WidgetTheme::from_visuals(&Visuals::default().widgets.open),
    keyboard: KeyboardTheme = KeyboardTheme::default(),
    suggestions: SuggestionsTheme = SuggestionsTheme::default(),
    text_input: TextInputTheme = TextInputTheme::default(),
    menus: MenusTheme = MenusTheme::default(),
    mappings: MappingsTheme = MappingsTheme::default(),
    move_window: MoveWindowTheme = MoveWindowTheme::default(),
    notifications: NotificationsTheme = NotificationsTheme::default(),
    battery: BatteryTheme = BatteryTheme::default(),
});

section!(KeyboardTheme {
    inactive: WidgetTheme = WidgetTheme::keyboard(
        &Visuals::default().widgets.inactive,
        Color32::from_rgba_premultiplied(60, 60, 60, 128)
    ),
    hovered: WidgetTheme = WidgetTheme::keyboard(
        &Visuals::default().widgets.hovered,
        Color32::from_rgba_premultiplied(80, 80, 80, 180)
    ),
    active: WidgetTheme = WidgetTheme::keyboard(
        &Visuals::default().widgets.active,
        Color32::from_rgba_premultiplied(100, 100, 100, 200)
    ),
    selection_background_color: [u8; 4] =
        Color32::from_rgba_premultiplied(50, 100, 180, 220).to_srgba_unmultiplied(),
    selection_text_color: [u8; 4] = [255, 255, 255, 255],
    left_selection_color: [u8; 4] = [50, 100, 180, 255],
    right_selection_color: [u8; 4] = [50, 150, 80, 255],
    dual_selection_color: [u8; 4] = [120, 60, 180, 255],
    modifier_text_color: [u8; 4] = [255, 255, 255, 255],
    key_press_color: [u8; 4] = [255, 255, 255, 180],
    key_press_duration_ms: u16 = 180,
    key_groups: Vec<KeyColorGroup> = Vec::new(),
});

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct KeyColorGroup {
    pub(crate) keys: Vec<String>,
    pub(crate) background_color: Option<[u8; 4]>,
    pub(crate) text_color: Option<[u8; 4]>,
}

impl KeyColorGroup {
    fn validate(&self) -> Result<()> {
        if self.keys.is_empty() || self.keys.iter().any(String::is_empty) {
            bail!("keyboard.key_groups needs nonempty keys");
        }
        if self.background_color.is_none() && self.text_color.is_none() {
            bail!("keyboard.key_groups needs background_color or text_color");
        }
        Ok(())
    }
}

section!(SuggestionsTheme {
    background_color: [u8; 4] = [64, 68, 76, 175],
    text_color: [u8; 4] = [230, 230, 230, 255],
    selected_background_color: [u8; 4] = [74, 114, 164, 215],
    selected_text_color: [u8; 4] = [255, 255, 255, 255],
    empty_slot_background: [u8; 4] = [50, 54, 62, 90],
    armed_color: [u8; 4] = [50, 200, 90, 255],
    disarmed_color: [u8; 4] = [128, 128, 128, 255],
    new_word_mark_color: [u8; 4] = [50, 200, 90, 255],
    corner_radius: f32 = 10.0,
    selected_outline_width: f32 = 1.0,
});

section!(TextInputTheme {
    background_color: [u8; 4] = [255, 255, 255, 255],
    text_color: [u8; 4] = [0, 0, 0, 255],
    cursor_color: [u8; 4] = [0, 0, 0, 255],
});

section!(MenusTheme {
    heading_color: [u8; 4] = [255, 255, 255, 255],
    muted_text_color: [u8; 4] = [128, 128, 128, 255],
});

section!(MappingsTheme {
    row_focus_color: [u8; 4] = [40, 90, 160, 80],
    table_focus_color: [u8; 4] = [40, 90, 160, 40],
    focus_border_color: [u8; 4] = [80, 160, 255, 255],
    focus_border_width: f32 = 2.0,
    text_color: [u8; 4] = [255, 255, 255, 255],
    warning_color: [u8; 4] = [255, 255, 0, 255],
    error_color: [u8; 4] = [255, 120, 120, 255],
    success_color: [u8; 4] = [140, 220, 140, 255],
    unsaved_color: [u8; 4] = [255, 180, 60, 255],
    editor_background_color: [u8; 4] = [28, 28, 32, 255],
    editor_border_color: [u8; 4] = [140, 140, 150, 255],
    editor_border_width: f32 = 1.5,
    editor_corner_radius: f32 = Visuals::default().window_corner_radius.nw as f32,
});

section!(MoveWindowTheme {
    background_color: [u8; 4] = [16, 16, 16, 48],
    border_color: [u8; 4] = [255, 255, 255, 210],
    border_width: f32 = 2.0,
    corner_radius: f32 = 4.0,
    text_color: [u8; 4] = [255, 255, 255, 255],
    text_shadow_color: [u8; 4] = [0, 0, 0, 255],
});

section!(NotificationsTheme {
    background_color: [u8; 4] = [28, 28, 30, 245],
    border_color: [u8; 4] = [255, 255, 255, 26],
    border_width: f32 = 1.0,
    corner_radius: f32 = 8.0,
    muted_text_color: [u8; 4] = [170, 170, 175, 255],
    info_color: [u8; 4] = [70, 180, 220, 255],
    warning_color: [u8; 4] = [230, 140, 40, 255],
    error_color: [u8; 4] = [220, 50, 50, 255],
});

section!(BatteryTheme {
    empty: [u8; 4] = [220, 50, 50, 255],
    low: [u8; 4] = [230, 140, 40, 255],
    medium: [u8; 4] = [230, 200, 60, 255],
    high: [u8; 4] = [120, 190, 80, 255],
    full: [u8; 4] = [50, 200, 90, 255],
    charging: [u8; 4] = [70, 180, 220, 255],
    unknown: [u8; 4] = [180, 180, 180, 255],
});

impl Theme {
    #[cfg(test)]
    pub(crate) fn parse(text: &str) -> Result<Self> {
        let mut overlay: toml::Value = toml::from_str(text).context("parse theme")?;
        if let Some(table) = overlay.as_table_mut() {
            table.remove("name");
        }
        Self::parse_overlay(overlay)
    }

    fn parse_overlay(overlay: toml::Value) -> Result<Self> {
        let mut base = toml::Value::try_from(Self::default()).context("theme defaults")?;
        if let Some(text_color) = overlay.get("text_color") {
            for state in ["noninteractive", "inactive", "hovered", "active", "open"] {
                base[state]["text_color"] = text_color.clone();
            }
        }
        crate::config_overlay::merge_toml(&mut base, &overlay);
        let theme: Self = base.try_into().context("parse theme")?;
        theme.validate()?;
        Ok(theme)
    }

    fn validate(&self) -> Result<()> {
        for group in &self.keyboard.key_groups {
            group.validate()?;
        }
        for (name, opacity) in [
            ("keyboard_opacity", self.keyboard_opacity),
            ("ui_opacity", self.ui_opacity),
        ] {
            if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
                bail!("{name} must be finite and between 0 and 1");
            }
        }
        for widget in [
            &self.noninteractive,
            &self.inactive,
            &self.hovered,
            &self.active,
            &self.open,
            &self.keyboard.inactive,
            &self.keyboard.hovered,
            &self.keyboard.active,
        ] {
            widget.validate()?;
        }
        for (name, value) in [
            ("selection_border_width", self.selection_border_width),
            ("window_border_width", self.window_border_width),
            (
                "suggestions.selected_outline_width",
                self.suggestions.selected_outline_width,
            ),
            (
                "mappings.focus_border_width",
                self.mappings.focus_border_width,
            ),
            (
                "mappings.editor_border_width",
                self.mappings.editor_border_width,
            ),
            ("move_window.border_width", self.move_window.border_width),
            (
                "notifications.border_width",
                self.notifications.border_width,
            ),
        ] {
            validate_size(name, value)?;
        }
        for radius in [
            self.window_corner_radius,
            self.suggestions.corner_radius,
            self.mappings.editor_corner_radius,
            self.move_window.corner_radius,
            self.notifications.corner_radius,
        ] {
            validate_radius(radius)?;
        }
        Ok(())
    }

    pub(crate) fn visuals(&self) -> Visuals {
        let mut visuals = Visuals {
            panel_fill: color(self.background_color),
            window_fill: color(self.background_color),
            weak_text_color: Some(color(self.muted_text_color)),
            ..Visuals::default()
        };
        visuals.selection.bg_fill = color(self.selection_background_color);
        visuals.selection.stroke = Stroke::new(
            self.selection_border_width,
            color(self.selection_border_color),
        );
        visuals.window_stroke =
            Stroke::new(self.window_border_width, color(self.window_border_color));
        visuals.window_corner_radius = self.window_corner_radius.into();
        self.noninteractive
            .apply(&mut visuals.widgets.noninteractive);
        self.inactive.apply(&mut visuals.widgets.inactive);
        self.hovered.apply(&mut visuals.widgets.hovered);
        self.active.apply(&mut visuals.widgets.active);
        self.open.apply(&mut visuals.widgets.open);
        visuals
    }

    pub(crate) fn window_visuals(&self, transparent: bool, opacity: f32) -> Visuals {
        let mut visuals = self.visuals();
        let [r, g, b, a] = self.background_color;
        visuals.panel_fill = color([
            r,
            g,
            b,
            if transparent {
                (a as f32 * opacity.clamp(0.0, 1.0)).round() as u8
            } else {
                u8::MAX
            },
        ]);
        visuals.window_fill = if transparent {
            Color32::TRANSPARENT
        } else {
            visuals.panel_fill
        };
        visuals
    }
}

fn validate_size(name: &str, value: f32) -> Result<()> {
    if !value.is_finite() || value < 0.0 {
        bail!("{name} must be finite and nonnegative");
    }
    Ok(())
}

fn validate_radius(value: f32) -> Result<()> {
    validate_size("corner_radius", value)?;
    if value > u8::MAX as f32 {
        bail!("corner_radius must be at most 255");
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) struct ThemeCatalog(BTreeMap<String, Theme>);

impl Default for ThemeCatalog {
    fn default() -> Self {
        Self(BTreeMap::from([(
            DEFAULT_THEME_NAME.to_owned(),
            Theme::default(),
        )]))
    }
}

impl ThemeCatalog {
    pub(crate) fn load(
        config_path: &Path,
        files: &[String],
    ) -> Result<(Self, Vec<std::path::PathBuf>)> {
        let mut catalog = Self::default();
        let mut skipped = Vec::new();
        for path in expand_theme_files(config_path, files)? {
            let loaded = (|| {
                let text = std::fs::read_to_string(&path)
                    .with_context(|| format!("read theme {}", path.display()))?;
                let mut overlay: toml::Value = toml::from_str(&text)
                    .with_context(|| format!("parse theme {}", path.display()))?;
                let name = overlay
                    .as_table_mut()
                    .and_then(|table| table.remove("name"))
                    .ok_or_else(|| anyhow::anyhow!("theme file is missing required name"))?
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("theme name must be a string"))?
                    .to_owned();
                if name.trim().is_empty() || name == DEFAULT_THEME_NAME {
                    bail!("theme name {name:?} is empty or reserved");
                }
                let theme = Theme::parse_overlay(overlay)
                    .with_context(|| format!("theme {name:?} ({})", path.display()))?;
                Ok::<_, anyhow::Error>((name, theme))
            })();

            match loaded {
                Ok((name, theme)) if !catalog.0.contains_key(&name) => {
                    catalog.0.insert(name, theme);
                }
                Ok(_) | Err(_) => {
                    skipped.push(path);
                }
            }
        }
        Ok((catalog, skipped))
    }

    pub(crate) fn get(&self, name: &str) -> &Theme {
        self.0.get(name).unwrap_or(&self.0[DEFAULT_THEME_NAME])
    }

    pub(crate) fn names(&self) -> Vec<&str> {
        std::iter::once(DEFAULT_THEME_NAME)
            .chain(
                self.0
                    .keys()
                    .filter(|name| name.as_str() != DEFAULT_THEME_NAME)
                    .map(String::as_str),
            )
            .collect()
    }

    pub(crate) fn contains(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    #[cfg(test)]
    pub(crate) fn with_names(names: &[&str]) -> Self {
        let mut catalog = Self::default();
        for name in names {
            catalog.0.insert((*name).to_owned(), Theme::default());
        }
        catalog
    }
}

pub(crate) fn expand_theme_files(
    config_path: &Path,
    files: &[String],
) -> Result<Vec<std::path::PathBuf>> {
    let config_dir = config_path.parent().unwrap_or(Path::new(""));
    let mut paths = Vec::new();
    for file in files {
        let pattern_path = if Path::new(file).is_absolute() {
            file.clone()
        } else {
            config_dir.join(file).to_string_lossy().into_owned()
        };
        let pattern =
            glob::glob(&pattern_path).with_context(|| format!("invalid theme glob {file:?}"))?;
        let mut matches = pattern
            .map(|entry| entry.with_context(|| format!("expand theme glob {file:?}")))
            .collect::<Result<Vec<_>>>()?;
        matches.sort();
        if matches.is_empty() && !file.contains(['*', '?', '[']) {
            matches.push(Path::new(&pattern_path).to_path_buf());
        }
        for path in matches {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_root_is_the_shared_scope() {
        let theme = Theme::parse("background_color = [1, 2, 3, 255]\ntext_color = [4, 5, 6, 255]\n[hovered]\ntext_color = [7, 8, 9, 255]\n[suggestions]\ntext_color = [10, 11, 12, 255]").unwrap();
        let visuals = theme.visuals();
        assert_eq!(visuals.panel_fill, color([1, 2, 3, 255]));
        assert_eq!(visuals.text_color(), color([4, 5, 6, 255]));
        assert_eq!(visuals.widgets.hovered.text_color(), color([7, 8, 9, 255]));
        assert_eq!(theme.suggestions.text_color, [10, 11, 12, 255]);
        let serialized = toml::Value::try_from(theme).unwrap();
        assert!(serialized.get("background_color").is_some());
        assert!(serialized.get("shared").is_none());
        assert!(Theme::parse("[shared]\nbackground_color = [1, 2, 3, 255]").is_err());
    }

    #[test]
    fn sparse_theme_inherits_defaults() {
        let theme = Theme::parse("[suggestions]\nbackground_color = [1, 2, 3, 4]\n").unwrap();
        assert_eq!(theme.suggestions.background_color, [1, 2, 3, 4]);
        assert_eq!(theme.keyboard, Theme::default().keyboard);
        assert_eq!(theme.suggestions.corner_radius, 10.0);
        let partial_widget = Theme::parse("[hovered]\nbackground_color = [1, 2, 3, 255]").unwrap();
        assert_eq!(
            partial_widget.hovered.border_width,
            Theme::default().hovered.border_width
        );
    }

    #[test]
    fn invalid_theme_values_are_rejected() {
        for text in [
            "unknown = 1",
            "[keyboard]\nunknown = 1",
            "[suggestions]\nbackground_color = [1, 2, 3]",
            "[suggestions]\nbackground_color = [256, 0, 0, 255]",
            "[suggestions]\ncorner_radius = -1.0",
            "[notifications]\nborder_width = nan",
            "[move_window]\ncorner_radius = 256.0",
            "keyboard_opacity = -0.1",
            "keyboard_opacity = 1.1",
            "ui_opacity = nan",
            "ui_opacity = inf",
        ] {
            assert!(Theme::parse(text).is_err(), "{text}");
        }
    }

    #[test]
    fn key_groups_are_sparse_and_strict() {
        let theme = Theme::parse(
            "[[keyboard.key_groups]]\nkeys = ['Return', 'exit']\nbackground_color = [1, 2, 3, 4]\n\
             [[keyboard.key_groups]]\nkeys = [' ']\ntext_color = [5, 6, 7, 8]",
        )
        .unwrap();
        assert_eq!(theme.keyboard.key_groups.len(), 2);
        assert_eq!(theme.keyboard.key_groups[0].text_color, None);
        assert_eq!(theme.keyboard.key_groups[1].background_color, None);
        assert!(Theme::default().keyboard.key_groups.is_empty());
        assert_eq!(Theme::parse("").unwrap(), Theme::default());

        for body in [
            "keys = []\nbackground_color = [1, 2, 3, 4]",
            "keys = ['q', '']\nbackground_color = [1, 2, 3, 4]",
            "keys = ['q']",
            "background_color = [1, 2, 3, 4]",
            "keys = ['q']\nunknown = 1\ntext_color = [1, 2, 3, 4]",
            "keys = ['q']\nbackground_color = [1, 2, 3]",
            "keys = ['q']\ntext_color = [256, 2, 3, 4]",
        ] {
            assert!(
                Theme::parse(&format!("[[keyboard.key_groups]]\n{body}")).is_err(),
                "{body}"
            );
        }
    }

    #[test]
    fn shared_widget_text_colors_retain_state_defaults_and_overrides() {
        let defaults = Visuals::default();
        let builtin = Theme::default().visuals();
        assert_eq!(builtin.override_text_color, defaults.override_text_color);
        assert_eq!(builtin.widgets.hovered, defaults.widgets.hovered);
        let theme =
            Theme::parse("text_color = [1, 2, 3, 255]\n[hovered]\ntext_color = [4, 5, 6, 255]")
                .unwrap();
        let visuals = theme.visuals();
        assert_eq!(visuals.text_color(), color([1, 2, 3, 255]));
        assert_eq!(visuals.widgets.inactive.text_color(), color([1, 2, 3, 255]));
        assert_eq!(visuals.widgets.hovered.text_color(), color([4, 5, 6, 255]));
    }

    #[test]
    fn window_background_preserves_rgb_and_applies_opacity() {
        let theme = Theme::parse("background_color = [20, 40, 60, 0]").unwrap();
        let opaque = theme.window_visuals(false, 0.0);
        assert_eq!(opaque.panel_fill, Color32::from_rgb(20, 40, 60));
        assert_eq!(opaque.window_fill, opaque.panel_fill);
        assert_eq!(
            theme.window_visuals(true, 1.0).panel_fill,
            Color32::TRANSPARENT
        );
        let theme = Theme::parse("background_color = [20, 40, 60, 128]").unwrap();
        let transparent = theme.window_visuals(true, 0.5);
        assert_eq!(transparent.panel_fill, color([20, 40, 60, 64]));
        assert_eq!(transparent.window_fill, Color32::TRANSPARENT);
        assert_eq!(
            theme.window_visuals(true, 0.0).panel_fill,
            Color32::TRANSPARENT
        );
    }

    #[test]
    fn keyboard_defaults_preserve_premultiplied_colors() {
        let keyboard = Theme::default().keyboard;
        assert_eq!(
            color(keyboard.inactive.background_color),
            Color32::from_rgba_premultiplied(60, 60, 60, 128)
        );
        assert_eq!(
            color(keyboard.selection_background_color),
            Color32::from_rgba_premultiplied(50, 100, 180, 220)
        );
    }

    #[test]
    fn old_steam_controller_palette_matches_reference() {
        let theme = Theme::parse(include_str!("../themes/old-steam-controller.toml")).unwrap();
        assert_eq!(theme.keyboard_opacity, 1.0);
        assert_eq!(theme.ui_opacity, 1.0);
        assert_eq!(theme.background_color, [15, 40, 61, 255]);
        assert_eq!(theme.keyboard.inactive.background_color, [25, 62, 87, 255]);
        assert_eq!(
            theme.keyboard.inactive.weak_background_color,
            [25, 62, 87, 255]
        );
        assert_eq!(theme.keyboard.inactive.text_color, [163, 163, 163, 255]);
        assert_eq!(theme.keyboard.hovered.background_color, [39, 81, 108, 255]);
    }

    #[test]
    fn themes_own_keyboard_and_menu_opacity() {
        let theme = Theme::parse("keyboard_opacity = 0.7\nui_opacity = 0.4").unwrap();
        let serialized = toml::Value::try_from(theme).unwrap();
        assert_eq!(
            serialized["keyboard_opacity"].as_float(),
            Some(0.7_f32 as f64)
        );
        assert_eq!(serialized["ui_opacity"].as_float(), Some(0.4_f32 as f64));
    }
}

//! Single-definition settings schema.
//!
//! Every settings-screen row is declared once, on the config field it edits
//! (see `kosk_config_derive::config_section`). This module holds the runtime
//! side: pages, value lenses, control behavior, and the settings registry.

use std::sync::{Arc, OnceLock};

use crate::controller::ControllerKind;

use super::Config;

/// Default `controller_map` file name. The mappings still come from
/// `mappings.toml`; only the name no longer hides inside the built-in TOML.
pub(crate) const DEFAULT_MAPPINGS_FILE: &str = "mappings.toml";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Page {
    Suggestions,
    Overlay,
    Typing,
    Debug,
    Device(ControllerKind, DevicePage),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DevicePage {
    Pads,
    Stick,
    Triggers,
}

pub(crate) fn device_name(target: ControllerKind) -> &'static str {
    match target {
        ControllerKind::Ps4 => "DualShock 4",
        _ => "Steam Controller",
    }
}

impl DevicePage {
    pub(crate) fn title_suffix(self) -> &'static str {
        match self {
            DevicePage::Pads => "Pads",
            DevicePage::Stick => "Stick",
            DevicePage::Triggers => "Triggers",
        }
    }
}

impl Page {
    pub(crate) fn title(&self) -> String {
        match self {
            Page::Suggestions => "Suggestions".to_owned(),
            Page::Overlay => "Overlay".to_owned(),
            Page::Typing => "Typing".to_owned(),
            Page::Debug => "Debug".to_owned(),
            Page::Device(target, sheet) => {
                format!("{} {}", device_name(*target), sheet.title_suffix())
            }
        }
    }
}

/// Composed read/write access from the config root to one field.
#[derive(Clone)]
pub(crate) struct Lens<T> {
    read: Arc<dyn for<'a> Fn(&'a Config) -> &'a T + Send + Sync>,
    write: Arc<dyn for<'a> Fn(&'a mut Config) -> &'a mut T + Send + Sync>,
}

impl Lens<Config> {
    pub(crate) fn root() -> Self {
        Self {
            read: Arc::new(|cfg: &Config| cfg),
            write: Arc::new(|cfg: &mut Config| cfg),
        }
    }
}

impl<T: 'static> Lens<T> {
    pub(crate) fn field<U: 'static>(
        &self,
        get: fn(&T) -> &U,
        get_mut: fn(&mut T) -> &mut U,
    ) -> Lens<U> {
        let read = Arc::clone(&self.read);
        let write = Arc::clone(&self.write);
        Lens {
            read: Arc::new(move |cfg| get(read(cfg))),
            write: Arc::new(move |cfg| get_mut(write(cfg))),
        }
    }

    fn read<'a>(&self, cfg: &'a Config) -> &'a T {
        (self.read)(cfg)
    }

    fn write<'a>(&self, cfg: &'a mut Config) -> &'a mut T {
        (self.write)(cfg)
    }
}

/// One settings-screen row.
pub(crate) struct Setting {
    /// Full TOML path, e.g. `"ps4.stick.warp"`.
    pub key: &'static str,
    pub page: Page,
    pub label: &'static str,
    pub explain: &'static str,
    pub advanced: bool,
    pub control: Box<dyn Control>,
}

impl Setting {
    pub(crate) fn format(&self, cfg: &Config) -> String {
        self.control.format(cfg)
    }

    pub(crate) fn nudge(&self, cfg: &mut Config, dir: i32) -> bool {
        self.control.nudge(cfg, dir)
    }

    pub(crate) fn copy(&self, dst: &mut Config, src: &Config) {
        self.control.copy(dst, src);
    }

    pub(crate) fn is_toggle(&self) -> bool {
        self.control.is_toggle()
    }

    /// Decimals for UI display. `None` for non-float settings.
    pub(crate) fn decimals(&self) -> Option<usize> {
        self.control.decimals()
    }
}

pub(crate) trait Control: Send + Sync {
    fn format(&self, cfg: &Config) -> String;
    /// Returns whether the value changed.
    fn nudge(&self, cfg: &mut Config, dir: i32) -> bool;
    fn copy(&self, dst: &mut Config, src: &Config);
    fn is_toggle(&self) -> bool;
    fn decimals(&self) -> Option<usize>;
}

pub(crate) fn on_off(value: bool) -> String {
    if value {
        "On".to_owned()
    } else {
        "Off".to_owned()
    }
}

/// Values adjustable with left/right input.
pub(crate) trait Step: Clone + PartialEq + Send + Sync + 'static {
    fn step_value(current: Self, dir: i32, min: Self, max: Self, step: Self, digits: u32) -> Self;
    fn format_value(value: &Self, digits: u32, unit: Option<&'static str>) -> String;
}

fn step_index(index: usize, len: usize, dir: i32) -> usize {
    if len == 0 {
        return 0;
    }
    (index as i32 + dir).rem_euclid(len as i32) as usize
}

fn with_unit(text: String, unit: Option<&'static str>) -> String {
    match unit {
        Some(unit) => format!("{text} {unit}"),
        None => text,
    }
}

impl Step for f32 {
    fn step_value(current: Self, dir: i32, min: Self, max: Self, step: Self, digits: u32) -> Self {
        let next = (current + step * dir as f32).clamp(min, max);
        let factor = 10f32.powi(digits as i32);
        (next * factor).round() / factor
    }

    fn format_value(value: &Self, digits: u32, unit: Option<&'static str>) -> String {
        let text = if digits == 0 {
            format!("{}", value.round() as i32)
        } else {
            format!("{:.*}", digits as usize, value)
        };
        with_unit(text, unit)
    }
}

impl Step for u64 {
    fn step_value(current: Self, dir: i32, min: Self, max: Self, step: Self, _digits: u32) -> Self {
        let next = current as i64 + dir as i64 * step as i64;
        next.clamp(min as i64, max as i64) as u64
    }

    fn format_value(value: &Self, _digits: u32, unit: Option<&'static str>) -> String {
        with_unit(value.to_string(), unit)
    }
}

impl Step for u8 {
    fn step_value(current: Self, dir: i32, min: Self, max: Self, step: Self, _digits: u32) -> Self {
        let next = current as i32 + dir * step as i32;
        next.clamp(min as i32, max as i32) as u8
    }

    fn format_value(value: &Self, _digits: u32, unit: Option<&'static str>) -> String {
        with_unit(value.to_string(), unit)
    }
}

impl Step for usize {
    fn step_value(current: Self, dir: i32, min: Self, max: Self, step: Self, _digits: u32) -> Self {
        let next = current as i64 + dir as i64 * step as i64;
        next.clamp(min as i64, max as i64) as usize
    }

    fn format_value(value: &Self, _digits: u32, unit: Option<&'static str>) -> String {
        with_unit(value.to_string(), unit)
    }
}

/// Categorical values cycled with left/right input.
pub(crate) trait Choice: Clone + PartialEq + Send + Sync + 'static {
    const ALL: &'static [Self];
    fn label(&self) -> &'static str;

    fn cycle(&self, dir: i32) -> Self {
        let index = Self::ALL.iter().position(|v| v == self).unwrap_or(0);
        Self::ALL[step_index(index, Self::ALL.len(), dir)].clone()
    }
}

pub(crate) struct BoolControl {
    pub lens: Lens<bool>,
}

pub(crate) struct NumControl<V> {
    pub lens: Lens<V>,
    pub min: V,
    pub max: V,
    pub step: V,
    pub digits: u32,
    pub unit: Option<&'static str>,
}

pub(crate) struct ChoiceControl<V> {
    pub lens: Lens<V>,
}

pub(crate) struct MirrorChoiceControl<V> {
    pub lens: Lens<V>,
    pub mirror: Lens<V>,
}

impl Control for BoolControl {
    fn format(&self, cfg: &Config) -> String {
        on_off(*self.lens.read(cfg))
    }

    fn nudge(&self, cfg: &mut Config, _dir: i32) -> bool {
        let lens = &self.lens;
        let next = !*lens.read(cfg);
        *lens.write(cfg) = next;
        true
    }

    fn copy(&self, dst: &mut Config, src: &Config) {
        let value = *self.lens.read(src);
        *self.lens.write(dst) = value;
    }

    fn is_toggle(&self) -> bool {
        true
    }

    fn decimals(&self) -> Option<usize> {
        None
    }
}

impl<V: Step> Control for NumControl<V> {
    fn format(&self, cfg: &Config) -> String {
        V::format_value(self.lens.read(cfg), self.digits, self.unit)
    }

    fn nudge(&self, cfg: &mut Config, dir: i32) -> bool {
        let lens = &self.lens;
        let next = V::step_value(
            lens.read(cfg).clone(),
            dir,
            self.min.clone(),
            self.max.clone(),
            self.step.clone(),
            self.digits,
        );
        if next == *lens.read(cfg) {
            return false;
        }
        *lens.write(cfg) = next;
        true
    }

    fn copy(&self, dst: &mut Config, src: &Config) {
        let value = self.lens.read(src).clone();
        *self.lens.write(dst) = value;
    }

    fn is_toggle(&self) -> bool {
        false
    }

    fn decimals(&self) -> Option<usize> {
        Some(self.digits as usize)
    }
}

impl<V: Choice> Control for ChoiceControl<V> {
    fn format(&self, cfg: &Config) -> String {
        self.lens.read(cfg).label().to_owned()
    }

    fn nudge(&self, cfg: &mut Config, dir: i32) -> bool {
        let lens = &self.lens;
        let next = lens.read(cfg).cycle(dir);
        if next == *lens.read(cfg) {
            return false;
        }
        *lens.write(cfg) = next;
        true
    }

    fn copy(&self, dst: &mut Config, src: &Config) {
        let value = self.lens.read(src).clone();
        *self.lens.write(dst) = value;
    }

    fn is_toggle(&self) -> bool {
        false
    }

    fn decimals(&self) -> Option<usize> {
        None
    }
}

impl<V: Choice> Control for MirrorChoiceControl<V> {
    fn format(&self, cfg: &Config) -> String {
        self.lens.read(cfg).label().to_owned()
    }

    fn nudge(&self, cfg: &mut Config, dir: i32) -> bool {
        let next = self.lens.read(cfg).cycle(dir);
        if next == *self.lens.read(cfg) && next == *self.mirror.read(cfg) {
            return false;
        }
        *self.lens.write(cfg) = next.clone();
        *self.mirror.write(cfg) = next;
        true
    }

    fn copy(&self, dst: &mut Config, src: &Config) {
        let value = self.lens.read(src).clone();
        *self.lens.write(dst) = value.clone();
        *self.mirror.write(dst) = value;
    }

    fn is_toggle(&self) -> bool {
        false
    }

    fn decimals(&self) -> Option<usize> {
        None
    }
}

pub(crate) fn leak_key(key: String) -> &'static str {
    Box::leak(key.into_boxed_str())
}

static ALL: OnceLock<Vec<Setting>> = OnceLock::new();

pub(crate) fn settings() -> &'static [Setting] {
    ALL.get_or_init(|| {
        let mut out = Vec::new();
        Config::__kosk_collect(&Lens::root(), None, String::new(), &mut out);
        out
    })
}

pub(crate) fn setting_for_key(key: &str) -> Option<&'static Setting> {
    settings().iter().find(|setting| setting.key == key)
}

/// Settings on one page: basic rows first, then advanced, both stable.
pub(crate) fn page_settings(page: Page) -> Vec<&'static Setting> {
    let mut basic = Vec::new();
    let mut advanced = Vec::new();
    for setting in settings() {
        if setting.page == page {
            if setting.advanced {
                advanced.push(setting);
            } else {
                basic.push(setting);
            }
        }
    }
    basic.into_iter().chain(advanced).collect()
}

/// Decimals for writing a settings float. At least 1, so `30.0` never becomes
/// TOML integer `30`. `None` for non-settings floats (shortest `f32` text).
pub(crate) fn save_decimals(key: &str) -> Option<usize> {
    setting_for_key(key)?.decimals().map(|digits| digits.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Config {
        toml::from_str(
            r#"
            layouts = { main = "kb.toml" }
            keyboard_opacity = 1.0
            ui_opacity = 0.4
            "#,
        )
        .unwrap()
    }

    #[test]
    fn lens_reads_and_writes_nested_fields() {
        let lens = Lens::root()
            .field(|cfg: &Config| &cfg.sc2, |cfg: &mut Config| &mut cfg.sc2)
            .field(
                |sc2: &super::super::Sc2Config| &sc2.pad,
                |sc2: &mut super::super::Sc2Config| &mut sc2.pad,
            )
            .field(
                |pad: &super::super::AimProfile| &pad.scale_x,
                |pad: &mut super::super::AimProfile| &mut pad.scale_x,
            );
        let mut cfg = sample();
        cfg.sc2.pad.scale_x = 2.0;
        assert_eq!(*lens.read(&cfg), 2.0);
        *lens.write(&mut cfg) = 5.0;
        assert_eq!(cfg.sc2.pad.scale_x, 5.0);
    }

    #[test]
    fn float_steps_round_to_digits() {
        assert!((f32::step_value(0.3, 1, 0.2, 1.0, 0.05, 2) - 0.35).abs() < 0.001);
        assert!((f32::step_value(1.0, 1, 0.2, 1.0, 0.05, 2) - 1.0).abs() < f32::EPSILON);
        assert!((f32::step_value(0.2, -1, 0.2, 1.0, 0.05, 2) - 0.2).abs() < f32::EPSILON);
        assert_eq!(f32::step_value(30.4, 1, 16.0, 48.0, 1.0, 0), 31.0);
    }

    #[test]
    fn float_formats_match_display_rules() {
        assert_eq!(f32::format_value(&0.35, 2, None), "0.35");
        assert_eq!(f32::format_value(&30.0, 0, None), "30");
        assert_eq!(u64::format_value(&240, 0, Some("ms")), "240 ms");
        assert_eq!(usize::format_value(&6, 0, None), "6");
        assert_eq!(u8::format_value(&40, 0, None), "40");
    }

    #[test]
    fn settings_cover_every_saved_float_key() {
        for key in [
            "keyboard_opacity",
            "ui_opacity",
            "scale_x",
            "sc2.pad.scale_x",
            "sc2.pad.select_sticky",
            "sc2.pad_origin_stretch_max_gain",
            "ps4.stick.warp",
            "completion.ngram.lambda_trigram",
            "completion.ngram.lambda_typo",
            "completion.ngram.backoff_alpha",
            "completion.ui.armed_dot_radius",
            "text_input.font_size",
        ] {
            let setting = setting_for_key(key).unwrap_or_else(|| panic!("missing {key}"));
            assert!(setting.decimals().is_some(), "{key}");
            assert!(save_decimals(key).is_some(), "{key}");
        }
    }
}

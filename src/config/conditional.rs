use super::{validate_config_values, validate_loaded_config, Config};
use crate::when::{WhenContext, WhenExpr};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const LIVE_SETTINGS: &[&str] = &[
    "active_theme",
    "layouts",
    "start_layout",
    "transparent",
    "debug",
    "stick_pad_cursors",
    "scale_x",
    "scale_y",
    "event_debounce_ms",
    "event_debounce_repeat_ms",
    "text_input",
    "completion",
    "controller_map",
    "sc2",
    "ps4",
    "battery",
];

const RELOAD_SETTINGS: &[&str] = &[
    "layouts",
    "scale_x",
    "scale_y",
    "controller_map",
    "sc2",
    "ps4",
    "completion",
];

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(try_from = "RuleSource", into = "RuleSource")]
pub(super) struct Rule {
    source: RuleSource,
    condition: Option<WhenExpr>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RuleSource {
    file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    when: Option<String>,
}

impl TryFrom<RuleSource> for Rule {
    type Error = String;

    fn try_from(source: RuleSource) -> Result<Self, Self::Error> {
        if source.file.trim().is_empty() {
            return Err("config_overlays requires a nonempty file name".into());
        }
        let condition = source.when.as_deref().map(WhenExpr::parse).transpose()?;
        if let Some(condition) = &condition {
            validate_condition(condition)?;
        }
        Ok(Self { source, condition })
    }
}

impl From<Rule> for RuleSource {
    fn from(rule: Rule) -> Self {
        rule.source
    }
}

fn validate_condition(condition: &WhenExpr) -> Result<(), String> {
    match condition {
        WhenExpr::FocusedApp(_) => Ok(()),
        WhenExpr::Not(inner) => validate_condition(inner),
        WhenExpr::And(left, right) | WhenExpr::Or(left, right) => {
            validate_condition(left)?;
            validate_condition(right)
        }
        WhenExpr::Flag(_) => {
            Err("config_overlays conditions support focusedApp, !, &&, || and parentheses".into())
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct LoadedOverlay {
    patch: toml::Value,
    path: PathBuf,
    files: Vec<PathBuf>,
}

fn resolve_path(directory: &Path, value: &mut toml::Value) -> Result<()> {
    let path = value.as_str().context("file path must be a string")?;
    if Path::new(path).is_absolute() {
        return Ok(());
    }
    *value = toml::Value::String(directory.join(path).to_string_lossy().into_owned());
    Ok(())
}

fn resolve_paths(patch: &mut toml::Value, directory: &Path) -> Result<()> {
    if let Some(layouts) = patch.get_mut("layouts").and_then(toml::Value::as_table_mut) {
        for (_, value) in layouts.iter_mut() {
            // Built-in layout names remain available without an on-disk copy.
            if value.as_str().is_some_and(|path| {
                !directory.join(path).is_file()
                    && crate::config_overlay::builtin_layout(path).is_some()
            }) {
                continue;
            }
            resolve_path(directory, value)?;
        }
    }
    let Some(completion) = patch.get_mut("completion") else {
        return Ok(());
    };
    for (section, key) in [
        ("dictionary", "wordlist"),
        ("ngram", "model_dir"),
        ("user_cache", "path"),
    ] {
        if let Some(value) = completion
            .get_mut(section)
            .and_then(|table| table.get_mut(key))
        {
            resolve_path(directory, value)?;
        }
    }
    if let Some(app_types) = completion
        .get_mut("app_types")
        .and_then(toml::Value::as_table_mut)
    {
        for (_, app) in app_types.iter_mut() {
            if let Some(value) = app.get_mut("wordlist") {
                resolve_path(directory, value)?;
            }
        }
    }
    Ok(())
}

pub(super) fn load(base: &mut Config, config_path: &Path) -> Result<()> {
    let directory = config_path.parent().unwrap_or(Path::new(""));
    let directory = std::fs::canonicalize(if directory.as_os_str().is_empty() {
        Path::new(".")
    } else {
        directory
    })?;
    let mut loaded = Vec::new();
    for rule in &base.config_overlays {
        let path = directory.join(&rule.source.file);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("read config overlay {}", path.display()))?;
        let mut patch: toml::Value = toml::from_str(&text)
            .with_context(|| format!("parse config overlay {}", path.display()))?;
        let table = patch.as_table().context("config overlay must be a table")?;
        for key in table.keys() {
            if !LIVE_SETTINGS.contains(&key.as_str()) {
                bail!("config overlay {} cannot override {key}", path.display());
            }
        }
        resolve_paths(&mut patch, path.parent().unwrap_or(&directory))?;
        let mut files = vec![crate::config_overlay::watch_key(&path)];
        if let Some(value) = patch.get("controller_map").filter(|value| value.is_str()) {
            let mappings_path = path
                .parent()
                .unwrap_or(&directory)
                .join(value.as_str().unwrap());
            let text = std::fs::read_to_string(&mappings_path)
                .with_context(|| format!("read overlay mappings {}", mappings_path.display()))?;
            patch["controller_map"] = toml::from_str(&text)
                .with_context(|| format!("parse overlay mappings {}", mappings_path.display()))?;
            files.push(crate::config_overlay::watch_key(&mappings_path));
        }
        let overlay = LoadedOverlay { patch, path, files };
        let mut value = toml::Value::try_from(&*base)?;
        merge(&mut value, &overlay.patch);
        let config: Config = value
            .try_into()
            .with_context(|| format!("validate config overlay {}", overlay.path.display()))?;
        validate_loaded_config(&config, config_path)?;
        loaded.push(overlay);
    }
    base.loaded_overlays = Arc::new(loaded);
    Ok(())
}

fn merge(base: &mut toml::Value, patch: &toml::Value) {
    crate::config_overlay::merge_toml(base, patch);
    if let Some(mappings) = patch.get("controller_map") {
        crate::config_overlay::merge_mappings(&mut base["controller_map"], mappings);
    }
}

pub(super) fn append_paths(files: &mut Vec<PathBuf>, base: &Config, config_path: &Path) {
    for overlay in base.loaded_overlays.iter() {
        for path in &overlay.files {
            if !files.contains(path) {
                files.push(path.clone());
            }
        }
        let layouts = overlay.patch.get("layouts").and_then(toml::Value::as_table);
        if let Some(layouts) = layouts {
            for value in layouts.values().filter_map(toml::Value::as_str) {
                let path = crate::config_overlay::watch_key(
                    &config_path.parent().unwrap_or(Path::new("")).join(value),
                );
                if !files.contains(&path) {
                    files.push(path);
                }
            }
        }
    }
}

#[derive(Debug)]
pub(super) struct Runtime {
    pub(super) base: Config,
    pub(super) effective: Config,
    matching: Vec<usize>,
    failed_matching: Option<Vec<usize>>,
}

impl Runtime {
    pub(super) fn new(base: Config) -> Self {
        Self {
            effective: base.clone(),
            base,
            matching: Vec::new(),
            failed_matching: None,
        }
    }

    pub(super) fn refresh(&mut self, focused_app: Option<String>) -> Result<bool> {
        let context = WhenContext {
            focused_app,
            ..Default::default()
        };
        let matching: Vec<_> = self
            .base
            .config_overlays
            .iter()
            .enumerate()
            .filter(|(_, rule)| {
                rule.condition
                    .as_ref()
                    .is_none_or(|condition| condition.eval(&context))
            })
            .map(|(index, _)| index)
            .collect();
        if matching == self.matching || self.failed_matching.as_ref() == Some(&matching) {
            return Ok(false);
        }
        let effective = match self.resolve(&matching) {
            Ok(effective) => effective,
            Err(error) => {
                self.failed_matching = Some(matching);
                return Err(error);
            }
        };
        let previous = toml::Value::try_from(&self.effective)?;
        let value = toml::Value::try_from(&effective)?;
        let reload = RELOAD_SETTINGS
            .iter()
            .any(|key| previous.get(*key) != value.get(*key));
        self.effective = effective;
        self.matching = matching;
        self.failed_matching = None;
        Ok(reload)
    }

    fn resolve(&self, matching: &[usize]) -> Result<Config> {
        let mut value = toml::Value::try_from(&self.base)?;
        for &index in matching {
            merge(&mut value, &self.base.loaded_overlays[index].patch);
        }
        let mut effective: Config = value.try_into().context("apply config overlays")?;
        validate_config_values(&effective)?;
        effective.theme_catalog = Arc::clone(&self.base.theme_catalog);
        effective.loaded_overlays = Arc::clone(&self.base.loaded_overlays);
        if !effective
            .theme_catalog
            .contains(effective.active_theme.name())
        {
            effective
                .active_theme
                .set_name(crate::theme::DEFAULT_THEME_NAME);
        }
        Ok(effective)
    }

    pub(super) fn replace(&mut self, base: Config) -> Result<()> {
        let mut next = Self::new(base);
        next.refresh(crate::platform::foreground::current_basename())?;
        *self = next;
        Ok(())
    }
}

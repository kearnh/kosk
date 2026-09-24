//! Built-in config, mappings, and layouts, merged with a sparse user file.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub const CONFIG_VERSION: i64 = 1;
pub const UNBIND_ACTION: &str = "none";

const BUILTIN_CONFIG: &str = include_str!("../config.toml");
const BUILTIN_MAPPINGS: &str = include_str!("../mappings.toml");
const BUILTIN_LAYOUT_MAIN: &str = include_str!("../old_sc.toml");
const BUILTIN_LAYOUT_SYMBOLS: &str = include_str!("../old_sc_symbols.toml");
const BUILTIN_UNIGRAMS: &str = include_str!("../data/completion/en/unigrams.tsv");

pub fn builtin_config_toml() -> &'static str {
    BUILTIN_CONFIG
}

pub fn builtin_mappings_toml() -> &'static str {
    BUILTIN_MAPPINGS
}

pub fn builtin_unigrams() -> &'static str {
    BUILTIN_UNIGRAMS
}

pub fn builtin_layout(rel: &str) -> Option<&'static str> {
    match file_name(rel) {
        "old_sc.toml" => Some(BUILTIN_LAYOUT_MAIN),
        "old_sc_symbols.toml" => Some(BUILTIN_LAYOUT_SYMBOLS),
        _ => None,
    }
}

pub fn user_config_path() -> Result<PathBuf> {
    let root = std::env::var("LOCALAPPDATA").context("LOCALAPPDATA is not set")?;
    Ok(PathBuf::from(root).join("kosk").join("config.toml"))
}

/// `%LOCALAPPDATA%\kosk\config.toml`, creating `config_version = 1` when absent.
pub fn ensure_user_config_file() -> Result<PathBuf> {
    let path = user_config_path()?;
    if path.exists() {
        return Ok(path);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    std::fs::write(&path, format!("config_version = {CONFIG_VERSION}\n"))
        .with_context(|| format!("create {}", path.display()))?;
    Ok(path)
}

pub fn file_version(doc: &toml_edit::DocumentMut) -> i64 {
    doc.get("config_version")
        .and_then(|item| item.as_integer())
        .unwrap_or(0)
}

/// Raise `doc` to [`CONFIG_VERSION`]. Returns whether the document changed.
pub fn migrate_document(doc: &mut toml_edit::DocumentMut) -> Result<bool> {
    let mut version = file_version(doc);
    if version > CONFIG_VERSION {
        bail!("config_version {version} is newer than this program ({CONFIG_VERSION})");
    }
    let start = version;
    while version < CONFIG_VERSION {
        match version {
            0 => {
                doc["config_version"] = toml_edit::value(CONFIG_VERSION);
            }
            _ => bail!("no migration from config_version {version}"),
        }
        version = CONFIG_VERSION;
    }
    Ok(version != start)
}

/// Run migrations on a config blob (a user file or a recording). Does not write.
pub fn migrate_toml(text: &str) -> Result<String> {
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .context("parse config for migration")?;
    migrate_document(&mut doc)?;
    Ok(doc.to_string())
}

/// Tables merge key by key. Arrays and scalars in `overlay` replace `base`.
pub fn merge_toml(base: &mut toml::Value, overlay: &toml::Value) {
    let (Some(base_table), Some(over_table)) = (base.as_table_mut(), overlay.as_table()) else {
        *base = overlay.clone();
        return;
    };
    for (key, value) in over_table {
        if key == "controller_map" {
            continue;
        }
        match base_table.get_mut(key) {
            Some(existing) if existing.is_table() && value.is_table() => {
                merge_toml(existing, value);
            }
            _ => {
                base_table.insert(key.clone(), value.clone());
            }
        }
    }
}

/// Per-binding merge. A value of [`UNBIND_ACTION`] drops that binding.
pub fn merge_mappings(base: &mut toml::Value, overlay: &toml::Value) {
    let (Some(base_table), Some(over_table)) = (base.as_table_mut(), overlay.as_table()) else {
        return;
    };
    for (mode, mode_over) in over_table {
        let Some(mode_over_table) = mode_over.as_table() else {
            base_table.insert(mode.clone(), mode_over.clone());
            continue;
        };
        let mode_base = base_table
            .entry(mode.clone())
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
        let Some(mode_base_table) = mode_base.as_table_mut() else {
            continue;
        };
        for (binding, action) in mode_over_table {
            if action.as_str() == Some(UNBIND_ACTION) {
                mode_base_table.remove(binding);
            } else {
                mode_base_table.insert(binding.clone(), action.clone());
            }
        }
    }
}

pub fn read_user_mappings(config_dir: &Path, rel: &str) -> Result<Option<toml::Value>> {
    let path = config_dir.join(rel);
    if !path.is_file() {
        return Ok(None);
    }
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let value = text
        .parse()
        .with_context(|| format!("parse {}", path.display()))?;
    Ok(Some(value))
}

/// Config directory, then the working directory, then a built-in layout.
pub fn read_layout(config_dir: Option<&Path>, rel: &str) -> Result<String> {
    if let Some(path) = existing_file(config_dir, rel) {
        return std::fs::read_to_string(&path)
            .with_context(|| format!("read layout {}", path.display()));
    }
    builtin_layout(rel)
        .map(str::to_owned)
        .ok_or_else(|| anyhow::anyhow!("cannot find layout file \"{rel}\""))
}

/// A file beside the config, else the same relative path from the working directory.
pub fn existing_file(config_dir: Option<&Path>, rel: &str) -> Option<PathBuf> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return rel_path.is_file().then(|| rel_path.to_path_buf());
    }
    if let Some(dir) = config_dir {
        let path = dir.join(rel_path);
        if path.is_file() {
            return Some(path);
        }
    }
    let from_cwd = std::env::current_dir().ok()?.join(rel_path);
    from_cwd.is_file().then_some(from_cwd)
}

/// Directory beside the config, else the same relative path from the working directory.
pub fn existing_dir(config_dir: Option<&Path>, rel: &Path) -> Option<PathBuf> {
    if rel.is_absolute() {
        return rel.is_dir().then(|| rel.to_path_buf());
    }
    if let Some(dir) = config_dir {
        let path = dir.join(rel);
        if path.is_dir() {
            return Some(path);
        }
    }
    let from_cwd = std::env::current_dir().ok()?.join(rel);
    from_cwd.is_dir().then_some(from_cwd)
}

fn file_name(rel: &str) -> &str {
    Path::new(rel)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(rel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_nested_table_and_replace_array() {
        let mut base: toml::Value = toml::from_str(
            r#"
            stick_scale_x = 1.0
            preferred_controller = ["sc2"]
            [sc2]
            trigger_left_threshold = 40
            [completion.ui]
            columns = 3
            rows = 2
            "#,
        )
        .unwrap();
        let over: toml::Value = toml::from_str(
            r#"
            preferred_controller = ["ps4"]
            [sc2]
            trigger_left_threshold = 10
            [completion.ui]
            columns = 4
            "#,
        )
        .unwrap();
        merge_toml(&mut base, &over);
        let table = base.as_table().unwrap();
        assert_eq!(table["stick_scale_x"].as_float().unwrap(), 1.0);
        assert_eq!(
            table["preferred_controller"].as_array().unwrap()[0].as_str(),
            Some("ps4")
        );
        assert_eq!(
            table["sc2"]["trigger_left_threshold"].as_integer(),
            Some(10)
        );
        assert_eq!(table["completion"]["ui"]["columns"].as_integer(), Some(4));
        assert_eq!(table["completion"]["ui"]["rows"].as_integer(), Some(2));
    }

    #[test]
    fn unbind_removes_default_binding() {
        let mut base: toml::Value = toml::from_str(
            r#"
            [Keyboard]
            "faceTop" = "toggleShift"
            "faceBottom" = "sendKey.enter"
            "#,
        )
        .unwrap();
        let over: toml::Value = toml::from_str(
            r#"
            [Keyboard]
            "faceTop" = "none"
            "faceRight" = "toggleCtrl"
            "#,
        )
        .unwrap();
        merge_mappings(&mut base, &over);
        let keyboard = base["Keyboard"].as_table().unwrap();
        assert!(!keyboard.contains_key("faceTop"));
        assert_eq!(keyboard["faceBottom"].as_str(), Some("sendKey.enter"));
        assert_eq!(keyboard["faceRight"].as_str(), Some("toggleCtrl"));
    }

    #[test]
    fn version_zero_becomes_one() {
        let mut doc = "keyboard_opacity = 0.7\n"
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        assert!(migrate_document(&mut doc).unwrap());
        assert_eq!(file_version(&doc), CONFIG_VERSION);
        assert!(doc.to_string().contains("keyboard_opacity"));
    }

    #[test]
    fn newer_version_is_rejected() {
        let mut doc = "config_version = 99\n"
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        assert!(migrate_document(&mut doc).is_err());
    }

    #[test]
    fn builtin_layout_names() {
        assert!(builtin_layout("old_sc.toml")
            .unwrap()
            .contains("stick_rest_left"));
        assert!(builtin_layout("old_sc_symbols.toml").is_some());
        assert!(builtin_layout("missing.toml").is_none());
    }
}

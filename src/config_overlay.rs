//! Built-in config, mappings, and layouts, merged with a sparse user file.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub(crate) const CONFIG_VERSION: i64 = 3;
pub(crate) const CONFIG_VERSION_KEY: &str = "config_version";
pub(crate) const CONTROLLER_MAP_KEY: &str = "controller_map";
pub(crate) const UNBIND_ACTION: &str = "none";

pub(crate) fn newer_version_message(file_version: i64) -> String {
    format!(
        "Your settings file was saved by a newer kosk (version {file_version}); unknown settings are ignored"
    )
}

const BUILTIN_MAPPINGS: &str = include_str!("../mappings.toml");
const BUILTIN_LAYOUT_MAIN: &str = include_str!("../old_sc.toml");
const BUILTIN_LAYOUT_SYMBOLS: &str = include_str!("../old_sc_symbols.toml");
const BUILTIN_UNIGRAMS: &str = include_str!("../data/completion/en/unigrams.tsv");

pub(crate) fn builtin_mappings_toml() -> &'static str {
    BUILTIN_MAPPINGS
}

pub(crate) fn builtin_unigrams() -> &'static str {
    BUILTIN_UNIGRAMS
}

pub(crate) fn builtin_layout(rel: &str) -> Option<&'static str> {
    match rel {
        "old_sc.toml" => Some(BUILTIN_LAYOUT_MAIN),
        "old_sc_symbols.toml" => Some(BUILTIN_LAYOUT_SYMBOLS),
        _ => None,
    }
}

pub(crate) fn user_config_path() -> Result<PathBuf> {
    let root = std::env::var("LOCALAPPDATA").context("LOCALAPPDATA is not set")?;
    Ok(PathBuf::from(root).join("kosk").join("config.toml"))
}

/// `%LOCALAPPDATA%\kosk\config.toml`, creating `config_version` at [`CONFIG_VERSION`] when absent.
pub(crate) fn ensure_user_config_file() -> Result<PathBuf> {
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

pub(crate) fn file_version(doc: &toml_edit::DocumentMut) -> i64 {
    doc.get(CONFIG_VERSION_KEY)
        .and_then(|item| item.as_integer())
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfigVersionOutcome {
    Unchanged,
    Stamped,
    /// File was written by a newer kosk. Loaded as-is.
    Newer,
}

/// Stamp an unversioned document; reject unsupported older schemas.
pub(crate) fn normalize_document(doc: &mut toml_edit::DocumentMut) -> Result<ConfigVersionOutcome> {
    if doc.get(CONFIG_VERSION_KEY).is_none() {
        doc[CONFIG_VERSION_KEY] = toml_edit::value(CONFIG_VERSION);
        return Ok(ConfigVersionOutcome::Stamped);
    }

    let version = file_version(doc);
    if version > CONFIG_VERSION {
        return Ok(ConfigVersionOutcome::Newer);
    }
    if version != CONFIG_VERSION {
        bail!("unsupported config_version {version} (expected {CONFIG_VERSION})");
    }
    Ok(ConfigVersionOutcome::Unchanged)
}

/// Normalize a config blob without writing a file.
/// A newer blob is returned unchanged and posts a one-time notice.
pub(crate) fn normalize_toml(text: &str) -> Result<String> {
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .context("parse config")?;
    if normalize_document(&mut doc)? == ConfigVersionOutcome::Newer {
        crate::user_notify::notify(crate::user_notify::Notice::newer_settings(file_version(
            &doc,
        )));
    }
    Ok(doc.to_string())
}

/// Tables merge key by key. Arrays and scalars in `overlay` replace `base`.
pub(crate) fn merge_toml(base: &mut toml::Value, overlay: &toml::Value) {
    let (Some(base_table), Some(over_table)) = (base.as_table_mut(), overlay.as_table()) else {
        *base = overlay.clone();
        return;
    };
    for (key, value) in over_table {
        if key == CONTROLLER_MAP_KEY {
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
pub(crate) fn merge_mappings(base: &mut toml::Value, overlay: &toml::Value) {
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

pub(crate) fn read_user_mappings(config_dir: &Path, rel: &str) -> Result<Option<toml::Value>> {
    let path = config_dir.join(rel);
    if !path.is_file() {
        return Ok(None);
    }
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let value = toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    Ok(Some(value))
}

/// Beside the config file, then the built-in copy. No working-directory fallback.
pub(crate) fn read_layout(config_dir: Option<&Path>, rel: &str) -> Result<String> {
    let rel_path = Path::new(rel);
    let on_disk = if rel_path.is_absolute() {
        rel_path.is_file().then(|| rel_path.to_path_buf())
    } else {
        config_dir.and_then(|dir| {
            let path = dir.join(rel_path);
            path.is_file().then_some(path)
        })
    };
    if let Some(path) = on_disk {
        return std::fs::read_to_string(&path)
            .with_context(|| format!("read layout {}", path.display()));
    }
    builtin_layout(rel)
        .map(str::to_owned)
        .ok_or_else(|| anyhow::anyhow!("cannot find layout file \"{rel}\""))
}

/// The config file, its mappings file, and its layout files, as [`watch_key`]s.
/// Files that do not exist yet are included so creating one reloads.
pub(crate) fn config_file_paths(
    config_path: &Path,
    mappings: Option<&str>,
    layouts: &[(String, String)],
) -> Vec<PathBuf> {
    let config_dir = config_path.parent().unwrap_or(Path::new(""));
    let mut out = vec![watch_key(config_path)];
    let rels = mappings
        .into_iter()
        .chain(layouts.iter().map(|(_, rel)| rel.as_str()));
    for rel in rels {
        let key = watch_key(&config_dir.join(rel));
        if !out.contains(&key) {
            out.push(key);
        }
    }
    out
}

pub(crate) fn append_theme_paths(files: &mut Vec<PathBuf>, config_path: &Path, themes: &[String]) {
    let dir = config_path.parent().unwrap_or(Path::new(""));
    for file in themes {
        let has_glob = file.contains(['*', '?', '[']);
        let pattern = if Path::new(file).is_absolute() {
            PathBuf::from(file)
        } else {
            dir.join(file)
        };
        if has_glob {
            let path = pattern.clone();
            if !files.contains(&path) {
                files.push(path);
            }
            let mut base = PathBuf::new();
            for component in pattern.components() {
                if component
                    .as_os_str()
                    .to_string_lossy()
                    .contains(['*', '?', '['])
                {
                    break;
                }
                base.push(component);
            }
            if !base.as_os_str().is_empty() && !files.contains(&base) {
                files.push(base);
            }
        }
        let paths = crate::theme::expand_theme_files(config_path, std::slice::from_ref(file))
            .unwrap_or_default();
        for path in paths {
            let path = watch_key(&path);
            if !files.contains(&path) {
                files.push(path);
            }
        }
    }
}

/// Canonical parent joined with the file name, so a file that does not exist
/// yet compares equal to the path of a later event.
pub(crate) fn watch_key(path: &Path) -> PathBuf {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return path.to_path_buf();
    };
    match std::fs::canonicalize(parent) {
        Ok(parent) => parent.join(name),
        Err(_) => path.to_path_buf(),
    }
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
    fn unversioned_document_gets_current_version() {
        let mut doc = "scale_x = 37.0\n"
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        assert_eq!(
            normalize_document(&mut doc).unwrap(),
            ConfigVersionOutcome::Stamped
        );
        assert_eq!(file_version(&doc), CONFIG_VERSION);
        assert!(doc.to_string().contains("scale_x"));
    }

    #[test]
    fn newer_version_is_left_unchanged() {
        let mut doc = "config_version = 99\nunknown = true\n"
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        assert_eq!(
            normalize_document(&mut doc).unwrap(),
            ConfigVersionOutcome::Newer
        );
        assert_eq!(file_version(&doc), 99);
        assert!(doc.to_string().contains("unknown"));
    }

    #[test]
    fn obsolete_config_versions_are_rejected() {
        for version in 0..CONFIG_VERSION {
            let mut doc = format!("config_version = {version}\n")
                .parse::<toml_edit::DocumentMut>()
                .unwrap();
            assert!(normalize_document(&mut doc).is_err(), "{version}");
        }
    }

    #[test]
    fn builtin_layout_names() {
        assert!(builtin_layout("old_sc.toml")
            .unwrap()
            .contains("stick_rest_left"));
        assert!(builtin_layout("old_sc_symbols.toml").is_some());
        assert!(builtin_layout("missing.toml").is_none());
        assert!(builtin_layout("foo/old_sc.toml").is_none());
    }

    #[test]
    fn layout_beside_config_beats_builtin() {
        let dir = std::env::temp_dir().join(format!(
            "kosk-layout-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("old_sc.toml"), "custom layout\n").unwrap();
        let text = read_layout(Some(&dir), "old_sc.toml").unwrap();
        assert_eq!(text, "custom layout\n");
        let builtin = read_layout(Some(&dir), "old_sc_symbols.toml").unwrap();
        assert!(builtin.contains("stick_rest"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

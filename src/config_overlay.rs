//! Built-in config, mappings, and layouts, merged with a sparse user file.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub(crate) const CONFIG_VERSION: i64 = 1;
pub(crate) const CONFIG_VERSION_KEY: &str = "config_version";
pub(crate) const CONTROLLER_MAP_KEY: &str = "controller_map";
pub(crate) const UNBIND_ACTION: &str = "none";

pub(crate) fn newer_version_message(file_version: i64) -> String {
    format!(
        "{CONFIG_VERSION_KEY} {file_version} is newer than this kosk ({CONFIG_VERSION}); unknown settings are ignored"
    )
}

const BUILTIN_CONFIG: &str = include_str!("../config.toml");
const BUILTIN_MAPPINGS: &str = include_str!("../mappings.toml");
const BUILTIN_LAYOUT_MAIN: &str = include_str!("../old_sc.toml");
const BUILTIN_LAYOUT_SYMBOLS: &str = include_str!("../old_sc_symbols.toml");
const BUILTIN_UNIGRAMS: &str = include_str!("../data/completion/en/unigrams.tsv");

pub(crate) fn builtin_config_toml() -> &'static str {
    BUILTIN_CONFIG
}

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

/// `%LOCALAPPDATA%\kosk\config.toml`, creating `config_version = 1` when absent.
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
pub(crate) enum MigrateOutcome {
    Unchanged,
    Migrated,
    /// File was written by a newer kosk. Loaded as-is.
    Newer,
}

fn migrate_0_to_1(doc: &mut toml_edit::DocumentMut) {
    doc[CONFIG_VERSION_KEY] = toml_edit::value(1_i64);
}

const MIGRATIONS: &[fn(&mut toml_edit::DocumentMut)] = &[migrate_0_to_1];

/// Raise `doc` to [`CONFIG_VERSION`]. A newer file is left unchanged.
pub(crate) fn migrate_document(doc: &mut toml_edit::DocumentMut) -> Result<MigrateOutcome> {
    const _: () = assert!(MIGRATIONS.len() as i64 == CONFIG_VERSION);
    apply_migrations(doc, MIGRATIONS, CONFIG_VERSION)
}

fn apply_migrations(
    doc: &mut toml_edit::DocumentMut,
    steps: &[fn(&mut toml_edit::DocumentMut)],
    target: i64,
) -> Result<MigrateOutcome> {
    let version = file_version(doc);
    if version > target {
        return Ok(MigrateOutcome::Newer);
    }
    if version == target {
        return Ok(MigrateOutcome::Unchanged);
    }
    if version < 0 {
        bail!("config_version {version} is invalid");
    }
    let from = version as usize;
    if from > steps.len() {
        bail!("no migration from config_version {version}");
    }
    for step in &steps[from..] {
        step(doc);
    }
    doc[CONFIG_VERSION_KEY] = toml_edit::value(target);
    Ok(MigrateOutcome::Migrated)
}

/// Run migrations on a config blob (a user file or a recording). Does not write.
/// A newer blob is returned unchanged after [`crate::user_notify::notify_user`].
pub(crate) fn migrate_toml(text: &str) -> Result<String> {
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .context("parse config for migration")?;
    if migrate_document(&mut doc)? == MigrateOutcome::Newer {
        crate::user_notify::notify_user(&newer_version_message(file_version(&doc)));
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
    let value = text
        .parse()
        .with_context(|| format!("parse {}", path.display()))?;
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

/// Absolute layout files that sit outside the config directory. Built-ins are omitted.
pub(crate) fn external_layout_paths(
    config_dir: &Path,
    layouts: &[(String, String)],
) -> Vec<PathBuf> {
    let config_dir = std::fs::canonicalize(config_dir).unwrap_or_else(|_| config_dir.to_path_buf());
    let mut out = Vec::new();
    for (_, rel) in layouts {
        let path = PathBuf::from(rel);
        if !path.is_absolute() || !path.is_file() {
            continue;
        }
        let Ok(canon) = std::fs::canonicalize(&path) else {
            continue;
        };
        if canon.parent() == Some(config_dir.as_path()) {
            continue;
        }
        out.push(canon);
    }
    out
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
        assert_eq!(
            migrate_document(&mut doc).unwrap(),
            MigrateOutcome::Migrated
        );
        assert_eq!(file_version(&doc), CONFIG_VERSION);
        assert!(doc.to_string().contains("keyboard_opacity"));
    }

    #[test]
    fn newer_version_is_left_unchanged() {
        let mut doc = "config_version = 99\nunknown = true\n"
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        assert_eq!(migrate_document(&mut doc).unwrap(), MigrateOutcome::Newer);
        assert_eq!(file_version(&doc), 99);
        assert!(doc.to_string().contains("unknown"));
    }

    #[test]
    fn two_steps_run_in_order() {
        fn step0(doc: &mut toml_edit::DocumentMut) {
            doc["first"] = toml_edit::value(1_i64);
        }
        fn step1(doc: &mut toml_edit::DocumentMut) {
            doc["second"] = toml_edit::value(2_i64);
        }
        let mut doc = "keyboard_opacity = 0.7\n"
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        assert_eq!(
            apply_migrations(&mut doc, &[step0, step1], 2).unwrap(),
            MigrateOutcome::Migrated
        );
        assert_eq!(doc["first"].as_integer(), Some(1));
        assert_eq!(doc["second"].as_integer(), Some(2));
        assert_eq!(file_version(&doc), 2);
        assert!(doc.to_string().contains("keyboard_opacity"));
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

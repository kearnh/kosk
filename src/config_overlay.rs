//! Built-in config, mappings, and layouts, merged with a sparse user file.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub(crate) const CONFIG_VERSION: i64 = 2;
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
pub(crate) enum MigrateOutcome {
    Unchanged,
    Migrated,
    /// File was written by a newer kosk. Loaded as-is.
    Newer,
}

fn migrate_0_to_1(doc: &mut toml_edit::DocumentMut) {
    doc[CONFIG_VERSION_KEY] = toml_edit::value(1_i64);
}

const AIM_KEY_MOVES: &[(&str, &str)] = &[
    ("stick_scale_x", "scale_x"),
    ("stick_scale_y", "scale_y"),
    ("stick_warp", "warp"),
    ("stick_select_sticky", "select_sticky"),
    ("stick_select_lock_ms", "select_lock_ms"),
];

const AIM_PROFILES: &[(&str, &str)] = &[("sc2", "pad"), ("sc2", "stick"), ("ps4", "stick")];

fn migrate_1_to_2(doc: &mut toml_edit::DocumentMut) {
    for (old, new) in AIM_KEY_MOVES {
        let Some(value) = doc.get(old).cloned() else {
            continue;
        };
        for (family, profile) in AIM_PROFILES {
            insert_aim_key(doc, family, profile, new, value.clone());
        }
        doc.remove(old);
    }
}

fn insert_aim_key(
    doc: &mut toml_edit::DocumentMut,
    family: &str,
    profile: &str,
    key: &str,
    value: toml_edit::Item,
) {
    let root = doc.as_table_mut();
    if !root.get(family).is_some_and(|item| item.is_table()) {
        root.insert(family, toml_edit::Item::Table(toml_edit::Table::new()));
    }
    let family_table = root
        .get_mut(family)
        .and_then(|item| item.as_table_mut())
        .expect("controller table");
    if !family_table
        .get(profile)
        .is_some_and(|item| item.is_table())
    {
        family_table.insert(profile, toml_edit::Item::Table(toml_edit::Table::new()));
    }
    let profile_table = family_table
        .get_mut(profile)
        .and_then(|item| item.as_table_mut())
        .expect("aim profile table");
    if !profile_table.contains_key(key) {
        profile_table.insert(key, value);
    }
}

const MIGRATIONS: &[fn(&mut toml_edit::DocumentMut)] = &[migrate_0_to_1, migrate_1_to_2];

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
/// A newer blob is returned unchanged and posts a one-time notice.
pub(crate) fn migrate_toml(text: &str) -> Result<String> {
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .context("parse config for migration")?;
    if migrate_document(&mut doc)? == MigrateOutcome::Newer {
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
    fn aim_keys_copy_into_each_profile() {
        let mut doc = "\
config_version = 1
stick_scale_x = 4.2
stick_warp = 0.5
[sc2.stick]
scale_x = 9.0
"
        .parse::<toml_edit::DocumentMut>()
        .unwrap();
        assert_eq!(
            migrate_document(&mut doc).unwrap(),
            MigrateOutcome::Migrated
        );
        assert_eq!(file_version(&doc), CONFIG_VERSION);
        assert!(doc.get("stick_scale_x").is_none());
        assert!(doc.get("stick_warp").is_none());
        assert_eq!(doc["sc2"]["pad"]["scale_x"].as_float(), Some(4.2));
        assert_eq!(doc["sc2"]["pad"]["warp"].as_float(), Some(0.5));
        assert_eq!(doc["sc2"]["stick"]["scale_x"].as_float(), Some(9.0));
        assert_eq!(doc["sc2"]["stick"]["warp"].as_float(), Some(0.5));
        assert_eq!(doc["ps4"]["stick"]["scale_x"].as_float(), Some(4.2));
        assert_eq!(doc["ps4"]["stick"]["warp"].as_float(), Some(0.5));
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

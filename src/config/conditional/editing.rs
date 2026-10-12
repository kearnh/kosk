use super::*;
use crate::config::{self, schema::Setting};
use std::ops::Range;

const MAX_EXPANDED_TERMS: usize = 128;
const CONDITION_DIRECTORY: &str = "conditions";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum EditingScope {
    #[default]
    Main,
    Overlay(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TermJoin {
    #[default]
    And,
    Or,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ConditionTerm {
    pub join: TermJoin,
    pub executable: String,
    pub negated: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ConditionDraft {
    pub scope: EditingScope,
    pub name: String,
    pub terms: Vec<ConditionTerm>,
    original: Option<String>,
    original_terms: Vec<ConditionTerm>,
}

impl ConditionDraft {
    pub(crate) fn row_editable(&self) -> bool {
        self.original.is_none() || !self.original_terms.is_empty()
    }

    pub(crate) fn new() -> Self {
        Self {
            scope: EditingScope::Main,
            name: String::new(),
            terms: Vec::new(),
            original: None,
            original_terms: Vec::new(),
        }
    }

    pub(crate) fn groups(&self) -> Vec<Range<usize>> {
        let mut groups = Vec::new();
        let mut start = 0;
        for (index, term) in self.terms.iter().enumerate().skip(1) {
            if term.join == TermJoin::Or {
                groups.push(start..index);
                start = index;
            }
        }
        if !self.terms.is_empty() {
            groups.push(start..self.terms.len());
        }
        groups
    }

    pub(crate) fn expression(&self) -> Result<Option<String>> {
        if self.original.is_some() && self.terms == self.original_terms {
            return Ok(self.original.clone());
        }
        if self.terms.is_empty() {
            bail!("Add at least one condition");
        }
        let mut expression = String::new();
        for (index, term) in self.terms.iter().enumerate() {
            let executable = term.executable.trim();
            if executable.is_empty() || executable.contains(['/', '\\']) {
                bail!("Select an app or enter its executable name");
            }
            if index > 0 {
                expression.push_str(match term.join {
                    TermJoin::And => " && ",
                    TermJoin::Or => " || ",
                });
            }
            if term.negated {
                expression.push('!');
            }
            expression.push_str(&format!(
                "focusedApp({})",
                serde_json::to_string(executable)?
            ));
        }
        validate_condition(&WhenExpr::parse(&expression).map_err(anyhow::Error::msg)?)
            .map_err(anyhow::Error::msg)?;
        Ok(Some(expression))
    }
}

fn expand(expression: &WhenExpr, negated: bool) -> Option<Vec<Vec<ConditionTerm>>> {
    match expression {
        WhenExpr::FocusedApp(executable) => Some(vec![vec![ConditionTerm {
            executable: executable.clone(),
            negated,
            ..Default::default()
        }]]),
        WhenExpr::Not(inner) => expand(inner, !negated),
        WhenExpr::And(left, right) | WhenExpr::Or(left, right) => {
            let left = expand(left, negated)?;
            let right = expand(right, negated)?;
            let conjunction = matches!(expression, WhenExpr::And(..)) != negated;
            let groups = if conjunction {
                if left.len().checked_mul(right.len())? > MAX_EXPANDED_TERMS {
                    return None;
                }
                left.iter()
                    .flat_map(|a| {
                        right.iter().map(move |b| {
                            let mut group = a.clone();
                            group.extend(b.clone());
                            group
                        })
                    })
                    .collect::<Vec<_>>()
            } else {
                left.into_iter().chain(right).collect()
            };
            (groups.iter().map(Vec::len).sum::<usize>() <= MAX_EXPANDED_TERMS).then_some(groups)
        }
        WhenExpr::Flag(_) => None,
    }
}

fn draft_for(rule: &Rule) -> ConditionDraft {
    let mut terms = Vec::new();
    if let Some(groups) = rule.condition.as_ref().and_then(|expr| expand(expr, false)) {
        for (index, mut group) in groups.into_iter().enumerate() {
            if index > 0 {
                group[0].join = TermJoin::Or;
            }
            terms.extend(group);
        }
    }
    ConditionDraft {
        scope: EditingScope::Overlay(rule.source.file.clone()),
        name: rule.source.name.clone().unwrap_or_else(|| {
            Path::new(&rule.source.file)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        }),
        original: rule.source.when.clone(),
        original_terms: terms.clone(),
        terms,
    }
}

pub(crate) fn condition_choices() -> Vec<ConditionDraft> {
    config::settings_config()
        .config_overlays
        .iter()
        .map(draft_for)
        .collect()
}

pub(crate) fn condition_apps() -> Vec<(String, String)> {
    crate::platform::apps::running_apps()
}

fn index_for(base: &Config, scope: &EditingScope) -> Result<Option<usize>> {
    let EditingScope::Overlay(file) = scope else {
        return Ok(None);
    };
    base.config_overlays
        .iter()
        .position(|rule| &rule.source.file == file)
        .map(Some)
        .context("The selected condition was removed; select Main config or another condition")
}

fn scoped_config(base: &Config, scope: &EditingScope) -> Result<Config> {
    let Some(index) = index_for(base, scope)? else {
        return Ok(base.clone());
    };
    let mut value = toml::Value::try_from(base)?;
    merge(&mut value, &base.loaded_overlays[index].patch);
    let mut config: Config = value.try_into()?;
    config.theme_catalog = Arc::clone(&base.theme_catalog);
    config.loaded_overlays = Arc::clone(&base.loaded_overlays);
    Ok(config)
}

pub(crate) fn editing_config(scope: &EditingScope) -> Result<Config> {
    scoped_config(&config::settings_config(), scope)
}

pub(crate) fn editing_label(scope: &EditingScope) -> String {
    condition_choices()
        .into_iter()
        .find(|draft| &draft.scope == scope)
        .map(|draft| draft.name)
        .unwrap_or_else(|| "Main config".into())
}

fn setting_value<'a>(value: &'a toml::Value, key: &str) -> Result<&'a toml::Value> {
    key.split('.').try_fold(value, |value, part| {
        value.get(part).context("Unknown setting")
    })
}

fn patch_setting(patch: &mut toml::Value, key: &str, value: toml::Value) -> Result<()> {
    let mut parts = key.split('.').peekable();
    if !LIVE_SETTINGS.contains(&parts.clone().next().unwrap_or_default()) {
        bail!("This setting belongs to Main config");
    }
    let mut table = patch
        .as_table_mut()
        .context("config overlay must be a table")?;
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            table.insert(part.into(), value);
            return Ok(());
        }
        table = table
            .entry(part)
            .or_insert_with(|| toml::Value::Table(Default::default()))
            .as_table_mut()
            .context("config overlay setting must be a table")?;
    }
    Ok(())
}

fn apply_edits(
    base: &mut Config,
    scope: &EditingScope,
    edited: &Config,
    rows: &[&Setting],
) -> Result<()> {
    let Some(index) = index_for(base, scope)? else {
        for row in rows {
            row.copy(base, edited);
        }
        return Ok(());
    };
    let value = toml::Value::try_from(edited)?;
    let overlay = &mut Arc::make_mut(&mut base.loaded_overlays)[index];
    for row in rows {
        patch_setting(
            &mut overlay.patch,
            row.key,
            setting_value(&value, row.key)?.clone(),
        )?;
    }
    Ok(())
}

pub(crate) fn preview_edits(
    scope: &EditingScope,
    edited: &Config,
    rows: &[&Setting],
) -> Result<()> {
    let mut base = config::settings_config();
    apply_edits(&mut base, scope, edited, rows)?;
    validate_config_values(&scoped_config(&base, scope)?)?;
    config::replace_live(base);
    Ok(())
}

fn write_edited_overlay(path: &Path, edited: &Config, rows: &[&Setting]) -> Result<()> {
    let mut document = std::fs::read_to_string(path)?.parse::<toml_edit::DocumentMut>()?;
    let value = toml::Value::try_from(edited)?;
    for row in rows {
        if !LIVE_SETTINGS.contains(&row.key.split('.').next().unwrap_or_default()) {
            bail!("This setting belongs to Main config");
        }
        let mut parts = row.key.split('.').peekable();
        let mut table: &mut dyn toml_edit::TableLike = document.as_table_mut();
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                let mut item = config::toml_to_item(row.key, setting_value(&value, row.key)?)?;
                if let Some(existing) = table.get(part).and_then(toml_edit::Item::as_value)
                    && let Some(value) = item.as_value_mut()
                {
                    *value.decor_mut() = existing.decor().clone();
                }
                if let Some(existing) = table.get_mut(part) {
                    *existing = item;
                } else {
                    table.insert(part, item);
                }
                break;
            }
            if !table.contains_key(part) {
                table.insert(part, toml_edit::Item::Table(toml_edit::Table::new()));
            }
            table = table
                .get_mut(part)
                .and_then(toml_edit::Item::as_table_like_mut)
                .context("config overlay setting must be a table")?;
        }
    }
    atomic_write(path, &document.to_string())
}

fn atomic_write(path: &Path, text: &str) -> Result<()> {
    let temporary = path.with_extension(format!("toml.{}.tmp", std::process::id()));
    std::fs::write(&temporary, text)?;
    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

fn writable_path() -> Result<&'static Path> {
    if !config::uses_user_config()
        || config::user_config_unreadable()
        || config::preferred_is_replay()
    {
        bail!("Conditions can be saved only to a readable user config outside replay");
    }
    config::CONFIG_PATH
        .get()
        .map(PathBuf::as_path)
        .context("Config path not set")
}

pub(crate) fn save_edits(scope: &EditingScope, edited: &Config, rows: &[&Setting]) -> Result<()> {
    if *scope == EditingScope::Main {
        return config::persist_settings(|disk| {
            for row in rows {
                row.copy(disk, edited);
            }
        });
    }
    let path = writable_path()?;
    let mut base = config::DISK_CONFIG
        .get()
        .context("disk config is not initialized")?
        .lock()
        .unwrap()
        .clone();
    let index = index_for(&base, scope)?.context("No condition selected")?;
    apply_edits(&mut base, scope, edited, rows)?;
    validate_loaded_config(&scoped_config(&base, scope)?, path)?;
    write_edited_overlay(&base.loaded_overlays[index].path, edited, rows)?;
    config::store_disk_config(base.clone());
    config::replace_live(base);
    Ok(())
}

fn write_definition(
    base: &mut Config,
    path: &Path,
    draft: &ConditionDraft,
) -> Result<EditingScope> {
    let name = draft.name.trim();
    if name.is_empty() {
        bail!("Enter a condition name");
    }
    let when = if draft.terms.is_empty()
        && matches!(draft.scope, EditingScope::Overlay(_))
        && draft.original.is_none()
    {
        None
    } else {
        draft.expression()?
    };
    let directory = path.parent().context("Config file has no directory")?;
    let index = index_for(base, &draft.scope)?;
    let file = if let Some(index) = index {
        base.config_overlays[index].source.file.clone()
    } else {
        std::fs::create_dir_all(directory.join(CONDITION_DIRECTORY))?;
        let mut number = 1;
        loop {
            let file = format!("{CONDITION_DIRECTORY}/condition-{number}.toml");
            if !directory.join(&file).exists() {
                break file;
            }
            number += 1;
        }
    };
    let rule = Rule::try_from(RuleSource {
        file: file.clone(),
        name: Some(name.into()),
        when,
    })
    .map_err(anyhow::Error::msg)?;
    let new_file = index.is_none();
    if new_file {
        atomic_write(&directory.join(&file), "")?;
    }
    let previous = base.clone();
    if let Some(index) = index {
        base.config_overlays[index] = rule;
    } else {
        base.config_overlays.push(rule);
    }
    let result = load(base, path).and_then(|_| {
        config::write_user_overlay(path, base, config::controller_map_file().as_deref())
    });
    if let Err(error) = result {
        *base = previous;
        if new_file {
            let _ = std::fs::remove_file(directory.join(&file));
        }
        return Err(error);
    }
    Ok(EditingScope::Overlay(file))
}

pub(crate) fn save_condition(draft: &ConditionDraft) -> Result<EditingScope> {
    let base = config::settings_config();
    if let Some(index) = index_for(&base, &draft.scope)? {
        let existing = draft_for(&base.config_overlays[index]);
        if draft.name == existing.name && draft.terms == existing.terms {
            return Ok(draft.scope.clone());
        }
    }
    let path = writable_path()?;
    let mut base = config::DISK_CONFIG
        .get()
        .context("disk config is not initialized")?
        .lock()
        .unwrap()
        .clone();
    let scope = write_definition(&mut base, path, draft)?;
    config::store_disk_config(base.clone());
    config::replace_live(base);
    Ok(scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "kosk-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        directory
    }

    fn term(executable: &str, join: TermJoin, negated: bool) -> ConditionTerm {
        ConditionTerm {
            executable: executable.into(),
            join,
            negated,
        }
    }

    #[test]
    fn connectors_split_and_join_groups_without_changing_terms() {
        let mut draft = ConditionDraft::new();
        draft.terms = vec![
            term("a.exe", TermJoin::And, false),
            term("b.exe", TermJoin::And, true),
            term("c.exe", TermJoin::Or, false),
        ];
        assert_eq!(draft.groups(), [0..2, 2..3]);
        let expression = WhenExpr::parse(&draft.expression().unwrap().unwrap()).unwrap();
        for (app, expected) in [("a.exe", true), ("b.exe", false), ("c.exe", true)] {
            assert_eq!(
                expression.eval(&WhenContext {
                    focused_app: Some(app.into()),
                    ..Default::default()
                }),
                expected
            );
        }
        draft.terms[2].join = TermJoin::And;
        assert_eq!(draft.groups(), std::iter::once(0..3).collect::<Vec<_>>());
        draft.terms[1].join = TermJoin::Or;
        assert_eq!(draft.groups(), [0..1, 1..3]);
    }

    #[test]
    fn imported_nested_expression_is_displayed_in_equivalent_groups_and_preserved() {
        let source = r#"(focusedApp("a.exe") || focusedApp("b.exe")) && !focusedApp("c.exe")"#;
        let rule = Rule::try_from(RuleSource {
            file: "game.toml".into(),
            name: Some("Games".into()),
            when: Some(source.into()),
        })
        .unwrap();
        let mut draft = draft_for(&rule);
        assert_eq!(draft.groups(), [0..2, 2..4]);
        assert_eq!(draft.expression().unwrap().as_deref(), Some(source));
        draft.original = None;
        let normalized = WhenExpr::parse(&draft.expression().unwrap().unwrap()).unwrap();
        for app in [
            None,
            Some("a.exe"),
            Some("b.exe"),
            Some("c.exe"),
            Some("other.exe"),
        ] {
            let context = WhenContext {
                focused_app: app.map(str::to_owned),
                ..Default::default()
            };
            assert_eq!(
                normalized.eval(&context),
                rule.condition.as_ref().unwrap().eval(&context)
            );
        }
    }

    #[test]
    fn named_condition_create_rename_and_sparse_edits_preserve_other_sources() {
        let directory = temp_dir("named-condition");
        let path = directory.join("config.toml");
        let main = "# main comment\nconfig_version = 3\nscale_x = 36.0\nfuture_setting = true\n";
        std::fs::write(&path, main).unwrap();
        let mut base = config::read_merged_config(&path, config::ConfigSource::Explicit)
            .unwrap()
            .0;
        let mut draft = ConditionDraft::new();
        draft.name = "Games".into();
        draft.terms.push(term("factorio.exe", TermJoin::And, false));
        let scope = write_definition(&mut base, &path, &draft).unwrap();
        let index = index_for(&base, &scope).unwrap().unwrap();
        let overlay_path = base.loaded_overlays[index].path.clone();
        std::fs::write(
            &overlay_path,
            "# game comment\nscale_x = 44.0\n[sc2.stick]\nscale_y = 2.5\n",
        )
        .unwrap();
        load(&mut base, &path).unwrap();
        let mut edited = scoped_config(&base, &scope).unwrap();
        assert_eq!(edited.scale_x, 44.0);
        assert_eq!(base.scale_x, 36.0);
        edited.scale_x = 36.0;
        let rows = [config::schema::setting_for_key("scale_x").unwrap()];
        write_edited_overlay(&overlay_path, &edited, &rows).unwrap();
        let saved = std::fs::read_to_string(&overlay_path).unwrap();
        assert!(saved.contains("# game comment"));
        let value: toml::Value = toml::from_str(&saved).unwrap();
        assert_eq!(value["scale_x"].as_float(), Some(36.0));
        assert_eq!(value["sc2"]["stick"]["scale_y"].as_float(), Some(2.5));
        assert!(value.get("completion").is_none());
        draft = draft_for(&base.config_overlays[index]);
        draft.name = "Factory".into();
        assert_eq!(write_definition(&mut base, &path, &draft).unwrap(), scope);
        assert_eq!(std::fs::read_to_string(&overlay_path).unwrap(), saved);
        let main = std::fs::read_to_string(&path).unwrap();
        assert!(main.contains("# main comment"));
        let main: toml::Value = toml::from_str(&main).unwrap();
        assert_eq!(main["scale_x"].as_float(), Some(36.0));
        assert_eq!(main["future_setting"].as_bool(), Some(true));
        assert_eq!(main["config_overlays"][0]["name"].as_str(), Some("Factory"));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn invalid_draft_does_not_create_overlay_files_or_change_main() {
        let directory = temp_dir("invalid-condition");
        let path = directory.join("config.toml");
        std::fs::write(&path, "scale_x = 36.0\n").unwrap();
        let mut base = Config::default();
        let mut draft = ConditionDraft::new();
        draft.name = "Game".into();
        draft.terms.push(ConditionTerm::default());
        assert!(write_definition(&mut base, &path, &draft).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "scale_x = 36.0\n");
        assert!(!directory.join(CONDITION_DIRECTORY).exists());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn edits_preserve_inline_tables_and_sibling_overrides() {
        let directory = temp_dir("inline-condition");
        let path = directory.join("overlay.toml");
        std::fs::write(
            &path,
            "# inline comment\ndebug = { show_hitboxes = false, show_pad_bounds = true }\n",
        )
        .unwrap();
        let mut edited = Config::default();
        let row = config::schema::setting_for_key("debug.show_hitboxes").unwrap();
        row.nudge(&mut edited, 1);
        write_edited_overlay(&path, &edited, &[row]).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# inline comment"));
        let value: toml::Value = toml::from_str(&text).unwrap();
        assert_eq!(value["debug"]["show_hitboxes"].as_bool(), Some(true));
        assert_eq!(value["debug"]["show_pad_bounds"].as_bool(), Some(true));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn oversized_import_retains_expression_and_allows_only_renaming() {
        let source = (0..=MAX_EXPANDED_TERMS)
            .map(|index| format!("focusedApp(\"app-{index}.exe\")"))
            .collect::<Vec<_>>()
            .join(" || ");
        let rule = Rule::try_from(RuleSource {
            file: "large.toml".into(),
            name: None,
            when: Some(source.clone()),
        })
        .unwrap();
        let mut draft = draft_for(&rule);
        assert!(!draft.row_editable());
        draft.name = "Apps".into();
        assert_eq!(draft.expression().unwrap(), Some(source));
    }
}

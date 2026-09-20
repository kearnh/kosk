//! User-declared exe → type map. Unlisted processes use the implicit catch-all.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Result};

use super::settings::CompletionAppTypeConfig;

/// Persist / resolve id for processes not listed in `[completion.app_types]`.
pub const CATCH_ALL_TYPE: &str = "*";

#[derive(Debug, Clone, Default)]
pub struct AppTypeMap {
    exe_to_type: HashMap<String, String>,
}

impl AppTypeMap {
    pub fn from_config(types: &HashMap<String, CompletionAppTypeConfig>) -> Result<Self> {
        validate_app_types(types)?;
        let mut exe_to_type = HashMap::new();
        for (name, t) in types {
            for exe in &t.exes {
                exe_to_type.insert(normalize_exe(exe), name.clone());
            }
        }
        Ok(Self { exe_to_type })
    }

    pub fn resolve(&self, exe: Option<&str>) -> String {
        let Some(exe) = exe else {
            return CATCH_ALL_TYPE.to_string();
        };
        self.exe_to_type
            .get(&normalize_exe(exe))
            .cloned()
            .unwrap_or_else(|| CATCH_ALL_TYPE.to_string())
    }
}

pub fn validate_app_types(types: &HashMap<String, CompletionAppTypeConfig>) -> Result<()> {
    let mut exe_to_type: HashMap<String, String> = HashMap::new();
    for (name, t) in types {
        if name.is_empty() {
            bail!("app type name is empty");
        }
        if name == CATCH_ALL_TYPE {
            bail!("app type name '{CATCH_ALL_TYPE}' is reserved for the implicit catch-all");
        }
        if t.exes.is_empty() {
            bail!("app type '{name}' has no exes");
        }
        for exe in &t.exes {
            let key = normalize_exe(exe);
            if key.is_empty() {
                bail!("app type '{name}' has an empty exe");
            }
            if let Some(other) = exe_to_type.get(&key) {
                bail!("exe '{key}' is in app types '{other}' and '{name}'");
            }
            exe_to_type.insert(key, name.clone());
        }
    }
    Ok(())
}

pub fn normalize_exe(exe: &str) -> String {
    Path::new(exe.trim())
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(exe)
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::completion::settings::CompletionAppTypeConfig;

    fn browser_programming() -> HashMap<String, CompletionAppTypeConfig> {
        let mut m = HashMap::new();
        m.insert(
            "browser".into(),
            CompletionAppTypeConfig {
                exes: vec!["firefox.exe".into(), "chrome.exe".into()],
                wordlist: None,
            },
        );
        m.insert(
            "programming".into(),
            CompletionAppTypeConfig {
                exes: vec!["code.exe".into()],
                wordlist: None,
            },
        );
        m
    }

    #[test]
    fn firefox_maps_to_browser() {
        let map = AppTypeMap::from_config(&browser_programming()).unwrap();
        assert_eq!(map.resolve(Some("Firefox.exe")), "browser");
        assert_eq!(
            map.resolve(Some(r"C:\Program Files\Mozilla Firefox\firefox.exe")),
            "browser"
        );
    }

    #[test]
    fn unmatched_is_catch_all() {
        let map = AppTypeMap::from_config(&browser_programming()).unwrap();
        assert_eq!(map.resolve(Some("notepad.exe")), CATCH_ALL_TYPE);
        assert_eq!(map.resolve(None), CATCH_ALL_TYPE);
    }

    #[test]
    fn duplicate_exe_errors() {
        let mut m = browser_programming();
        m.get_mut("programming")
            .unwrap()
            .exes
            .push("firefox.exe".into());
        let err = AppTypeMap::from_config(&m).unwrap_err().to_string();
        assert!(err.contains("firefox.exe"), "{err}");
        assert!(err.contains("browser"), "{err}");
        assert!(err.contains("programming"), "{err}");
    }

    #[test]
    fn reserved_catch_all_name_rejected() {
        let mut m = HashMap::new();
        m.insert(
            CATCH_ALL_TYPE.into(),
            CompletionAppTypeConfig {
                exes: vec!["x.exe".into()],
                wordlist: None,
            },
        );
        let err = AppTypeMap::from_config(&m).unwrap_err().to_string();
        assert!(err.contains("reserved"), "{err}");
    }

    #[test]
    fn empty_name_rejected() {
        let mut m = HashMap::new();
        m.insert(
            "".into(),
            CompletionAppTypeConfig {
                exes: vec!["x.exe".into()],
                wordlist: None,
            },
        );
        assert!(AppTypeMap::from_config(&m).is_err());
    }
}

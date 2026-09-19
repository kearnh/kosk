//! TOML mapping values: a bare action string, `{ action, when }`, or an array of those.

use serde::{Deserialize, Serialize};

use crate::controller::ControllerBinding;
use crate::when::WhenExpr;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MappingRule {
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MappingValue {
    Action(String),
    Rules(Vec<MappingRule>),
    Rule(MappingRule),
}

/// One editor pill: a binding plus optional `when` (None = unconditional).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappingPill {
    pub binding: ControllerBinding,
    pub when: Option<String>,
}

impl MappingPill {
    pub fn always(binding: ControllerBinding) -> Self {
        Self {
            binding,
            when: None,
        }
    }

    pub fn label(&self) -> String {
        match &self.when {
            Some(w) if !w.is_empty() => format!("{} | {w}", self.binding),
            _ => self.binding.to_string(),
        }
    }
}

impl MappingValue {
    pub fn from_action(action: impl Into<String>) -> Self {
        Self::Action(action.into())
    }

    pub fn rules(&self) -> Result<Vec<MappingRule>, String> {
        match self {
            Self::Action(action) => Ok(vec![MappingRule {
                action: action.clone(),
                when: None,
            }]),
            Self::Rule(rule) => Ok(vec![rule.clone()]),
            Self::Rules(rules) => {
                if rules.is_empty() {
                    return Err("mapping array must not be empty".to_owned());
                }
                Ok(rules.clone())
            }
        }
    }
}

impl From<&str> for MappingValue {
    fn from(action: &str) -> Self {
        Self::Action(action.to_owned())
    }
}

impl From<String> for MappingValue {
    fn from(action: String) -> Self {
        Self::Action(action)
    }
}

/// Validate fallback placement before compiling to a [`crate::controller::bindings::BindingTarget`].
pub fn validate_rule_order(rules: &[MappingRule]) -> Result<(), String> {
    let mut seen_fallback = false;
    for (i, rule) in rules.iter().enumerate() {
        if let Some(src) = &rule.when {
            if seen_fallback {
                return Err(
                    "when-clause after a fallback (entry without when must be last)".to_owned(),
                );
            }
            WhenExpr::parse(src)?;
            continue;
        }
        if seen_fallback {
            return Err("two fallback entries (at most one entry without when)".to_owned());
        }
        if i != rules.len() - 1 {
            return Err("fallback (entry without when) must be last".to_owned());
        }
        seen_fallback = true;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::{ControllerBinding, ControllerButton};
    use std::collections::HashMap;

    #[test]
    fn parses_string_table_and_array() {
        let toml_src = r#"
[Keyboard]
"padLeft" = "sendKeyUnderLeftStick"
"faceRight" = { action = "cancelSuggestion", when = "suggestionSelected" }
"faceBottom" = [
  { action = "acceptSuggestion", when = "suggestionSelected" },
  { action = "sendKey.enter" },
]
"#;
        let map: HashMap<String, HashMap<ControllerBinding, MappingValue>> =
            toml::from_str(toml_src).expect("parse");
        let kb = map.get("Keyboard").expect("keyboard");
        let pad = kb
            .get(&ControllerBinding::Single(ControllerButton::PadLeft))
            .expect("pad");
        assert_eq!(pad, &MappingValue::from_action("sendKeyUnderLeftStick"));
        let face_right = kb
            .get(&ControllerBinding::Single(ControllerButton::FaceRight))
            .expect("faceRight");
        match face_right {
            MappingValue::Rule(r) => {
                assert_eq!(r.action, "cancelSuggestion");
                assert_eq!(r.when.as_deref(), Some("suggestionSelected"));
            }
            other => panic!("{other:?}"),
        }
        let face_bottom = kb
            .get(&ControllerBinding::Single(ControllerButton::FaceBottom))
            .expect("faceBottom");
        match face_bottom {
            MappingValue::Rules(rs) => {
                assert_eq!(rs.len(), 2);
                assert_eq!(rs[0].action, "acceptSuggestion");
                assert_eq!(rs[1].when, None);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn fallback_must_be_last() {
        let rules = vec![
            MappingRule {
                action: "sendKey.enter".into(),
                when: None,
            },
            MappingRule {
                action: "acceptSuggestion".into(),
                when: Some("suggestionSelected".into()),
            },
        ];
        assert!(validate_rule_order(&rules).unwrap_err().contains("last"));
    }

    #[test]
    fn repo_mappings_toml_parses() {
        let src = include_str!("../../mappings.toml");
        let map: HashMap<String, HashMap<ControllerBinding, MappingValue>> =
            toml::from_str(src).expect("repo mappings.toml");
        let kb = map.get("Keyboard").expect("Keyboard");
        assert!(matches!(
            kb.get(&ControllerBinding::Single(ControllerButton::FaceRight)),
            Some(MappingValue::Rule(_))
        ));
        assert!(matches!(
            kb.get(&ControllerBinding::Single(ControllerButton::TriggerLeft)),
            Some(MappingValue::Rules(_))
        ));
    }

    #[test]
    fn two_fallbacks_error() {
        let rules = vec![
            MappingRule {
                action: "a".into(),
                when: None,
            },
            MappingRule {
                action: "b".into(),
                when: None,
            },
        ];
        assert!(validate_rule_order(&rules).unwrap_err().contains("last"));
    }
}

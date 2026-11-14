use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub status_fields: Vec<String>,
    pub key_bindings: BTreeMap<String, String>,
    pub disabled_skills: BTreeSet<String>,
    pub reduce_motion: bool,
    pub ambient_art: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            status_fields: ["model", "directory", "branch", "context", "permissions"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            key_bindings: BTreeMap::new(),
            disabled_skills: BTreeSet::new(),
            reduce_motion: false,
            ambient_art: true,
        }
    }
}

pub const STATUS_FIELDS: &[(&str, &str, &str)] = &[
    ("model", "Model", "Active model and provider"),
    (
        "directory",
        "Directory",
        "Current workspace, shortened relative to home",
    ),
    (
        "branch",
        "Git branch",
        "Current repository branch when available",
    ),
    (
        "context",
        "Context",
        "Estimated conversation context, clearly marked as an estimate",
    ),
    (
        "permissions",
        "Permissions",
        "Actual tool policy and dangerous-tool approval setting",
    ),
    (
        "tokens",
        "Tokens",
        "Session token estimates; provider-reported counts are shown in Usage",
    ),
    ("session", "Session name", "Named session, when set"),
    (
        "activity",
        "Activity",
        "Current agent phase and active tool",
    ),
    ("elapsed", "Elapsed", "Time since this process started"),
    ("version", "Version", "Installed OsmoGrep version"),
];

impl Preferences {
    pub fn normalize(&mut self) {
        let overrides = std::mem::take(&mut self.key_bindings);
        for (id, value) in overrides {
            let _ = crate::ui::keymap::assign(self, &id, &value);
        }
        let mut seen = BTreeSet::new();
        self.status_fields.retain(|id| {
            STATUS_FIELDS.iter().any(|(key, _, _)| key == id) && seen.insert(id.clone())
        });
        self.status_fields.truncate(STATUS_FIELDS.len());
        self.disabled_skills = self.disabled_skills.iter().take(512).cloned().collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_and_conflicting_keymaps_cannot_override_protected_controls() {
        let mut settings:Preferences=serde_json::from_str(r#"{"key_bindings":{"copy":"f1","commands":"ctrl+p","skills":"ctrl+p","future":"f12"}}"#).unwrap();
        settings.normalize();
        assert_eq!(
            settings.key_bindings.get("commands").map(String::as_str),
            Some("ctrl+p")
        );
        assert!(!settings.key_bindings.contains_key("copy"));
        assert!(!settings.key_bindings.contains_key("skills"));
        assert!(!settings.key_bindings.contains_key("future"));
    }
    #[test]
    fn old_settings_receive_defaults_and_unknown_status_fields_are_safe() {
        let mut settings: Preferences =
            serde_json::from_str(r#"{"status_fields":["model","future","model","branch"]}"#)
                .unwrap();
        settings.normalize();
        assert_eq!(settings.status_fields, vec!["model", "branch"]);
        assert!(settings.ambient_art);
        assert!(!settings.reduce_motion);
        assert!(settings.key_bindings.is_empty());
    }
}

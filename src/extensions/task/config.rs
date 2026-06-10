//! Task extension configuration.
//!
//! Defines `TaskConfig` and `StatusKeywords` with YAML deserialization
//! and default values matching the project requirements.

use serde::Deserialize;

use crate::error::RagtagError;

/// Allowed time unit values (fixed set, not user-configurable).
pub const ALLOWED_TIME_UNITS: &[&str] = &["hours", "days", "weeks"];

/// Configuration for the task extension.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TaskConfig {
    /// The tag name used for tasks (default: "task").
    pub tag_name: String,
    /// Default owner for new tasks (default: "me").
    pub default_owner: String,
    /// Default time units (default: "hours").
    pub default_time_units: String,
    /// Default status for new tasks (default: "new").
    pub default_status: String,
    /// Status keywords grouped by category.
    pub status_keywords: StatusKeywords,
    /// Status categories to exclude from list/summary by default.
    /// Defaults to ["done", "abandoned"]. These use the category names
    /// (the keys in status_keywords), not individual keyword values.
    pub exclude_status_categories: Vec<String>,
}

impl Default for TaskConfig {
    fn default() -> Self {
        Self {
            tag_name: "task".to_string(),
            default_owner: "me".to_string(),
            default_time_units: "hours".to_string(),
            default_status: "new".to_string(),
            status_keywords: StatusKeywords::default(),
            exclude_status_categories: vec!["done".to_string(), "abandoned".to_string()],
        }
    }
}

impl TaskConfig {
    /// Validates the configuration.
    pub fn validate(&self) -> Result<(), RagtagError> {
        if !ALLOWED_TIME_UNITS.contains(&self.default_time_units.as_str()) {
            return Err(RagtagError::ExtensionError {
                extension_name: "Task Manager".to_string(),
                message: format!(
                    "invalid default_time_units \"{}\" — allowed values: {}",
                    self.default_time_units,
                    ALLOWED_TIME_UNITS.join(", ")
                ),
            });
        }

        if !self
            .all_status_keywords()
            .contains(&self.default_status.as_str())
        {
            return Err(RagtagError::ExtensionError {
                extension_name: "Task Manager".to_string(),
                message: format!(
                    "invalid default_status \"{}\" — must be a recognized status keyword",
                    self.default_status
                ),
            });
        }

        Ok(())
    }

    /// Returns a flattened list of all valid status keywords.
    pub fn all_status_keywords(&self) -> Vec<&str> {
        let mut keywords = Vec::new();
        keywords.extend(self.status_keywords.done.iter().map(|s| s.as_str()));
        keywords.extend(self.status_keywords.active.iter().map(|s| s.as_str()));
        keywords.extend(self.status_keywords.blocked.iter().map(|s| s.as_str()));
        keywords.extend(self.status_keywords.abandoned.iter().map(|s| s.as_str()));
        keywords.extend(self.status_keywords.inactive.iter().map(|s| s.as_str()));
        keywords
    }

    /// Returns the list of status keywords that belong to excluded categories.
    ///
    /// Looks up each category name in `exclude_status_categories` and collects
    /// all individual keywords from those categories.
    pub fn get_excluded_keywords(&self) -> Vec<String> {
        let mut excluded = Vec::new();
        for cat in &self.exclude_status_categories {
            match cat.as_str() {
                "done" => excluded.extend(self.status_keywords.done.iter().cloned()),
                "active" => excluded.extend(self.status_keywords.active.iter().cloned()),
                "blocked" => excluded.extend(self.status_keywords.blocked.iter().cloned()),
                "abandoned" => excluded.extend(self.status_keywords.abandoned.iter().cloned()),
                "inactive" => excluded.extend(self.status_keywords.inactive.iter().cloned()),
                _ => {}
            }
        }
        excluded
    }

    /// Deserializes from a raw YAML value.
    pub fn from_config_value(val: &serde_yml::Value) -> Result<Self, RagtagError> {
        serde_yml::from_value(val.clone()).map_err(|e| RagtagError::ExtensionError {
            extension_name: "Task Manager".to_string(),
            message: format!("invalid task configuration: {e}"),
        })
    }
}

/// Status keywords grouped by category.
///
/// Each category maps to a color for output formatting.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct StatusKeywords {
    pub done: Vec<String>,
    pub active: Vec<String>,
    pub blocked: Vec<String>,
    pub abandoned: Vec<String>,
    pub inactive: Vec<String>,
}

impl Default for StatusKeywords {
    fn default() -> Self {
        Self {
            done: vec![
                "done".to_string(),
                "finished".to_string(),
                "complete".to_string(),
                "completed".to_string(),
            ],
            active: vec![
                "active".to_string(),
                "underway".to_string(),
                "working".to_string(),
                "wip".to_string(),
            ],
            blocked: vec!["blocked".to_string()],
            abandoned: vec![
                "abandoned".to_string(),
                "deleted".to_string(),
                "removed".to_string(),
                "dead".to_string(),
            ],
            inactive: vec![
                "inactive".to_string(),
                "pending".to_string(),
                "new".to_string(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TaskConfig::default();
        assert_eq!(config.tag_name, "task");
        assert_eq!(config.default_owner, "me");
        assert_eq!(config.default_time_units, "hours");
        assert_eq!(config.default_status, "new");
    }

    #[test]
    fn test_validate_ok() {
        let config = TaskConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_bad_time_units() {
        let mut config = TaskConfig::default();
        config.default_time_units = "fortnights".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_bad_status() {
        let mut config = TaskConfig::default();
        config.default_status = "unknown_status".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_all_status_keywords() {
        let config = TaskConfig::default();
        let all = config.all_status_keywords();
        assert!(all.contains(&"done"));
        assert!(all.contains(&"active"));
        assert!(all.contains(&"blocked"));
        assert!(all.contains(&"abandoned"));
        assert!(all.contains(&"new"));
    }

    #[test]
    fn test_from_config_value() {
        let yaml = serde_yml::from_str::<serde_yml::Value>(
            r#"
tag_name: "todo"
default_owner: "alice"
"#,
        )
        .unwrap();
        let config = TaskConfig::from_config_value(&yaml).unwrap();
        assert_eq!(config.tag_name, "todo");
        assert_eq!(config.default_owner, "alice");
    }

    #[test]
    fn test_default_status_keywords() {
        let kw = StatusKeywords::default();
        assert_eq!(kw.done.len(), 4);
        assert_eq!(kw.active.len(), 4);
        assert_eq!(kw.blocked.len(), 1);
        assert_eq!(kw.abandoned.len(), 4);
        assert_eq!(kw.inactive.len(), 3);
    }

    #[test]
    fn test_default_exclude_status_categories() {
        let config = TaskConfig::default();
        assert_eq!(
            config.exclude_status_categories,
            vec!["done".to_string(), "abandoned".to_string()]
        );
    }

    #[test]
    fn test_get_excluded_keywords_default() {
        let config = TaskConfig::default();
        let excluded = config.get_excluded_keywords();
        // Should include all "done" and "abandoned" keywords
        assert!(excluded.contains(&"done".to_string()));
        assert!(excluded.contains(&"finished".to_string()));
        assert!(excluded.contains(&"complete".to_string()));
        assert!(excluded.contains(&"completed".to_string()));
        assert!(excluded.contains(&"abandoned".to_string()));
        assert!(excluded.contains(&"deleted".to_string()));
        assert!(excluded.contains(&"removed".to_string()));
        assert!(excluded.contains(&"dead".to_string()));
        // Should NOT include active/blocked/inactive keywords
        assert!(!excluded.contains(&"active".to_string()));
        assert!(!excluded.contains(&"blocked".to_string()));
        assert!(!excluded.contains(&"new".to_string()));
    }

    #[test]
    fn test_get_excluded_keywords_empty() {
        let mut config = TaskConfig::default();
        config.exclude_status_categories = vec![];
        let excluded = config.get_excluded_keywords();
        assert!(excluded.is_empty());
    }
}

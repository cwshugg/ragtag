//! Config inspection command.
//!
//! Provides the `config get` subcommand, which prints the value of any
//! config field using dot-notation. This enables external tools (like
//! editor plugins) to retrieve config values without parsing YAML.

use crate::config::Config;
use crate::error::RagtagError;
use crate::extensions::task::config::TaskConfig;

/// Config key for the task extension section.
const TASKS_CONFIG_KEY: &str = "tasks";

/// Runs the `config get` command.
///
/// Serializes the resolved config to a `serde_yml::Value` tree,
/// merges resolved extension configs (with defaults applied),
/// then traverses the tree using dot-notation segments from `key`.
///
/// # Errors
///
/// Returns `RagtagError::InvalidConfig` if the key is unknown or
/// traversal fails (e.g., indexing through a scalar value).
pub fn run_get(key: &str, config: &Config) -> Result<String, RagtagError> {
    let mut root = serde_yml::to_value(config)
        .map_err(|e| RagtagError::InvalidConfig(format!("failed to serialize config: {e}")))?;

    // Merge resolved task extension config (with defaults applied).
    let task_config = config
        .extension_configs
        .get(TASKS_CONFIG_KEY)
        .map(|raw| TaskConfig::from_config_value(raw).unwrap_or_default())
        .unwrap_or_default();

    let resolved_tasks = serde_yml::to_value(&task_config)
        .map_err(|e| RagtagError::InvalidConfig(format!("failed to serialize task config: {e}")))?;

    if let serde_yml::Value::Mapping(ref mut map) = root {
        map.insert(
            serde_yml::Value::String(TASKS_CONFIG_KEY.to_string()),
            resolved_tasks,
        );
        // Remove the raw extension_configs map — we've merged resolved versions.
        map.remove(serde_yml::Value::String("extension_configs".to_string()));
    }

    // Traverse the value tree using dot-notation segments.
    let segments: Vec<&str> = key.split('.').collect();
    let mut current = &root;

    for (i, segment) in segments.iter().enumerate() {
        match current {
            serde_yml::Value::Mapping(map) => {
                let key_val = serde_yml::Value::String((*segment).to_string());
                match map.get(&key_val) {
                    Some(val) => current = val,
                    None => {
                        let path = segments[..=i].join(".");
                        return Err(RagtagError::InvalidConfig(format!(
                            "unknown config key \"{path}\""
                        )));
                    }
                }
            }
            _ => {
                let path = segments[..i].join(".");
                return Err(RagtagError::InvalidConfig(format!(
                    "\"{path}\" is not a section; cannot access \"{path}.{segment}\""
                )));
            }
        }
    }

    Ok(format_value(current))
}

/// Formats a `serde_yml::Value` for human-readable output.
///
/// Strings are printed without quotes, numbers and booleans as-is,
/// sequences in JSON-like bracket notation, and mappings in braces.
fn format_value(val: &serde_yml::Value) -> String {
    match val {
        serde_yml::Value::Null => "null".to_string(),
        serde_yml::Value::Bool(b) => b.to_string(),
        serde_yml::Value::Number(n) => n.to_string(),
        serde_yml::Value::String(s) => s.clone(),
        serde_yml::Value::Sequence(seq) => {
            let items: Vec<String> = seq
                .iter()
                .map(|v| match v {
                    serde_yml::Value::String(s) => format!("\"{s}\""),
                    other => format_value(other),
                })
                .collect();
            format!("[{}]", items.join(", "))
        }
        serde_yml::Value::Mapping(map) => {
            let items: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    let key_str = format_value(k);
                    let val_str = format_value(v);
                    format!("{key_str}: {val_str}")
                })
                .collect();
            format!("{{{}}}", items.join(", "))
        }
        serde_yml::Value::Tagged(tagged) => format_value(&tagged.value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a default config for testing.
    fn default_config() -> Config {
        Config::default()
    }

    #[test]
    fn test_get_respect_gitignore() {
        let config = default_config();
        let result = run_get("respect_gitignore", &config).unwrap();
        assert_eq!(result, "true");
    }

    #[test]
    fn test_get_skip_hidden() {
        let config = default_config();
        let result = run_get("skip_hidden", &config).unwrap();
        assert_eq!(result, "true");
    }

    #[test]
    fn test_get_max_file_size() {
        let config = default_config();
        let result = run_get("max_file_size", &config).unwrap();
        assert_eq!(result, "10485760");
    }

    #[test]
    fn test_get_max_depth_null() {
        let config = default_config();
        let result = run_get("max_depth", &config).unwrap();
        assert_eq!(result, "null");
    }

    #[test]
    fn test_get_max_depth_some() {
        let mut config = default_config();
        config.max_depth = Some(10);
        let result = run_get("max_depth", &config).unwrap();
        assert_eq!(result, "10");
    }

    #[test]
    fn test_get_output_color() {
        let config = default_config();
        let result = run_get("output.color", &config).unwrap();
        assert_eq!(result, "auto");
    }

    #[test]
    fn test_get_tasks_tag_name() {
        let config = default_config();
        let result = run_get("tasks.tag_name", &config).unwrap();
        assert_eq!(result, "task");
    }

    #[test]
    fn test_get_tasks_default_owner() {
        let config = default_config();
        let result = run_get("tasks.default_owner", &config).unwrap();
        assert_eq!(result, "me");
    }

    #[test]
    fn test_get_tasks_default_time_units() {
        let config = default_config();
        let result = run_get("tasks.default_time_units", &config).unwrap();
        assert_eq!(result, "hours");
    }

    #[test]
    fn test_get_tasks_default_status() {
        let config = default_config();
        let result = run_get("tasks.default_status", &config).unwrap();
        assert_eq!(result, "new");
    }

    #[test]
    fn test_get_tasks_exclude_status_categories() {
        let config = default_config();
        let result = run_get("tasks.exclude_status_categories", &config).unwrap();
        assert_eq!(result, r#"["done", "abandoned"]"#);
    }

    #[test]
    fn test_get_tasks_status_keywords_done() {
        let config = default_config();
        let result = run_get("tasks.status_keywords.done", &config).unwrap();
        assert_eq!(result, r#"["done", "finished", "complete", "completed"]"#);
    }

    #[test]
    fn test_get_tasks_status_keywords_active() {
        let config = default_config();
        let result = run_get("tasks.status_keywords.active", &config).unwrap();
        assert_eq!(result, r#"["active", "underway", "working", "wip"]"#);
    }

    #[test]
    fn test_get_ignore_patterns_empty() {
        let config = default_config();
        let result = run_get("ignore_patterns", &config).unwrap();
        assert_eq!(result, "[]");
    }

    #[test]
    fn test_get_ignore_patterns_populated() {
        let mut config = default_config();
        config.ignore_patterns = vec!["*.git".to_string(), "node_modules".to_string()];
        let result = run_get("ignore_patterns", &config).unwrap();
        assert_eq!(result, r#"["*.git", "node_modules"]"#);
    }

    #[test]
    fn test_get_unknown_key() {
        let config = default_config();
        let result = run_get("nonexistent_field", &config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknown config key"));
        assert!(err.contains("nonexistent_field"));
    }

    #[test]
    fn test_get_traversal_through_scalar() {
        let config = default_config();
        let result = run_get("max_file_size.foo", &config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("is not a section"));
    }

    #[test]
    fn test_get_unknown_nested_key() {
        let config = default_config();
        let result = run_get("tasks.nonexistent", &config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknown config key"));
        assert!(err.contains("tasks.nonexistent"));
    }

    #[test]
    fn test_get_with_custom_task_config() {
        let yaml = r#"
tasks:
  tag_name: "todo"
  default_owner: "alice"
"#;
        let config: Config = serde_yml::from_str(yaml).unwrap();
        assert_eq!(run_get("tasks.tag_name", &config).unwrap(), "todo");
        assert_eq!(run_get("tasks.default_owner", &config).unwrap(), "alice");
        // Defaults should still apply for unspecified fields.
        assert_eq!(
            run_get("tasks.default_time_units", &config).unwrap(),
            "hours"
        );
    }
}

//! Config inspection command.
//!
//! Provides the `config get` subcommand, which prints the value of any
//! config field using dot-notation. This enables external tools (like
//! editor plugins) to retrieve config values without parsing YAML.

use crate::config::Config;
use crate::error::RagtagError;
use crate::extensions::task::{config::TaskConfig, TASKS_CONFIG_KEY};

/// Machine-readable effective configuration output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DumpFormat {
    /// Sorted path assignments with JSON scalar values.
    Flat,
    /// A recursively key-sorted YAML document.
    Yaml,
}

/// Builds the complete recognized effective configuration tree.
fn build_effective_config_value<F>(
    config: &Config,
    mut is_environment_derived: F,
) -> Result<yaml_serde::Value, RagtagError>
where
    F: FnMut(&str) -> bool,
{
    let mut root = yaml_serde::to_value(config)
        .map_err(|e| RagtagError::InvalidConfig(format!("failed to serialize config: {e}")))?;

    // Resolve extension defaults and discard unregistered flattened sections.
    let task_config = config
        .extension_configs
        .get(TASKS_CONFIG_KEY)
        .map(TaskConfig::from_config_value)
        .transpose()?
        .unwrap_or_default();
    let resolved_tasks = yaml_serde::to_value(&task_config)
        .map_err(|e| RagtagError::InvalidConfig(format!("failed to serialize task config: {e}")))?;
    if let yaml_serde::Value::Mapping(ref mut map) = root {
        for extension_key in config.extension_configs.keys() {
            map.remove(yaml_serde::Value::String(extension_key.clone()));
        }
        map.insert(
            yaml_serde::Value::String(TASKS_CONFIG_KEY.to_string()),
            resolved_tasks,
        );
    }
    redact_value(&mut root, &mut is_environment_derived);
    Ok(root)
}

/// Replaces every environment-derived string in an effective value tree.
fn redact_value<F>(value: &mut yaml_serde::Value, is_environment_derived: &mut F)
where
    F: FnMut(&str) -> bool,
{
    match value {
        yaml_serde::Value::String(text) if is_environment_derived(text) => {
            *text = "<environment-derived>".to_string();
        }
        yaml_serde::Value::Sequence(sequence) => {
            for child in sequence {
                redact_value(child, is_environment_derived);
            }
        }
        yaml_serde::Value::Mapping(mapping) => {
            for child in mapping.values_mut() {
                redact_value(child, is_environment_derived);
            }
        }
        yaml_serde::Value::Tagged(tagged) => {
            redact_value(&mut tagged.value, is_environment_derived);
        }
        _ => {}
    }
}

/// Runs the `config get` command.
///
/// Serializes the resolved config to a `yaml_serde::Value` tree,
/// merges resolved extension configs (with defaults applied),
/// then traverses the tree using dot-notation segments from `key`.
///
/// # Errors
///
/// Returns `RagtagError::InvalidConfig` if the key is unknown or
/// traversal fails (e.g., indexing through a scalar value).
pub fn run_get<F>(
    key: &str,
    config: &Config,
    mut is_environment_derived: F,
) -> Result<String, RagtagError>
where
    F: FnMut(&str) -> bool,
{
    // Reject empty or whitespace-only keys.
    if key.trim().is_empty() {
        return Err(RagtagError::InvalidConfig(
            "config key must not be empty".to_string(),
        ));
    }

    let root = build_effective_config_value(config, &mut is_environment_derived)?;

    // Traverse the value tree using dot-notation segments.
    let segments: Vec<&str> = key.split('.').collect();
    let mut current = &root;

    for (i, segment) in segments.iter().enumerate() {
        match current {
            yaml_serde::Value::Mapping(map) => {
                let key_val = yaml_serde::Value::String((*segment).to_string());
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

/// Renders the complete effective configuration.
pub fn run_dump<F>(
    config: &Config,
    format: DumpFormat,
    is_environment_derived: F,
) -> Result<String, RagtagError>
where
    F: FnMut(&str) -> bool,
{
    let root = build_effective_config_value(config, is_environment_derived)?;
    match format {
        DumpFormat::Flat => render_flat(&root),
        DumpFormat::Yaml => render_yaml(root),
    }
}

/// Renders deterministic path assignments with JSON-compatible right sides.
fn render_flat(root: &yaml_serde::Value) -> Result<String, RagtagError> {
    fn visit(
        value: &yaml_serde::Value,
        path: &str,
        lines: &mut Vec<String>,
    ) -> Result<(), RagtagError> {
        match value {
            yaml_serde::Value::Mapping(mapping) if mapping.is_empty() => {
                lines.push(format!("{path} = {{}}"));
            }
            yaml_serde::Value::Mapping(mapping) => {
                for (key, child) in mapping {
                    let yaml_serde::Value::String(key) = key else {
                        return Err(RagtagError::InvalidConfig(
                            "effective configuration contains a non-string key".to_string(),
                        ));
                    };
                    let child_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    visit(child, &child_path, lines)?;
                }
            }
            yaml_serde::Value::Sequence(sequence) if sequence.is_empty() => {
                lines.push(format!("{path} = []"));
            }
            yaml_serde::Value::Sequence(sequence) => {
                for (index, child) in sequence.iter().enumerate() {
                    visit(child, &format!("{path}[{index}]"), lines)?;
                }
            }
            yaml_serde::Value::Tagged(tagged) => visit(&tagged.value, path, lines)?,
            scalar => {
                let json = serde_json::to_string(scalar).map_err(|error| {
                    RagtagError::InvalidConfig(format!(
                        "failed to serialize effective config scalar: {error}"
                    ))
                })?;
                lines.push(format!("{path} = {json}"));
            }
        }
        Ok(())
    }

    let mut lines = Vec::new();
    visit(root, "", &mut lines)?;
    lines.sort();
    Ok(format!("{}\n", lines.join("\n")))
}

/// Recursively sorts mappings before deterministic YAML serialization.
fn sort_mappings(value: yaml_serde::Value) -> Result<yaml_serde::Value, RagtagError> {
    Ok(match value {
        yaml_serde::Value::Mapping(mapping) => {
            let mut entries = mapping
                .into_iter()
                .map(|(key, value)| match key {
                    yaml_serde::Value::String(key) => Ok((key, sort_mappings(value)?)),
                    _ => Err(RagtagError::InvalidConfig(
                        "effective configuration contains a non-string key".to_string(),
                    )),
                })
                .collect::<Result<Vec<_>, _>>()?;
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            let mut sorted = yaml_serde::Mapping::new();
            for (key, value) in entries {
                sorted.insert(yaml_serde::Value::String(key), value);
            }
            yaml_serde::Value::Mapping(sorted)
        }
        yaml_serde::Value::Sequence(sequence) => yaml_serde::Value::Sequence(
            sequence
                .into_iter()
                .map(sort_mappings)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        yaml_serde::Value::Tagged(mut tagged) => {
            tagged.value = sort_mappings(tagged.value)?;
            yaml_serde::Value::Tagged(tagged)
        }
        scalar => scalar,
    })
}

/// Renders one sorted YAML document with exactly one trailing newline.
fn render_yaml(root: yaml_serde::Value) -> Result<String, RagtagError> {
    let sorted = sort_mappings(root)?;
    let rendered = yaml_serde::to_string(&sorted)
        .map_err(|e| RagtagError::InvalidConfig(format!("failed to serialize config: {e}")))?;
    Ok(format!("{}\n", rendered.trim_end_matches('\n')))
}

/// Formats a `yaml_serde::Value` for human-readable output.
///
/// Strings are printed without quotes, numbers and booleans as-is,
/// sequences in JSON-like bracket notation, and mappings in braces.
fn format_value(val: &yaml_serde::Value) -> String {
    match val {
        yaml_serde::Value::Null => "null".to_string(),
        yaml_serde::Value::Bool(b) => b.to_string(),
        yaml_serde::Value::Number(n) => n.to_string(),
        yaml_serde::Value::String(s) => s.clone(),
        yaml_serde::Value::Sequence(seq) => {
            let items: Vec<String> = seq
                .iter()
                .map(|v| match v {
                    yaml_serde::Value::String(s) => format!("\"{s}\""),
                    other => format_value(other),
                })
                .collect();
            format!("[{}]", items.join(", "))
        }
        yaml_serde::Value::Mapping(map) => {
            let items: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    let key_str = match k {
                        yaml_serde::Value::String(key) => key.clone(),
                        other => format_value(other),
                    };
                    let val_str = format_value(v);
                    format!("{key_str}: {val_str}")
                })
                .collect();
            format!("{{{}}}", items.join(", "))
        }
        yaml_serde::Value::Tagged(tagged) => format_value(&tagged.value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Alias;

    /// Builds a default config for testing.
    fn default_config() -> Config {
        Config::default()
    }

    /// Reads config without environment-derived values.
    fn get(key: &str, config: &Config) -> Result<String, RagtagError> {
        run_get(key, config, |_| false)
    }

    #[test]
    fn test_get_scalar_defaults() {
        let config = default_config();
        for (key, expected) in [
            ("respect_gitignore", "true"),
            ("skip_hidden", "true"),
            ("max_file_size", "10485760"),
            ("max_depth", "null"),
            ("output.color", "auto"),
            ("files.default_directory", "."),
            ("files.filename_format", "%Y-%m-%d_%H-%M-%S.md"),
            ("tasks.tag_name", "task"),
            ("tasks.default_owner", "me"),
            ("tasks.default_worktime_units", "hours"),
            ("tasks.default_status", "new"),
        ] {
            assert_eq!(get(key, &config).unwrap(), expected, "key: {key}");
        }
    }

    #[test]
    fn test_get_max_depth_some() {
        let mut config = default_config();
        config.max_depth = Some(10);
        let result = get("max_depth", &config).unwrap();
        assert_eq!(result, "10");
    }

    #[test]
    fn test_get_file_defaults_and_overrides() {
        let config = default_config();
        assert_eq!(get("files.default_directory", &config).unwrap(), ".");
        assert_eq!(
            get("files.filename_format", &config).unwrap(),
            "%Y-%m-%d_%H-%M-%S.md"
        );

        let config: Config = yaml_serde::from_str(
            "files:\n  default_directory: notes\n  filename_format: \"%Y%m%d-%3f.txt\"\n",
        )
        .unwrap();
        assert_eq!(get("files.default_directory", &config).unwrap(), "notes");
        assert_eq!(
            get("files.filename_format", &config).unwrap(),
            "%Y%m%d-%3f.txt"
        );
    }

    #[test]
    fn test_get_tasks_exclude_status_categories() {
        let config = default_config();
        let result = get("tasks.exclude_status_categories", &config).unwrap();
        assert_eq!(result, r#"["done", "abandoned"]"#);
    }

    #[test]
    fn test_get_tasks_status_keywords_done() {
        let config = default_config();
        let result = get("tasks.status_keywords.done", &config).unwrap();
        assert_eq!(result, r#"["done", "finished", "complete", "completed"]"#);
    }

    #[test]
    fn test_get_tasks_status_keywords_active() {
        let config = default_config();
        let result = get("tasks.status_keywords.active", &config).unwrap();
        assert_eq!(result, r#"["active", "underway", "working", "wip"]"#);
    }

    #[test]
    fn test_get_ignore_patterns_empty() {
        let config = default_config();
        let result = get("ignore_patterns", &config).unwrap();
        assert_eq!(result, "[]");
    }

    #[test]
    fn test_get_ignore_patterns_populated() {
        let mut config = default_config();
        config.ignore_patterns = vec!["*.git".to_string(), "node_modules".to_string()];
        let result = get("ignore_patterns", &config).unwrap();
        assert_eq!(result, r#"["*.git", "node_modules"]"#);
    }

    #[test]
    fn test_get_unknown_key() {
        let config = default_config();
        let result = get("nonexistent_field", &config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknown config key"));
        assert!(err.contains("nonexistent_field"));
    }

    #[test]
    fn test_get_traversal_through_scalar() {
        let config = default_config();
        let result = get("max_file_size.foo", &config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("is not a section"));
    }

    #[test]
    fn test_get_unknown_nested_key() {
        let config = default_config();
        let result = get("tasks.nonexistent", &config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknown config key"));
        assert!(err.contains("tasks.nonexistent"));
    }

    #[test]
    fn test_get_empty_key() {
        let config = default_config();
        let result = get("", &config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("config key must not be empty"));
    }

    #[test]
    fn test_get_whitespace_key() {
        let config = default_config();
        let result = get("   ", &config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("config key must not be empty"));
    }

    #[test]
    fn test_get_unknown_extension_key() {
        let mut config = default_config();
        config.extension_configs.insert(
            "custom_thing".to_string(),
            yaml_serde::Value::Mapping(yaml_serde::Mapping::new()),
        );
        let result = get("custom_thing", &config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unknown config key"));
    }

    #[test]
    fn test_get_with_custom_task_config() {
        let yaml = r#"
tasks:
  tag_name: "todo"
  default_owner: "alice"
"#;
        let config: Config = yaml_serde::from_str(yaml).unwrap();
        assert_eq!(get("tasks.tag_name", &config).unwrap(), "todo");
        assert_eq!(get("tasks.default_owner", &config).unwrap(), "alice");
        // Defaults should still apply for unspecified fields.
        assert_eq!(
            get("tasks.default_worktime_units", &config).unwrap(),
            "hours"
        );
    }

    #[test]
    fn test_get_applies_value_provenance() {
        let config: Config =
            yaml_serde::from_str("tasks:\n  default_owner: environment-secret\n").unwrap();

        assert_eq!(
            run_get("tasks.default_owner", &config, |_| false).unwrap(),
            "environment-secret"
        );
        assert_eq!(
            run_get("tasks.default_owner", &config, |value| {
                value == "environment-secret"
            })
            .unwrap(),
            "<environment-derived>"
        );
    }

    #[test]
    fn test_get_aliases_uses_canonical_name_and_names_fields() {
        let mut config = default_config();
        config.aliases = vec![
            Alias {
                names: vec!["legacy".to_string()],
                arguments: vec!["summary".to_string()],
            },
            Alias {
                names: vec!["active".to_string(), "a".to_string()],
                arguments: vec!["query".to_string(), "two words".to_string()],
            },
        ];
        let value = get("aliases", &config).unwrap();
        assert!(value.contains("name: legacy"));
        assert!(value.contains(r#"names: ["active", "a"]"#));
        assert!(value.contains("arguments: summary"));
        assert!(value.contains("arguments: query 'two words'"));
    }

    #[test]
    fn dump_flat_has_sorted_paths_json_scalars_and_explicit_empty_collections() {
        let mut config = default_config();
        config.ignore_patterns = vec!["a\nb".to_string()];
        let output = run_dump(&config, DumpFormat::Flat, |_| false).unwrap();
        let lines = output.lines().collect::<Vec<_>>();
        let mut sorted = lines.clone();
        sorted.sort();
        assert_eq!(lines, sorted);
        assert!(output.contains("aliases = []\n"));
        assert!(output.contains("ignore_patterns[0] = \"a\\nb\"\n"));
        assert!(output.contains("tags.presets = []\n"));
        assert!(output.contains("tasks.default_owner = \"me\"\n"));
        assert!(output.ends_with('\n'));
        assert!(!output.ends_with("\n\n"));
    }

    #[test]
    fn dump_redacts_nested_core_preset_and_task_strings() {
        let config: Config = yaml_serde::from_str(
            "ignore_patterns: [secret]\ntags:\n  presets:\n    - nickname: secret\n      value: '@safe'\ntasks:\n  default_owner: secret\n",
        )
        .unwrap();
        let output = run_dump(&config, DumpFormat::Flat, |value| value == "secret").unwrap();
        assert_eq!(output.matches("\"<environment-derived>\"").count(), 3);
        assert!(!output.contains("\"secret\""));
    }

    #[test]
    fn dump_yaml_and_flat_represent_the_same_effective_tree() {
        let config = default_config();
        let yaml = run_dump(&config, DumpFormat::Yaml, |_| false).unwrap();
        let value: yaml_serde::Value = yaml_serde::from_str(&yaml).unwrap();
        assert_eq!(
            value["tags"]["presets"],
            yaml_serde::Value::Sequence(vec![])
        );
        assert_eq!(value["tasks"]["default_owner"].as_str(), Some("me"));
        assert!(yaml.ends_with('\n'));
        assert!(!yaml.ends_with("\n\n"));
    }

    #[test]
    fn dump_propagates_invalid_task_configuration() {
        let mut config = default_config();
        config.extension_configs.insert(
            TASKS_CONFIG_KEY.to_string(),
            yaml_serde::Value::String("invalid".to_string()),
        );
        assert!(run_dump(&config, DumpFormat::Flat, |_| false).is_err());
    }
}

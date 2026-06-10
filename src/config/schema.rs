//! Config schema definitions.
//!
//! Defines the `Config`, `OutputConfig`, and `ColorMode` types
//! used for YAML deserialization. All fields have defaults so that
//! an empty config file is valid.

use serde::Deserialize;
use std::collections::HashMap;

/// The maximum number of ignore patterns allowed.
const MAX_IGNORE_PATTERNS: usize = 256;

/// The maximum length of a single ignore pattern.
const MAX_PATTERN_LENGTH: usize = 1024;

/// The default maximum file size in bytes (10 MB).
const DEFAULT_MAX_FILE_SIZE: u64 = 10_485_760;

/// Color mode for output.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ColorMode {
    /// Auto-detect based on terminal capabilities.
    #[default]
    Auto,
    /// Always use colors.
    Always,
    /// Never use colors.
    Never,
}

impl<'de> Deserialize<'de> for ColorMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_lowercase().as_str() {
            "auto" => Ok(ColorMode::Auto),
            "always" => Ok(ColorMode::Always),
            "never" => Ok(ColorMode::Never),
            _ => Err(serde::de::Error::custom(format!(
                "invalid color mode \"{s}\" — expected \"auto\", \"always\", or \"never\""
            ))),
        }
    }
}

/// Output configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct OutputConfig {
    /// Color mode.
    pub color: ColorMode,
    /// Default attributes shown in `tasks list`.
    pub default_list_attributes: Vec<String>,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            color: ColorMode::Auto,
            default_list_attributes: vec![
                "id".to_string(),
                "status".to_string(),
                "title".to_string(),
                "description".to_string(),
            ],
        }
    }
}

/// The core ragtag configuration.
///
/// All fields have defaults, so a minimal or empty YAML file is valid.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Regex patterns for file paths to ignore.
    pub ignore_patterns: Vec<String>,
    /// Whether to respect .gitignore files.
    pub respect_gitignore: bool,
    /// Whether to skip hidden files and directories.
    pub skip_hidden: bool,
    /// Whether to skip binary files.
    pub skip_binary: bool,
    /// Maximum directory depth (None = unlimited).
    pub max_depth: Option<usize>,
    /// Maximum file size in bytes to scan.
    pub max_file_size: u64,
    /// Output configuration.
    pub output: OutputConfig,
    /// Extension configuration sections (raw YAML values).
    /// Keys are extension config keys (e.g., "tasks").
    #[serde(flatten)]
    pub extension_configs: HashMap<String, serde_yml::Value>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ignore_patterns: Vec::new(),
            respect_gitignore: true,
            skip_hidden: true,
            skip_binary: true,
            max_depth: None,
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            output: OutputConfig::default(),
            extension_configs: HashMap::new(),
        }
    }
}

/// The maximum allowed value for `max_file_size` (100 MB).
const MAX_ALLOWED_FILE_SIZE: u64 = 100 * 1024 * 1024;

impl Config {
    /// Validates the configuration values.
    ///
    /// Checks ignore pattern counts, lengths, and max_file_size bounds.
    pub fn validate(&self) -> Result<(), crate::error::RagtagError> {
        if self.ignore_patterns.len() > MAX_IGNORE_PATTERNS {
            return Err(crate::error::RagtagError::InvalidConfig(format!(
                "too many ignore patterns ({}) — maximum is {MAX_IGNORE_PATTERNS}",
                self.ignore_patterns.len()
            )));
        }
        for (i, pattern) in self.ignore_patterns.iter().enumerate() {
            if pattern.len() > MAX_PATTERN_LENGTH {
                return Err(crate::error::RagtagError::InvalidConfig(format!(
                    "ignore pattern #{} exceeds maximum length of {MAX_PATTERN_LENGTH} characters",
                    i + 1
                )));
            }
        }
        if self.max_file_size > MAX_ALLOWED_FILE_SIZE {
            return Err(crate::error::RagtagError::InvalidConfig(format!(
                "max_file_size {} exceeds maximum allowed value of {MAX_ALLOWED_FILE_SIZE}",
                self.max_file_size
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.ignore_patterns.is_empty());
        assert!(config.respect_gitignore);
        assert!(config.skip_hidden);
        assert!(config.skip_binary);
        assert_eq!(config.max_depth, None);
        assert_eq!(config.max_file_size, 10_485_760);
        assert_eq!(config.output.color, ColorMode::Auto);
        assert_eq!(config.output.default_list_attributes.len(), 4);
    }

    #[test]
    fn test_full_yaml_deserialization() {
        let yaml = r#"
ignore_patterns:
  - ".*\\.pdf$"
  - "target/"
respect_gitignore: false
skip_hidden: false
skip_binary: false
max_depth: 5
max_file_size: 1048576
output:
  color: "never"
  default_list_attributes:
    - "id"
    - "title"
"#;
        let config: Config = serde_yml::from_str(yaml).unwrap();
        assert_eq!(config.ignore_patterns.len(), 2);
        assert!(!config.respect_gitignore);
        assert!(!config.skip_hidden);
        assert!(!config.skip_binary);
        assert_eq!(config.max_depth, Some(5));
        assert_eq!(config.max_file_size, 1_048_576);
        assert_eq!(config.output.color, ColorMode::Never);
        assert_eq!(config.output.default_list_attributes.len(), 2);
    }

    #[test]
    fn test_empty_yaml() {
        let config: Config = serde_yml::from_str("{}").unwrap();
        assert!(config.respect_gitignore);
    }

    #[test]
    fn test_partial_yaml() {
        let yaml = r#"
skip_hidden: false
"#;
        let config: Config = serde_yml::from_str(yaml).unwrap();
        assert!(!config.skip_hidden);
        assert!(config.respect_gitignore); // default
    }

    #[test]
    fn test_color_mode_auto() {
        let yaml = r#"output: { color: "auto" }"#;
        let config: Config = serde_yml::from_str(yaml).unwrap();
        assert_eq!(config.output.color, ColorMode::Auto);
    }

    #[test]
    fn test_color_mode_always() {
        let yaml = r#"output: { color: "always" }"#;
        let config: Config = serde_yml::from_str(yaml).unwrap();
        assert_eq!(config.output.color, ColorMode::Always);
    }

    #[test]
    fn test_color_mode_invalid() {
        let yaml = r#"output: { color: "rainbow" }"#;
        let result: Result<Config, _> = serde_yml::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_extension_configs_captured() {
        let yaml = r#"
tasks:
  tag_name: "todo"
  default_owner: "alice"
"#;
        let config: Config = serde_yml::from_str(yaml).unwrap();
        assert!(config.extension_configs.contains_key("tasks"));
    }

    #[test]
    fn test_validate_too_many_patterns() {
        let mut config = Config::default();
        config.ignore_patterns = vec![".*".to_string(); 300];
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_pattern_too_long() {
        let mut config = Config::default();
        config.ignore_patterns = vec!["x".repeat(2000)];
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_ok() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_max_file_size_too_large() {
        let mut config = Config::default();
        config.max_file_size = 200 * 1024 * 1024; // 200 MB, exceeds 100 MB limit
        assert!(config.validate().is_err());
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("max_file_size"));
    }
}

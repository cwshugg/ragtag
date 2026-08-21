//! Config schema definitions.
//!
//! Defines the `Config`, `OutputConfig`, and `ColorMode` types
//! used for YAML deserialization. All fields have defaults so that
//! an empty config file is valid.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;

/// The maximum number of ignore patterns allowed.
const MAX_IGNORE_PATTERNS: usize = 256;

/// The maximum number of aliases allowed.
const MAX_ALIASES: usize = 256;

/// The maximum length of a single ignore pattern.
const MAX_PATTERN_LENGTH: usize = 1024;

/// The default maximum file size in bytes (10 MB).
const DEFAULT_MAX_FILE_SIZE: u64 = 10_485_760;

/// Default directory for files created by `file touch`.
const DEFAULT_FILE_DIRECTORY: &str = ".";

/// Default UTC strftime pattern for files created by `file touch`.
const DEFAULT_FILENAME_FORMAT: &str = "%Y-%m-%d_%H-%M-%S.md";

/// Color mode for output.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
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

/// A user-defined command alias.
///
/// Running `ragtag <name>` expands to the alias's `arguments` and executes
/// the result as if typed directly.
///
/// In the YAML config, `arguments` is written as a single shell-like string
/// (e.g., `arguments: "task summary"`). It is split into individual tokens
/// with shell-like quoting semantics (via the `shlex` crate) at load time and
/// stored as a `Vec<String>`. When serialized, the tokens are joined back into
/// a single shell-quoted string so the external YAML shape is preserved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias {
    /// The alias name, invoked as `ragtag <name>`.
    pub name: String,
    /// The tokens the alias expands to (e.g., `["task", "summary"]`).
    pub arguments: Vec<String>,
}

impl<'de> Deserialize<'de> for Alias {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        /// The on-disk shape of an alias: `arguments` is a single string.
        #[derive(Deserialize)]
        struct RawAlias {
            name: String,
            arguments: String,
        }

        let raw = RawAlias::deserialize(deserializer)?;
        let arguments = shlex::split(&raw.arguments).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "alias \"{}\" has an invalid arguments string: {:?}",
                raw.name, raw.arguments
            ))
        })?;
        if arguments.is_empty() {
            return Err(serde::de::Error::custom(format!(
                "alias \"{}\" has empty arguments",
                raw.name
            )));
        }
        Ok(Alias {
            name: raw.name,
            arguments,
        })
    }
}

impl Serialize for Alias {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        // Join the tokens back into a single shell-quoted string so the
        // serialized form matches the documented single-string config shape.
        let joined = shlex::try_join(self.arguments.iter().map(String::as_str))
            .map_err(serde::ser::Error::custom)?;
        let mut state = serializer.serialize_struct("Alias", 2)?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("arguments", &joined)?;
        state.end()
    }
}

impl fmt::Display for Alias {
    /// Renders the alias's expansion as a space-joined command string, for
    /// use in help text.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.arguments.join(" "))
    }
}

/// Output configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct OutputConfig {
    /// Color mode.
    pub color: ColorMode,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            color: ColorMode::Auto,
        }
    }
}

/// Configuration for files created by the built-in file command.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct FileConfig {
    /// Directory used when `file touch` has no explicit path.
    pub default_directory: PathBuf,
    /// Chrono strftime pattern used to generate the default filename.
    pub filename_format: String,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            default_directory: PathBuf::from(DEFAULT_FILE_DIRECTORY),
            filename_format: DEFAULT_FILENAME_FORMAT.to_string(),
        }
    }
}

impl FileConfig {
    /// Validates static file-creation configuration.
    pub fn validate(&self) -> Result<(), crate::error::RagtagError> {
        if self.default_directory.as_os_str().is_empty() {
            return Err(crate::error::RagtagError::InvalidConfig(
                "files.default_directory must not be empty".to_string(),
            ));
        }
        if self.filename_format.is_empty() {
            return Err(crate::error::RagtagError::InvalidConfig(
                "files.filename_format must not be empty".to_string(),
            ));
        }
        if chrono::format::StrftimeItems::new(&self.filename_format)
            .any(|item| matches!(item, chrono::format::Item::Error))
        {
            return Err(crate::error::RagtagError::InvalidConfig(format!(
                "files.filename_format contains invalid strftime syntax: {:?}",
                self.filename_format
            )));
        }
        Ok(())
    }
}

/// The core ragtag configuration.
///
/// All fields have defaults, so a minimal or empty YAML file is valid.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    /// Regex patterns for file paths to ignore.
    pub ignore_patterns: Vec<String>,
    /// Whether to respect .gitignore files.
    pub respect_gitignore: bool,
    /// Whether to skip hidden files and directories.
    pub skip_hidden: bool,
    /// Maximum directory depth (None = unlimited).
    pub max_depth: Option<usize>,
    /// Maximum file size in bytes to scan.
    pub max_file_size: u64,
    /// Output configuration.
    pub output: OutputConfig,
    /// Configuration for files created by the built-in file command.
    pub files: FileConfig,
    /// User-defined command aliases. Empty by default (no default aliases).
    pub aliases: Vec<Alias>,
    /// Extension configuration sections (raw YAML values).
    /// Keys are extension config section names in the YAML file (e.g., "tasks" for the task extension).
    #[serde(flatten)]
    pub extension_configs: HashMap<String, serde_yml::Value>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ignore_patterns: Vec::new(),
            respect_gitignore: true,
            skip_hidden: true,
            max_depth: None,
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            output: OutputConfig::default(),
            files: FileConfig::default(),
            aliases: Vec::new(),
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
        self.files.validate()?;
        Ok(())
    }

    /// Validates the alias list against the set of real command names.
    ///
    /// `real_command_names` must contain every built-in command name
    /// (e.g., `summary`, `query`, `config`) and every extension command
    /// name (e.g., `task`). This is run at startup, before any command
    /// executes, so collisions are caught early.
    ///
    /// Errors on:
    /// - more aliases than `MAX_ALIASES`,
    /// - an empty alias name,
    /// - an alias name that collides with a real command name,
    /// - a duplicate alias name,
    /// - an alias whose `arguments` expand to no tokens.
    pub fn validate_aliases(
        &self,
        real_command_names: &HashSet<String>,
    ) -> Result<(), crate::error::RagtagError> {
        if self.aliases.len() > MAX_ALIASES {
            return Err(crate::error::RagtagError::InvalidConfig(format!(
                "too many aliases ({}) — maximum is {MAX_ALIASES}",
                self.aliases.len()
            )));
        }
        let mut seen: HashSet<&str> = HashSet::new();
        for alias in &self.aliases {
            if alias.name.is_empty() {
                return Err(crate::error::RagtagError::InvalidConfig(
                    "alias name must not be empty".to_string(),
                ));
            }
            if real_command_names.contains(&alias.name) {
                return Err(crate::error::RagtagError::InvalidConfig(format!(
                    "alias \"{}\" collides with an existing command name",
                    alias.name
                )));
            }
            if !seen.insert(alias.name.as_str()) {
                return Err(crate::error::RagtagError::InvalidConfig(format!(
                    "duplicate alias name \"{}\"",
                    alias.name
                )));
            }
            // Ensure the alias actually expands to a command. A parseable
            // but empty token list (e.g., a whitespace-only arguments string)
            // is rejected here as well as at deserialization time.
            if alias.arguments.is_empty() {
                return Err(crate::error::RagtagError::InvalidConfig(format!(
                    "alias \"{}\" has empty arguments",
                    alias.name
                )));
            }
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
        assert_eq!(config.max_depth, None);
        assert_eq!(config.max_file_size, 10_485_760);
        assert_eq!(config.output.color, ColorMode::Auto);
        assert_eq!(config.files, FileConfig::default());
    }

    #[test]
    fn test_full_yaml_deserialization() {
        let yaml = r#"
ignore_patterns:
  - ".*\\.pdf$"
  - "target/"
respect_gitignore: false
skip_hidden: false
max_depth: 5
max_file_size: 1048576
output:
  color: "never"
files:
  default_directory: "notes"
  filename_format: "%Y%m%d-%3f.txt"
"#;
        let config: Config = serde_yml::from_str(yaml).unwrap();
        assert_eq!(config.ignore_patterns.len(), 2);
        assert!(!config.respect_gitignore);
        assert!(!config.skip_hidden);
        assert_eq!(config.max_depth, Some(5));
        assert_eq!(config.max_file_size, 1_048_576);
        assert_eq!(config.output.color, ColorMode::Never);
        assert_eq!(config.files.default_directory, PathBuf::from("notes"));
        assert_eq!(config.files.filename_format, "%Y%m%d-%3f.txt");
        assert!(!config.extension_configs.contains_key("files"));
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
        let config = Config {
            ignore_patterns: vec![".*".to_string(); 300],
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_pattern_too_long() {
        let config = Config {
            ignore_patterns: vec!["x".repeat(2000)],
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_ok() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_file_config_validation() {
        let mut config = Config::default();
        config.files.default_directory = PathBuf::new();
        assert!(config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("default_directory"));

        config.files.default_directory = PathBuf::from(".");
        config.files.filename_format.clear();
        assert!(config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("filename_format"));

        config.files.filename_format = "%".to_string();
        assert!(config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("strftime"));

        config.files.filename_format = "%Y-%m-%d_%H-%M-%S-%3f.md".to_string();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_max_file_size_too_large() {
        let config = Config {
            max_file_size: 200 * 1024 * 1024, // 200 MB, exceeds 100 MB limit
            ..Default::default()
        };
        assert!(config.validate().is_err());
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("max_file_size"));
    }

    // === Aliases ===

    /// A set of "real" command names for alias-collision testing.
    fn real_commands() -> HashSet<String> {
        ["config", "summary", "query", "file", "task"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn test_aliases_parse_from_yaml() {
        let yaml = r#"
aliases:
  - name: "my-alias"
    arguments: "task summary"
  - name: "active"
    arguments: "query task --filter status=active"
"#;
        let config: Config = serde_yml::from_str(yaml).unwrap();
        assert_eq!(config.aliases.len(), 2);
        assert_eq!(config.aliases[0].name, "my-alias");
        assert_eq!(config.aliases[0].arguments, vec!["task", "summary"]);
        assert_eq!(config.aliases[1].name, "active");
        assert_eq!(
            config.aliases[1].arguments,
            vec!["query", "task", "--filter", "status=active"]
        );
        // The `aliases` field must be a real field, not swallowed by the
        // flattened `extension_configs` map.
        assert!(!config.extension_configs.contains_key("aliases"));
        config.validate_aliases(&real_commands()).unwrap();
    }

    #[test]
    fn test_aliases_absent_is_ok() {
        let yaml = "skip_hidden: false\n";
        let config: Config = serde_yml::from_str(yaml).unwrap();
        assert!(config.aliases.is_empty());
        config.validate_aliases(&real_commands()).unwrap();
    }

    #[test]
    fn test_aliases_empty_list_is_ok() {
        let yaml = "aliases: []\n";
        let config: Config = serde_yml::from_str(yaml).unwrap();
        assert!(config.aliases.is_empty());
        config.validate_aliases(&real_commands()).unwrap();
    }

    #[test]
    fn test_aliases_coexist_with_extension_configs() {
        let yaml = r#"
aliases:
  - name: "my-alias"
    arguments: "task summary"
tasks:
  tag_name: "todo"
"#;
        let config: Config = serde_yml::from_str(yaml).unwrap();
        assert_eq!(config.aliases.len(), 1);
        assert!(config.extension_configs.contains_key("tasks"));
        assert!(!config.extension_configs.contains_key("aliases"));
    }

    #[test]
    fn test_aliases_duplicate_name_errors() {
        let config = Config {
            aliases: vec![
                Alias {
                    name: "dup".to_string(),
                    arguments: vec!["summary".to_string()],
                },
                Alias {
                    name: "dup".to_string(),
                    arguments: vec!["query".to_string()],
                },
            ],
            ..Default::default()
        };
        let err = config.validate_aliases(&real_commands()).unwrap_err();
        assert!(err.to_string().contains("duplicate alias name"));
    }

    #[test]
    fn test_aliases_empty_name_errors() {
        let config = Config {
            aliases: vec![Alias {
                name: String::new(),
                arguments: vec!["summary".to_string()],
            }],
            ..Default::default()
        };
        let err = config.validate_aliases(&real_commands()).unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn test_aliases_collision_with_builtin_errors() {
        let config = Config {
            aliases: vec![Alias {
                name: "summary".to_string(),
                arguments: vec!["query".to_string()],
            }],
            ..Default::default()
        };
        let err = config.validate_aliases(&real_commands()).unwrap_err();
        assert!(err.to_string().contains("collides"));
        assert!(err.to_string().contains("summary"));
    }

    #[test]
    fn test_aliases_collision_with_extension_command_errors() {
        let config = Config {
            aliases: vec![Alias {
                name: "task".to_string(),
                arguments: vec!["summary".to_string()],
            }],
            ..Default::default()
        };
        let err = config.validate_aliases(&real_commands()).unwrap_err();
        assert!(err.to_string().contains("collides"));
        assert!(err.to_string().contains("task"));
    }

    #[test]
    fn test_aliases_empty_arguments_errors() {
        let config = Config {
            aliases: vec![Alias {
                name: "blank".to_string(),
                arguments: Vec::new(),
            }],
            ..Default::default()
        };
        let err = config.validate_aliases(&real_commands()).unwrap_err();
        assert!(err.to_string().contains("empty arguments"));
    }

    #[test]
    fn test_aliases_valid_passes() {
        let config = Config {
            aliases: vec![Alias {
                name: "my-alias".to_string(),
                arguments: vec!["task".to_string(), "summary".to_string()],
            }],
            ..Default::default()
        };
        config.validate_aliases(&real_commands()).unwrap();
    }

    #[test]
    fn test_aliases_too_many_errors() {
        let aliases = (0..=MAX_ALIASES)
            .map(|i| Alias {
                name: format!("alias{i}"),
                arguments: vec!["summary".to_string()],
            })
            .collect();
        let config = Config {
            aliases,
            ..Default::default()
        };
        let err = config.validate_aliases(&real_commands()).unwrap_err();
        assert!(err.to_string().contains("too many aliases"));
    }

    #[test]
    fn test_aliases_at_cap_passes() {
        let aliases = (0..MAX_ALIASES)
            .map(|i| Alias {
                name: format!("alias{i}"),
                arguments: vec!["summary".to_string()],
            })
            .collect();
        let config = Config {
            aliases,
            ..Default::default()
        };
        config.validate_aliases(&real_commands()).unwrap();
    }

    // === Argument parsing (shell-like quoting at load time) ===

    /// Deserializes a single alias from a `name`/`arguments` YAML mapping.
    fn alias_from_yaml(name: &str, arguments: &str) -> Result<Alias, serde_yml::Error> {
        let yaml = format!("name: {name:?}\narguments: {arguments:?}\n");
        serde_yml::from_str(&yaml)
    }

    #[test]
    fn test_arguments_parse_simple() {
        let alias = alias_from_yaml("a", "task summary").unwrap();
        assert_eq!(alias.arguments, vec!["task", "summary"]);
    }

    #[test]
    fn test_arguments_parse_quoted_segment() {
        let alias = alias_from_yaml("a", r#"task get "two words""#).unwrap();
        assert_eq!(alias.arguments, vec!["task", "get", "two words"]);
    }

    #[test]
    fn test_arguments_parse_single_quotes() {
        let alias = alias_from_yaml("a", "query --filter 'status=active'").unwrap();
        assert_eq!(alias.arguments, vec!["query", "--filter", "status=active"]);
    }

    #[test]
    fn test_arguments_parse_collapses_whitespace() {
        let alias = alias_from_yaml("a", "task    summary").unwrap();
        assert_eq!(alias.arguments, vec!["task", "summary"]);
    }

    #[test]
    fn test_arguments_parse_unterminated_quote_errors() {
        assert!(alias_from_yaml("a", r#"task get "unterminated"#).is_err());
    }

    #[test]
    fn test_arguments_parse_whitespace_only_errors() {
        let err = alias_from_yaml("a", "   ").unwrap_err();
        assert!(err.to_string().contains("empty arguments"));
    }

    #[test]
    fn test_alias_round_trip_serialization() {
        let config = Config {
            aliases: vec![
                Alias {
                    name: "my-alias".to_string(),
                    arguments: vec!["task".to_string(), "summary".to_string()],
                },
                Alias {
                    name: "spaced".to_string(),
                    arguments: vec![
                        "query".to_string(),
                        "--filter".to_string(),
                        "status = active".to_string(),
                    ],
                },
            ],
            ..Default::default()
        };
        let yaml = serde_yml::to_string(&config).unwrap();
        let restored: Config = serde_yml::from_str(&yaml).unwrap();
        assert_eq!(restored.aliases, config.aliases);
    }
}

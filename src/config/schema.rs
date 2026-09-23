//! Config schema definitions.
//!
//! Defines the `Config`, `OutputConfig`, and `ColorMode` types
//! used for YAML deserialization. All fields have defaults so that
//! an empty config file is valid.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// The maximum number of ignore patterns allowed.
const MAX_IGNORE_PATTERNS: usize = 256;

/// The maximum number of alias definitions allowed.
const MAX_ALIASES: usize = 256;

/// The maximum aggregate number of configured alias names.
const MAX_ALIAS_NAMES: usize = 256;

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
/// the result as if typed directly. A definition can have one `name` or an
/// ordered, nonempty `names` sequence of peer invocation names. If expansion
/// token zero exactly names another alias, composition continues recursively.
///
/// In YAML, `arguments` is one shell-like template string. It is split into
/// tokens without expanding environment references. Each token is interpolated
/// independently at invocation time. Serialization joins the tokens back into
/// one canonical shell-quoted string.
#[derive(Clone, PartialEq, Eq)]
pub struct Alias {
    /// The ordered peer names that invoke this alias definition.
    pub names: Vec<String>,
    /// The deferred alias argument tokens.
    pub arguments: Vec<String>,
}

impl std::fmt::Debug for Alias {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Alias")
            .field("name_count", &self.names.len())
            .field("argument_count", &self.arguments.len())
            .finish()
    }
}

impl<'de> Deserialize<'de> for Alias {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        /// The on-disk shape of an alias.
        #[derive(Deserialize)]
        struct RawAlias {
            name: Option<String>,
            names: Option<Vec<String>>,
            arguments: String,
        }

        let raw = RawAlias::deserialize(deserializer)?;
        let names = match (raw.name, raw.names) {
            (Some(name), None) => vec![name],
            (None, Some(names)) if !names.is_empty() => names,
            (Some(_), Some(_)) => {
                return Err(serde::de::Error::custom(
                    "alias must specify exactly one of \"name\" or \"names\", not both",
                ));
            }
            (None, None) => {
                return Err(serde::de::Error::custom(
                    "alias must specify exactly one of \"name\" or \"names\"",
                ));
            }
            (None, Some(_)) => {
                return Err(serde::de::Error::custom(
                    "alias \"names\" must contain at least one name",
                ));
            }
        };
        let arguments = shlex::split(&raw.arguments).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "alias \"{}\" has an invalid arguments string",
                names[0]
            ))
        })?;
        if arguments.is_empty() {
            return Err(serde::de::Error::custom(format!(
                "alias \"{}\" has empty arguments",
                names[0]
            )));
        }
        Ok(Alias { names, arguments })
    }
}

impl Serialize for Alias {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        let joined = shlex::try_join(self.arguments.iter().map(String::as_str))
            .map_err(serde::ser::Error::custom)?;
        let mut state = serializer.serialize_struct("Alias", 2)?;
        if self.names.len() == 1 {
            state.serialize_field("name", &self.names[0])?;
        } else {
            state.serialize_field("names", &self.names)?;
        }
        state.serialize_field("arguments", &joined)?;
        state.end()
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
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct FileConfig {
    /// Directory used when `file touch` has no explicit path.
    pub default_directory: PathBuf,
    /// Chrono strftime pattern used to generate the default filename.
    pub filename_format: String,
}

impl std::fmt::Debug for FileConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FileConfig")
            .field("default_directory", &"<redacted>")
            .field("filename_format", &"<redacted>")
            .finish()
    }
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
#[derive(Clone, Deserialize, Serialize)]
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

impl std::fmt::Debug for Config {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Config")
            .field("ignore_patterns", &"<redacted>")
            .field("respect_gitignore", &self.respect_gitignore)
            .field("skip_hidden", &self.skip_hidden)
            .field("max_depth", &self.max_depth)
            .field("max_file_size", &self.max_file_size)
            .field("output", &"<redacted>")
            .field("files", &"<redacted>")
            .field("aliases", &"<redacted>")
            .field("extension_configs", &"<redacted>")
            .finish()
    }
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
    /// `real_command_names` must contain the fully built clap command universe:
    /// every declared built-in and extension command plus clap's generated
    /// `help` subcommand. [`crate::cli::real_command_names`] is the sanctioned
    /// producer. This is run at startup, before any command executes, so
    /// collisions are caught early.
    ///
    /// Errors on:
    /// - more aliases than `MAX_ALIASES`,
    /// - more aggregate names than `MAX_ALIAS_NAMES`,
    /// - an empty names vector or individual name,
    /// - any name that collides with a real command name,
    /// - any duplicate name within or across definitions,
    /// - an alias whose `arguments` token list is empty.
    pub fn validate_aliases(
        &self,
        real_command_names: &[String],
    ) -> Result<(), crate::error::RagtagError> {
        if self.aliases.len() > MAX_ALIASES {
            return Err(crate::error::RagtagError::InvalidConfig(format!(
                "too many aliases ({}) — maximum is {MAX_ALIASES}",
                self.aliases.len()
            )));
        }
        let real_names: HashSet<&str> = real_command_names.iter().map(String::as_str).collect();
        let total_names = self.aliases.iter().try_fold(0usize, |count, alias| {
            count.checked_add(alias.names.len()).ok_or_else(|| {
                crate::error::RagtagError::InvalidConfig(format!(
                    "too many alias names — maximum is {MAX_ALIAS_NAMES}"
                ))
            })
        })?;
        if total_names > MAX_ALIAS_NAMES {
            return Err(crate::error::RagtagError::InvalidConfig(format!(
                "too many alias names ({total_names}) — maximum is {MAX_ALIAS_NAMES}"
            )));
        }

        let mut seen: HashSet<&str> = HashSet::new();
        for alias in &self.aliases {
            if alias.names.is_empty() {
                return Err(crate::error::RagtagError::InvalidConfig(
                    "alias must contain at least one name".to_string(),
                ));
            }
            for name in &alias.names {
                if name.is_empty() {
                    return Err(crate::error::RagtagError::InvalidConfig(
                        "alias name must not be empty".to_string(),
                    ));
                }
                if real_names.contains(name.as_str()) {
                    return Err(crate::error::RagtagError::InvalidConfig(format!(
                        "alias \"{name}\" collides with an existing command name"
                    )));
                }
                if !seen.insert(name) {
                    return Err(crate::error::RagtagError::InvalidConfig(format!(
                        "duplicate alias name \"{name}\""
                    )));
                }
            }
            if alias.arguments.is_empty() {
                return Err(crate::error::RagtagError::InvalidConfig(format!(
                    "alias \"{}\" has empty arguments",
                    alias.names[0]
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

    /// Ordered real command names for alias-collision testing.
    fn real_commands() -> Vec<String> {
        ["config", "summary", "query", "file", "task"]
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    /// Creates a runtime alias definition for validation tests.
    fn alias(names: &[&str], arguments: &[&str]) -> Alias {
        Alias {
            names: names.iter().map(|name| (*name).to_string()).collect(),
            arguments: arguments
                .iter()
                .map(|argument| (*argument).to_string())
                .collect(),
        }
    }

    #[test]
    fn aliases_accept_legacy_and_ordered_multi_name_forms() {
        let config: Config = serde_yml::from_str(
            "aliases:\n  - name: legacy\n    arguments: \"task summary\"\n  - names: [active, a]\n    arguments: \"query task --filter 'status=active'\"\n",
        )
        .unwrap();
        assert_eq!(config.aliases[0].names, ["legacy"]);
        assert_eq!(config.aliases[1].names, ["active", "a"]);
        assert_eq!(
            config.aliases[1].arguments,
            ["query", "task", "--filter", "status=active"]
        );
        assert!(!config.extension_configs.contains_key("aliases"));
        config.validate_aliases(&real_commands()).unwrap();
    }

    #[test]
    fn aliases_ignore_unknown_metadata_fields() {
        let alias: Alias = serde_yml::from_str(
            "name: legacy\narguments: summary\ndescription: handy\nfuture_metadata:\n  category: reporting\n",
        )
        .unwrap();
        assert_eq!(alias.names, ["legacy"]);
        assert_eq!(alias.arguments, ["summary"]);

        let canonical = serde_yml::to_string(&alias).unwrap();
        assert!(!canonical.contains("description"));
        assert!(!canonical.contains("future_metadata"));
    }

    #[test]
    fn aliases_serialize_canonical_name_shapes_and_round_trip_quoting() {
        let config = Config {
            aliases: vec![
                alias(&["one"], &["task", "summary"]),
                alias(&["many", "m"], &["query", "two words"]),
            ],
            ..Default::default()
        };
        let yaml = serde_yml::to_string(&config).unwrap();
        assert!(yaml.contains("- name: one"));
        assert!(yaml.contains("- names:"));
        assert!(yaml.contains("  - many"));
        assert!(yaml.contains("  - m"));
        let restored: Config = serde_yml::from_str(&yaml).unwrap();
        assert_eq!(restored.aliases, config.aliases);

        let one_names: Alias =
            serde_yml::from_str("names: [single]\narguments: summary\n").unwrap();
        let canonical = serde_yml::to_string(&one_names).unwrap();
        assert!(canonical.contains("name: single"));
        assert!(!canonical.contains("names:"));
    }

    #[test]
    fn aliases_reject_invalid_naming_shapes_and_arguments() {
        for yaml in [
            "name: a\nnames: [b]\narguments: summary\n",
            "arguments: summary\n",
            "names: []\narguments: summary\n",
            "name: [a]\narguments: summary\n",
            "names: a\narguments: summary\n",
            "name: a\narguments: \"   \"\n",
            "name: a\narguments: 'query \"unterminated'\n",
        ] {
            assert!(serde_yml::from_str::<Alias>(yaml).is_err(), "{yaml}");
        }

        let deferred: Alias =
            serde_yml::from_str("name: a\narguments: 'query \"$RUNTIME\"'\n").unwrap();
        assert_eq!(deferred.arguments, ["query", "$RUNTIME"]);
    }

    #[test]
    fn alias_validation_checks_every_name_and_runtime_invariant() {
        for (aliases, expected) in [
            (vec![alias(&["dup", "dup"], &["summary"])], "duplicate"),
            (
                vec![
                    alias(&["first", "shared"], &["summary"]),
                    alias(&["shared"], &["query"]),
                ],
                "duplicate",
            ),
            (vec![alias(&["ok", "task"], &["summary"])], "collides"),
            (vec![alias(&[""], &["summary"])], "empty"),
            (vec![alias(&["blank"], &[])], "empty arguments"),
            (vec![alias(&[], &["summary"])], "at least one"),
        ] {
            let config = Config {
                aliases,
                ..Default::default()
            };
            assert!(
                config
                    .validate_aliases(&real_commands())
                    .unwrap_err()
                    .to_string()
                    .contains(expected),
                "{expected}"
            );
        }
    }

    #[test]
    fn alias_definition_and_name_caps_accept_256_and_reject_257() {
        let at_cap = (0..MAX_ALIASES)
            .map(|index| alias(&[&format!("a{index}")], &["summary"]))
            .collect::<Vec<_>>();
        Config {
            aliases: at_cap,
            ..Default::default()
        }
        .validate_aliases(&real_commands())
        .unwrap();

        let too_many_definitions = (0..=MAX_ALIASES)
            .map(|index| alias(&[&format!("a{index}")], &["summary"]))
            .collect();
        let error = Config {
            aliases: too_many_definitions,
            ..Default::default()
        }
        .validate_aliases(&real_commands())
        .unwrap_err();
        assert!(error.to_string().contains("too many aliases"));

        let names = (0..=MAX_ALIAS_NAMES)
            .map(|index| format!("n{index}"))
            .collect();
        let error = Config {
            aliases: vec![Alias {
                names,
                arguments: vec!["summary".to_string()],
            }],
            ..Default::default()
        }
        .validate_aliases(&real_commands())
        .unwrap_err();
        assert!(error.to_string().contains("too many alias names"));
    }
}

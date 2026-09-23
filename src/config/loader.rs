//! Config file discovery and loading.
//!
//! Implements walk-up discovery from the current directory, stopping at
//! `.git` boundaries or the filesystem root.

use std::io::Read;
use std::path::{Path, PathBuf};

use super::interpolation::{interpolate_config_value_with, InterpolationProvenance};
use super::schema::Config;
use crate::error::RagtagError;

/// Config file names to search for, in order of preference at each directory level.
///
/// A dotfile takes precedence over a non-dotfile, and within the same base name
/// the `.yaml` extension takes precedence over `.yml`.
const CONFIG_FILE_NAMES: &[&str] = &[".ragtag.yaml", ".ragtag.yml", "ragtag.yaml", "ragtag.yml"];

/// Maximum number of bytes accepted from one configuration file.
pub const MAX_CONFIG_FILE_SIZE: u64 = 1024 * 1024;

/// Failure while parsing raw or environment-expanded configuration.
#[derive(Debug)]
enum ConfigParseFailure {
    Raw(serde_yml::Error),
    EnvironmentDerived,
}

/// Parses, interpolates, and deserializes config with an injected lookup.
fn parse_config_with<F>(
    content: &str,
    lookup: &mut F,
) -> Result<(Config, InterpolationProvenance), ConfigParseFailure>
where
    F: FnMut(&str) -> Option<String>,
{
    let mut value: serde_yml::Value =
        serde_yml::from_str(content).map_err(ConfigParseFailure::Raw)?;
    let provenance = interpolate_config_value_with(&mut value, lookup);
    match serde_yml::from_value(value) {
        Ok(config) => Ok((config, provenance)),
        Err(source) if provenance.is_empty() => Err(ConfigParseFailure::Raw(source)),
        Err(_) => Err(ConfigParseFailure::EnvironmentDerived),
    }
}

/// Parses config using the current process environment.
fn parse_config(content: &str) -> Result<(Config, InterpolationProvenance), ConfigParseFailure> {
    parse_config_with(content, &mut |name| std::env::var(name).ok())
}

/// A validated configuration together with its lexical ragtag root.
#[derive(Clone)]
pub struct LoadedConfig {
    /// Parsed and validated application configuration.
    pub config: Config,
    /// Parent of the selected config file, or startup cwd when none exists.
    pub root_dir: PathBuf,
    /// Resolved selected config path, or `None` when defaults are in use.
    pub source_path: Option<PathBuf>,
    /// Environment-derived config values retained only for safe output handling.
    interpolation: InterpolationProvenance,
}

impl std::fmt::Debug for LoadedConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LoadedConfig")
            .field("config", &"<configuration values redacted>")
            .field("root_dir", &self.root_dir)
            .field("source_path", &self.source_path)
            .field(
                "has_environment_interpolation",
                &self.has_environment_interpolation(),
            )
            .finish()
    }
}

impl LoadedConfig {
    /// Returns whether configuration values came from environment references.
    pub fn has_environment_interpolation(&self) -> bool {
        !self.interpolation.is_empty()
    }

    /// Returns whether a configuration value came from environment interpolation.
    pub fn is_environment_derived_value(&self, value: &str) -> bool {
        self.interpolation.contains(value)
    }
}

/// Reads one regular config file without permitting unbounded allocation.
fn read_config_file(path: &Path) -> Result<String, RagtagError> {
    let metadata = std::fs::metadata(path).map_err(|source| RagtagError::FileRead {
        path: path.to_path_buf(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(RagtagError::InvalidConfig(format!(
            "config file \"{}\" must be a regular file",
            path.display()
        )));
    }
    if metadata.len() > MAX_CONFIG_FILE_SIZE {
        return Err(RagtagError::InvalidConfig(format!(
            "config file \"{}\" exceeds maximum size of {MAX_CONFIG_FILE_SIZE} bytes",
            path.display()
        )));
    }

    let file = std::fs::File::open(path).map_err(|source| RagtagError::FileRead {
        path: path.to_path_buf(),
        source,
    })?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_CONFIG_FILE_SIZE + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| RagtagError::FileRead {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() as u64 > MAX_CONFIG_FILE_SIZE {
        return Err(RagtagError::InvalidConfig(format!(
            "config file \"{}\" exceeds maximum size of {MAX_CONFIG_FILE_SIZE} bytes",
            path.display()
        )));
    }
    String::from_utf8(bytes).map_err(|source| RagtagError::FileRead {
        path: path.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, source),
    })
}

/// Loads a ragtag configuration.
///
/// If `cli_path` is provided, loads from that explicit path. Otherwise,
/// walks up from `start_dir` looking for a config file.
pub fn load_config(cli_path: Option<&Path>, start_dir: &Path) -> Result<LoadedConfig, RagtagError> {
    let config_path = match cli_path {
        Some(path) => {
            let resolved = if path.is_absolute() {
                path.to_path_buf()
            } else {
                start_dir.join(path)
            };
            if !resolved.exists() {
                return Err(RagtagError::ConfigNotFound(resolved));
            }
            Some(resolved)
        }
        None => discover_config_file(start_dir),
    };

    match config_path {
        Some(path) => {
            log::info!("loaded config from {}", path.display());
            let content = read_config_file(&path)?;
            let (config, interpolation) = match parse_config(&content) {
                Ok(parsed) => parsed,
                Err(ConfigParseFailure::Raw(source)) => {
                    return Err(RagtagError::ConfigParse {
                        path: path.clone(),
                        source: Box::new(source),
                    });
                }
                Err(ConfigParseFailure::EnvironmentDerived) => {
                    return Err(RagtagError::EnvironmentDerivedConfig);
                }
            };
            if let Err(error) = config.validate() {
                if interpolation.is_empty() {
                    return Err(error);
                }
                return Err(RagtagError::EnvironmentDerivedConfig);
            }
            let root_dir = path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| start_dir.to_path_buf());
            Ok(LoadedConfig {
                config,
                root_dir,
                source_path: Some(path),
                interpolation,
            })
        }
        None => Ok(LoadedConfig {
            config: Config::default(),
            root_dir: start_dir.to_path_buf(),
            source_path: None,
            interpolation: InterpolationProvenance::default(),
        }),
    }
}

/// Discovers a config file by walking up from `start_dir`.
///
/// At each directory level, checks the names in [`CONFIG_FILE_NAMES`] in order.
/// Stops at a directory containing `.git` or at the filesystem root.
pub fn discover_config_file(start_dir: &Path) -> Option<PathBuf> {
    let mut current = start_dir.to_path_buf();

    loop {
        // Check for config files at this level
        for name in CONFIG_FILE_NAMES {
            let candidate = current.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }

        // Stop at .git boundary (use symlink_metadata to avoid following dangling symlinks)
        if current.join(".git").symlink_metadata().is_ok() {
            return None;
        }

        // Move to parent
        match current.parent() {
            Some(parent) if parent != current => {
                current = parent.to_path_buf();
            }
            _ => return None, // Reached filesystem root
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;

    #[test]
    fn test_load_default_when_no_config() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load_config(None, dir.path()).unwrap();
        assert!(loaded.config.respect_gitignore);
        assert_eq!(loaded.root_dir, dir.path());
        assert_eq!(loaded.source_path, None);
    }

    #[test]
    fn test_load_explicit_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join(".ragtag.yaml");
        fs::write(&config_path, "skip_hidden: false\n").unwrap();
        let loaded = load_config(Some(&config_path), dir.path()).unwrap();
        assert!(!loaded.config.skip_hidden);
        assert_eq!(loaded.root_dir, dir.path());
        assert_eq!(loaded.source_path, Some(config_path));
    }

    #[test]
    fn test_config_size_limit_accepts_boundary_and_rejects_next_byte() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("bounded.yaml");
        let prefix = "skip_hidden: false\n#";
        let at_limit = format!(
            "{prefix}{}",
            "x".repeat(MAX_CONFIG_FILE_SIZE as usize - prefix.len())
        );
        fs::write(&config_path, &at_limit).unwrap();
        let loaded = load_config(Some(&config_path), dir.path()).unwrap();
        assert!(!loaded.config.skip_hidden);

        fs::write(&config_path, format!("{at_limit}x")).unwrap();
        let error = load_config(Some(&config_path), dir.path()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("exceeds maximum size of 1048576 bytes"),
            "{error}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_config_loader_rejects_non_regular_special_file() {
        let special = Path::new("/dev/zero");
        if !special.exists() {
            return;
        }

        let error = load_config(Some(special), Path::new("/")).unwrap_err();
        assert!(
            error.to_string().contains("must be a regular file"),
            "{error}"
        );
    }

    #[test]
    fn test_load_relative_explicit_config_preserves_lexical_root() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("config");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("ragtag.yaml"), "").unwrap();
        let loaded = load_config(Some(Path::new("config/ragtag.yaml")), dir.path()).unwrap();
        assert_eq!(loaded.root_dir, dir.path().join("config"));
    }

    #[test]
    fn test_discovered_config_provides_parent_root() {
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("child");
        fs::create_dir(&child).unwrap();
        fs::write(dir.path().join(".ragtag.yaml"), "").unwrap();
        let loaded = load_config(None, &child).unwrap();
        assert_eq!(loaded.root_dir, dir.path());
    }

    #[test]
    fn test_load_explicit_missing() {
        let result = load_config(Some(Path::new("/nonexistent/.ragtag.yaml")), Path::new("."));
        assert!(matches!(result, Err(RagtagError::ConfigNotFound(_))));
    }

    #[test]
    fn test_discover_in_current_dir() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join(".ragtag.yaml");
        fs::write(&config_path, "").unwrap();
        let found = discover_config_file(dir.path());
        assert_eq!(found, Some(config_path));
    }

    #[test]
    fn test_discover_prefers_dotfile() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".ragtag.yaml"), "").unwrap();
        fs::write(dir.path().join("ragtag.yaml"), "").unwrap();
        let found = discover_config_file(dir.path());
        assert!(found.unwrap().ends_with(".ragtag.yaml"));
    }

    #[test]
    fn test_discover_dot_yml_only() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join(".ragtag.yml");
        fs::write(&config_path, "").unwrap();
        let found = discover_config_file(dir.path());
        assert_eq!(found, Some(config_path));
    }

    #[test]
    fn test_discover_plain_yml_only() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("ragtag.yml");
        fs::write(&config_path, "").unwrap();
        let found = discover_config_file(dir.path());
        assert_eq!(found, Some(config_path));
    }

    #[test]
    fn test_discover_dot_yaml_beats_dot_yml() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".ragtag.yaml"), "").unwrap();
        fs::write(dir.path().join(".ragtag.yml"), "").unwrap();
        let found = discover_config_file(dir.path());
        assert!(found.unwrap().ends_with(".ragtag.yaml"));
    }

    #[test]
    fn test_discover_dot_yml_beats_plain_yaml() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".ragtag.yml"), "").unwrap();
        fs::write(dir.path().join("ragtag.yaml"), "").unwrap();
        let found = discover_config_file(dir.path());
        assert!(found.unwrap().ends_with(".ragtag.yml"));
    }

    #[test]
    fn test_discover_plain_yaml_beats_plain_yml() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("ragtag.yaml"), "").unwrap();
        fs::write(dir.path().join("ragtag.yml"), "").unwrap();
        let found = discover_config_file(dir.path());
        assert!(found.unwrap().ends_with("ragtag.yaml"));
    }

    #[test]
    fn test_discover_walks_up() {
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("subdir");
        fs::create_dir(&child).unwrap();
        fs::write(dir.path().join(".ragtag.yaml"), "").unwrap();
        let found = discover_config_file(&child);
        assert!(found.is_some());
    }

    #[test]
    fn test_discover_stops_at_git() {
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("subdir");
        fs::create_dir(&child).unwrap();
        // Place .git in child — should stop here, not find parent config
        fs::create_dir(child.join(".git")).unwrap();
        fs::write(dir.path().join(".ragtag.yaml"), "").unwrap();
        let found = discover_config_file(&child);
        assert!(found.is_none());
    }

    #[test]
    fn test_discover_returns_none_at_root() {
        // From a temp dir with no configs anywhere up to .git or root
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("a");
        fs::create_dir(&child).unwrap();
        // Place .git to bound the walk
        fs::create_dir(child.join(".git")).unwrap();
        let found = discover_config_file(&child);
        assert!(found.is_none());
    }

    #[test]
    fn test_load_invalid_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join(".ragtag.yaml");
        fs::write(&config_path, "invalid: [yaml: {{{").unwrap();
        let result = load_config(Some(&config_path), dir.path());
        assert!(matches!(result, Err(RagtagError::ConfigParse { .. })));
    }

    #[test]
    fn interpolation_runs_after_yaml_parsing_before_typed_config_consumption() {
        let yaml = r#"
ignore_patterns: ["$IGNORE", "prefix-${SUFFIX}"]
output:
  color: "$COLOR"
files:
  default_directory: "$DIRECTORY"
  filename_format: "$FORMAT"
aliases:
  - name: "$ALIAS_NAME"
    arguments: 'query "$RUNTIME_TAG"'
tasks:
  tag_name: "$TAG_NAME"
  default_owner: "$OWNER"
  status_keywords:
    active: ["$ACTIVE"]
custom_extension:
  nested:
    - "$CUSTOM"
"#;
        let values = HashMap::from([
            ("IGNORE", "target/"),
            ("SUFFIX", "cache"),
            ("COLOR", "never"),
            ("DIRECTORY", "notes"),
            ("FORMAT", "fixed.md"),
            ("ALIAS_NAME", "dynamic"),
            ("RUNTIME_TAG", "must-remain-deferred"),
            ("TAG_NAME", "todo"),
            ("OWNER", "Alice"),
            ("ACTIVE", "doing"),
            ("CUSTOM", "extension-value"),
        ]);
        let (config, provenance) = parse_config_with(yaml, &mut |name| {
            values.get(name).map(|value| (*value).to_string())
        })
        .unwrap();

        assert!(!provenance.is_empty());
        assert_eq!(config.ignore_patterns, ["target/", "prefix-cache"]);
        assert_eq!(config.output.color, crate::config::ColorMode::Never);
        assert_eq!(config.files.default_directory, PathBuf::from("notes"));
        assert_eq!(config.files.filename_format, "fixed.md");
        assert_eq!(config.aliases[0].names, ["dynamic"]);
        assert_eq!(config.aliases[0].arguments, ["query", "$RUNTIME_TAG"]);

        let tasks = config
            .extension_configs
            .get("tasks")
            .unwrap()
            .as_mapping()
            .unwrap();
        assert_eq!(
            tasks.get(serde_yml::Value::String("tag_name".to_string())),
            Some(&serde_yml::Value::String("todo".to_string()))
        );
        let active = tasks
            .get(serde_yml::Value::String("status_keywords".to_string()))
            .unwrap()
            .as_mapping()
            .unwrap()
            .get(serde_yml::Value::String("active".to_string()))
            .unwrap()
            .as_sequence()
            .unwrap();
        assert_eq!(active, &[serde_yml::Value::String("doing".to_string())]);
        let custom = config
            .extension_configs
            .get("custom_extension")
            .unwrap()
            .as_mapping()
            .unwrap();
        let nested = custom
            .get(serde_yml::Value::String("nested".to_string()))
            .unwrap()
            .as_sequence()
            .unwrap();
        assert_eq!(
            nested,
            &[serde_yml::Value::String("extension-value".to_string())]
        );
    }

    #[test]
    fn environment_derived_typed_errors_never_retain_resolved_values() {
        const SECRET: &str = "sentinel-secret-must-not-leak";
        let error = parse_config_with("output:\n  color: \"$SECRET_COLOR\"\n", &mut |name| {
            (name == "SECRET_COLOR").then(|| SECRET.to_string())
        })
        .unwrap_err();

        let public_error = match error {
            ConfigParseFailure::EnvironmentDerived => RagtagError::EnvironmentDerivedConfig,
            ConfigParseFailure::Raw(source) => RagtagError::ConfigParse {
                path: PathBuf::from("config.yaml"),
                source: Box::new(source),
            },
        };
        assert!(!public_error.to_string().contains(SECRET));
        assert!(!format!("{public_error:?}").contains(SECRET));
        assert!(public_error
            .to_string()
            .contains("environment interpolation"));
    }

    #[test]
    fn loaded_config_debug_never_exposes_environment_derived_values() {
        const SECRET: &str = "sentinel-debug-secret";
        let (config, interpolation) = parse_config_with(
            "tasks:\n  default_owner: \"$SECRET_OWNER\"\n",
            &mut |name| (name == "SECRET_OWNER").then(|| SECRET.to_string()),
        )
        .unwrap();
        let loaded = LoadedConfig {
            config,
            root_dir: PathBuf::from("."),
            source_path: Some(PathBuf::from("config.yaml")),
            interpolation,
        };

        let debug = format!("{loaded:?}");
        assert!(!debug.contains(SECRET));
        assert!(debug.contains("configuration values redacted"));
        assert!(!format!("{:?}", loaded.config).contains(SECRET));
    }
}

//! Config file discovery and loading.
//!
//! Implements walk-up discovery from the current directory, stopping at
//! `.git` boundaries or the filesystem root.

use std::path::{Path, PathBuf};

use super::schema::Config;
use crate::error::RagtagError;

/// Config file names to search for, in order of preference at each directory level.
const CONFIG_FILE_NAMES: &[&str] = &[".ragtag.yaml", "ragtag.yaml"];

/// Loads a ragtag configuration.
///
/// If `cli_path` is provided, loads from that explicit path. Otherwise,
/// walks up from `start_dir` looking for a config file.
pub fn load_config(cli_path: Option<&Path>, start_dir: &Path) -> Result<Config, RagtagError> {
    let config_path = match cli_path {
        Some(path) => {
            if !path.exists() {
                return Err(RagtagError::ConfigNotFound(path.to_path_buf()));
            }
            Some(path.to_path_buf())
        }
        None => discover_config_file(start_dir),
    };

    match config_path {
        Some(path) => {
            log::info!("loaded config from {}", path.display());
            let content = std::fs::read_to_string(&path).map_err(|e| RagtagError::FileRead {
                path: path.clone(),
                source: e,
            })?;
            let config: Config =
                serde_yml::from_str(&content).map_err(|e| RagtagError::ConfigParse {
                    path: path.clone(),
                    source: Box::new(e),
                })?;
            config.validate()?;
            Ok(config)
        }
        None => Ok(Config::default()),
    }
}

/// Discovers a config file by walking up from `start_dir`.
///
/// At each directory level, checks for `.ragtag.yaml` then `ragtag.yaml`.
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
    use std::fs;

    #[test]
    fn test_load_default_when_no_config() {
        let dir = tempfile::tempdir().unwrap();
        let config = load_config(None, dir.path()).unwrap();
        assert!(config.respect_gitignore);
    }

    #[test]
    fn test_load_explicit_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join(".ragtag.yaml");
        fs::write(&config_path, "skip_hidden: false\n").unwrap();
        let config = load_config(Some(&config_path), dir.path()).unwrap();
        assert!(!config.skip_hidden);
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
}

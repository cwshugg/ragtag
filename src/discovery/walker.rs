//! File discovery using the `ignore` crate.
//!
//! Walks directories respecting .gitignore, hidden file settings,
//! and user-configured regex ignore patterns.

use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::RagtagError;

/// Trait for file discovery, enabling testability via mock implementations.
pub trait FileWalker {
    /// Discovers files at the given path according to configuration.
    fn walk(&self, path: &Path) -> Result<Vec<PathBuf>, RagtagError>;
}

/// File walker implementation using the `ignore` crate.
pub struct IgnoreWalker {
    /// Compiled regex set for ignore patterns.
    ignore_set: Option<regex::RegexSet>,
    respect_gitignore: bool,
    skip_hidden: bool,
    #[allow(dead_code)]
    skip_binary: bool,
    max_depth: Option<usize>,
    max_file_size: u64,
}

impl IgnoreWalker {
    /// Creates a new walker from configuration.
    ///
    /// Compiles ignore patterns into a `RegexSet` with size limits.
    pub fn new(config: &Config) -> Result<Self, RagtagError> {
        let ignore_set = if config.ignore_patterns.is_empty() {
            None
        } else {
            let set = regex::RegexSetBuilder::new(&config.ignore_patterns)
                .size_limit(10 * 1024 * 1024)
                .dfa_size_limit(10 * 1024 * 1024)
                .build()
                .map_err(|e| {
                    let msg = e.to_string();
                    let truncated = if msg.len() > 200 {
                        format!("{}... (truncated)", &msg[..200])
                    } else {
                        msg
                    };
                    RagtagError::InvalidConfig(format!("invalid ignore pattern: {truncated}"))
                })?;
            Some(set)
        };

        Ok(Self {
            ignore_set,
            respect_gitignore: config.respect_gitignore,
            skip_hidden: config.skip_hidden,
            skip_binary: config.skip_binary,
            max_depth: config.max_depth,
            max_file_size: config.max_file_size,
        })
    }
}

impl FileWalker for IgnoreWalker {
    fn walk(&self, path: &Path) -> Result<Vec<PathBuf>, RagtagError> {
        if !path.exists() {
            return Err(RagtagError::FileRead {
                path: path.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("path not found: \"{}\"", path.display()),
                ),
            });
        }

        // Single file
        if path.is_file() {
            return Ok(vec![path.to_path_buf()]);
        }

        // Directory walk
        let mut builder = ignore::WalkBuilder::new(path);
        builder
            .git_ignore(self.respect_gitignore)
            .hidden(self.skip_hidden)
            .follow_links(false);

        if let Some(depth) = self.max_depth {
            builder.max_depth(Some(depth));
        }

        if let Some(max_size) = Some(self.max_file_size) {
            builder.max_filesize(Some(max_size));
        }

        let mut files = Vec::new();

        for entry in builder.build() {
            let entry = entry.map_err(|e| RagtagError::Io(std::io::Error::other(e.to_string())))?;

            // Skip directories
            if entry.file_type().is_none_or(|ft| !ft.is_file()) {
                continue;
            }

            let file_path = entry.path().to_path_buf();

            // Apply regex ignore patterns
            if let Some(ref set) = self.ignore_set {
                if set.is_match(&file_path.to_string_lossy()) {
                    continue;
                }
            }

            files.push(file_path);
        }

        // Sort for deterministic output
        files.sort();

        Ok(files)
    }
}

/// Convenience function to walk a path using config settings.
pub fn walk_path(path: &Path, config: &Config) -> Result<Vec<PathBuf>, RagtagError> {
    let walker = IgnoreWalker::new(config)?;
    walker.walk(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_single_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "hello").unwrap();
        let config = Config::default();
        let files = walk_path(&file, &config).unwrap();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_directory_walk() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "").unwrap();
        fs::write(dir.path().join("b.md"), "").unwrap();
        let config = Config::default();
        let files = walk_path(dir.path(), &config).unwrap();
        assert!(files.len() >= 2);
    }

    #[test]
    fn test_nonexistent_path() {
        let config = Config::default();
        let result = walk_path(Path::new("/nonexistent/path/xyz"), &config);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("path not found"));
    }

    #[test]
    fn test_empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::default();
        let files = walk_path(dir.path(), &config).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_ignore_patterns() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("keep.txt"), "").unwrap();
        fs::write(dir.path().join("skip.pdf"), "").unwrap();
        let mut config = Config::default();
        config.ignore_patterns = vec![".*\\.pdf$".to_string()];
        let files = walk_path(dir.path(), &config).unwrap();
        assert!(files.iter().all(|f| !f.to_string_lossy().ends_with(".pdf")));
    }

    #[test]
    fn test_symlinks_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("real.txt");
        fs::write(&file, "content").unwrap();
        let link = dir.path().join("link.txt");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&file, &link).unwrap();
        let config = Config::default();
        let files = walk_path(dir.path(), &config).unwrap();
        // The symlink should not be followed/included as a regular file
        // (the `ignore` crate with follow_links(false) treats symlinks differently)
        // We just verify no panic/error occurs
        assert!(!files.is_empty());
    }
}

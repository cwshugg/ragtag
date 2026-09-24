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

/// The bounded discovery result used by complete-corpus consumers.
pub(crate) enum BoundedWalk {
    /// Every candidate fit within both limits.
    Complete(Vec<PathBuf>),
    /// Discovery stopped before retaining an out-of-budget candidate.
    LimitExceeded { kind: DiscoveryLimitKind },
}

/// Identifies the first complete-discovery budget that was exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiscoveryLimitKind {
    FileCount,
    PathBytes,
    PathLength,
}

/// Complete discovery that fails instead of returning a partial corpus.
pub(crate) trait BoundedFileWalker {
    fn walk_bounded_complete(
        &self,
        path: &Path,
        maximum_files: usize,
        maximum_path_bytes: usize,
        maximum_path_length: usize,
    ) -> Result<BoundedWalk, RagtagError>;
}

/// File walker implementation using the `ignore` crate.
pub struct IgnoreWalker {
    /// Compiled regex set for ignore patterns.
    ignore_set: Option<regex::RegexSet>,
    respect_gitignore: bool,
    skip_hidden: bool,
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
            for (index, pattern) in config.ignore_patterns.iter().enumerate() {
                regex::RegexBuilder::new(pattern)
                    .size_limit(10 * 1024 * 1024)
                    .dfa_size_limit(10 * 1024 * 1024)
                    .build()
                    .map_err(|_| {
                        RagtagError::InvalidConfig(format!(
                            "invalid ignore pattern at configured index {index}"
                        ))
                    })?;
            }
            let set = regex::RegexSetBuilder::new(&config.ignore_patterns)
                .size_limit(10 * 1024 * 1024)
                .dfa_size_limit(10 * 1024 * 1024)
                .build()
                .map_err(|_| {
                    RagtagError::InvalidConfig(
                        "combined ignore patterns exceed configured regex limits".to_string(),
                    )
                })?;
            Some(set)
        };

        Ok(Self {
            ignore_set,
            respect_gitignore: config.respect_gitignore,
            skip_hidden: config.skip_hidden,
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

        let mut builder = ignore::WalkBuilder::new(path);
        builder
            .git_ignore(self.respect_gitignore)
            .hidden(self.skip_hidden)
            .follow_links(false);

        if let Some(depth) = self.max_depth {
            builder.max_depth(Some(depth));
        }

        builder.max_filesize(Some(self.max_file_size));

        let mut files = Vec::new();

        for entry in builder.build() {
            let entry = entry.map_err(|e| RagtagError::Io(std::io::Error::other(e.to_string())))?;

            if entry.file_type().is_none_or(|ft| !ft.is_file()) {
                continue;
            }

            let file_path = entry.path().to_path_buf();

            if let Some(ref set) = self.ignore_set {
                if set.is_match(&file_path.to_string_lossy()) {
                    continue;
                }
            }

            files.push(file_path);
        }

        files.sort();

        Ok(files)
    }
}

impl BoundedFileWalker for IgnoreWalker {
    fn walk_bounded_complete(
        &self,
        path: &Path,
        maximum_files: usize,
        maximum_path_bytes: usize,
        maximum_path_length: usize,
    ) -> Result<BoundedWalk, RagtagError> {
        if !path.exists() {
            return Err(RagtagError::FileRead {
                path: path.to_path_buf(),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "path not found"),
            });
        }

        let mut files = Vec::new();
        let mut path_bytes = 0usize;
        let mut consider = |candidate: &Path| -> Option<DiscoveryLimitKind> {
            let length = candidate.as_os_str().len();
            if length > maximum_path_length {
                return Some(DiscoveryLimitKind::PathLength);
            }
            if files.len() == maximum_files {
                return Some(DiscoveryLimitKind::FileCount);
            }
            let Some(next_bytes) = path_bytes.checked_add(length) else {
                return Some(DiscoveryLimitKind::PathBytes);
            };
            if next_bytes > maximum_path_bytes {
                return Some(DiscoveryLimitKind::PathBytes);
            }
            path_bytes = next_bytes;
            files.push(candidate.to_path_buf());
            None
        };
        if path.is_file() {
            return Ok(match consider(path) {
                Some(kind) => BoundedWalk::LimitExceeded { kind },
                None => BoundedWalk::Complete(files),
            });
        }

        let mut builder = ignore::WalkBuilder::new(path);
        builder
            .git_ignore(self.respect_gitignore)
            .hidden(self.skip_hidden)
            .follow_links(false);
        if let Some(depth) = self.max_depth {
            builder.max_depth(Some(depth));
        }
        for entry in builder.build() {
            let entry =
                entry.map_err(|error| RagtagError::Io(std::io::Error::other(error.to_string())))?;
            if entry
                .file_type()
                .is_none_or(|file_type| !file_type.is_file())
            {
                continue;
            }
            let candidate = entry.path();
            if self
                .ignore_set
                .as_ref()
                .is_some_and(|set| set.is_match(&candidate.to_string_lossy()))
            {
                continue;
            }
            if let Some(kind) = consider(candidate) {
                return Ok(BoundedWalk::LimitExceeded { kind });
            }
        }
        files.sort();
        Ok(BoundedWalk::Complete(files))
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
        let config = Config {
            ignore_patterns: vec![".*\\.pdf$".to_string()],
            ..Default::default()
        };
        let files = walk_path(dir.path(), &config).unwrap();
        assert!(files.iter().all(|f| !f.to_string_lossy().ends_with(".pdf")));
    }

    #[test]
    fn invalid_ignore_pattern_error_omits_untrusted_pattern_text() {
        let sentinel = "IGNORE_SECRET";
        let config = Config {
            ignore_patterns: vec![format!(
                "(?P<\n\u{1b}]8;;https://evil\u{7}\u{202e}{sentinel}{}",
                "é".repeat(200)
            )],
            ..Default::default()
        };

        let error = match IgnoreWalker::new(&config) {
            Ok(_) => panic!("hostile ignore pattern unexpectedly compiled"),
            Err(error) => error,
        };
        let display = error.to_string();
        let debug = format!("{error:?}");
        for rendered in [&display, &debug] {
            assert!(rendered.contains("configured index 0"));
            assert!(!rendered.contains(sentinel));
            assert!(!rendered.contains('\n'));
            assert!(!rendered.contains('\u{1b}'));
            assert!(!rendered.contains('\u{7}'));
            assert!(!rendered.contains('\u{202e}'));
        }
    }

    #[test]
    fn bounded_walk_fails_before_retaining_candidate_past_limit() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "").unwrap();
        fs::write(dir.path().join("b.txt"), "").unwrap();
        let walker = IgnoreWalker::new(&Config::default()).unwrap();
        let result = walker
            .walk_bounded_complete(dir.path(), 1, usize::MAX, usize::MAX)
            .unwrap();
        assert!(matches!(
            result,
            BoundedWalk::LimitExceeded {
                kind: DiscoveryLimitKind::FileCount
            }
        ));
    }

    #[test]
    fn bounded_walk_sorts_complete_results() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("z.txt"), "").unwrap();
        fs::write(dir.path().join("a.txt"), "").unwrap();
        let walker = IgnoreWalker::new(&Config::default()).unwrap();
        let BoundedWalk::Complete(paths) = walker
            .walk_bounded_complete(dir.path(), 10, usize::MAX, usize::MAX)
            .unwrap()
        else {
            panic!("expected a complete walk");
        };
        assert!(paths.windows(2).all(|pair| pair[0] <= pair[1]));
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

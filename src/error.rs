//! Error types for the ragtag application.
//!
//! Provides a unified `RagtagError` enum covering all failure modes
//! across config loading, file I/O, parsing, and extension execution.

use std::path::PathBuf;

/// The primary error type for the ragtag application.
///
/// Each variant captures the context needed to produce a helpful,
/// user-facing error message.
#[derive(Debug, thiserror::Error)]
pub enum RagtagError {
    /// The specified config file was not found.
    #[error("error: config file not found: \"{0}\"")]
    ConfigNotFound(PathBuf),

    /// Failed to parse the config file as YAML.
    #[error("error: failed to parse config file \"{path}\": {source}")]
    ConfigParse {
        path: PathBuf,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The config file contains invalid values.
    #[error("error: invalid config: {0}")]
    InvalidConfig(String),

    /// Failed to read a file.
    #[error("error: failed to read \"{path}\": {source}")]
    FileRead {
        path: PathBuf,
        source: std::io::Error,
    },

    /// Failed to write a file.
    #[error("error: failed to write \"{path}\": {source}")]
    FileWrite {
        path: PathBuf,
        source: std::io::Error,
    },

    /// A parsing error occurred in a specific file.
    #[error("error: parse error in \"{file}\" line {line}: {message}")]
    ParseError {
        file: PathBuf,
        line: usize,
        message: String,
    },

    /// An invalid filter expression was provided.
    #[error("error: invalid filter expression: {0}")]
    InvalidFilter(String),

    /// Invalid user input (e.g., empty search string).
    #[error("error: invalid input: {0}")]
    InvalidInput(String),

    /// Attempted to edit a symlinked file.
    #[error("error: cannot edit symlinked file \"{0}\" — resolve the symlink or edit the target file directly")]
    SymlinkEdit(PathBuf),

    /// An unknown command was provided.
    #[error("error: unknown command \"{0}\"")]
    UnknownCommand(String),

    /// An error from an extension.
    #[error("error [{extension_name}]: {message}")]
    ExtensionError {
        extension_name: String,
        message: String,
    },

    /// A catch-all I/O error.
    #[error("error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_not_found_display() {
        let err = RagtagError::ConfigNotFound(PathBuf::from("/path/to/config"));
        assert!(err.to_string().contains("/path/to/config"));
    }

    #[test]
    fn test_extension_error_display() {
        let err = RagtagError::ExtensionError {
            extension_name: "Task Manager".to_string(),
            message: "task not found".to_string(),
        };
        assert!(err.to_string().contains("[Task Manager]"));
        assert!(err.to_string().contains("task not found"));
    }

    #[test]
    fn test_symlink_edit_display() {
        let err = RagtagError::SymlinkEdit(PathBuf::from("notes/link.md"));
        assert!(err.to_string().contains("symlinked file"));
        assert!(err.to_string().contains("notes/link.md"));
    }
}

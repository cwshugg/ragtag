//! Error types for the ragtag application.
//!
//! Provides a unified `RagtagError` enum covering all failure modes
//! across config loading, file I/O, parsing, and extension execution.

use std::path::PathBuf;
use std::process::ExitStatus;

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

    /// Environment-derived configuration failed validation without disclosure.
    #[error(
        "error: configuration is invalid after environment interpolation; review referenced variables and expected field types"
    )]
    EnvironmentDerivedConfig,

    /// A command using environment-derived configuration failed safely.
    #[error(
        "error: command failed while using environment-derived configuration; review referenced variables and command inputs"
    )]
    EnvironmentDerivedConfigCommand,

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

    /// A supplied tag is not exactly one valid parser tag.
    #[error("error: invalid tag {input:?}: {reason}")]
    InvalidTag { input: String, reason: String },

    /// A target path or generated filename is invalid.
    #[error("error: invalid file target \"{path}\": {reason}")]
    InvalidFileTarget { path: PathBuf, reason: String },

    /// Failed to create the target's parent directories.
    #[error("error: failed to create parent directory \"{parent}\" for \"{target}\": {source}")]
    FileParentCreate {
        parent: PathBuf,
        target: PathBuf,
        source: std::io::Error,
    },

    /// The requested creation target already exists.
    #[error("error: target already exists: \"{0}\"")]
    FileTargetExists(PathBuf),

    /// Failed to exclusively create the target.
    #[error("error: failed to create \"{path}\": {source}")]
    FileCreate {
        path: PathBuf,
        source: std::io::Error,
    },

    /// Failed while writing or flushing a newly created target.
    #[error("error: failed to write newly created file \"{path}\": {source}")]
    FileCreateWrite {
        path: PathBuf,
        source: std::io::Error,
    },

    /// EDITOR is unavailable or cannot be parsed.
    #[error("error: invalid EDITOR configuration: {0}")]
    InvalidEditor(String),

    /// The configured editor could not be launched.
    #[error("error: failed to launch editor for \"{path}\": {source}; the created file remains")]
    EditorLaunch {
        path: PathBuf,
        source: std::io::Error,
    },

    /// The configured editor exited unsuccessfully.
    #[error(
        "error: editor exited unsuccessfully for \"{path}\" ({status}); the created file remains"
    )]
    EditorExit { path: PathBuf, status: ExitStatus },

    /// Attempted to edit a symlinked file.
    #[error("error: cannot edit symlinked file \"{0}\" — resolve the symlink or edit the target file directly")]
    SymlinkEdit(PathBuf),

    /// An unknown command was provided.
    #[error("error: unknown command \"{0}\"")]
    UnknownCommand(String),

    /// An outer token is ambiguous across real commands and aliases.
    #[error(
        "error: alias command \"{token}\" is ambiguous; candidates: {}",
        candidates.join(", ")
    )]
    AliasOuterAmbiguous {
        /// The unresolved outer token.
        token: String,
        /// Deterministically ordered matching names.
        candidates: Vec<String>,
    },

    /// Recursive alias composition revisited an active definition.
    #[error("error: alias expansion cycle: {}", chain.join(" -> "))]
    AliasCycle {
        /// Selected and referenced spellings in expansion order.
        chain: Vec<String>,
    },

    /// Recursive alias composition would exceed its definition-depth bound.
    #[error(
        "error: alias expansion exceeds maximum depth of {limit}: {}",
        chain.join(" -> ")
    )]
    AliasExpansionDepthExceeded {
        /// Configured depth limit.
        limit: usize,
        /// Selected and referenced spellings in expansion order.
        chain: Vec<String>,
    },

    /// Recursive alias composition would exceed its token bound.
    #[error(
        "error: alias expansion exceeds maximum of {limit} arguments ({count_display}): {}",
        chain.join(" -> "),
        count_display = if *count == usize::MAX {
            "projected count overflowed usize; saturated count is usize::MAX".to_string()
        } else {
            format!("projected count: {count}")
        }
    )]
    AliasExpansionArgumentsExceeded {
        /// Configured expanded-token limit.
        limit: usize,
        /// Exact projection, or `usize::MAX` when checked arithmetic overflowed.
        count: usize,
        /// Selected and referenced spellings in expansion order.
        chain: Vec<String>,
    },

    /// A command using environment-derived data failed without disclosure.
    #[error(
        "error: alias \"{alias}\" command failed after environment interpolation; review its variable values and expected arguments"
    )]
    EnvironmentDerivedAliasCommand {
        /// Alias spelling selected by the user.
        alias: String,
    },

    /// An alias terminated at a definite unknown command target.
    #[error("error: alias target is not a command (chain: {})", chain.join(" -> "))]
    AliasTargetUnknown {
        /// Selected and referenced spellings in expansion order.
        chain: Vec<String>,
    },

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

    #[test]
    fn test_alias_errors_preserve_structured_order_and_counts() {
        let ambiguity = RagtagError::AliasOuterAmbiguous {
            token: "su".to_string(),
            candidates: vec!["summary".to_string(), "sum-all".to_string()],
        };
        assert_eq!(
            ambiguity.to_string(),
            "error: alias command \"su\" is ambiguous; candidates: summary, sum-all"
        );

        let cycle = RagtagError::AliasCycle {
            chain: vec!["a".to_string(), "alt-a".to_string()],
        };
        assert_eq!(
            cycle.to_string(),
            "error: alias expansion cycle: a -> alt-a"
        );

        let depth = RagtagError::AliasExpansionDepthExceeded {
            limit: 32,
            chain: vec!["a".to_string(), "b".to_string()],
        };
        assert_eq!(
            depth.to_string(),
            "error: alias expansion exceeds maximum depth of 32: a -> b"
        );

        let exact = RagtagError::AliasExpansionArgumentsExceeded {
            limit: 4096,
            count: 4097,
            chain: vec!["a".to_string(), "b".to_string()],
        };
        assert!(exact.to_string().contains("projected count: 4097"));
        assert!(exact.to_string().contains("a -> b"));

        let saturated = RagtagError::AliasExpansionArgumentsExceeded {
            limit: 4096,
            count: usize::MAX,
            chain: vec!["a".to_string()],
        };
        assert!(saturated.to_string().contains("overflowed usize"));
        assert!(saturated.to_string().contains("usize::MAX"));

        let unknown = RagtagError::AliasTargetUnknown {
            chain: vec!["a".to_string(), "b".to_string()],
        };
        assert_eq!(
            unknown.to_string(),
            "error: alias target is not a command (chain: a -> b)"
        );
    }
}

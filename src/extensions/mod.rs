//! Extension system for ragtag.
//!
//! Defines the `TagExtension` trait, `ExtensionRegistry`, `ExtensionContext`,
//! `ValidationMessage`, and the `DefaultTagParser` implementation.

pub mod task;

use std::io::Write;
use std::path::Path;

use crate::config::{ColorMode, Config};
use crate::discovery::FileWalker;
use crate::edit::FileEditor;
use crate::error::RagtagError;
use crate::models::{Tag, TagLocation};
use crate::parser::scan_file;

/// Trait for parsing tags from file contents.
pub trait TagParser {
    /// Parses all tags from the given file contents.
    fn parse_file(&self, contents: &str, path: &Path) -> Vec<Tag>;
}

/// Default tag parser implementation that delegates to `scan_file`.
pub struct DefaultTagParser;

impl TagParser for DefaultTagParser {
    fn parse_file(&self, contents: &str, path: &Path) -> Vec<Tag> {
        scan_file(contents, path)
    }
}

/// Context passed to extensions during execution.
///
/// Provides controlled access to core infrastructure via trait objects,
/// making extensions testable with mock implementations.
pub struct ExtensionContext<'a> {
    /// File discovery service.
    pub walker: &'a dyn FileWalker,
    /// Tag parser service.
    pub parser: &'a dyn TagParser,
    /// File editor service.
    pub editor: &'a dyn FileEditor,
    /// Resolved color mode.
    pub color_mode: ColorMode,
    /// Full loaded config.
    pub config: &'a Config,
    /// Standard output writer.
    pub stdout: &'a mut dyn Write,
    /// Standard error writer.
    pub stderr: &'a mut dyn Write,
}

/// A validation message from an extension.
#[derive(Debug, Clone)]
pub struct ValidationMessage {
    /// Severity level.
    pub level: ValidationLevel,
    /// The message text.
    pub message: String,
    /// Optional location reference.
    pub location: Option<TagLocation>,
}

/// Severity level for validation messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationLevel {
    /// Error — prevents further processing.
    Error,
    /// Warning — displayed but processing continues.
    Warning,
}

/// Trait that all tag extensions must implement.
///
/// Provides identity, configuration, validation, CLI, and formatting hooks.
pub trait TagExtension {
    /// The tag name this extension handles (e.g., "task").
    fn tag_name(&self) -> &str;
    /// Human-readable display name (e.g., "Task Manager").
    fn display_name(&self) -> &str;
    /// Brief description for help text.
    fn description(&self) -> &str;

    /// YAML config key (e.g., "tasks"). `None` if no config needed.
    fn config_key(&self) -> Option<&str>;
    /// Initialize with config data.
    fn init(&mut self, config_value: Option<&serde_yml::Value>) -> Result<(), RagtagError>;

    /// Validate a tag of this extension's type.
    fn validate_tag(&self, tag: &Tag) -> Vec<ValidationMessage>;

    /// Build the clap `Command` for this extension's CLI subcommand.
    fn cli_command(&self) -> clap::Command;
    /// Execute the extension's command.
    fn execute(
        &self,
        matches: &clap::ArgMatches,
        ctx: &mut ExtensionContext,
    ) -> Result<(), RagtagError>;

    /// Format a single tag for output. Return `None` to use default formatting.
    fn format_tag(&self, _tag: &Tag, _color_mode: &ColorMode) -> Option<String> {
        None
    }

    /// Format a summary for a set of tags. Return `None` to use default formatting.
    fn format_summary(&self, _tags: &[Tag], _color_mode: &ColorMode) -> Option<String> {
        None
    }
}

/// Registry of all loaded extensions.
///
/// Enforces tag name uniqueness and provides lookup by tag name or command name.
pub struct ExtensionRegistry {
    extensions: Vec<Box<dyn TagExtension>>,
}

impl ExtensionRegistry {
    /// Creates a new empty registry.
    pub fn new() -> Self {
        Self {
            extensions: Vec::new(),
        }
    }

    /// Registers an extension, checking for tag name collisions.
    pub fn register(&mut self, ext: Box<dyn TagExtension>) -> Result<(), RagtagError> {
        let tag_name = ext.tag_name().to_string();
        if self.extensions.iter().any(|e| e.tag_name() == tag_name) {
            return Err(RagtagError::InvalidConfig(format!(
                "duplicate extension tag name: \"{tag_name}\""
            )));
        }
        self.extensions.push(ext);
        Ok(())
    }

    /// Looks up an extension by its tag name.
    pub fn get_by_tag_name(&self, name: &str) -> Option<&dyn TagExtension> {
        self.extensions
            .iter()
            .find(|e| e.tag_name() == name)
            .map(|e| e.as_ref())
    }

    /// Looks up an extension by its CLI command name.
    ///
    /// Convention: the command name is the config key or tag name + "s".
    pub fn get_by_command_name(&self, name: &str) -> Option<&dyn TagExtension> {
        self.extensions.iter().find_map(|e| {
            let cmd = e.cli_command();
            if cmd.get_name() == name {
                Some(e.as_ref())
            } else {
                None
            }
        })
    }

    /// Returns all registered extensions.
    pub fn all(&self) -> &[Box<dyn TagExtension>] {
        &self.extensions
    }

    /// Returns mutable references to all registered extensions for initialization.
    pub fn all_mut(&mut self) -> &mut [Box<dyn TagExtension>] {
        &mut self.extensions
    }

    /// Collects CLI commands from all registered extensions.
    pub fn cli_commands(&self) -> Vec<clap::Command> {
        self.extensions.iter().map(|e| e.cli_command()).collect()
    }
}

impl Default for ExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal mock extension for testing.
    struct MockExtension {
        name: &'static str,
        cmd_name: &'static str,
    }

    impl MockExtension {
        fn with_names(name: &'static str, cmd_name: &'static str) -> Self {
            Self { name, cmd_name }
        }
    }

    impl TagExtension for MockExtension {
        fn tag_name(&self) -> &str {
            self.name
        }
        fn display_name(&self) -> &str {
            "Mock"
        }
        fn description(&self) -> &str {
            "A mock extension"
        }
        fn config_key(&self) -> Option<&str> {
            None
        }
        fn init(&mut self, _: Option<&serde_yml::Value>) -> Result<(), RagtagError> {
            Ok(())
        }
        fn validate_tag(&self, _: &Tag) -> Vec<ValidationMessage> {
            vec![]
        }
        fn cli_command(&self) -> clap::Command {
            clap::Command::new(self.cmd_name)
        }
        fn execute(
            &self,
            _: &clap::ArgMatches,
            _: &mut ExtensionContext,
        ) -> Result<(), RagtagError> {
            Ok(())
        }
    }

    #[test]
    fn test_register_and_lookup() {
        let mut registry = ExtensionRegistry::new();
        registry
            .register(Box::new(MockExtension::with_names("mock", "mocks")))
            .unwrap();
        assert!(registry.get_by_tag_name("mock").is_some());
        assert!(registry.get_by_tag_name("other").is_none());
    }

    #[test]
    fn test_tag_name_collision() {
        let mut registry = ExtensionRegistry::new();
        registry
            .register(Box::new(MockExtension::with_names("mock", "mocks")))
            .unwrap();
        let result = registry.register(Box::new(MockExtension::with_names("mock", "mocks")));
        assert!(result.is_err());
    }

    #[test]
    fn test_cli_commands() {
        let mut registry = ExtensionRegistry::new();
        registry
            .register(Box::new(MockExtension::with_names("mock", "mocks")))
            .unwrap();
        let cmds = registry.cli_commands();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].get_name(), "mocks");
    }

    #[test]
    fn test_get_by_command_name() {
        let mut registry = ExtensionRegistry::new();
        registry
            .register(Box::new(MockExtension::with_names("task", "tasks")))
            .unwrap();
        assert!(registry.get_by_command_name("tasks").is_some());
    }

    #[test]
    fn test_validation_message() {
        let msg = ValidationMessage {
            level: ValidationLevel::Error,
            message: "test error".to_string(),
            location: None,
        };
        assert_eq!(msg.level, ValidationLevel::Error);
    }

    #[test]
    fn test_default_tag_parser() {
        let parser = DefaultTagParser;
        let tags = parser.parse_file("hello @tag world", Path::new("test.md"));
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "tag");
    }
}

//! Task extension — `@task` tag handling.
//!
//! Implements task tracking functionality as a modular extension.
//! The core system knows nothing about task semantics; all task-specific
//! behavior lives here.

pub mod cli;
pub mod commands;
pub mod config;
pub mod models;
pub mod output;
pub mod validation;

use crate::config::ColorMode;
use crate::error::RagtagError;
use crate::extensions::{ExtensionContext, TagExtension, ValidationMessage};
use crate::models::Tag;

/// The task extension implementing `TagExtension`.
pub struct TaskExtension {
    config: config::TaskConfig,
}

impl TaskExtension {
    /// Creates a new task extension with default configuration.
    pub fn new() -> Self {
        Self {
            config: config::TaskConfig::default(),
        }
    }
}

impl Default for TaskExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl TagExtension for TaskExtension {
    fn tag_name(&self) -> &str {
        &self.config.tag_name
    }

    fn display_name(&self) -> &str {
        "Task Manager"
    }

    fn description(&self) -> &str {
        "Track and manage tasks embedded in plain text files"
    }

    fn config_key(&self) -> Option<&str> {
        Some("tasks")
    }

    fn init(&mut self, config_value: Option<&serde_yml::Value>) -> Result<(), RagtagError> {
        self.config = match config_value {
            Some(val) => config::TaskConfig::from_config_value(val)?,
            None => config::TaskConfig::default(),
        };
        self.config.validate()?;
        Ok(())
    }

    fn validate_tag(&self, tag: &Tag) -> Vec<ValidationMessage> {
        validation::validate_task_tag(tag, &self.config)
    }

    fn cli_command(&self) -> clap::Command {
        cli::build_tasks_command()
    }

    fn execute(
        &self,
        matches: &clap::ArgMatches,
        ctx: &mut ExtensionContext,
    ) -> Result<(), RagtagError> {
        commands::dispatch(matches, &self.config, ctx)
    }

    fn format_tag(&self, tag: &Tag, color_mode: &ColorMode) -> Option<String> {
        match models::TaskTag::from_tag(tag, &self.config, "") {
            Ok(task) => Some(output::format_task_line(
                &task,
                &[],
                color_mode,
                &self.config,
            )),
            Err(_) => None,
        }
    }

    fn format_summary(&self, tags: &[Tag], color_mode: &ColorMode) -> Option<String> {
        Some(output::format_task_summary(tags, &self.config, color_mode))
    }
}

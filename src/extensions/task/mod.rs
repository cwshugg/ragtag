//! Task extension — `@task` tag handling.
//!
//! Implements task tracking functionality as a modular extension.
//! The core system knows nothing about task semantics; all task-specific
//! behavior lives here.

pub mod cli;
pub mod commands;
pub mod config;
pub mod filter;
pub mod models;
pub mod output;
pub(crate) mod semantics;
pub mod validation;

use crate::config::ColorMode;
use crate::error::RagtagError;
use crate::extensions::{ExtensionContext, TagExtension, ValidationMessage};
use crate::models::Tag;

/// Canonical top-level configuration key for the task extension.
pub(crate) const TASKS_CONFIG_KEY: &str = "tasks";

/// The task extension implementing `TagExtension`.
pub struct TaskExtension {
    semantics: std::sync::Arc<semantics::TaskSemantics>,
}

impl TaskExtension {
    /// Creates a configured task extension.
    pub(crate) fn new(semantics: std::sync::Arc<semantics::TaskSemantics>) -> Self {
        Self { semantics }
    }

    #[cfg(test)]
    pub(crate) fn semantics(&self) -> &std::sync::Arc<semantics::TaskSemantics> {
        &self.semantics
    }
}

impl TagExtension for TaskExtension {
    fn tag_name(&self) -> &str {
        &self.semantics.config().tag_name
    }

    fn display_name(&self) -> &str {
        "Task Manager"
    }

    fn description(&self) -> &str {
        "Track and manage tasks embedded in plain text files"
    }

    fn command_name(&self) -> &str {
        "task"
    }

    fn validate_tag(&self, tag: &Tag) -> Vec<ValidationMessage> {
        validation::validate_task_tag(tag, self.semantics.config())
    }

    fn execute(
        &self,
        matches: &clap::ArgMatches,
        ctx: &mut ExtensionContext,
    ) -> Result<(), RagtagError> {
        commands::dispatch(matches, &self.semantics, ctx)
    }

    fn format_tag(&self, tag: &Tag, color_mode: &ColorMode) -> Option<String> {
        match self.semantics.task_from_tag(tag) {
            Ok(task) => Some(output::format_task_line(
                &task,
                color_mode,
                self.semantics.config(),
            )),
            Err(_) => None,
        }
    }

    fn format_summary(&self, tags: &[Tag], color_mode: &ColorMode) -> Option<String> {
        Some(output::format_task_summary(
            tags,
            self.semantics.config(),
            color_mode,
        ))
    }
}

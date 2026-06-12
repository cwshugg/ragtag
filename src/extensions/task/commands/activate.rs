//! Task activate command.
//!
//! Sets a task's status to the first configured ACTIVE status keyword
//! (default: `"active"`), auto-updates `time_last_updated`, and either
//! edits the file in-place or prints the updated `@task(...)` string
//! when `--no-edit` is specified.

use super::super::config::TaskConfig;
use super::status_change::run_status_change;
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;

/// Runs the `task activate` command.
pub fn run(
    matches: &clap::ArgMatches,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(), RagtagError> {
    let active_status = config
        .status_keywords
        .active
        .first()
        .ok_or_else(|| RagtagError::ExtensionError {
            extension_name: "Task Manager".to_string(),
            message: "no active status keywords configured".to_string(),
        })?
        .clone();

    run_status_change(matches, config, ctx, &active_status, "Activated")
}

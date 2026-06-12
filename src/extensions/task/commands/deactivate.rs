//! Task deactivate command.
//!
//! Sets a task's status to the first configured INACTIVE status keyword
//! (default: `"inactive"`), auto-updates `time_last_updated`, and either
//! edits the file in-place or prints the updated `@task(...)` string
//! when `--no-edit` is specified.

use super::super::config::TaskConfig;
use super::status_change::run_status_change;
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;

/// Runs the `task deactivate` command.
pub fn run(
    matches: &clap::ArgMatches,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(), RagtagError> {
    let inactive_status = config
        .status_keywords
        .inactive
        .first()
        .ok_or_else(|| RagtagError::ExtensionError {
            extension_name: "Task Manager".to_string(),
            message: "no inactive status keywords configured".to_string(),
        })?
        .clone();

    run_status_change(matches, config, ctx, &inactive_status, "Deactivated")
}

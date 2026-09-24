//! Task prioritize command.
//!
//! Sets a task's `priority` attribute to a caller-supplied non-negative integer
//! value, auto-updates `time_last_updated`, and either edits the file in-place
//! or prints the updated `@task(...)` string when `--no-edit` is specified.

use std::path::Path;

use chrono::Utc;

use super::super::config::TaskConfig;
use super::create::escape_for_tag;
use super::{mutate_task, TaskMutation};
use crate::cli;
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;

/// Runs the `task prioritize` command.
pub fn run(
    matches: &clap::ArgMatches,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(), RagtagError> {
    let priority_str = matches
        .get_one::<String>("priority")
        .expect("required argument");
    let id = matches.get_one::<String>("id").expect("required argument");
    let no_edit = matches.get_flag("no-edit");

    // Validate: priority must be a non-negative integer (u32).
    let priority: u32 = priority_str
        .parse()
        .map_err(|_| RagtagError::ExtensionError {
            extension_name: "Task Manager".to_string(),
            message: format!(
            "invalid priority \"{priority_str}\" — expected a non-negative integer (e.g., 0, 1, 2)"
        ),
        })?;

    let path_str = cli::resolve_path(matches);
    let path = Path::new(&path_str);

    let priority_formatted = priority.to_string();
    mutate_task(id, path, no_edit, config, ctx, |task| {
        let now_ts = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let ts_formatted = format!("\"{}\"", escape_for_tag(&now_ts));
        Ok(TaskMutation::new(
            vec![
                ("priority".to_string(), priority_formatted),
                ("time_last_updated".to_string(), ts_formatted),
            ],
            task.task_type.clone(),
            format!("Prioritized task {} (priority → {priority})", task.id),
        ))
    })
}

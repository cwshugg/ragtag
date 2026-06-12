//! Task prioritize command.
//!
//! Sets a task's `priority` attribute to a caller-supplied non-negative integer
//! value, auto-updates `time_last_updated`, and either edits the file in-place
//! or prints the updated `@task(...)` string when `--no-edit` is specified.

use std::path::Path;

use chrono::Utc;

use super::super::config::TaskConfig;
use super::create::escape_for_tag;
use super::find_task_by_id;
use crate::cli;
use crate::edit::{edit_task_tag, write_file_atomically};
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
    let priority: u32 = priority_str.parse().map_err(|_| RagtagError::ExtensionError {
        extension_name: "Task Manager".to_string(),
        message: format!(
            "invalid priority \"{priority_str}\" — expected a non-negative integer (e.g., 0, 1, 2)"
        ),
    })?;

    let path_str = cli::resolve_path(matches);
    let path = Path::new(&path_str);

    let (task, content) = find_task_by_id(id, path, config, ctx)?;

    // Compute the auto-updated timestamp and format attribute values.
    let now_ts = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let ts_formatted = format!("\"{}\"", escape_for_tag(&now_ts));
    let priority_formatted = priority.to_string();

    let original_tag = &content[task.raw_span.clone()];

    // Apply the priority change and the auto-managed timestamp in a
    // single format-preserving edit. New attributes (e.g. an older task
    // missing `time_last_updated`) are appended for backward compatibility.
    let modified_tag = edit_task_tag(
        original_tag,
        &[
            ("priority", &priority_formatted),
            ("time_last_updated", &ts_formatted),
        ],
    )?;

    if no_edit {
        writeln!(ctx.stdout, "{modified_tag}").map_err(RagtagError::Io)?;
    } else {
        // Reconstruct full file content and write atomically.
        let mut new_content = String::with_capacity(content.len());
        new_content.push_str(&content[..task.raw_span.start]);
        new_content.push_str(&modified_tag);
        new_content.push_str(&content[task.raw_span.end..]);
        write_file_atomically(&task.location.file_path, &new_content)?;
        writeln!(
            ctx.stdout,
            "Prioritized task {} (priority → {})",
            task.id, priority
        )
        .map_err(RagtagError::Io)?;
    }

    Ok(())
}

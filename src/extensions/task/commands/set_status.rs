//! Task set-status command.

use std::path::Path;

use super::super::config::TaskConfig;
use super::super::output::format_task_detail;
use super::create::escape_for_tag;
use super::{find_task_by_id, prompt_for_value};
use crate::cli;
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;

/// Runs the set-status command.
pub fn run(
    matches: &clap::ArgMatches,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(), RagtagError> {
    let id = matches
        .get_one::<String>("id")
        .ok_or_else(|| RagtagError::ExtensionError {
            extension_name: "Task Manager".to_string(),
            message: "missing required argument --id".to_string(),
        })?;

    let path_str = cli::resolve_path(matches);
    let path = Path::new(&path_str);

    let (mut task, _) = find_task_by_id(id, path, config, ctx)?;

    // Get new status
    let new_status = if let Some(status) = matches.get_one::<String>("status") {
        status.clone()
    } else {
        prompt_for_value(ctx, &task, config, "New status: ")?
    };

    // Validate status
    if !config.all_status_keywords().contains(&new_status.as_str()) {
        return Err(RagtagError::ExtensionError {
            extension_name: "Task Manager".to_string(),
            message: format!(
                "invalid status \"{}\" — allowed statuses: {}",
                new_status,
                config.all_status_keywords().join(", ")
            ),
        });
    }

    // Edit file
    ctx.editor.update_tag_attribute(
        &task.location.file_path,
        task.raw_span.clone(),
        "status",
        &format!("\"{}\"", escape_for_tag(&new_status)),
    )?;

    task.status = new_status;

    // Print updated task detail
    writeln!(
        ctx.stdout,
        "{}",
        format_task_detail(&task, config, &ctx.color_mode)
    )
    .map_err(RagtagError::Io)?;

    Ok(())
}

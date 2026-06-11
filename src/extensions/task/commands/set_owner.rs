//! Task set-owner command.

use std::path::Path;

use super::super::config::TaskConfig;
use super::super::output::format_task_detail;
use super::create::escape_for_tag;
use super::{find_task_by_id, prompt_for_value};
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;

/// Runs the set-owner command.
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

    let path_str = matches
        .get_one::<String>("path")
        .map(|s| s.as_str())
        .unwrap_or(".");
    let path = Path::new(path_str);

    let (mut task, _) = find_task_by_id(id, path, config, ctx)?;

    let new_owner = if let Some(owner) = matches.get_one::<String>("owner") {
        owner.clone()
    } else {
        prompt_for_value(ctx, &task, config, "New owner: ")?
    };

    ctx.editor.update_tag_attribute(
        &task.location.file_path,
        task.raw_span.clone(),
        "owner",
        &format!("\"{}\"", escape_for_tag(&new_owner)),
    )?;

    task.owner = new_owner;

    writeln!(
        ctx.stdout,
        "{}",
        format_task_detail(&task, config, &ctx.color_mode)
    )
    .map_err(RagtagError::Io)?;

    Ok(())
}

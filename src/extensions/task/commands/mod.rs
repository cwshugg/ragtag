//! Task command dispatcher.
//!
//! Routes task sub-subcommands (create, list, set-status, etc.)
//! to their respective implementations.

pub mod create;
pub mod get;
pub mod list;
pub mod set_owner;
pub mod set_parent;
pub mod set_priority;
pub mod set_status;
pub mod set_time;
pub mod summary;

use std::path::Path;

use super::config::TaskConfig;
use super::models::TaskTag;
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;

/// Dispatches to the appropriate task subcommand.
pub fn dispatch(
    matches: &clap::ArgMatches,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(), RagtagError> {
    match matches.subcommand() {
        Some(("create", sub_m)) => create::run(sub_m, config, ctx),
        Some(("get", sub_m)) => get::run(sub_m, config, ctx),
        Some(("list", sub_m)) => list::run(sub_m, config, ctx),
        Some(("summary", sub_m)) => summary::run(sub_m, config, ctx),
        Some(("set-status", sub_m)) => set_status::run(sub_m, config, ctx),
        Some(("set-priority", sub_m)) => set_priority::run(sub_m, config, ctx),
        Some(("set-time", sub_m)) => set_time::run(sub_m, config, ctx),
        Some(("set-owner", sub_m)) => set_owner::run(sub_m, config, ctx),
        Some(("set-parent", sub_m)) => set_parent::run(sub_m, config, ctx),
        _ => Err(RagtagError::UnknownCommand(
            "unknown task subcommand".to_string(),
        )),
    }
}

/// Finds a task by ID (exact or prefix) across all discovered files.
///
/// Returns the task and the file content it was found in.
/// Errors if no task is found, or if multiple tasks match the prefix.
pub fn find_task_by_id(
    id: &str,
    path: &Path,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(TaskTag, String), RagtagError> {
    let files = ctx.walker.walk(path)?;
    let mut all_tasks: Vec<(TaskTag, String)> = Vec::new();

    for file_path in &files {
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("skipping unreadable file {}: {}", file_path.display(), e);
                continue;
            }
        };
        let tags = ctx.parser.parse_file(&content, file_path);
        for tag in &tags {
            if tag.name == config.tag_name {
                match TaskTag::from_tag(tag, config, &content) {
                    Ok(task) => all_tasks.push((task, content.clone())),
                    Err(e) => {
                        log::warn!(
                            "failed to parse task tag at {}:{}: {}",
                            file_path.display(),
                            tag.location.line,
                            e
                        );
                    }
                }
            }
        }
    }

    // Try exact match first
    let exact: Vec<(TaskTag, String)> = all_tasks
        .iter()
        .filter(|(t, _)| t.id == id)
        .cloned()
        .collect();
    if exact.len() == 1 {
        return Ok(exact.into_iter().next().expect("guaranteed by check"));
    }

    // Try prefix match
    let prefix: Vec<(TaskTag, String)> = all_tasks
        .into_iter()
        .filter(|(t, _)| t.id.starts_with(id))
        .collect();

    match prefix.len() {
        0 => Err(RagtagError::ExtensionError {
            extension_name: "Task Manager".to_string(),
            message: format!(
                "task not found with id \"{id}\"\nhint: run 'ragtag task list' to see all tasks"
            ),
        }),
        1 => Ok(prefix.into_iter().next().expect("guaranteed by match arm")),
        _ => {
            let mut details = format!(
                "Multiple tasks match id prefix \"{id}\". Be more specific:\n"
            );
            for (t, _) in &prefix {
                details.push_str(&format!(
                    "  {} {} {}\n",
                    t.id,
                    t.location.file_path.display(),
                    t.title
                ));
            }
            Err(RagtagError::ExtensionError {
                extension_name: "Task Manager".to_string(),
                message: details,
            })
        }
    }
}

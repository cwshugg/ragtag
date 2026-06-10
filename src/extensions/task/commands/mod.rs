//! Task command dispatcher.
//!
//! Routes task sub-subcommands (create, list, set-status, etc.)
//! to their respective implementations.

pub mod create;
pub mod list;
pub mod set_owner;
pub mod set_parent;
pub mod set_status;
pub mod set_time;

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
        Some(("list", sub_m)) => list::run(sub_m, config, ctx),
        Some(("set-status", sub_m)) => set_status::run(sub_m, config, ctx),
        Some(("set-time", sub_m)) => set_time::run(sub_m, config, ctx),
        Some(("set-owner", sub_m)) => set_owner::run(sub_m, config, ctx),
        Some(("set-parent", sub_m)) => set_parent::run(sub_m, config, ctx),
        _ => Err(RagtagError::UnknownCommand(
            "unknown task subcommand".to_string(),
        )),
    }
}

/// Finds a task by ID across all discovered files.
///
/// Returns the task and the file content it was found in.
/// Errors if no task is found, or if duplicate IDs are detected.
pub fn find_task_by_id(
    id: &str,
    path: &Path,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(TaskTag, String), RagtagError> {
    let files = ctx.walker.walk(path)?;
    let mut found: Vec<(TaskTag, String)> = Vec::new();

    for file_path in &files {
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let tags = ctx.parser.parse_file(&content, file_path);
        for tag in &tags {
            if tag.name == config.tag_name {
                if let Ok(task) = TaskTag::from_tag(tag, config, &content) {
                    if task.id == id {
                        found.push((task, content.clone()));
                    }
                }
            }
        }
    }

    match found.len() {
        0 => Err(RagtagError::ExtensionError {
            extension_name: "Task Manager".to_string(),
            message: format!(
                "task not found with id \"{id}\"\nhint: run 'ragtag tasks list' to see all tasks"
            ),
        }),
        1 => Ok(found.into_iter().next().unwrap()),
        _ => {
            let locations: Vec<String> = found
                .iter()
                .map(|(t, _)| format!("{}:{}", t.location.file_path.display(), t.location.line))
                .collect();
            Err(RagtagError::ExtensionError {
                extension_name: "Task Manager".to_string(),
                message: format!(
                    "multiple tasks found with id \"{}\" — found at: {}",
                    id,
                    locations.join(", ")
                ),
            })
        }
    }
}

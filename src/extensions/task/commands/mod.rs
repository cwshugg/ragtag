//! Task command dispatcher.
//!
//! Routes task sub-subcommands (create, list, get-attr, set-attr, etc.)
//! to their respective implementations.

pub mod create;
pub mod get;
pub mod get_attr;
pub mod list;
pub mod set_attr;
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
        Some(("get-attr", sub_m)) => get_attr::run(sub_m, config, ctx),
        Some(("set-attr", sub_m)) => set_attr::run(sub_m, config, ctx),
        _ => Err(RagtagError::UnknownCommand(
            "unknown task subcommand".to_string(),
        )),
    }
}

/// Collects all tasks from discovered files.
///
/// Walks the file tree, parses tags, and returns all valid `TaskTag` instances.
/// Invalid or unreadable files are skipped with a warning.
pub fn collect_tasks(
    path: &Path,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<Vec<TaskTag>, RagtagError> {
    let files = ctx.walker.walk(path)?;
    let mut tasks: Vec<TaskTag> = Vec::new();

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
                match TaskTag::from_tag(tag, config) {
                    Ok(task) => tasks.push(task),
                    Err(_) => continue,
                }
            }
        }
    }

    Ok(tasks)
}

/// Finds a task by ID (exact or prefix) across all discovered files.
///
/// Returns the task and the file content it was found in.
/// Errors if no task is found, or if multiple tasks match the prefix.
///
/// Title search is intentionally excluded here. Mutation commands
/// (`set-attr`) must operate by ID only for safety — matching by title
/// substring could inadvertently modify the wrong task when titles
/// are ambiguous. Read-only lookup by title is available via
/// `search_tasks` in the `get` module.
///
/// NOTE: This function intentionally duplicates the file-walking logic
/// from `collect_tasks`. The duplication is deliberate because
/// `find_task_by_id` tracks the source file path for each task and
/// performs a targeted file re-read when the match is found, whereas
/// `collect_tasks` only collects `TaskTag` values. The two functions
/// have different return types and ownership needs, so merging them
/// would add complexity without meaningful benefit.
pub fn find_task_by_id(
    id: &str,
    path: &Path,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(TaskTag, String), RagtagError> {
    let files = ctx.walker.walk(path)?;

    // Collect tasks with their source file path (not content) to avoid
    // cloning file content for every task in every file.
    let mut all_tasks: Vec<(TaskTag, std::path::PathBuf)> = Vec::new();

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
                match TaskTag::from_tag(tag, config) {
                    Ok(task) => all_tasks.push((task, file_path.clone())),
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
    let exact_idx: Vec<usize> = all_tasks
        .iter()
        .enumerate()
        .filter(|(_, (t, _))| t.id == id)
        .map(|(i, _)| i)
        .collect();
    if exact_idx.len() == 1 {
        let (task, file_path) = all_tasks
            .into_iter()
            .nth(exact_idx[0])
            .expect("guaranteed by check");
        // Re-read the file to return its content. This is a deliberate
        // double-read (TOCTOU) to avoid keeping all file contents in memory
        // during the initial scan. The file is assumed stable between reads,
        // which is reasonable for a single-user CLI tool.
        let content = std::fs::read_to_string(&file_path).map_err(RagtagError::Io)?;
        return Ok((task, content));
    }

    // Try prefix match
    let prefix_idx: Vec<usize> = all_tasks
        .iter()
        .enumerate()
        .filter(|(_, (t, _))| t.id.starts_with(id))
        .map(|(i, _)| i)
        .collect();

    match prefix_idx.len() {
        0 => Err(RagtagError::ExtensionError {
            extension_name: "Task Manager".to_string(),
            message: format!(
                "task not found with id \"{id}\"\nhint: run 'ragtag task list' to see all tasks"
            ),
        }),
        1 => {
            let (task, file_path) = all_tasks
                .into_iter()
                .nth(prefix_idx[0])
                .expect("guaranteed by match arm");
            // Re-read the file (same TOCTOU rationale as the exact-match
            // branch above — avoids holding all file contents in memory).
            let content = std::fs::read_to_string(&file_path).map_err(RagtagError::Io)?;
            Ok((task, content))
        }
        _ => {
            let mut details = format!(
                "Multiple tasks match id prefix \"{id}\". Please provide a longer ID string.\n"
            );
            for &i in &prefix_idx {
                let (ref t, _) = all_tasks[i];
                details.push_str(&format!(
                    "{} {} {}\n",
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

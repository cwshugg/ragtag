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

use std::borrow::Cow;
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

/// Validates that a filter expression is parseable.
///
/// A valid filter must contain one of the comparison operators:
/// `!=`, `>=`, `<=`, `>`, `<`, or `=`.
pub fn validate_task_filter(filter: &str) -> Result<(), RagtagError> {
    if filter.contains("!=")
        || filter.contains(">=")
        || filter.contains("<=")
        || filter.contains('>')
        || filter.contains('<')
        || filter.contains('=')
    {
        Ok(())
    } else {
        Err(RagtagError::InvalidFilter(format!(
            "\"{filter}\" — expected format: field=value, field!=value, field>value, etc."
        )))
    }
}

/// Applies a simple filter expression to a task.
///
/// Supports `=`, `!=`, `>`, `<`, `>=`, and `<=` operators.
/// Numeric fields are compared numerically; string fields are compared
/// lexicographically.
pub fn apply_task_filter(task: &TaskTag, filter: &str) -> bool {
    if let Some((field, value)) = filter.split_once("!=") {
        get_task_field_str(task, field.trim()) != value.trim()
    } else if let Some((field, value)) = filter.split_once(">=") {
        compare_field(task, field.trim(), value.trim(), |a, b| a >= b)
    } else if let Some((field, value)) = filter.split_once("<=") {
        compare_field(task, field.trim(), value.trim(), |a, b| a <= b)
    } else if let Some((field, value)) = filter.split_once('>') {
        compare_field(task, field.trim(), value.trim(), |a, b| a > b)
    } else if let Some((field, value)) = filter.split_once('<') {
        compare_field(task, field.trim(), value.trim(), |a, b| a < b)
    } else if let Some((field, value)) = filter.split_once('=') {
        get_task_field_str(task, field.trim()) == value.trim()
    } else {
        // Should not reach here since we validate above
        false
    }
}

/// Gets a task field as a string for comparison.
///
/// Returns an empty string for unrecognized field names and logs a warning.
pub fn get_task_field_str<'a>(task: &'a TaskTag, field: &str) -> Cow<'a, str> {
    match field {
        "id" => Cow::Borrowed(&task.id),
        "pid" => task
            .pid
            .as_deref()
            .map_or_else(|| Cow::Owned(String::new()), Cow::Borrowed),
        "title" => Cow::Borrowed(&task.title),
        "description" => task
            .description
            .as_deref()
            .map_or_else(|| Cow::Owned(String::new()), Cow::Borrowed),
        "owner" => Cow::Borrowed(&task.owner),
        "status" => Cow::Borrowed(&task.status),
        // `None` values produce an empty string, which sorts before any numeric
        // string in lexicographic fallback (e.g., "" < "0" < "1"). This means
        // tasks without a priority sort first when sorting by priority.
        "priority" => Cow::Owned(task.priority.map(|p| p.to_string()).unwrap_or_default()),
        // Same empty-string-for-None behavior applies to time fields.
        "time_spent" => Cow::Owned(task.time_spent.map(|t| t.to_string()).unwrap_or_default()),
        "ttc_estimate" => Cow::Owned(task.ttc_estimate.map(|t| t.to_string()).unwrap_or_default()),
        "ttc_actual" => Cow::Owned(task.ttc_actual.map(|t| t.to_string()).unwrap_or_default()),
        "time_units" => Cow::Borrowed(&task.time_units),
        _ => {
            log::warn!("unknown task field \"{field}\" in filter/sort expression");
            Cow::Owned(String::new())
        }
    }
}

/// Compares a task field numerically if possible, otherwise lexicographically.
fn compare_field(task: &TaskTag, field: &str, value: &str, cmp: fn(f64, f64) -> bool) -> bool {
    let field_str = get_task_field_str(task, field);
    if let (Ok(a), Ok(b)) = (field_str.parse::<f64>(), value.parse::<f64>()) {
        cmp(a, b)
    } else {
        // Fall back to lexicographic string comparison for non-numeric values.
        let ordering = (*field_str).cmp(value);
        match ordering {
            std::cmp::Ordering::Less => cmp(-1.0, 0.0),
            std::cmp::Ordering::Equal => cmp(0.0, 0.0),
            std::cmp::Ordering::Greater => cmp(1.0, 0.0),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TagLocation;
    use std::path::PathBuf;

    fn make_task(id: &str, status: &str, owner: &str, priority: Option<u32>) -> TaskTag {
        TaskTag {
            id: id.to_string(),
            pid: None,
            title: format!("Task {id}"),
            description: None,
            owner: owner.to_string(),
            status: status.to_string(),
            priority,
            time_spent: None,
            ttc_estimate: Some(4.0),
            ttc_actual: None,
            time_units: "hours".to_string(),
            location: TagLocation::new(PathBuf::from("test.md"), 1, 1, 0, 50),
            raw_span: 0..50,
        }
    }

    #[test]
    fn test_validate_task_filter_valid_expressions() {
        assert!(validate_task_filter("status=active").is_ok());
        assert!(validate_task_filter("status!=done").is_ok());
        assert!(validate_task_filter("priority>0").is_ok());
        assert!(validate_task_filter("priority<5").is_ok());
        assert!(validate_task_filter("priority>=1").is_ok());
        assert!(validate_task_filter("priority<=3").is_ok());
    }

    #[test]
    fn test_validate_task_filter_invalid_expression() {
        assert!(validate_task_filter("nooperator").is_err());
    }

    #[test]
    fn test_apply_task_filter_equality() {
        let task = make_task("abc", "active", "alice", Some(1));
        assert!(apply_task_filter(&task, "status=active"));
        assert!(!apply_task_filter(&task, "status=done"));
        assert!(apply_task_filter(&task, "owner=alice"));
        assert!(!apply_task_filter(&task, "owner=bob"));
    }

    #[test]
    fn test_apply_task_filter_not_equal() {
        let task = make_task("abc", "active", "alice", Some(1));
        assert!(apply_task_filter(&task, "status!=done"));
        assert!(!apply_task_filter(&task, "status!=active"));
    }

    #[test]
    fn test_apply_task_filter_numeric_comparison() {
        let task = make_task("abc", "active", "alice", Some(2));
        assert!(apply_task_filter(&task, "priority>1"));
        assert!(!apply_task_filter(&task, "priority>2"));
        assert!(apply_task_filter(&task, "priority>=2"));
        assert!(apply_task_filter(&task, "priority<3"));
        assert!(!apply_task_filter(&task, "priority<2"));
        assert!(apply_task_filter(&task, "priority<=2"));
    }

    #[test]
    fn test_get_task_field_str_all_fields() {
        let task = make_task("abc123", "active", "alice", Some(1));
        assert_eq!(&*get_task_field_str(&task, "id"), "abc123");
        assert_eq!(&*get_task_field_str(&task, "status"), "active");
        assert_eq!(&*get_task_field_str(&task, "owner"), "alice");
        assert_eq!(&*get_task_field_str(&task, "priority"), "1");
        assert_eq!(&*get_task_field_str(&task, "time_units"), "hours");
    }

    #[test]
    fn test_get_task_field_str_none_priority() {
        let task = make_task("abc123", "active", "alice", None);
        assert_eq!(&*get_task_field_str(&task, "priority"), "");
    }

    #[test]
    fn test_get_task_field_str_unknown_field() {
        let task = make_task("abc123", "active", "alice", Some(1));
        assert_eq!(&*get_task_field_str(&task, "nonexistent"), "");
    }
}

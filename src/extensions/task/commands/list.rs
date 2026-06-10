//! Task list command.
//!
//! Discovers and displays all tasks matching filters, with configurable
//! attribute display and sorting.

use std::path::Path;

use super::super::config::TaskConfig;
use super::super::models::TaskTag;
use super::super::output::format_task_line;
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;

/// Runs the list command.
pub fn run(
    matches: &clap::ArgMatches,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(), RagtagError> {
    let path_str = matches
        .get_one::<String>("path")
        .map(|s| s.as_str())
        .unwrap_or(".");
    let path = Path::new(path_str);

    let sort_field = matches.get_one::<String>("sort").cloned();
    let reverse = matches.get_flag("reverse");

    let filters: Vec<String> = matches
        .get_many::<String>("filter")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();

    // Discover files
    let files = ctx.walker.walk(path)?;

    // Parse and collect tasks
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
                match TaskTag::from_tag(tag, config, &content) {
                    Ok(task) => tasks.push(task),
                    Err(_) => continue,
                }
            }
        }
    }

    // Apply filters
    if !filters.is_empty() {
        // Validate all filters before applying
        for f in &filters {
            validate_task_filter(f)?;
        }
        tasks.retain(|task| filters.iter().all(|f| apply_task_filter(task, f)));
    }

    // Sort (default: by priority)
    let effective_sort = sort_field.as_deref().unwrap_or("priority");
    sort_tasks(&mut tasks, effective_sort, reverse);

    // Output
    for task in &tasks {
        let line = format_task_line(task, &ctx.color_mode, config);
        writeln!(ctx.stdout, "{line}").map_err(RagtagError::Io)?;
    }

    Ok(())
}

/// Validates that a filter expression is parseable.
fn validate_task_filter(filter: &str) -> Result<(), RagtagError> {
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
fn apply_task_filter(task: &TaskTag, filter: &str) -> bool {
    // Parse filter: field=value, field!=value, field>value, field<value
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
///
/// TODO: Return `Cow<'_, str>` or `&str` instead of `String` to avoid
/// unnecessary allocations on every call during filtering and sorting.
fn get_task_field_str(task: &TaskTag, field: &str) -> String {
    match field {
        "id" => task.id.clone(),
        "pid" => task.pid.clone().unwrap_or_default(),
        "title" => task.title.clone(),
        "description" => task.description.clone().unwrap_or_default(),
        "owner" => task.owner.clone(),
        "status" => task.status.clone(),
        "priority" => task.priority.map(|p| p.to_string()).unwrap_or_default(),
        "time_spent" => task.time_spent.map(|t| t.to_string()).unwrap_or_default(),
        "ttc_estimate" => task.ttc_estimate.to_string(),
        "ttc_actual" => task.ttc_actual.map(|t| t.to_string()).unwrap_or_default(),
        "time_units" => task.time_units.clone(),
        _ => {
            log::warn!("unknown task field \"{field}\" in filter/sort expression");
            String::new()
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
        let ordering = field_str.as_str().cmp(value);
        match ordering {
            std::cmp::Ordering::Less => cmp(-1.0, 0.0),
            std::cmp::Ordering::Equal => cmp(0.0, 0.0),
            std::cmp::Ordering::Greater => cmp(1.0, 0.0),
        }
    }
}

/// Sorts tasks by a field name.
pub fn sort_tasks(tasks: &mut [TaskTag], field: &str, reverse: bool) {
    tasks.sort_by(|a, b| {
        let va = get_task_field_str(a, field);
        let vb = get_task_field_str(b, field);

        // Try numeric comparison first
        let ordering = if let (Ok(na), Ok(nb)) = (va.parse::<f64>(), vb.parse::<f64>()) {
            na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal)
        } else {
            va.cmp(&vb)
        };

        if reverse {
            ordering.reverse()
        } else {
            ordering
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TagLocation;
    use std::path::PathBuf;

    fn make_task(id: &str, status: &str, priority: Option<u32>, title: &str) -> TaskTag {
        TaskTag {
            id: id.to_string(),
            pid: None,
            title: title.to_string(),
            description: None,
            owner: "me".to_string(),
            status: status.to_string(),
            priority,
            time_spent: None,
            ttc_estimate: 4.0,
            ttc_actual: None,
            time_units: "hours".to_string(),
            location: TagLocation::new(PathBuf::from("test.md"), 1, 1, 0, 50),
            raw_span: 0..50,
            trailing_text: None,
        }
    }

    #[test]
    fn test_filter_by_status() {
        let task = make_task("abc", "active", None, "Test");
        assert!(apply_task_filter(&task, "status=active"));
        assert!(!apply_task_filter(&task, "status=done"));
    }

    #[test]
    fn test_filter_not_equal() {
        let task = make_task("abc", "active", None, "Test");
        assert!(apply_task_filter(&task, "status!=done"));
        assert!(!apply_task_filter(&task, "status!=active"));
    }

    #[test]
    fn test_sort_by_title() {
        let mut tasks = vec![
            make_task("b", "active", None, "Banana"),
            make_task("a", "active", None, "Apple"),
        ];
        sort_tasks(&mut tasks, "title", false);
        assert_eq!(tasks[0].title, "Apple");
        assert_eq!(tasks[1].title, "Banana");
    }

    #[test]
    fn test_sort_by_priority() {
        let mut tasks = vec![
            make_task("a", "active", Some(2), "A"),
            make_task("b", "active", Some(0), "B"),
        ];
        sort_tasks(&mut tasks, "priority", false);
        assert_eq!(tasks[0].priority, Some(0));
    }

    #[test]
    fn test_sort_reverse() {
        let mut tasks = vec![
            make_task("a", "active", None, "Apple"),
            make_task("b", "active", None, "Banana"),
        ];
        sort_tasks(&mut tasks, "title", true);
        assert_eq!(tasks[0].title, "Banana");
    }

    #[test]
    fn test_validate_task_filter_valid() {
        assert!(validate_task_filter("status=active").is_ok());
        assert!(validate_task_filter("priority>0").is_ok());
        assert!(validate_task_filter("status!=done").is_ok());
    }

    #[test]
    fn test_validate_task_filter_invalid() {
        assert!(validate_task_filter("statusinvalid").is_err());
    }
}

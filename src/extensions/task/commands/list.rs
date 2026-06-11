//! Task list command.
//!
//! Discovers and displays all tasks matching filters, with configurable
//! attribute display and sorting.

use std::path::Path;

use super::super::config::TaskConfig;
use super::super::models::TaskTag;
use super::super::output::format_task_line;
use super::{collect_tasks, get_task_field_str};
use crate::cli;
use crate::error::RagtagError;
use crate::extensions::task::filter::{evaluate_filter, parse_filter_expr, validate_filter_expr};
use crate::extensions::ExtensionContext;

/// Runs the list command.
pub fn run(
    matches: &clap::ArgMatches,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(), RagtagError> {
    let path_str = cli::resolve_path(matches);
    let path = Path::new(&path_str);

    let sort_field = matches.get_one::<String>("sort").cloned();
    let reverse = matches.get_flag("reverse");

    let filter_expr_str = matches.get_one::<String>("filter").cloned();

    // Discover and parse tasks
    let mut tasks = collect_tasks(path, config, ctx)?;

    // Apply filter expression
    if let Some(ref expr_str) = filter_expr_str {
        let parsed = parse_filter_expr(expr_str)?;
        validate_filter_expr(&parsed)?;
        tasks.retain(|task| evaluate_filter(&parsed, task));
    }

    // Apply default status exclusion (exclude done/abandoned by default)
    let show_all = matches.get_flag("all");
    let filter_mentions_status = filter_expr_str
        .as_ref()
        .is_some_and(|e| e.contains("status"));
    if !show_all && !filter_mentions_status {
        let excluded = config.get_excluded_keywords();
        tasks.retain(|t| !excluded.contains(&t.status));
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
    use crate::extensions::task::commands::{apply_task_filter, validate_task_filter};
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
            ttc_estimate: Some(4.0),
            ttc_actual: None,
            time_units: "hours".to_string(),
            location: TagLocation::new(PathBuf::from("test.md"), 1, 1, 0, 50),
            raw_span: 0..50,
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

    #[test]
    fn test_done_tasks_excluded_by_default() {
        let config = TaskConfig::default();
        let mut tasks = vec![
            make_task("a", "active", Some(1), "Active task"),
            make_task("b", "done", Some(2), "Done task"),
            make_task("c", "abandoned", Some(3), "Abandoned task"),
            make_task("d", "blocked", Some(4), "Blocked task"),
        ];

        // Simulate default exclusion (no --all, no status filter)
        let excluded = config.get_excluded_keywords();
        tasks.retain(|t| !excluded.contains(&t.status));

        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, "a");
        assert_eq!(tasks[1].id, "d");
    }

    #[test]
    fn test_all_flag_shows_everything() {
        let config = TaskConfig::default();
        let tasks = vec![
            make_task("a", "active", Some(1), "Active task"),
            make_task("b", "done", Some(2), "Done task"),
            make_task("c", "abandoned", Some(3), "Abandoned task"),
            make_task("d", "blocked", Some(4), "Blocked task"),
        ];

        // With --all, no exclusion is applied
        let show_all = true;
        let filter_mentions_status = false;
        let mut filtered = tasks.clone();
        if !show_all && !filter_mentions_status {
            let excluded = config.get_excluded_keywords();
            filtered.retain(|t| !excluded.contains(&t.status));
        }

        assert_eq!(filtered.len(), 4);
    }

    #[test]
    fn test_status_filter_overrides_exclusion() {
        use crate::extensions::task::filter::{evaluate_filter, parse_filter_expr};

        let config = TaskConfig::default();
        let tasks = vec![
            make_task("a", "active", Some(1), "Active task"),
            make_task("b", "done", Some(2), "Done task"),
            make_task("c", "abandoned", Some(3), "Abandoned task"),
        ];

        // When filter mentions status, exclusion is disabled
        let show_all = false;
        let filter_expr_str = Some("status=done".to_string());
        let filter_mentions_status = filter_expr_str
            .as_ref()
            .is_some_and(|e| e.contains("status"));

        let mut filtered = tasks.clone();
        if !show_all && !filter_mentions_status {
            let excluded = config.get_excluded_keywords();
            filtered.retain(|t| !excluded.contains(&t.status));
        }

        // Apply the explicit filter expression
        if let Some(ref expr_str) = filter_expr_str {
            let parsed = parse_filter_expr(expr_str).unwrap();
            filtered.retain(|task| evaluate_filter(&parsed, task));
        }

        // Should show only the "done" task
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].status, "done");
    }
}

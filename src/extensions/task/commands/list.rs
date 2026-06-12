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

    // Determine output format
    let format = matches
        .get_one::<String>("format")
        .map(|s| s.as_str())
        .unwrap_or("default");

    // Output
    match format {
        "raw" => {
            for (i, task) in tasks.iter().enumerate() {
                if i > 0 {
                    writeln!(ctx.stdout).map_err(RagtagError::Io)?;
                }
                format_task_raw(task, ctx)?;
            }
        }
        _ => {
            for task in &tasks {
                let line = format_task_line(task, &ctx.color_mode, config);
                writeln!(ctx.stdout, "{line}").map_err(RagtagError::Io)?;
            }
        }
    }

    Ok(())
}

/// Outputs a task in raw key=value format for machine consumption.
///
/// Each attribute is on its own line. No color codes are applied.
fn format_task_raw(task: &TaskTag, ctx: &mut ExtensionContext) -> Result<(), RagtagError> {
    let file_path = task.location.file_path.display();
    let line_num = task.location.line;

    writeln!(ctx.stdout, "id={}", task.id).map_err(RagtagError::Io)?;
    writeln!(ctx.stdout, "title={}", task.title).map_err(RagtagError::Io)?;
    writeln!(ctx.stdout, "owner={}", task.owner).map_err(RagtagError::Io)?;
    writeln!(ctx.stdout, "status={}", task.status).map_err(RagtagError::Io)?;
    writeln!(
        ctx.stdout,
        "priority={}",
        task.priority.map(|p| p.to_string()).unwrap_or_default()
    )
    .map_err(RagtagError::Io)?;
    writeln!(
        ctx.stdout,
        "description={}",
        task.description.as_deref().unwrap_or("")
    )
    .map_err(RagtagError::Io)?;
    writeln!(ctx.stdout, "file={file_path}").map_err(RagtagError::Io)?;
    writeln!(ctx.stdout, "line={line_num}").map_err(RagtagError::Io)?;
    writeln!(
        ctx.stdout,
        "worktime_spent={}",
        task.worktime_spent
            .map(|t| t.to_string())
            .unwrap_or_default()
    )
    .map_err(RagtagError::Io)?;
    writeln!(
        ctx.stdout,
        "worktime_estimate={}",
        task.worktime_estimate
            .map(|t| t.to_string())
            .unwrap_or_default()
    )
    .map_err(RagtagError::Io)?;
    writeln!(
        ctx.stdout,
        "time_created={}",
        task.time_created.as_deref().unwrap_or("")
    )
    .map_err(RagtagError::Io)?;
    writeln!(
        ctx.stdout,
        "time_last_updated={}",
        task.time_last_updated.as_deref().unwrap_or("")
    )
    .map_err(RagtagError::Io)?;
    writeln!(ctx.stdout, "worktime_units={}", task.worktime_units).map_err(RagtagError::Io)?;
    writeln!(ctx.stdout, "pid={}", task.pid.as_deref().unwrap_or("")).map_err(RagtagError::Io)?;
    Ok(())
}

/// Sorts tasks by a field name. The special value `"appearance"` sorts by
/// file path then line number, preserving the order tasks appear in files.
pub fn sort_tasks(tasks: &mut [TaskTag], field: &str, reverse: bool) {
    tasks.sort_by(|a, b| {
        let ordering = if field == "appearance" {
            a.location
                .file_path
                .cmp(&b.location.file_path)
                .then_with(|| a.location.line.cmp(&b.location.line))
        } else {
            let va = get_task_field_str(a, field);
            let vb = get_task_field_str(b, field);

            // Try numeric comparison first
            if let (Ok(na), Ok(nb)) = (va.parse::<f64>(), vb.parse::<f64>()) {
                na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                va.cmp(&vb)
            }
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
            worktime_spent: None,
            worktime_estimate: Some(4.0),
            time_created: None,
            time_last_updated: None,
            worktime_units: "hours".to_string(),
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

    #[test]
    fn test_format_task_raw_output() {
        use crate::config::{ColorMode, Config};
        use crate::discovery::FileWalker;
        use crate::edit::FileEditor;
        use crate::extensions::{DefaultTagParser, ExtensionContext};
        use std::ops::Range;

        // Minimal stub implementations for ExtensionContext dependencies.
        struct StubWalker;
        impl FileWalker for StubWalker {
            fn walk(&self, _path: &Path) -> Result<Vec<PathBuf>, RagtagError> {
                Ok(vec![])
            }
        }

        struct StubEditor;
        impl FileEditor for StubEditor {
            fn update_tag_attribute(
                &self,
                _file_path: &Path,
                _tag_span: Range<usize>,
                _attr_name: &str,
                _new_value: &str,
            ) -> Result<(), RagtagError> {
                Ok(())
            }
        }

        let walker = StubWalker;
        let parser = DefaultTagParser;
        let editor = StubEditor;
        let config = Config::default();
        let mut output = Vec::new();
        let mut stderr = Vec::new();

        let mut ctx = ExtensionContext {
            walker: &walker,
            parser: &parser,
            editor: &editor,
            color_mode: ColorMode::Never,
            config: &config,
            stdout: &mut output,
            stderr: &mut stderr,
        };

        let task = make_task("abc123", "active", Some(1), "Test task");
        format_task_raw(&task, &mut ctx).unwrap();

        let result = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = result.lines().collect();

        assert_eq!(lines[0], "id=abc123");
        assert_eq!(lines[1], "title=Test task");
        assert_eq!(lines[2], "owner=me");
        assert_eq!(lines[3], "status=active");
        assert_eq!(lines[4], "priority=1");
        assert_eq!(lines[5], "description=");
        assert_eq!(lines[6], "file=test.md");
        assert_eq!(lines[7], "line=1");
        assert_eq!(lines[8], "worktime_spent=");
        assert_eq!(lines[9], "worktime_estimate=4");
        assert_eq!(lines[10], "time_created=");
        assert_eq!(lines[11], "time_last_updated=");
        assert_eq!(lines[12], "worktime_units=hours");
        assert_eq!(lines[13], "pid=");
        assert_eq!(lines.len(), 14);
    }
}

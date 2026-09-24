//! Task list command.
//!
//! Discovers and displays all tasks matching filters, with configurable
//! attribute display and sorting.

use std::path::Path;

use serde::Serialize;

use super::super::config::TaskConfig;
use super::super::models::TaskTag;
use super::super::output::format_task_line;
use super::{select_tasks, task_field_value};
use crate::cli;
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;

/// Runs the list command.
pub fn run(
    matches: &clap::ArgMatches,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(), RagtagError> {
    let path_str = cli::resolve_path(matches);
    let path = Path::new(&path_str);

    if matches.get_flag("discover-files-jsonl") {
        return format_discovered_sources(path, ctx);
    }

    let sort_field = matches.get_one::<String>("sort").cloned();
    let reverse = matches.get_flag("reverse");

    let filter_expr = matches.get_one::<String>("filter").map(String::as_str);
    let mut tasks = select_tasks(path, filter_expr, matches.get_flag("all"), config, ctx)?;

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
        "jsonl" => {
            for task in &tasks {
                format_task_jsonl(task, &config.tag_name, ctx)?;
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

/// One file selected by the configured Ragtag walker.
#[derive(Serialize)]
struct TaskSourceDiscovery<'a> {
    file: &'a Path,
}

/// Emits the exact bounded source set selected by Ragtag discovery.
fn format_discovered_sources(path: &Path, ctx: &mut ExtensionContext) -> Result<(), RagtagError> {
    for file in ctx.walker.walk(path)? {
        let record = TaskSourceDiscovery { file: &file };
        let encoded = serde_json::to_vec(&record)
            .map_err(|error| RagtagError::Io(std::io::Error::other(error)))?;
        ctx.stdout.write_all(&encoded).map_err(RagtagError::Io)?;
        writeln!(ctx.stdout).map_err(RagtagError::Io)?;
    }
    Ok(())
}

/// Exact source identity for a task occurrence in the scanned snapshot.
#[derive(Serialize)]
struct TaskSource<'a> {
    tag_name: &'a str,
    file: &'a Path,
    line: usize,
    column: usize,
    byte_start: usize,
    byte_end: usize,
}

/// Stable, normalized machine-readable task-list record.
#[derive(Serialize)]
struct TaskJsonlRecord<'a> {
    id: &'a str,
    pid: Option<&'a str>,
    title: &'a str,
    description: Option<&'a str>,
    owner: &'a str,
    status: &'a str,
    #[serde(rename = "type")]
    task_type: &'a str,
    priority: Option<u32>,
    worktime_spent: Option<f64>,
    worktime_estimate: Option<f64>,
    time_created: Option<&'a str>,
    time_last_updated: Option<&'a str>,
    worktime_units: &'a str,
    source: TaskSource<'a>,
}

/// Outputs one self-contained JSON object followed by one newline.
fn format_task_jsonl(
    task: &TaskTag,
    tag_name: &str,
    ctx: &mut ExtensionContext,
) -> Result<(), RagtagError> {
    let source_range = task.location.byte_range();
    let record = TaskJsonlRecord {
        id: &task.id,
        pid: task.pid.as_deref(),
        title: &task.title,
        description: task.description.as_deref(),
        owner: &task.owner,
        status: &task.status,
        task_type: task.task_type.as_str(),
        priority: task.priority,
        worktime_spent: task.worktime_spent,
        worktime_estimate: task.worktime_estimate,
        time_created: task.time_created.as_deref(),
        time_last_updated: task.time_last_updated.as_deref(),
        worktime_units: &task.worktime_units,
        source: TaskSource {
            tag_name,
            file: &task.location.file_path,
            line: task.location.line,
            column: task.location.column,
            byte_start: source_range.start,
            byte_end: source_range.end,
        },
    };

    // Serialize fully before writing so an unsupported value (such as a
    // non-UTF-8 path) cannot leave a partial JSON record on stdout.
    let encoded = serde_json::to_vec(&record)
        .map_err(|error| RagtagError::Io(std::io::Error::other(error)))?;
    ctx.stdout.write_all(&encoded).map_err(RagtagError::Io)?;
    writeln!(ctx.stdout).map_err(RagtagError::Io)
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
    writeln!(ctx.stdout, "type={}", task.task_type).map_err(RagtagError::Io)?;
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
            let va = task_field_value(a, field).unwrap_or_else(|| {
                log::warn!("sort expression references an unknown task field");
                std::borrow::Cow::Borrowed("")
            });
            let vb = task_field_value(b, field).unwrap_or_else(|| {
                log::warn!("sort expression references an unknown task field");
                std::borrow::Cow::Borrowed("")
            });

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
            task_type: crate::extensions::task::models::TaskType::Item,
            priority,
            worktime_spent: None,
            worktime_estimate: Some(4.0),
            time_created: None,
            time_last_updated: None,
            worktime_units: "hours".to_string(),
            location: TagLocation::new(PathBuf::from("test.md"), 1, 1, 0, 50),
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
        assert_eq!(lines[4], "type=item");
        assert_eq!(lines[5], "priority=1");
        assert_eq!(lines[6], "description=");
        assert_eq!(lines[7], "file=test.md");
        assert_eq!(lines[8], "line=1");
        assert_eq!(lines[9], "worktime_spent=");
        assert_eq!(lines[10], "worktime_estimate=4");
        assert_eq!(lines[11], "time_created=");
        assert_eq!(lines[12], "time_last_updated=");
        assert_eq!(lines[13], "worktime_units=hours");
        assert_eq!(lines[14], "pid=");
        assert_eq!(lines.len(), 15);
    }
}

//! Task summary command.
//!
//! Produces a table-like display of tasks grouped by a field
//! (status, owner, priority) with aligned columns and color-coded
//! status and priority values.

use std::collections::BTreeMap;
use std::path::Path;

use super::super::config::TaskConfig;
use super::super::models::TaskTag;
use super::super::output::{colorize_priority, colorize_status};
use super::list::sort_tasks;
use crate::config::ColorMode;
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;

/// Column headers for the summary table.
const HEADERS: &[&str] = &[
    "ID",
    "Title",
    "Owner",
    "Status",
    "Priority",
    "Time Spent",
    "TTC Est.",
    "TTC Act.",
    "Time Units",
];

/// Runs the summary command.
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

    let group_by = matches
        .get_one::<String>("group")
        .map(|s| s.as_str())
        .unwrap_or("status");

    let sort_by = matches.get_one::<String>("sort").cloned();

    // Discover and parse tasks
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
                match TaskTag::from_tag(tag, config, &content) {
                    Ok(task) => tasks.push(task),
                    Err(_) => continue,
                }
            }
        }
    }

    // Sort within groups
    if let Some(ref field) = sort_by {
        sort_tasks(&mut tasks, field, false);
    }

    // Group tasks
    let groups = group_tasks(&tasks, group_by);

    // Render output
    let output = format_summary_table(&groups, group_by, config, &ctx.color_mode);
    write!(ctx.stdout, "{output}").map_err(RagtagError::Io)?;

    Ok(())
}

/// Groups tasks by the specified field into an ordered map.
///
/// Returns a `BTreeMap` so groups are displayed in sorted order.
fn group_tasks<'a>(tasks: &'a [TaskTag], group_by: &str) -> BTreeMap<String, Vec<&'a TaskTag>> {
    let mut groups: BTreeMap<String, Vec<&TaskTag>> = BTreeMap::new();
    for task in tasks {
        let key = get_group_key(task, group_by);
        groups.entry(key).or_default().push(task);
    }
    groups
}

/// Extracts the grouping key from a task.
fn get_group_key(task: &TaskTag, group_by: &str) -> String {
    match group_by {
        "status" => task.status.clone(),
        "owner" => task.owner.clone(),
        "priority" => task
            .priority
            .map(|p| p.to_string())
            .unwrap_or_else(|| "(none)".to_string()),
        _ => task.status.clone(),
    }
}

/// Maximum display width for the title column. Titles longer than this are
/// truncated with a trailing "...".
const MAX_TITLE_WIDTH: usize = 40;

/// Truncates a string to `max_len` characters, appending "..." if truncated.
fn truncate_title(title: &str, max_len: usize) -> String {
    if title.chars().count() <= max_len {
        title.to_string()
    } else {
        let truncated: String = title.chars().take(max_len.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}

/// Formats the complete summary table output with group headers.
///
/// Column widths are computed globally across all groups so that every
/// table has the same width.
fn format_summary_table(
    groups: &BTreeMap<String, Vec<&TaskTag>>,
    group_by: &str,
    config: &TaskConfig,
    color_mode: &ColorMode,
) -> String {
    if groups.is_empty() {
        return "No tasks found.\n".to_string();
    }

    /// A pair of (plain_text, colored_text) cell values for one row.
    type RowPair = (Vec<String>, Vec<String>);

    // Build rows for all groups and compute column widths globally.
    let mut all_group_rows: Vec<(&String, Vec<RowPair>)> = Vec::new();
    let mut global_widths: Vec<usize> = HEADERS.iter().map(|h| h.len()).collect();

    for (key, tasks) in groups {
        let rows = build_rows(tasks, config, color_mode);
        for (plain, _) in &rows {
            for (i, val) in plain.iter().enumerate() {
                if i < global_widths.len() && val.len() > global_widths[i] {
                    global_widths[i] = val.len();
                }
            }
        }
        all_group_rows.push((key, rows));
    }

    let mut output = String::new();
    let mut first = true;

    for (key, rows) in &all_group_rows {
        if !first {
            output.push('\n');
        }
        first = false;

        // Group header
        output.push_str(&format!("{}: {}\n", capitalize(group_by), key));

        // Header row
        let header_line = format_row(HEADERS, &global_widths);
        output.push_str(&header_line);
        output.push('\n');

        // Separator
        let sep: Vec<String> = global_widths.iter().map(|w| "-".repeat(*w)).collect();
        let sep_strs: Vec<&str> = sep.iter().map(|s| s.as_str()).collect();
        output.push_str(&format_row(&sep_strs, &global_widths));
        output.push('\n');

        // Data rows
        for (plain_row, color_row) in rows {
            let line = format_colored_row(plain_row, color_row, &global_widths);
            output.push_str(&line);
            output.push('\n');
        }
    }

    output
}

/// Builds rows of (plain_values, colored_values) for width calculation and display.
///
/// Plain values are used for column width computation (no ANSI codes).
/// Colored values are used for actual display output.
fn build_rows(
    tasks: &[&TaskTag],
    config: &TaskConfig,
    color_mode: &ColorMode,
) -> Vec<(Vec<String>, Vec<String>)> {
    tasks
        .iter()
        .map(|task| {
            let title = truncate_title(&task.title, MAX_TITLE_WIDTH);
            let plain = vec![
                task.id.clone(),
                title.clone(),
                task.owner.clone(),
                task.status.clone(),
                task.priority
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                task.time_spent
                    .map(format_float)
                    .unwrap_or_else(|| "-".to_string()),
                format_float(task.ttc_estimate),
                task.ttc_actual
                    .map(format_float)
                    .unwrap_or_else(|| "-".to_string()),
                task.time_units.clone(),
            ];

            let colored = vec![
                task.id.clone(),
                title,
                task.owner.clone(),
                colorize_status(&task.status, &config.status_keywords, color_mode),
                task.priority
                    .map(|p| colorize_priority(p, color_mode))
                    .unwrap_or_else(|| "-".to_string()),
                task.time_spent
                    .map(format_float)
                    .unwrap_or_else(|| "-".to_string()),
                format_float(task.ttc_estimate),
                task.ttc_actual
                    .map(format_float)
                    .unwrap_or_else(|| "-".to_string()),
                task.time_units.clone(),
            ];

            (plain, colored)
        })
        .collect()
}

/// Formats a row of plain string slices with padding.
fn format_row(values: &[&str], widths: &[usize]) -> String {
    values
        .iter()
        .zip(widths.iter())
        .map(|(val, width)| format!("{val:<width$}"))
        .collect::<Vec<_>>()
        .join("  ")
}

/// Formats a row where some cells may contain ANSI color codes.
///
/// Uses `plain` values to determine padding widths, then applies the
/// padding to `colored` values (which may contain invisible ANSI bytes).
fn format_colored_row(plain: &[String], colored: &[String], widths: &[usize]) -> String {
    plain
        .iter()
        .zip(colored.iter())
        .zip(widths.iter())
        .map(|((p, c), width)| {
            let visible_len = p.len();
            if visible_len >= *width {
                c.clone()
            } else {
                let padding = width - visible_len;
                format!("{c}{}", " ".repeat(padding))
            }
        })
        .collect::<Vec<_>>()
        .join("  ")
}

/// Formats a float, removing trailing zeros for cleaner display.
fn format_float(val: f64) -> String {
    if val.fract() == 0.0 {
        format!("{}", val as i64)
    } else {
        format!("{val}")
    }
}

/// Capitalizes the first letter of a string.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TagLocation;
    use std::path::PathBuf;

    fn make_task(
        id: &str,
        title: &str,
        owner: &str,
        status: &str,
        priority: Option<u32>,
        time_spent: Option<f64>,
        ttc_estimate: f64,
        ttc_actual: Option<f64>,
    ) -> TaskTag {
        TaskTag {
            id: id.to_string(),
            pid: None,
            title: title.to_string(),
            description: None,
            owner: owner.to_string(),
            status: status.to_string(),
            priority,
            time_spent,
            ttc_estimate,
            ttc_actual,
            time_units: "hours".to_string(),
            location: TagLocation::new(PathBuf::from("test.md"), 1, 1, 0, 50),
            raw_span: 0..50,
            trailing_text: None,
        }
    }

    fn sample_tasks() -> Vec<TaskTag> {
        vec![
            make_task(
                "aaa1",
                "Task A",
                "alice",
                "active",
                Some(1),
                Some(2.0),
                8.0,
                None,
            ),
            make_task(
                "bbb2",
                "Task B",
                "bob",
                "done",
                Some(2),
                Some(4.0),
                4.0,
                Some(5.0),
            ),
            make_task(
                "ccc3",
                "Task C",
                "alice",
                "active",
                Some(0),
                None,
                6.0,
                None,
            ),
            make_task("ddd4", "Task D", "bob", "blocked", None, None, 10.0, None),
        ]
    }

    #[test]
    fn test_group_by_status() {
        let tasks = sample_tasks();
        let groups = group_tasks(&tasks, "status");
        assert_eq!(groups.len(), 3); // active, blocked, done
        assert!(groups.contains_key("active"));
        assert!(groups.contains_key("done"));
        assert!(groups.contains_key("blocked"));
        assert_eq!(groups["active"].len(), 2);
        assert_eq!(groups["done"].len(), 1);
        assert_eq!(groups["blocked"].len(), 1);
    }

    #[test]
    fn test_group_by_owner() {
        let tasks = sample_tasks();
        let groups = group_tasks(&tasks, "owner");
        assert_eq!(groups.len(), 2); // alice, bob
        assert_eq!(groups["alice"].len(), 2);
        assert_eq!(groups["bob"].len(), 2);
    }

    #[test]
    fn test_group_by_priority() {
        let tasks = sample_tasks();
        let groups = group_tasks(&tasks, "priority");
        // 0, 1, 2, (none)
        assert!(groups.contains_key("0"));
        assert!(groups.contains_key("1"));
        assert!(groups.contains_key("2"));
        assert!(groups.contains_key("(none)"));
    }

    #[test]
    fn test_format_summary_empty() {
        let groups: BTreeMap<String, Vec<&TaskTag>> = BTreeMap::new();
        let config = TaskConfig::default();
        let output = format_summary_table(&groups, "status", &config, &ColorMode::Never);
        assert_eq!(output, "No tasks found.\n");
    }

    #[test]
    fn test_format_summary_has_headers() {
        let tasks = sample_tasks();
        let groups = group_tasks(&tasks, "status");
        let config = TaskConfig::default();
        let output = format_summary_table(&groups, "status", &config, &ColorMode::Never);

        assert!(output.contains("Status: active"));
        assert!(output.contains("Status: done"));
        assert!(output.contains("Status: blocked"));
        assert!(output.contains("ID"));
        assert!(output.contains("Title"));
        assert!(output.contains("Owner"));
        assert!(output.contains("Status"));
        assert!(output.contains("Priority"));
    }

    #[test]
    fn test_format_summary_contains_task_data() {
        let tasks = sample_tasks();
        let groups = group_tasks(&tasks, "status");
        let config = TaskConfig::default();
        let output = format_summary_table(&groups, "status", &config, &ColorMode::Never);

        assert!(output.contains("aaa1"));
        assert!(output.contains("Task A"));
        assert!(output.contains("alice"));
        assert!(output.contains("bbb2"));
        assert!(output.contains("Task B"));
    }

    #[test]
    fn test_format_summary_no_color() {
        let tasks = sample_tasks();
        let groups = group_tasks(&tasks, "status");
        let config = TaskConfig::default();
        let output = format_summary_table(&groups, "status", &config, &ColorMode::Never);

        // Should have no ANSI escape codes
        assert!(!output.contains("\x1b["));
    }

    #[test]
    fn test_format_summary_with_color() {
        let tasks = sample_tasks();
        let groups = group_tasks(&tasks, "status");
        let config = TaskConfig::default();
        let output = format_summary_table(&groups, "status", &config, &ColorMode::Always);

        // Should have ANSI escape codes for colored status/priority
        assert!(output.contains("\x1b["));
    }

    #[test]
    fn test_format_float_integer() {
        assert_eq!(format_float(4.0), "4");
        assert_eq!(format_float(10.0), "10");
    }

    #[test]
    fn test_format_float_decimal() {
        assert_eq!(format_float(4.5), "4.5");
        assert_eq!(format_float(2.75), "2.75");
    }

    #[test]
    fn test_capitalize() {
        assert_eq!(capitalize("status"), "Status");
        assert_eq!(capitalize("owner"), "Owner");
        assert_eq!(capitalize(""), "");
    }

    #[test]
    fn test_get_group_key_status() {
        let task = make_task("a", "Test", "me", "active", Some(1), None, 4.0, None);
        assert_eq!(get_group_key(&task, "status"), "active");
    }

    #[test]
    fn test_get_group_key_owner() {
        let task = make_task("a", "Test", "alice", "active", Some(1), None, 4.0, None);
        assert_eq!(get_group_key(&task, "owner"), "alice");
    }

    #[test]
    fn test_get_group_key_priority_none() {
        let task = make_task("a", "Test", "me", "active", None, None, 4.0, None);
        assert_eq!(get_group_key(&task, "priority"), "(none)");
    }

    #[test]
    fn test_column_alignment() {
        let tasks = sample_tasks();
        let groups = group_tasks(&tasks, "status");
        let config = TaskConfig::default();
        let output = format_summary_table(&groups, "status", &config, &ColorMode::Never);

        // Within each group, header and separator lines should have the same length
        let lines: Vec<&str> = output.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            if lines[i].starts_with("===") {
                // Next line is header, line after is separator
                if i + 2 < lines.len() {
                    let header_len = lines[i + 1].trim_end().len();
                    let sep_len = lines[i + 2].trim_end().len();
                    assert_eq!(
                        header_len, sep_len,
                        "header and separator widths should match"
                    );
                }
            }
            i += 1;
        }
    }
}

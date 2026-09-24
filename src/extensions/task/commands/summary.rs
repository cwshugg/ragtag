//! Task summary command.
//!
//! Produces a table-like display of tasks grouped by a field
//! (status, owner, priority) with aligned columns and color-coded
//! status and priority values.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::Path;

use super::super::config::TaskConfig;
use super::super::models::TaskTag;
use super::super::output::{colorize_priority, colorize_status};
use super::list::sort_tasks;
use super::{select_tasks, task_field_value};
use crate::cli;
use crate::config::ColorMode;
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;
use crate::output::format::{
    colorize_path, display_width, pad_right, strip_dot_slash, terminal_safe, truncate,
};
use terminal_size::{terminal_size, Width};

/// Column headers for a summary table containing mixed task types.
const HEADERS_WITH_TYPE: &[&str] = &[
    "Path", "Title", "Type", "Owner", "Status", "Priority", "Time", "ID",
];

/// Column headers for a summary table containing one normalized task type.
const HEADERS_WITHOUT_TYPE: &[&str] =
    &["Path", "Title", "Owner", "Status", "Priority", "Time", "ID"];

/// Minimum title width before we stop shrinking.
const MIN_TITLE_WIDTH: usize = 20;

/// Fallback title width when terminal size cannot be detected.
const FALLBACK_TITLE_WIDTH: usize = 60;

/// Number of spaces between each column.
const COLUMN_GAP: usize = 2;

/// Runs the summary command.
pub fn run(
    matches: &clap::ArgMatches,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(), RagtagError> {
    let path_str = cli::resolve_path(matches);
    let path = Path::new(&path_str);

    let group_by = matches
        .get_one::<String>("group")
        .map(|s| s.as_str())
        .unwrap_or("priority");

    let sort_by = matches.get_one::<String>("sort").cloned();

    let filter_expr = matches.get_one::<String>("filter").map(String::as_str);
    let mut tasks = select_tasks(path, filter_expr, matches.get_flag("all"), config, ctx)?;

    // Sort within groups (default: priority)
    let effective_sort = sort_by.unwrap_or_else(|| "priority".to_string());
    sort_tasks(&mut tasks, &effective_sort, false);

    // Group tasks
    let groups = group_tasks(&tasks, group_by);

    // Render output
    let format = matches
        .get_one::<String>("format")
        .map(|s| s.as_str())
        .unwrap_or("table");

    let output = match format {
        "list" => format_summary_list(&groups, group_by, config, &ctx.color_mode),
        _ => format_summary_table(&groups, group_by, config, &ctx.color_mode),
    };
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
        "status" | "type" | "owner" => task_field_value(task, group_by)
            .expect("known grouping field")
            .into_owned(),
        "priority" => task_field_value(task, group_by)
            .filter(|value| !value.is_empty())
            .map_or_else(|| "(none)".to_string(), Cow::into_owned),
        _ => task.status.clone(),
    }
}

/// Maximum display width for the title column in summary tables.
/// Computes the maximum title column width by subtracting the widths of all
/// other columns (plus inter-column gaps) from the terminal width. Falls back
/// to `FALLBACK_TITLE_WIDTH` when the terminal size cannot be determined.
fn compute_title_width(non_title_widths: &[usize]) -> usize {
    let term_width = terminal_size().map(|(Width(w), _)| w as usize).unwrap_or(0);

    if term_width == 0 {
        return FALLBACK_TITLE_WIDTH;
    }

    // Title is at index 1 in both table schemas. Sum widths of all other columns.
    let other_width: usize = non_title_widths.iter().sum::<usize>();
    // Total gaps: (num_columns - 1) * COLUMN_GAP
    let total_gaps = (non_title_widths.len()) * COLUMN_GAP;
    let available = term_width.saturating_sub(other_width + total_gaps);

    available.max(MIN_TITLE_WIDTH)
}

/// Shared widths and title limit for every table in one summary render.
struct TableLayout {
    includes_type: bool,
    widths: Vec<usize>,
    max_title_width: usize,
}

impl TableLayout {
    /// Returns the shared widths for a group-specific column schema.
    fn widths_for(&self, include_type: bool) -> Vec<usize> {
        let mut widths = self.widths.clone();
        if self.includes_type && !include_type {
            widths.remove(2);
        }
        widths
    }
}

/// Computes one layout from every row that will be displayed.
fn compute_table_layout(
    tasks: &[&TaskTag],
    include_type: bool,
    config: &TaskConfig,
    color_mode: &ColorMode,
) -> TableLayout {
    let headers = if include_type {
        HEADERS_WITH_TYPE
    } else {
        HEADERS_WITHOUT_TYPE
    };
    let title_col = 1;
    let mut widths: Vec<usize> = headers.iter().map(|header| display_width(header)).collect();
    let rows = build_rows(tasks, config, color_mode, usize::MAX, include_type);

    for (plain, _) in &rows {
        for (index, value) in plain.iter().enumerate() {
            if index != title_col {
                widths[index] = widths[index].max(display_width(value));
            }
        }
    }

    let non_title_widths: Vec<usize> = widths
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != title_col)
        .map(|(_, width)| *width)
        .collect();
    let max_title_width = compute_title_width(&non_title_widths);
    widths[title_col] = display_width(headers[title_col]);
    for (plain, _) in rows {
        let title = truncate(&plain[title_col], max_title_width);
        widths[title_col] = widths[title_col].max(display_width(&title));
    }

    TableLayout {
        includes_type: include_type,
        widths,
        max_title_width,
    }
}

/// Formats the complete summary table output with group headers.
///
/// Each group independently selects its columns from the rows it displays.
fn format_summary_table(
    groups: &BTreeMap<String, Vec<&TaskTag>>,
    group_by: &str,
    config: &TaskConfig,
    color_mode: &ColorMode,
) -> String {
    if groups.is_empty() {
        return "No tasks found.\n".to_string();
    }

    // Collect group keys in the appropriate sort order.
    let sorted_keys: Vec<&String> = if group_by == "priority" {
        let mut keys: Vec<&String> = groups.keys().collect();
        keys.sort_by(|a, b| match (a.parse::<i64>(), b.parse::<i64>()) {
            (Ok(na), Ok(nb)) => na.cmp(&nb),
            _ => a.cmp(b),
        });
        keys
    } else {
        groups.keys().collect()
    };
    let all_tasks: Vec<&TaskTag> = groups
        .values()
        .flat_map(|tasks| tasks.iter().copied())
        .collect();
    let include_type = groups.values().any(|tasks| table_has_mixed_types(tasks));
    let layout = compute_table_layout(&all_tasks, include_type, config, color_mode);

    let mut output = String::new();
    let mut first = true;

    for key in sorted_keys {
        if !first {
            output.push('\n');
        }
        first = false;

        output.push_str(&format!(
            "{}: {}\n",
            capitalize(group_by),
            terminal_safe(key)
        ));
        output.push_str(&format_task_table_with_layout(
            &groups[key],
            config,
            color_mode,
            &layout,
        ));
    }

    output
}

/// Formats one task table, showing type only when its displayed rows are mixed.
#[cfg(test)]
fn format_task_table(tasks: &[&TaskTag], config: &TaskConfig, color_mode: &ColorMode) -> String {
    let include_type = table_has_mixed_types(tasks);
    let layout = compute_table_layout(tasks, include_type, config, color_mode);
    format_task_table_with_layout(tasks, config, color_mode, &layout)
}

/// Formats one task table using widths shared by its complete summary render.
fn format_task_table_with_layout(
    tasks: &[&TaskTag],
    config: &TaskConfig,
    color_mode: &ColorMode,
    layout: &TableLayout,
) -> String {
    let include_type = table_has_mixed_types(tasks);
    let headers = if include_type {
        HEADERS_WITH_TYPE
    } else {
        HEADERS_WITHOUT_TYPE
    };
    let widths = layout.widths_for(include_type);
    let rows = build_rows(
        tasks,
        config,
        color_mode,
        layout.max_title_width,
        include_type,
    );

    let mut output = String::new();
    output.push_str(&format_row_with_path(headers, &widths));
    output.push('\n');

    let separators: Vec<String> = widths.iter().map(|width| "-".repeat(*width)).collect();
    let separator_refs: Vec<&str> = separators.iter().map(String::as_str).collect();
    output.push_str(&format_row_with_path(&separator_refs, &widths));
    output.push('\n');

    for (plain, colored) in rows {
        output.push_str(&format_colored_row_with_path(&plain, &colored, &widths));
        output.push('\n');
    }

    output
}

/// Returns whether displayed rows contain different effective type strings.
fn table_has_mixed_types(tasks: &[&TaskTag]) -> bool {
    let Some(first) = tasks.first() else {
        return false;
    };
    tasks
        .iter()
        .skip(1)
        .any(|task| task.task_type.as_str() != first.task_type.as_str())
}

/// Formats the summary output as a multi-line list.
///
/// Each task is displayed as three or four lines:
/// 1. File path (colored)
/// 2. Full title (no truncation)
/// 3. Description (only if present)
/// 4. ID [OWNER] [PRIORITY/STATUS] TIME
///
/// Tasks are separated by blank lines, and groups are separated by
/// an extra blank line with a header line.
fn format_summary_list(
    groups: &BTreeMap<String, Vec<&TaskTag>>,
    group_by: &str,
    config: &TaskConfig,
    color_mode: &ColorMode,
) -> String {
    if groups.is_empty() {
        return "No tasks found.\n".to_string();
    }

    let mut output = String::new();
    let mut first_group = true;

    // Collect group keys in the appropriate sort order.
    // For priority, sort numerically; fall back to lexicographic for non-numeric keys.
    let sorted_keys: Vec<&String> = if group_by == "priority" {
        let mut keys: Vec<&String> = groups.keys().collect();
        keys.sort_by(|a, b| match (a.parse::<i64>(), b.parse::<i64>()) {
            (Ok(na), Ok(nb)) => na.cmp(&nb),
            _ => a.cmp(b),
        });
        keys
    } else {
        groups.keys().collect()
    };

    for group_key in &sorted_keys {
        let tasks = &groups[*group_key];
        if !first_group {
            output.push('\n');
        }
        first_group = false;

        // Group header
        output.push_str(&format!(
            "{}: {}\n\n",
            capitalize(group_by),
            terminal_safe(group_key)
        ));

        for (i, task) in tasks.iter().enumerate() {
            if i > 0 {
                output.push('\n');
            }

            let path = colorize_path(&task.location.file_path, color_mode);

            let time = format_time(task);
            let status = colorize_status(&task.status, &config.status_keywords, color_mode);
            let priority = task
                .priority
                .map(|p| colorize_priority(p, color_mode))
                .unwrap_or_else(|| "-".to_string());

            let id_str = if task.id.is_empty() {
                "-".to_string()
            } else {
                terminal_safe(&task.id).to_string()
            };

            output.push_str(&format!("{path}\n"));
            output.push_str(&format!("{}\n", terminal_safe(&task.title)));
            if let Some(desc) = &task.description {
                if !desc.is_empty() {
                    output.push_str(&format!("{}\n", terminal_safe(desc)));
                }
            }
            output.push_str(&format!(
                "{} [{}] [{}] [{}/{}] {}\n",
                id_str,
                terminal_safe(task.task_type.as_str()),
                terminal_safe(&task.owner),
                priority,
                status,
                time
            ));
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
    max_title_width: usize,
    include_type: bool,
) -> Vec<(Vec<String>, Vec<String>)> {
    tasks
        .iter()
        .map(|task| {
            let title = truncate(&terminal_safe(&task.title).to_string(), max_title_width);
            let time = format_time(task);
            let path_plain = terminal_safe(&strip_dot_slash(
                &task.location.file_path.display().to_string(),
            ))
            .to_string();
            let path_colored = colorize_path(&task.location.file_path, color_mode);

            let id_str = if task.id.is_empty() {
                "-".to_string()
            } else {
                terminal_safe(&task.id).to_string()
            };
            let owner = terminal_safe(&task.owner).to_string();
            let status = terminal_safe(&task.status).to_string();

            let mut plain = vec![
                path_plain,
                title.clone(),
                owner.clone(),
                status,
                task.priority
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                time.clone(),
                id_str.clone(),
            ];

            let mut colored = vec![
                path_colored,
                title,
                owner,
                colorize_status(&task.status, &config.status_keywords, color_mode),
                task.priority
                    .map(|p| colorize_priority(p, color_mode))
                    .unwrap_or_else(|| "-".to_string()),
                time,
                id_str,
            ];
            if include_type {
                let task_type = terminal_safe(task.task_type.as_str()).to_string();
                plain.insert(2, task_type.clone());
                colored.insert(2, task_type);
            }

            (plain, colored)
        })
        .collect()
}

/// Formats the combined time column.
///
/// Format: `WORKTIME_SPENT/WORKTIME_ESTIMATE TIME_UNIT`
/// If a value is `None`, shows `-`.
fn format_time(task: &TaskTag) -> String {
    let spent = task
        .worktime_spent
        .map(format_float)
        .unwrap_or_else(|| "-".to_string());
    let estimate = task
        .worktime_estimate
        .map(format_float)
        .unwrap_or_else(|| "-".to_string());
    format!("{spent}/{estimate} {}", terminal_safe(&task.worktime_units))
}

/// Formats a row with all columns padded to fixed widths.
fn format_row_with_path(values: &[&str], widths: &[usize]) -> String {
    values
        .iter()
        .zip(widths.iter())
        .map(|(value, width)| pad_right(value, *width))
        .collect::<Vec<_>>()
        .join("  ")
}

/// Formats a row where some cells may contain ANSI color codes.
///
/// Uses `plain` values to determine padding widths, then applies the
/// padding to `colored` values (which may contain invisible ANSI bytes).
fn format_colored_row_with_path(plain: &[String], colored: &[String], widths: &[usize]) -> String {
    plain
        .iter()
        .zip(colored.iter())
        .zip(widths.iter())
        .map(|((p, c), width)| {
            let visible_len = display_width(p);
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
        worktime_spent: Option<f64>,
        worktime_estimate: Option<f64>,
    ) -> TaskTag {
        make_task_with_desc(
            id,
            title,
            None,
            owner,
            status,
            priority,
            (worktime_spent, worktime_estimate),
        )
    }

    fn make_task_with_desc(
        id: &str,
        title: &str,
        description: Option<&str>,
        owner: &str,
        status: &str,
        priority: Option<u32>,
        worktime: (Option<f64>, Option<f64>),
    ) -> TaskTag {
        let (worktime_spent, worktime_estimate) = worktime;
        TaskTag {
            id: id.to_string(),
            pid: None,
            title: title.to_string(),
            description: description.map(|s| s.to_string()),
            owner: owner.to_string(),
            status: status.to_string(),
            task_type: crate::extensions::task::models::TaskType::Item,
            priority,
            worktime_spent,
            worktime_estimate,
            time_created: None,
            time_last_updated: None,
            worktime_units: "hours".to_string(),
            location: TagLocation::new(PathBuf::from("test.md"), 1, 1, 0, 50),
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
                Some(8.0),
            ),
            make_task(
                "bbb2",
                "Task B",
                "bob",
                "done",
                Some(2),
                Some(4.0),
                Some(4.0),
            ),
            make_task(
                "ccc3",
                "Task C",
                "alice",
                "active",
                Some(0),
                None,
                Some(6.0),
            ),
            make_task("ddd4", "Task D", "bob", "blocked", None, None, Some(10.0)),
        ]
    }

    fn display_column(line: &str, value: &str) -> usize {
        let byte_offset = line.find(value).expect("value should occur in row");
        display_width(&line[..byte_offset])
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

    /// Priority groups must be sorted numerically so that `2` appears before `11`.
    /// A lexicographic sort would put `"11"` before `"2"` (since `'1' < '2'`), which
    /// is the bug this test guards against.
    #[test]
    fn test_priority_numeric_sort() {
        let tasks = vec![
            make_task("t1", "Task 11", "alice", "active", Some(11), None, None),
            make_task("t2", "Task 2", "bob", "active", Some(2), None, None),
        ];
        let groups = group_tasks(&tasks, "priority");
        let config = TaskConfig::default();

        // Check table output
        let table_output = format_summary_table(&groups, "priority", &config, &ColorMode::Never);
        let pos_2 = table_output
            .find("Priority: 2")
            .expect("should contain 'Priority: 2'");
        let pos_11 = table_output
            .find("Priority: 11")
            .expect("should contain 'Priority: 11'");
        assert!(
            pos_2 < pos_11,
            "priority 2 should appear before 11 in table output, but got:\n{table_output}"
        );

        // Check list output
        let list_output = format_summary_list(&groups, "priority", &config, &ColorMode::Never);
        let pos_2 = list_output
            .find("Priority: 2")
            .expect("should contain 'Priority: 2'");
        let pos_11 = list_output
            .find("Priority: 11")
            .expect("should contain 'Priority: 11'");
        assert!(
            pos_2 < pos_11,
            "priority 2 should appear before 11 in list output, but got:\n{list_output}"
        );
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
        assert!(output.contains("Path"));
        assert!(output.contains("Title"));
        assert!(output.contains("Owner"));
        assert!(output.contains("Status"));
        assert!(output.contains("Priority"));
        assert!(output.contains("Time"));
        assert!(output.contains("ID"));
    }

    #[test]
    fn task_tables_preserve_type_visibility_ascii_output_and_shared_unicode_layout() {
        let config = TaskConfig::default();
        let plain = make_task(
            "id",
            "Title",
            "owner",
            "active",
            Some(2),
            Some(1.0),
            Some(3.0),
        );
        let plain_output = format_task_table(&[&plain], &config, &ColorMode::Never);
        assert_eq!(
            plain_output,
            concat!(
                "Path     Title  Owner  Status  Priority  Time       ID\n",
                "-------  -----  -----  ------  --------  ---------  --\n",
                "test.md  Title  owner  active  2         1/3 hours  id\n",
            )
        );

        let mut first = make_task(
            "标识",
            "e\u{301} title",
            "所有者",
            "active",
            Some(1),
            Some(1.0),
            Some(2.0),
        );
        first.location.file_path = PathBuf::from("路径/\u{1b}name.md");
        first.task_type = crate::extensions::task::models::TaskType::Custom("阶段".to_string());
        let first_item = make_task(
            "first-item",
            "ASCII",
            "owner",
            "bad\u{1b}[2J",
            Some(1),
            None,
            None,
        );

        let mut second = make_task(
            "emoji-id",
            "标题",
            "e\u{301}",
            "blocked",
            Some(2),
            None,
            None,
        );
        second.task_type = crate::extensions::task::models::TaskType::Custom("👩‍💻".to_string());
        second.worktime_units = "单位".to_string();
        let second_item = make_task(
            "second-item",
            "Plain",
            "owner",
            "active",
            Some(2),
            None,
            None,
        );

        let tasks = vec![first, first_item, second, second_item];
        let groups = group_tasks(&tasks, "priority");
        let output = format_summary_table(&groups, "priority", &config, &ColorMode::Never);
        let table_lines: Vec<&str> = output
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with("Priority:"))
            .collect();
        let expected_width = display_width(table_lines[0]);
        assert!(
            table_lines
                .iter()
                .all(|line| display_width(line) == expected_width),
            "{output}"
        );

        let header = table_lines[0];
        let unicode_row = table_lines
            .iter()
            .copied()
            .find(|line| line.contains("阶段"))
            .unwrap();
        let emoji_row = table_lines
            .iter()
            .copied()
            .find(|line| line.contains("👩‍💻"))
            .unwrap();
        for (heading, first_value, second_value) in [
            ("Type", "阶段", "👩‍💻"),
            ("Owner", "所有者", "e\u{301}"),
            ("Status", "active", "blocked"),
        ] {
            let expected = display_column(header, heading);
            assert_eq!(
                display_column(unicode_row, first_value),
                expected,
                "{output}"
            );
            assert_eq!(
                display_column(emoji_row, second_value),
                expected,
                "{output}"
            );
        }
        assert!(output.contains("\\u{1b}name.md"));
        assert!(output.contains("bad\\u{1b}[2J"));

        let homogeneous = format_task_table(&[&tasks[1]], &config, &ColorMode::Never);
        assert!(!homogeneous.lines().next().unwrap().contains("Type"));
        let mixed = format_task_table(&[&tasks[0], &tasks[1]], &config, &ColorMode::Never);
        assert!(mixed.lines().next().unwrap().contains("Type"));
    }

    #[test]
    fn test_format_summary_contains_task_data() {
        let tasks = sample_tasks();
        let groups = group_tasks(&tasks, "status");
        let config = TaskConfig::default();
        let output = format_summary_table(&groups, "status", &config, &ColorMode::Never);

        assert!(output.contains("test.md"));
        assert!(output.contains("Task A"));
        assert!(output.contains("alice"));
        assert!(output.contains("Task B"));
        // Check combined time column format
        assert!(output.contains("2/8 hours")); // Task A: spent=2, estimate=8
        assert!(output.contains("4/4 hours")); // Task B: spent=4, estimate=4
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
        let task = make_task("a", "Test", "me", "active", Some(1), None, Some(4.0));
        assert_eq!(get_group_key(&task, "status"), "active");
    }

    #[test]
    fn test_get_group_key_owner() {
        let task = make_task("a", "Test", "alice", "active", Some(1), None, Some(4.0));
        assert_eq!(get_group_key(&task, "owner"), "alice");
    }

    #[test]
    fn test_get_group_key_priority_none() {
        let task = make_task("a", "Test", "me", "active", None, None, Some(4.0));
        assert_eq!(get_group_key(&task, "priority"), "(none)");
    }

    #[test]
    fn test_get_group_key_type() {
        let mut task = make_task("a", "active", "alice", "Task", Some(1), None, None);
        task.task_type = crate::extensions::task::models::TaskType::Project;
        assert_eq!(get_group_key(&task, "type"), "project");
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

    #[test]
    fn test_format_summary_list_basic() {
        let tasks = vec![
            make_task(
                "aaa1",
                "Task A",
                "alice",
                "active",
                Some(1),
                Some(2.0),
                Some(8.0),
            ),
            make_task("bbb2", "Task B", "bob", "active", Some(0), None, None),
        ];
        let groups = group_tasks(&tasks, "status");
        let config = TaskConfig::default();
        let output = format_summary_list(&groups, "status", &config, &ColorMode::Never);

        // Should have group header
        assert!(output.contains("Status: active"));

        // Each task should show 3 lines: path, title, details
        assert!(output.contains("test.md"));
        assert!(output.contains("Task A"));
        assert!(output.contains("aaa1 [item] [alice] [1/active] 2/8 hours"));
        assert!(output.contains("Task B"));
        assert!(output.contains("bbb2 [item] [bob] [0/active] -/- hours"));
    }

    #[test]
    fn test_format_summary_list_empty() {
        let groups: BTreeMap<String, Vec<&TaskTag>> = BTreeMap::new();
        let config = TaskConfig::default();
        let output = format_summary_list(&groups, "status", &config, &ColorMode::Never);
        assert_eq!(output, "No tasks found.\n");
    }

    #[test]
    fn test_format_summary_list_multiple_groups() {
        let tasks = sample_tasks();
        let groups = group_tasks(&tasks, "status");
        let config = TaskConfig::default();
        let output = format_summary_list(&groups, "status", &config, &ColorMode::Never);

        // Should have multiple group headers
        assert!(output.contains("Status: active"));
        assert!(output.contains("Status: done"));
        assert!(output.contains("Status: blocked"));
    }

    #[test]
    fn test_format_summary_list_blank_line_between_tasks() {
        let tasks = vec![
            make_task("aaa1", "Task A", "alice", "active", Some(1), None, None),
            make_task("bbb2", "Task B", "bob", "active", Some(2), None, None),
        ];
        let groups = group_tasks(&tasks, "status");
        let config = TaskConfig::default();
        let output = format_summary_list(&groups, "status", &config, &ColorMode::Never);

        // Two tasks in same group should be separated by a blank line.
        // Expected structure:
        //   "Status: active\n"
        //   "\n"
        //   "test.md\n"
        //   "Task A\n"
        //   "aaa1 [item] [alice] [1/active] ...\n"
        //   "\n"          <-- blank line between tasks
        //   "test.md\n"
        //   "Task B\n"
        //   "bbb2 [item] [bob] [2/active] ...\n"
        assert!(
            output.contains("hours\n\ntest.md"),
            "should have blank line between tasks, got:\n{output}"
        );
    }

    #[test]
    fn test_format_summary_list_title_no_truncation() {
        let long_title = "A".repeat(100);
        let tasks = vec![make_task(
            "aaa1",
            &long_title,
            "alice",
            "active",
            Some(1),
            None,
            None,
        )];
        let groups = group_tasks(&tasks, "status");
        let config = TaskConfig::default();
        let output = format_summary_list(&groups, "status", &config, &ColorMode::Never);

        // Full title should appear (no truncation)
        assert!(output.contains(&long_title));
        // Should NOT contain truncation marker
        assert!(!output.contains("..."));
    }

    #[test]
    fn test_format_summary_list_no_color() {
        let tasks = vec![make_task(
            "aaa1",
            "Task A",
            "alice",
            "active",
            Some(0),
            None,
            None,
        )];
        let groups = group_tasks(&tasks, "status");
        let config = TaskConfig::default();
        let output = format_summary_list(&groups, "status", &config, &ColorMode::Never);

        // No ANSI escape codes
        assert!(!output.contains("\x1b["));
    }

    #[test]
    fn test_format_summary_list_with_color() {
        let tasks = vec![make_task(
            "aaa1",
            "Task A",
            "alice",
            "active",
            Some(0),
            None,
            None,
        )];
        let groups = group_tasks(&tasks, "status");
        let config = TaskConfig::default();
        let output = format_summary_list(&groups, "status", &config, &ColorMode::Always);

        // Should have ANSI escape codes for colored status/priority/path
        assert!(output.contains("\x1b["));
    }

    #[test]
    fn test_format_summary_list_with_description() {
        let tasks = vec![make_task_with_desc(
            "aaa1",
            "Task A",
            Some("This is the description"),
            "alice",
            "active",
            Some(1),
            (None, None),
        )];
        let groups = group_tasks(&tasks, "status");
        let config = TaskConfig::default();
        let output = format_summary_list(&groups, "status", &config, &ColorMode::Never);

        // Description should appear between title and details line
        assert!(output.contains("Task A\nThis is the description\naaa1 [item] [alice]"));
    }

    #[test]
    fn test_format_summary_list_without_description() {
        let tasks = vec![make_task(
            "aaa1",
            "Task A",
            "alice",
            "active",
            Some(1),
            None,
            None,
        )];
        let groups = group_tasks(&tasks, "status");
        let config = TaskConfig::default();
        let output = format_summary_list(&groups, "status", &config, &ColorMode::Never);

        // No description line — title should be immediately followed by details
        assert!(output.contains("Task A\naaa1 [item] [alice]"));
    }

    #[test]
    fn test_format_summary_list_empty_description_skipped() {
        let tasks = vec![make_task_with_desc(
            "aaa1",
            "Task A",
            Some(""),
            "alice",
            "active",
            Some(1),
            (None, None),
        )];
        let groups = group_tasks(&tasks, "status");
        let config = TaskConfig::default();
        let output = format_summary_list(&groups, "status", &config, &ColorMode::Never);

        // Empty description should be skipped — title immediately followed by details
        assert!(output.contains("Task A\naaa1 [item] [alice]"));
    }
}

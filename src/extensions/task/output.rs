//! Task output formatting and color rules.
//!
//! Handles colored status/priority display and task line/detail formatting.

use owo_colors::OwoColorize;

use super::config::TaskConfig;
use super::models::{categorize_status, StatusCategory, TaskTag};
use crate::config::ColorMode;
use crate::models::Tag;
use crate::output::format::{colorize_path, should_use_color, terminal_safe};

/// Formats a task as a single output line.
///
/// Format: `file/path.md: ID [TYPE] [OWNER] [PRIORITY/STATUS] TITLE`
/// Only file path, priority, and status are colored.
pub fn format_task_line(task: &TaskTag, color_mode: &ColorMode, config: &TaskConfig) -> String {
    let path = colorize_path(&task.location.file_path, color_mode);
    let status = colorize_status(&task.status, &config.status_keywords, color_mode);
    let priority = task
        .priority
        .map(|p| colorize_priority(p, color_mode))
        .unwrap_or_else(|| "-".to_string());

    format!(
        "{path}: {} [{}] [{}] [{priority}/{status}] {}",
        terminal_safe(&task.id),
        terminal_safe(task.task_type.as_str()),
        terminal_safe(&task.owner),
        terminal_safe(&task.title)
    )
}

/// Formats a full task detail listing (multi-line).
///
/// Attributes are shown in the specified order per the architecture.
pub fn format_task_detail(task: &TaskTag, config: &TaskConfig, color_mode: &ColorMode) -> String {
    let mut lines = Vec::new();

    lines.push(format!("Title: {}", terminal_safe(&task.title)));
    if let Some(ref desc) = task.description {
        lines.push(format!("Description: {}", terminal_safe(desc)));
    }
    lines.push(format!(
        "Path: {}",
        crate::output::format::colorize_path(&task.location.file_path, color_mode)
    ));
    lines.push(format!("ID: {}", terminal_safe(&task.id)));
    lines.push(format!("Type: {}", terminal_safe(task.task_type.as_str())));
    lines.push(format!("Owner: {}", terminal_safe(&task.owner)));
    lines.push(format!(
        "Status: {}",
        colorize_status(&task.status, &config.status_keywords, color_mode)
    ));
    if let Some(priority) = task.priority {
        lines.push(format!(
            "Priority: {}",
            colorize_priority(priority, color_mode)
        ));
    }
    if let Some(worktime_spent) = task.worktime_spent {
        lines.push(format!("Worktime Spent: {worktime_spent}"));
    }
    if let Some(worktime_estimate) = task.worktime_estimate {
        lines.push(format!("Worktime Estimate: {worktime_estimate}"));
    }
    if let Some(ref time_created) = task.time_created {
        lines.push(format!("Time Created: {}", terminal_safe(time_created)));
    }
    if let Some(ref time_last_updated) = task.time_last_updated {
        lines.push(format!(
            "Time Last Updated: {}",
            terminal_safe(time_last_updated)
        ));
    }
    lines.push(format!(
        "Worktime Units: {}",
        terminal_safe(&task.worktime_units)
    ));
    if let Some(ref pid) = task.pid {
        lines.push(format!("Parent ID: {}", terminal_safe(pid)));
    }

    lines.join("\n")
}

/// Formats a summary of task tags.
pub fn format_task_summary(tags: &[Tag], config: &TaskConfig, color_mode: &ColorMode) -> String {
    let mut done = 0usize;
    let mut active = 0usize;
    let mut blocked = 0usize;
    let mut abandoned = 0usize;
    let mut inactive = 0usize;
    let mut other = 0usize;

    for tag in tags {
        let status = tag
            .get_named_attribute("status")
            .and_then(|v| v.as_str())
            .unwrap_or(&config.default_status);
        match categorize_status(status, &config.status_keywords) {
            StatusCategory::Done => done += 1,
            StatusCategory::Active => active += 1,
            StatusCategory::Blocked => blocked += 1,
            StatusCategory::Abandoned => abandoned += 1,
            StatusCategory::Inactive => inactive += 1,
            StatusCategory::Unknown => other += 1,
        }
    }

    let use_color = should_use_color(color_mode);

    let mut parts = Vec::new();
    if done > 0 {
        let s = format!("{done} done");
        parts.push(if use_color {
            s.bright_green().to_string()
        } else {
            s
        });
    }
    if active > 0 {
        let s = format!("{active} active");
        parts.push(if use_color {
            s.bright_yellow().to_string()
        } else {
            s
        });
    }
    if blocked > 0 {
        let s = format!("{blocked} blocked");
        parts.push(if use_color {
            s.bright_red().to_string()
        } else {
            s
        });
    }
    if abandoned > 0 {
        let s = format!("{abandoned} abandoned");
        parts.push(if use_color {
            s.truecolor(255, 165, 0).to_string()
        } else {
            s
        });
    }
    if inactive > 0 {
        let s = format!("{inactive} inactive");
        parts.push(if use_color {
            s.bright_black().to_string()
        } else {
            s
        });
    }
    if other > 0 {
        parts.push(format!("{other} other"));
    }

    parts.join(", ")
}

/// Applies status color based on status category.
pub fn colorize_status(
    status: &str,
    keywords: &super::config::StatusKeywords,
    color_mode: &ColorMode,
) -> String {
    let use_color = should_use_color(color_mode);
    let safe_status = terminal_safe(status).to_string();
    if !use_color {
        return safe_status;
    }

    match categorize_status(status, keywords) {
        StatusCategory::Done => safe_status.bright_green().to_string(),
        StatusCategory::Active => safe_status.bright_yellow().to_string(),
        StatusCategory::Blocked => safe_status.bright_red().to_string(),
        StatusCategory::Abandoned => safe_status.truecolor(255, 165, 0).to_string(),
        StatusCategory::Inactive => safe_status.bright_black().to_string(),
        StatusCategory::Unknown => safe_status,
    }
}

/// Applies priority color based on urgency level.
///
/// - `0` → bright red (critical)
/// - `1` → orange (high)
/// - `2` → bright yellow (medium-high)
/// - `3` → light yellow-green (medium)
/// - `4` → bright green (low)
/// - `5+` → no color (minimal)
pub fn colorize_priority(priority: u32, color_mode: &ColorMode) -> String {
    let use_color = should_use_color(color_mode);
    let s = priority.to_string();
    if !use_color {
        return s;
    }
    match priority {
        0 => s.bright_red().to_string(),
        1 => s.truecolor(255, 165, 0).to_string(),
        2 => s.bright_yellow().to_string(),
        3 => s.truecolor(180, 230, 50).to_string(),
        4 => s.bright_green().to_string(),
        _ => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TagLocation;
    use std::path::PathBuf;

    fn make_task() -> TaskTag {
        TaskTag {
            id: "abc123def456789a".to_string(),
            pid: None,
            title: "Test Task".to_string(),
            description: Some("A test".to_string()),
            owner: "me".to_string(),
            status: "active".to_string(),
            task_type: crate::extensions::task::models::TaskType::Item,
            priority: Some(1),
            worktime_spent: Some(2.0),
            worktime_estimate: Some(4.5),
            time_created: Some("2026-06-12T09:00:00Z".to_string()),
            time_last_updated: Some("2026-06-12T10:00:00Z".to_string()),
            worktime_units: "hours".to_string(),
            location: TagLocation::new(PathBuf::from("test.md"), 1, 1, 0, 50),
        }
    }

    #[test]
    fn test_format_task_line_default() {
        let task = make_task();
        let config = TaskConfig::default();
        let line = format_task_line(&task, &ColorMode::Never, &config);
        // Format: path: ID [TYPE] [OWNER] [PRIORITY/STATUS] TITLE
        assert!(line.contains("test.md:"));
        assert!(line.contains("abc123"));
        assert!(line.contains("[item]"));
        assert!(line.contains("[me]"));
        assert!(line.contains("[1/active]"));
        assert!(line.contains("Test Task"));
    }

    #[test]
    fn test_format_task_detail() {
        let task = make_task();
        let config = TaskConfig::default();
        let detail = format_task_detail(&task, &config, &ColorMode::Never);
        assert!(detail.contains("Title: Test Task"));
        assert!(detail.contains("ID: abc123"));
        assert!(detail.contains("Owner: me"));
        assert!(detail.contains("Type: item"));
        assert!(detail.contains("Status: active"));
        assert!(detail.contains("Worktime Spent: 2"));
        assert!(detail.contains("Worktime Estimate: 4.5"));
        assert!(detail.contains("Time Created: 2026-06-12T09:00:00Z"));
        assert!(detail.contains("Time Last Updated: 2026-06-12T10:00:00Z"));
    }

    #[test]
    fn test_format_task_detail_order() {
        let task = make_task();
        let config = TaskConfig::default();
        let detail = format_task_detail(&task, &config, &ColorMode::Never);
        let title_pos = detail.find("Title:").unwrap();
        let path_pos = detail.find("Path:").unwrap();
        let id_pos = detail.find("ID:").unwrap();
        let type_pos = detail.find("Type:").unwrap();
        let owner_pos = detail.find("Owner:").unwrap();
        let status_pos = detail.find("Status:").unwrap();
        assert!(title_pos < path_pos);
        assert!(path_pos < id_pos);
        assert!(id_pos < type_pos);
        assert!(type_pos < owner_pos);
        assert!(owner_pos < status_pos);
    }

    #[test]
    fn test_colorize_status_never() {
        let kw = super::super::config::StatusKeywords::default();
        let result = colorize_status("done", &kw, &ColorMode::Never);
        assert_eq!(result, "done"); // No ANSI codes
    }

    #[test]
    fn test_colorize_priority_zero_never() {
        let result = colorize_priority(0, &ColorMode::Never);
        assert_eq!(result, "0");
    }

    #[test]
    fn test_colorize_priority_nonzero_high() {
        // Priority 5+ should have no color
        let result = colorize_priority(5, &ColorMode::Always);
        assert_eq!(result, "5");
    }

    #[test]
    fn test_colorize_priority_one_orange() {
        let result = colorize_priority(1, &ColorMode::Always);
        // Should contain ANSI truecolor escape for orange (255, 165, 0)
        assert!(result.contains("\x1b["));
        assert!(result.contains("1"));
    }

    #[test]
    fn test_colorize_priority_two_yellow() {
        let result = colorize_priority(2, &ColorMode::Always);
        assert!(result.contains("\x1b["));
        assert!(result.contains("2"));
    }

    #[test]
    fn test_colorize_priority_three_yellowgreen() {
        let result = colorize_priority(3, &ColorMode::Always);
        assert!(result.contains("\x1b["));
        assert!(result.contains("3"));
    }

    #[test]
    fn test_colorize_priority_four_green() {
        let result = colorize_priority(4, &ColorMode::Always);
        assert!(result.contains("\x1b["));
        assert!(result.contains("4"));
    }

    #[test]
    fn test_format_task_line_status_colored() {
        let task = make_task();
        let config = TaskConfig::default();
        let line = format_task_line(&task, &ColorMode::Always, &config);
        // "active" status should have ANSI color codes
        assert!(line.contains("\x1b["));
        assert!(line.contains("active"));
    }

    #[test]
    fn test_format_task_line_priority_zero_colored() {
        let mut task = make_task();
        task.priority = Some(0);
        let config = TaskConfig::default();
        let line = format_task_line(&task, &ColorMode::Always, &config);
        // Priority 0 should be red (has ANSI codes)
        assert!(line.contains("\x1b["));
    }

    #[test]
    fn test_format_task_line_priority_one_colored_not_red() {
        let task = make_task();
        let config = TaskConfig::default();
        let line = format_task_line(&task, &ColorMode::Always, &config);
        // Priority 1 should have color (orange truecolor), but NOT bright_red
        assert!(line.contains("1"));
        assert!(!line.contains("\x1b[91m1"));
    }

    #[test]
    fn test_format_task_line_no_color_mode() {
        let task = make_task();
        let config = TaskConfig::default();
        let line = format_task_line(&task, &ColorMode::Never, &config);
        // No ANSI codes when color is never
        assert!(!line.contains("\x1b["));
    }

    #[test]
    fn task_human_output_escapes_every_text_field_and_path() {
        let hostile = "line\ncarriage\rCSI\u{1b}[2J OSC\u{1b}]52;c;x\u{7} bidi\u{202e}";
        let mut task = make_task();
        task.id = hostile.to_string();
        task.pid = Some(hostile.to_string());
        task.title = hostile.to_string();
        task.description = Some(hostile.to_string());
        task.owner = hostile.to_string();
        task.status = hostile.to_string();
        task.time_created = Some(hostile.to_string());
        task.time_last_updated = Some(hostile.to_string());
        task.worktime_units = hostile.to_string();
        task.location.file_path = PathBuf::from(hostile);

        for output in [
            format_task_line(&task, &ColorMode::Never, &TaskConfig::default()),
            format_task_detail(&task, &TaskConfig::default(), &ColorMode::Never),
        ] {
            assert!(!output.contains('\n') || output.starts_with("Title:"));
            assert!(!output.contains('\r'));
            assert!(!output.contains('\u{1b}'));
            assert!(!output.contains('\u{7}'));
            assert!(!output.contains('\u{202e}'));
            assert!(output.contains("\\n"));
            assert!(output.contains("\\r"));
            assert!(output.contains("\\u{1b}[2J"));
            assert!(output.contains("\\u{7}"));
            assert!(output.contains("\\u{202e}"));
        }
    }
}

//! Task output formatting and color rules.
//!
//! Handles colored status/priority display and task line/detail formatting.

use owo_colors::OwoColorize;

use super::config::TaskConfig;
use super::models::{categorize_status, StatusCategory, TaskTag};
use crate::config::ColorMode;
use crate::models::Tag;
use crate::output::format::{colorize_path, should_use_color};

/// Formats a task as a single output line.
///
/// Format: `file/path.md: ID [OWNER/PRIORITY/STATUS] TITLE`
/// Only file path, priority, and status are colored.
pub fn format_task_line(task: &TaskTag, color_mode: &ColorMode, config: &TaskConfig) -> String {
    let path = colorize_path(&task.location.file_path, color_mode);
    let status = colorize_status(&task.status, &config.status_keywords, color_mode);
    let priority = task
        .priority
        .map(|p| colorize_priority(p, color_mode))
        .unwrap_or_else(|| "-".to_string());

    format!(
        "{path}: {} [{}] [{priority}/{status}] {}",
        task.id, task.owner, task.title
    )
}

/// Formats a full task detail listing (multi-line).
///
/// Attributes are shown in the specified order per the architecture.
pub fn format_task_detail(task: &TaskTag, config: &TaskConfig, color_mode: &ColorMode) -> String {
    let mut lines = Vec::new();

    lines.push(format!("Title: {}", task.title));
    if let Some(ref desc) = task.description {
        lines.push(format!("Description: {desc}"));
    }
    lines.push(format!(
        "Path: {}",
        crate::output::format::colorize_path(&task.location.file_path, color_mode)
    ));
    lines.push(format!("ID: {}", task.id));
    lines.push(format!("Owner: {}", task.owner));
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
    if let Some(ts) = task.time_spent {
        lines.push(format!("Time Spent: {ts}"));
    }
    if let Some(ta) = task.ttc_actual {
        lines.push(format!("Time-to-Completion Actual: {ta}"));
    }
    if let Some(te) = task.ttc_estimate {
        lines.push(format!("Time-to-Completion Estimate: {te}"));
    }
    lines.push(format!("Unit of Time: {}", task.time_units));
    if let Some(ref pid) = task.pid {
        lines.push(format!("Parent ID: {pid}"));
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
    if !use_color {
        return status.to_string();
    }

    match categorize_status(status, keywords) {
        StatusCategory::Done => status.bright_green().to_string(),
        StatusCategory::Active => status.bright_yellow().to_string(),
        StatusCategory::Blocked => status.bright_red().to_string(),
        StatusCategory::Abandoned => status.truecolor(255, 165, 0).to_string(),
        StatusCategory::Inactive => status.bright_black().to_string(),
        StatusCategory::Unknown => status.to_string(),
    }
}

/// Applies priority color (red if 0).
pub fn colorize_priority(priority: u32, color_mode: &ColorMode) -> String {
    let use_color = should_use_color(color_mode);
    let s = priority.to_string();
    if use_color && priority == 0 {
        s.bright_red().to_string()
    } else {
        s
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
            priority: Some(1),
            time_spent: Some(2.0),
            ttc_estimate: Some(4.5),
            ttc_actual: None,
            time_units: "hours".to_string(),
            location: TagLocation::new(PathBuf::from("test.md"), 1, 1, 0, 50),
            raw_span: 0..50,
        }
    }

    #[test]
    fn test_format_task_line_default() {
        let task = make_task();
        let config = TaskConfig::default();
        let line = format_task_line(&task, &ColorMode::Never, &config);
        // Format: path: ID [OWNER] [PRIORITY/STATUS] TITLE
        assert!(line.contains("test.md:"));
        assert!(line.contains("abc123"));
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
        assert!(detail.contains("Status: active"));
    }

    #[test]
    fn test_format_task_detail_order() {
        let task = make_task();
        let config = TaskConfig::default();
        let detail = format_task_detail(&task, &config, &ColorMode::Never);
        let title_pos = detail.find("Title:").unwrap();
        let path_pos = detail.find("Path:").unwrap();
        let id_pos = detail.find("ID:").unwrap();
        let owner_pos = detail.find("Owner:").unwrap();
        let status_pos = detail.find("Status:").unwrap();
        assert!(title_pos < path_pos);
        assert!(path_pos < id_pos);
        assert!(id_pos < owner_pos);
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
    fn test_colorize_priority_nonzero() {
        let result = colorize_priority(5, &ColorMode::Always);
        assert_eq!(result, "5");
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
    fn test_format_task_line_priority_nonzero_no_color() {
        let task = make_task();
        let config = TaskConfig::default();
        let line = format_task_line(&task, &ColorMode::Always, &config);
        // Priority 1 should NOT have bright_red coloring
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
}

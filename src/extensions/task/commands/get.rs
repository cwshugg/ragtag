//! Task get command.
//!
//! Looks up a task by ID or title substring and displays its details.

use std::path::Path;

use super::super::config::TaskConfig;
use super::super::models::TaskTag;
use super::super::output::format_task_detail;
use super::collect_tasks;
use crate::config::ColorMode;
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;
use crate::output::format::colorize_path;

/// Runs the get command.
pub fn run(
    matches: &clap::ArgMatches,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(), RagtagError> {
    let search = matches
        .get_one::<String>("search")
        .expect("search is required");
    let path_str = matches
        .get_one::<String>("path")
        .map(|s| s.as_str())
        .unwrap_or(".");
    let path = Path::new(path_str);

    // Discover and parse tasks
    let tasks = collect_tasks(path, config, ctx)?;

    // Apply default status exclusion unless --all
    let show_all = matches.get_flag("all");
    let filtered = if show_all {
        tasks
    } else {
        let excluded = config.get_excluded_keywords();
        tasks
            .into_iter()
            .filter(|t| !excluded.contains(&t.status))
            .collect()
    };

    // Search for matching tasks
    let matches_found = search_tasks(&filtered, search);

    // Format and print results
    let output = format_results(&matches_found, search, config, &ctx.color_mode);
    write!(ctx.stdout, "{output}").map_err(RagtagError::Io)?;

    Ok(())
}

/// Searches for tasks matching the search string.
///
/// First tries exact ID match. Then tries ID prefix match. If no ID
/// match is found, falls back to case-insensitive title substring matching.
fn search_tasks<'a>(tasks: &'a [TaskTag], search: &str) -> Vec<&'a TaskTag> {
    // Try exact ID match first
    let exact: Vec<&TaskTag> = tasks.iter().filter(|t| t.id == search).collect();
    if !exact.is_empty() {
        return exact;
    }

    // Try ID prefix match
    let prefix: Vec<&TaskTag> = tasks.iter().filter(|t| t.id.starts_with(search)).collect();
    if !prefix.is_empty() {
        return prefix;
    }

    // Fall back to case-insensitive title substring match
    let search_lower = search.to_lowercase();
    tasks
        .iter()
        .filter(|t| t.title.to_lowercase().contains(&search_lower))
        .collect()
}

/// Formats the search results for display.
fn format_results(
    matches: &[&TaskTag],
    search: &str,
    config: &TaskConfig,
    color_mode: &ColorMode,
) -> String {
    match matches.len() {
        0 => format!("No task found for \"{search}\".\n"),
        1 => {
            let task = matches[0];
            format!("{}\n", format_task_detail(task, config, color_mode))
        }
        _ => {
            let mut output = format!("Multiple tasks found for \"{search}\":\n");
            for task in matches {
                let path = colorize_path(&task.location.file_path, color_mode);
                output.push_str(&format!("  {}: {} {}\n", path, task.id, task.title));
            }
            output.push_str("Use a more specific search string or the full task ID.\n");
            output
        }
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
    ) -> TaskTag {
        TaskTag {
            id: id.to_string(),
            pid: None,
            title: title.to_string(),
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
    fn test_search_exact_id_match() {
        let tasks = vec![
            make_task("abc123", "Task A", "alice", "active", Some(1)),
            make_task("def456", "Task B", "bob", "done", Some(2)),
        ];
        let results = search_tasks(&tasks, "abc123");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "abc123");
    }

    #[test]
    fn test_search_title_substring() {
        let tasks = vec![
            make_task("abc123", "Design API endpoints", "alice", "active", Some(1)),
            make_task("def456", "Write tests", "bob", "active", Some(2)),
        ];
        let results = search_tasks(&tasks, "design");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Design API endpoints");
    }

    #[test]
    fn test_search_title_case_insensitive() {
        let tasks = vec![make_task(
            "abc123",
            "Design API endpoints",
            "alice",
            "active",
            Some(1),
        )];
        let results = search_tasks(&tasks, "DESIGN");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Design API endpoints");
    }

    #[test]
    fn test_search_multiple_matches() {
        let tasks = vec![
            make_task("abc123", "Design API", "alice", "active", Some(1)),
            make_task("def456", "Design DB", "bob", "active", Some(2)),
        ];
        let results = search_tasks(&tasks, "Design");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_no_matches() {
        let tasks = vec![make_task("abc123", "Task A", "alice", "active", Some(1))];
        let results = search_tasks(&tasks, "nonexistent");
        assert!(results.is_empty());
    }

    #[test]
    fn test_format_no_results() {
        let results: Vec<&TaskTag> = vec![];
        let config = TaskConfig::default();
        let output = format_results(&results, "xyz", &config, &ColorMode::Never);
        assert_eq!(output, "No task found for \"xyz\".\n");
    }

    #[test]
    fn test_format_single_result() {
        let task = make_task("abc123", "Test Task", "alice", "active", Some(1));
        let results = vec![&task];
        let config = TaskConfig::default();
        let output = format_results(&results, "abc123", &config, &ColorMode::Never);
        assert!(output.contains("Title: Test Task"));
        assert!(output.contains("ID: abc123"));
        assert!(output.contains("Owner: alice"));
    }

    #[test]
    fn test_format_multiple_results() {
        let task1 = make_task("abc123", "Task A", "alice", "active", Some(1));
        let task2 = make_task("def456", "Task B", "bob", "active", Some(2));
        let results = vec![&task1, &task2];
        let config = TaskConfig::default();
        let output = format_results(&results, "Task", &config, &ColorMode::Never);
        assert!(output.contains("Multiple tasks found for \"Task\":"));
        assert!(output.contains("abc123 Task A"));
        assert!(output.contains("def456 Task B"));
        assert!(output.contains("Use a more specific search string"));
    }

    #[test]
    fn test_id_match_takes_precedence_over_title() {
        // If a task ID matches, title search is not used
        let tasks = vec![
            make_task("design", "Something else", "alice", "active", Some(1)),
            make_task("abc123", "design related", "bob", "active", Some(2)),
        ];
        let results = search_tasks(&tasks, "design");
        // Should match by ID, not title
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "design");
    }
}

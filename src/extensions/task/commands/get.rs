//! Task get command.
//!
//! Looks up a task by ID or title substring and displays its details.

use std::path::Path;

use super::super::config::TaskConfig;
use super::super::models::TaskTag;
use super::super::output::format_task_detail;
use super::collect_tasks;
use crate::cli;
use crate::config::ColorMode;
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;
use crate::output::format::colorize_path;

/// Indicates how a search string matched tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MatchType {
    /// Matched by exact task ID.
    ExactId,
    /// Matched by task ID prefix.
    PrefixId,
    /// Matched by case-insensitive title substring.
    TitleSubstring,
}

/// Runs the get command.
pub fn run(
    matches: &clap::ArgMatches,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(), RagtagError> {
    let search = matches
        .get_one::<String>("search")
        .expect("search is required");

    // Reject empty or whitespace-only search strings. An empty string would
    // match all task IDs via prefix, which is never the user's intent.
    if search.trim().is_empty() {
        return Err(RagtagError::InvalidInput(
            "search string must not be empty".to_string(),
        ));
    }

    let path_str = cli::resolve_path(matches);
    let path = Path::new(&path_str);

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
    let (matches_found, match_type) = search_tasks(&filtered, search);

    // Format and print results
    let output = format_results(&matches_found, search, match_type, config, &ctx.color_mode);
    write!(ctx.stdout, "{output}").map_err(RagtagError::Io)?;

    Ok(())
}

/// Searches for tasks matching the search string.
///
/// First tries exact ID match. Then tries ID prefix match. If no ID
/// match is found, falls back to case-insensitive title substring matching.
fn search_tasks<'a>(tasks: &'a [TaskTag], search: &str) -> (Vec<&'a TaskTag>, MatchType) {
    // Try exact ID match first
    let exact: Vec<&TaskTag> = tasks.iter().filter(|t| t.id == search).collect();
    if !exact.is_empty() {
        return (exact, MatchType::ExactId);
    }

    // Try ID prefix match
    let prefix: Vec<&TaskTag> = tasks.iter().filter(|t| t.id.starts_with(search)).collect();
    if !prefix.is_empty() {
        return (prefix, MatchType::PrefixId);
    }

    // Fall back to case-insensitive title substring match.
    // NOTE: `TitleSubstring` is returned even when zero results match. It acts
    // as a sentinel in this case — the `0` match arm in `format_results`
    // ignores `match_type`, so the variant has no effect on output. Adding a
    // dedicated `MatchType::None` was considered but rejected as unnecessary
    // complexity.
    let search_lower = search.to_lowercase();
    let title_matches: Vec<&TaskTag> = tasks
        .iter()
        .filter(|t| t.title.to_lowercase().contains(&search_lower))
        .collect();
    (title_matches, MatchType::TitleSubstring)
}

/// Formats the search results for display.
fn format_results(
    matches: &[&TaskTag],
    search: &str,
    match_type: MatchType,
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
            let disambiguation_hint = match match_type {
                MatchType::ExactId => {
                    format!(
                        "Multiple tasks share the exact ID \"{search}\". Task IDs must be unique.\n"
                    )
                }
                MatchType::PrefixId => {
                    format!(
                        "Multiple tasks match id prefix \"{search}\". Please provide a longer ID string.\n"
                    )
                }
                MatchType::TitleSubstring => {
                    format!(
                        "Multiple tasks match title \"{search}\". Narrow your search or use a task ID.\n"
                    )
                }
            };
            let mut output = disambiguation_hint;
            for task in matches {
                let path = colorize_path(&task.location.file_path, color_mode);
                output.push_str(&format!("{} {} {}\n", task.id, path, task.title));
            }
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
    fn test_search_exact_id_match() {
        let tasks = vec![
            make_task("abc123", "Task A", "alice", "active", Some(1)),
            make_task("def456", "Task B", "bob", "done", Some(2)),
        ];
        let (results, _match_type) = search_tasks(&tasks, "abc123");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "abc123");
    }

    #[test]
    fn test_search_title_substring() {
        let tasks = vec![
            make_task("abc123", "Design API endpoints", "alice", "active", Some(1)),
            make_task("def456", "Write tests", "bob", "active", Some(2)),
        ];
        let (results, _match_type) = search_tasks(&tasks, "design");
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
        let (results, _match_type) = search_tasks(&tasks, "DESIGN");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Design API endpoints");
    }

    #[test]
    fn test_search_multiple_matches() {
        let tasks = vec![
            make_task("abc123", "Design API", "alice", "active", Some(1)),
            make_task("def456", "Design DB", "bob", "active", Some(2)),
        ];
        let (results, _match_type) = search_tasks(&tasks, "Design");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_no_matches() {
        let tasks = vec![make_task("abc123", "Task A", "alice", "active", Some(1))];
        let (results, _match_type) = search_tasks(&tasks, "nonexistent");
        assert!(results.is_empty());
    }

    #[test]
    fn test_format_no_results() {
        let results: Vec<&TaskTag> = vec![];
        let config = TaskConfig::default();
        let output = format_results(
            &results,
            "xyz",
            MatchType::TitleSubstring,
            &config,
            &ColorMode::Never,
        );
        assert_eq!(output, "No task found for \"xyz\".\n");
    }

    #[test]
    fn test_format_single_result() {
        let task = make_task("abc123", "Test Task", "alice", "active", Some(1));
        let results = vec![&task];
        let config = TaskConfig::default();
        let output = format_results(
            &results,
            "abc123",
            MatchType::ExactId,
            &config,
            &ColorMode::Never,
        );
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
        let output = format_results(
            &results,
            "Task",
            MatchType::TitleSubstring,
            &config,
            &ColorMode::Never,
        );
        assert!(output
            .contains("Multiple tasks match title \"Task\". Narrow your search or use a task ID."));
        assert!(output.contains("abc123"));
        assert!(output.contains("Task A"));
        assert!(output.contains("def456"));
        assert!(output.contains("Task B"));
    }

    #[test]
    fn test_search_match_type_exact_id() {
        let tasks = vec![make_task("abc123", "Task A", "alice", "active", Some(1))];
        let (_results, match_type) = search_tasks(&tasks, "abc123");
        assert_eq!(match_type, MatchType::ExactId);
    }

    #[test]
    fn test_search_match_type_prefix_id() {
        let tasks = vec![make_task("abc123", "Task A", "alice", "active", Some(1))];
        let (_results, match_type) = search_tasks(&tasks, "abc");
        assert_eq!(match_type, MatchType::PrefixId);
    }

    #[test]
    fn test_search_match_type_title() {
        let tasks = vec![make_task(
            "abc123",
            "Design API",
            "alice",
            "active",
            Some(1),
        )];
        let (_results, match_type) = search_tasks(&tasks, "design");
        assert_eq!(match_type, MatchType::TitleSubstring);
    }

    #[test]
    fn test_format_multiple_id_matches() {
        let task1 = make_task("abc123", "Task A", "alice", "active", Some(1));
        let task2 = make_task("abc456", "Task B", "bob", "active", Some(2));
        let results = vec![&task1, &task2];
        let config = TaskConfig::default();
        let output = format_results(
            &results,
            "abc",
            MatchType::PrefixId,
            &config,
            &ColorMode::Never,
        );
        assert!(output.contains("Multiple tasks match id prefix \"abc\""));
        assert!(output.contains("Please provide a longer ID string"));
    }

    #[test]
    fn test_format_multiple_title_matches() {
        let task1 = make_task("abc123", "Design API", "alice", "active", Some(1));
        let task2 = make_task("def456", "Design DB", "bob", "active", Some(2));
        let results = vec![&task1, &task2];
        let config = TaskConfig::default();
        let output = format_results(
            &results,
            "Design",
            MatchType::TitleSubstring,
            &config,
            &ColorMode::Never,
        );
        assert!(output.contains("Multiple tasks match title \"Design\""));
        assert!(output.contains("Narrow your search or use a task ID"));
    }

    #[test]
    fn test_id_match_takes_precedence_over_title() {
        // If a task ID matches, title search is not used
        let tasks = vec![
            make_task("design", "Something else", "alice", "active", Some(1)),
            make_task("abc123", "design related", "bob", "active", Some(2)),
        ];
        let (results, _match_type) = search_tasks(&tasks, "design");
        // Should match by ID, not title
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "design");
    }

    #[test]
    fn test_format_multiple_exact_id_matches() {
        // When multiple tasks share the exact same ID, the ExactId message
        // should indicate a uniqueness violation, not an "id prefix" message.
        let task1 = make_task("abc123", "Task A", "alice", "active", Some(1));
        let task2 = make_task("abc123", "Task B", "bob", "active", Some(2));
        let results = vec![&task1, &task2];
        let config = TaskConfig::default();
        let output = format_results(
            &results,
            "abc123",
            MatchType::ExactId,
            &config,
            &ColorMode::Never,
        );
        assert!(output.contains("Multiple tasks share the exact ID \"abc123\""));
        assert!(output.contains("Task IDs must be unique"));
    }

    #[test]
    fn test_search_empty_string() {
        // An empty search string should be rejected before reaching search_tasks.
        // This test verifies the guard in the `run` function indirectly by
        // confirming that an empty prefix would match everything.
        let tasks = vec![
            make_task("abc123", "Task A", "alice", "active", Some(1)),
            make_task("def456", "Task B", "bob", "active", Some(2)),
        ];
        let (results, match_type) = search_tasks(&tasks, "");
        // Without the guard, empty string matches all tasks via prefix.
        assert_eq!(results.len(), 2);
        assert_eq!(match_type, MatchType::PrefixId);
    }

    #[test]
    fn test_search_no_match_returns_title_substring() {
        // When no tasks match the search string at all, `search_tasks` returns
        // `MatchType::TitleSubstring` as a sentinel. This is acceptable because
        // the `0` match arm in `format_results` ignores the `match_type`
        // entirely, so the variant has no effect on the output.
        let tasks = vec![make_task("abc123", "Task A", "alice", "active", Some(1))];
        let (results, match_type) = search_tasks(&tasks, "zzz_no_match");
        assert!(results.is_empty());
        assert_eq!(match_type, MatchType::TitleSubstring);
    }
}

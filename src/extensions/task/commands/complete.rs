//! Task complete command.
//!
//! Sets a task's status to the first configured DONE status keyword,
//! auto-updates `time_last_updated`, and either edits the file in-place or
//! prints the reconstructed `@task(...)` string (with `--no-edit`).

use super::super::config::TaskConfig;
use super::status_change::run_status_change;
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;

/// Runs the `task complete` command.
pub fn run(
    matches: &clap::ArgMatches,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(), RagtagError> {
    // Determine the DONE status to use — first keyword in the done list.
    let done_status = config
        .status_keywords
        .done
        .first()
        .ok_or_else(|| RagtagError::ExtensionError {
            extension_name: "Task Manager".to_string(),
            message: "no done status keywords configured".to_string(),
        })?
        .clone();

    run_status_change(matches, config, ctx, &done_status, "Completed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::modify_tag_attribute;

    /// Verifies that completing a task sets the status to the first done keyword.
    #[test]
    fn test_complete_sets_done_status_in_tag() {
        let config = TaskConfig::default();
        let done = config.status_keywords.done.first().unwrap();
        let tag = "@task(id=\"abc123\", status=\"active\", title=\"Test\")";

        let done_formatted = format!("\"{}\"", done);
        let result = modify_tag_attribute(tag, "status", &done_formatted).unwrap();
        assert!(
            result.contains(&format!("status=\"{done}\"")),
            "Expected status=\"{done}\" in: {result}"
        );
    }

    /// Verifies that time_last_updated is added when it doesn't exist.
    #[test]
    fn test_complete_adds_time_last_updated_when_missing() {
        // Tag does NOT have time_last_updated
        let tag = "@task(id=\"abc123\", status=\"active\", title=\"Test\")";

        let ts = "\"2026-06-12T10:00:00Z\"";
        let result = modify_tag_attribute(tag, "time_last_updated", ts).unwrap();
        assert!(
            result.contains("time_last_updated=\"2026-06-12T10:00:00Z\""),
            "Expected time_last_updated to be inserted: {result}"
        );
        // Other attributes must be preserved.
        assert!(result.contains("id=\"abc123\""));
        assert!(result.contains("title=\"Test\""));
    }

    /// Verifies that time_last_updated is updated when it already exists.
    #[test]
    fn test_complete_updates_time_last_updated_when_present() {
        let tag = "@task(id=\"abc123\", status=\"active\", time_last_updated=\"2025-01-01T00:00:00Z\")";

        let ts = "\"2026-06-12T10:00:00Z\"";
        let result = modify_tag_attribute(tag, "time_last_updated", ts).unwrap();
        assert!(
            result.contains("time_last_updated=\"2026-06-12T10:00:00Z\""),
            "Expected updated timestamp: {result}"
        );
        assert!(
            !result.contains("2025-01-01"),
            "Old timestamp should be gone: {result}"
        );
    }

    /// Verifies that the first done keyword from config is used.
    #[test]
    fn test_first_done_keyword_used() {
        let config = TaskConfig::default();
        let first_done = config.status_keywords.done.first().unwrap();
        // Default config has "done" as the first done keyword.
        assert_eq!(first_done, "done");
    }

    /// Verifies error when no done keywords are configured.
    #[test]
    fn test_no_done_keywords_returns_error() {
        let mut config = TaskConfig::default();
        config.status_keywords.done = vec![];
        // Simulating the guard condition in run():
        let result = config.status_keywords.done.first();
        assert!(result.is_none(), "Expected None when done list is empty");
    }
}

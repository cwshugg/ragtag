//! Shared helper for status-change task commands.
//!
//! All status-change commands (`complete`, `activate`, `deactivate`, `block`,
//! `abandon`) use this single helper to:
//! 1. Find the task by ID.
//! 2. Set the `status` attribute to a caller-supplied keyword.
//! 3. Auto-update `time_last_updated` to the current UTC instant.
//! 4. Either write the file atomically *or* print the modified tag when
//!    `--no-edit` is specified.

use std::path::Path;

use chrono::Utc;

use super::super::config::TaskConfig;
use super::create::escape_for_tag;
use super::find_task_by_id;
use crate::cli;
use crate::edit::{edit_task_tag, write_file_atomically};
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;

/// Runs a generic status-change command.
///
/// * `matches`       – clap argument matches for the subcommand.
/// * `config`        – task extension configuration.
/// * `ctx`           – extension context (stdout / walker / parser).
/// * `target_status` – the exact status string to set (e.g., `"active"`).
/// * `verb`          – past-tense verb used in the confirmation message
///   (e.g., `"Activated"`, `"Completed"`).
pub fn run_status_change(
    matches: &clap::ArgMatches,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
    target_status: &str,
    verb: &str,
) -> Result<(), RagtagError> {
    let id = matches.get_one::<String>("id").expect("required argument");
    let no_edit = matches.get_flag("no-edit");

    let path_str = cli::resolve_path(matches);
    let path = Path::new(&path_str);

    let (task, content) = find_task_by_id(id, path, config, ctx)?;

    // Compute the auto-updated timestamp and format attribute values.
    let now_ts = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let ts_formatted = format!("\"{}\"", escape_for_tag(&now_ts));
    let status_formatted = format!("\"{}\"", escape_for_tag(target_status));

    let original_tag = &content[task.raw_span.clone()];

    // Apply the status change and the auto-managed timestamp in a
    // single format-preserving edit. New attributes (e.g. an older
    // task missing `time_last_updated`) are appended, preserving
    // backward compatibility.
    let modified_tag = edit_task_tag(
        original_tag,
        &[("status", &status_formatted), ("time_last_updated", &ts_formatted)],
    )?;

    if no_edit {
        writeln!(ctx.stdout, "{modified_tag}").map_err(RagtagError::Io)?;
    } else {
        // Reconstruct full file content and write atomically.
        let mut new_content = String::with_capacity(content.len());
        new_content.push_str(&content[..task.raw_span.start]);
        new_content.push_str(&modified_tag);
        new_content.push_str(&content[task.raw_span.end..]);
        write_file_atomically(&task.location.file_path, &new_content)?;
        writeln!(
            ctx.stdout,
            "{verb} task {} (status → \"{}\")",
            task.id, target_status
        )
        .map_err(RagtagError::Io)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::edit::modify_tag_attribute;
    use crate::extensions::task::config::TaskConfig;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn assert_status(tag: &str, status_keyword: &str) -> String {
        let formatted = format!("\"{}\"", status_keyword);
        let result = modify_tag_attribute(tag, "status", &formatted).unwrap();
        assert!(
            result.contains(&format!("status=\"{status_keyword}\"")),
            "Expected status=\"{status_keyword}\" in: {result}"
        );
        result
    }

    // -----------------------------------------------------------------------
    // Status keyword tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_activate_status_keyword() {
        let config = TaskConfig::default();
        let kw = config.status_keywords.active.first().unwrap();
        assert_eq!(kw, "active");
    }

    #[test]
    fn test_deactivate_status_keyword() {
        let config = TaskConfig::default();
        let kw = config.status_keywords.inactive.first().unwrap();
        assert_eq!(kw, "inactive");
    }

    #[test]
    fn test_block_status_keyword() {
        let config = TaskConfig::default();
        let kw = config.status_keywords.blocked.first().unwrap();
        assert_eq!(kw, "blocked");
    }

    #[test]
    fn test_abandon_status_keyword() {
        let config = TaskConfig::default();
        let kw = config.status_keywords.abandoned.first().unwrap();
        assert_eq!(kw, "abandoned");
    }

    // -----------------------------------------------------------------------
    // Tag mutation tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_status_active_in_tag() {
        let tag = "@task(id=\"t1\", status=\"new\", title=\"T\")";
        assert_status(tag, "active");
    }

    #[test]
    fn test_set_status_inactive_in_tag() {
        let tag = "@task(id=\"t1\", status=\"active\", title=\"T\")";
        assert_status(tag, "inactive");
    }

    #[test]
    fn test_set_status_blocked_in_tag() {
        let tag = "@task(id=\"t1\", status=\"active\", title=\"T\")";
        assert_status(tag, "blocked");
    }

    #[test]
    fn test_set_status_abandoned_in_tag() {
        let tag = "@task(id=\"t1\", status=\"active\", title=\"T\")";
        assert_status(tag, "abandoned");
    }

    // -----------------------------------------------------------------------
    // time_last_updated tests (shared behaviour)
    // -----------------------------------------------------------------------

    #[test]
    fn test_time_last_updated_added_when_missing() {
        let tag = "@task(id=\"t2\", status=\"new\", title=\"T\")";
        let ts = "\"2026-06-12T10:00:00Z\"";
        let result = modify_tag_attribute(tag, "time_last_updated", ts).unwrap();
        assert!(result.contains("time_last_updated=\"2026-06-12T10:00:00Z\""));
        assert!(result.contains("id=\"t2\""));
    }

    #[test]
    fn test_time_last_updated_replaced_when_present() {
        let tag = "@task(id=\"t3\", status=\"active\", time_last_updated=\"2025-01-01T00:00:00Z\")";
        let ts = "\"2026-06-12T10:00:00Z\"";
        let result = modify_tag_attribute(tag, "time_last_updated", ts).unwrap();
        assert!(result.contains("time_last_updated=\"2026-06-12T10:00:00Z\""));
        assert!(!result.contains("2025-01-01"));
    }

    // -----------------------------------------------------------------------
    // Missing keyword config tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_no_active_keywords_is_empty() {
        let mut config = TaskConfig::default();
        config.status_keywords.active = vec![];
        assert!(config.status_keywords.active.first().is_none());
    }

    #[test]
    fn test_no_inactive_keywords_is_empty() {
        let mut config = TaskConfig::default();
        config.status_keywords.inactive = vec![];
        assert!(config.status_keywords.inactive.first().is_none());
    }

    #[test]
    fn test_no_blocked_keywords_is_empty() {
        let mut config = TaskConfig::default();
        config.status_keywords.blocked = vec![];
        assert!(config.status_keywords.blocked.first().is_none());
    }

    #[test]
    fn test_no_abandoned_keywords_is_empty() {
        let mut config = TaskConfig::default();
        config.status_keywords.abandoned = vec![];
        assert!(config.status_keywords.abandoned.first().is_none());
    }
}

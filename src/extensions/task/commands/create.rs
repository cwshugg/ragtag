//! Task create command.
//!
//! Generates a new `@task(...)` string and prints it to stdout
//! for the user to copy into their files.

use chrono::Utc;

use super::super::config::{TaskConfig, ALLOWED_WORKTIME_UNITS};
use super::super::models::{TaskTag, TaskTagBuilder, TaskType};
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;
use crate::input::prompt::{make_prompt, PromptSession};
use crate::output::tag::{escape_tag_string_body, layout_tag};

pub use crate::output::tag::TagFormat;

/// Returns the current UTC time formatted as ISO 8601 (e.g. `2026-06-12T13:29:44Z`).
pub fn now_utc() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Generates a 16-character hex task ID using `getrandom`.
pub fn generate_task_id() -> Result<String, RagtagError> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).map_err(|e| {
        RagtagError::Io(std::io::Error::other(format!(
            "failed to generate random bytes: {e}"
        )))
    })?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Escapes special characters in a string for safe embedding in a tag attribute value.
///
/// Backslashes are escaped first (to avoid double-escaping), then double quotes.
pub fn escape_for_tag(s: &str) -> String {
    escape_tag_string_body(s)
}

/// Formats a `TaskTag` as an `@task(...)` string.
pub fn format_task_string(task: &TaskTag, config: &TaskConfig, fmt: TagFormat) -> String {
    let mut attrs = Vec::new();

    attrs.push(format!("id=\"{}\"", escape_for_tag(&task.id)));
    attrs.push(format!("title=\"{}\"", escape_for_tag(&task.title)));

    if let Some(ref pid) = task.pid {
        attrs.push(format!("pid=\"{}\"", escape_for_tag(pid)));
    }

    if let Some(ref desc) = task.description {
        attrs.push(format!("description=\"{}\"", escape_for_tag(desc)));
    }

    attrs.push(format!("owner=\"{}\"", escape_for_tag(&task.owner)));
    attrs.push(format!("status=\"{}\"", escape_for_tag(&task.status)));
    attrs.push(format!(
        "type=\"{}\"",
        escape_for_tag(task.task_type.as_str())
    ));

    if let Some(priority) = task.priority {
        attrs.push(format!("priority={priority}"));
    }

    // worktime_spent always included; defaults to 0 when not set
    let worktime_spent = task.worktime_spent.unwrap_or(0.0);
    attrs.push(format!("worktime_spent={worktime_spent}"));

    if let Some(worktime_estimate) = task.worktime_estimate {
        attrs.push(format!("worktime_estimate={worktime_estimate}"));
    }

    if let Some(ref time_created) = task.time_created {
        attrs.push(format!("time_created=\"{}\"", escape_for_tag(time_created)));
    }

    if let Some(ref time_last_updated) = task.time_last_updated {
        attrs.push(format!(
            "time_last_updated=\"{}\"",
            escape_for_tag(time_last_updated)
        ));
    }

    attrs.push(format!(
        "worktime_units=\"{}\"",
        escape_for_tag(&task.worktime_units)
    ));

    layout_tag(&config.tag_name, &attrs, fmt)
}

/// Runs the create command.
pub fn run(
    matches: &clap::ArgMatches,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(), RagtagError> {
    let id = generate_task_id()?;

    // Resolve --format flag
    let fmt = match matches.get_one::<String>("format").map(|s| s.as_str()) {
        Some("oneline") => TagFormat::Oneline,
        _ => TagFormat::Multiline,
    };

    let mut builder = TaskTagBuilder::new();
    builder.id = Some(id);
    // Treat `--title ""` (empty or whitespace-only) the same as omitting
    // `--title` — both routes fall through to interactive mode so that the
    // behavior matches the interactive `prompt_required` which rejects empty
    // titles.
    builder.title = matches
        .get_one::<String>("title")
        .filter(|s| !s.trim().is_empty())
        .cloned();
    builder.description = matches.get_one::<String>("description").cloned();
    builder.owner = matches.get_one::<String>("owner").cloned();
    builder.status = matches.get_one::<String>("status").cloned();
    builder.task_type = matches
        .get_one::<String>("type")
        .map(|value| TaskType::from_input(value));
    builder.priority = matches
        .get_one::<String>("priority")
        .and_then(|s| s.parse().ok());
    builder.worktime_spent = matches
        .get_one::<String>("worktime-spent")
        .and_then(|s| s.parse().ok());
    builder.worktime_estimate = matches
        .get_one::<String>("worktime-estimate")
        .and_then(|s| s.parse().ok());
    builder.worktime_units = matches.get_one::<String>("worktime-units").cloned();
    builder.pid = matches.get_one::<String>("pid").cloned();

    // Auto-set timestamps — never user-supplied.
    let ts = now_utc();
    builder.time_created = Some(ts.clone());
    builder.time_last_updated = Some(ts);

    // If title is missing, fall back to interactive mode for remaining fields
    if builder.title.is_none() {
        return run_interactive(config, ctx, builder, fmt);
    }

    let task = builder.build(config)?;
    let output = format_task_string(&task, config, fmt);
    writeln!(ctx.stdout, "{output}").map_err(RagtagError::Io)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Input validation helpers
// ---------------------------------------------------------------------------

/// Validates a priority string — must parse as a non-negative integer (`u32`).
pub(crate) fn validate_priority(v: &str) -> Result<(), String> {
    v.parse::<u32>()
        .map(|_| ())
        .map_err(|_| "Invalid priority \u{2014} must be a non-negative whole number.".to_string())
}

/// Validates a worktime estimate string — must parse as a non-negative `f64`.
pub(crate) fn validate_worktime_estimate(v: &str) -> Result<(), String> {
    v.parse::<f64>()
        .map_err(|_| {
            "Invalid worktime estimate \u{2014} must be a non-negative number.".to_string()
        })
        .and_then(|f| {
            if f >= 0.0 {
                Ok(())
            } else {
                Err("Invalid worktime estimate \u{2014} must be a non-negative number.".to_string())
            }
        })
}

/// Validates a worktime spent string — must parse as a non-negative `f64`.
pub(crate) fn validate_worktime_spent(v: &str) -> Result<(), String> {
    v.parse::<f64>()
        .map_err(|_| "Invalid worktime spent \u{2014} must be a non-negative number.".to_string())
        .and_then(|f| {
            if f >= 0.0 {
                Ok(())
            } else {
                Err("Invalid worktime spent \u{2014} must be a non-negative number.".to_string())
            }
        })
}

/// Validates a status string against the configured keyword list.
pub(crate) fn validate_status(v: &str, allowed: &[String]) -> Result<(), String> {
    if allowed.iter().any(|s| s == v) {
        Ok(())
    } else {
        Err(format!(
            "Invalid status \u{2014} allowed values: {}",
            allowed.join(", ")
        ))
    }
}

/// Validates a worktime-units string against the fixed allowed set.
pub(crate) fn validate_worktime_units(v: &str) -> Result<(), String> {
    if ALLOWED_WORKTIME_UNITS.contains(&v) {
        Ok(())
    } else {
        Err(format!(
            "Invalid worktime units \u{2014} allowed values: {}",
            ALLOWED_WORKTIME_UNITS.join(", ")
        ))
    }
}

// ---------------------------------------------------------------------------
// Interactive task creation
// ---------------------------------------------------------------------------

/// Runs interactive task creation, prompting for any fields not already set in the builder.
fn run_interactive(
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
    mut builder: TaskTagBuilder,
    fmt: TagFormat,
) -> Result<(), RagtagError> {
    if builder.id.is_none() {
        builder.id = Some(generate_task_id()?);
    }

    // Auto-set timestamps — never user-supplied.
    let ts = now_utc();
    builder.time_created = Some(ts.clone());
    builder.time_last_updated = Some(ts);

    let mut session = PromptSession::new()?;

    // ------------------------------------------------------------------
    // Title (required — re-prompt until non-empty or user cancels)
    // ------------------------------------------------------------------
    if builder.title.is_none() {
        let prompt = make_prompt("Title", None, session.is_tty);
        match session.prompt_required(&prompt, "Title is required.", ctx.stderr)? {
            Some(title) => builder.title = Some(title),
            None => {
                // Only reachable via TTY Ctrl+C / Ctrl+D
                writeln!(ctx.stderr, "Cancelled.").map_err(RagtagError::Io)?;
                return Ok(());
            }
        }
    }

    // Macro: after each optional prompt, bail out cleanly if the user cancelled.
    macro_rules! check_cancelled {
        () => {
            if session.cancelled {
                writeln!(ctx.stderr, "Cancelled.").map_err(RagtagError::Io)?;
                return Ok(());
            }
        };
    }

    let owner_default = config.default_owner.clone();
    let status_default = config.default_status.clone();
    let worktime_units_default = config.default_worktime_units.clone();

    // Description (free-form — no validation)
    if builder.description.is_none() {
        let prompt = make_prompt("Description", Some("(leave blank to skip)"), session.is_tty);
        builder.description = session.prompt_optional(&prompt, ctx.stderr, |_| Ok(()))?;
        check_cancelled!();
    }

    // Owner (free-form — no validation)
    if builder.owner.is_none() {
        let hint = format!("(leave blank to skip; default: {owner_default})");
        let prompt = make_prompt("Owner", Some(&hint), session.is_tty);
        builder.owner = session.prompt_optional(&prompt, ctx.stderr, |_| Ok(()))?;
        check_cancelled!();
    }

    // Status (must be a recognised keyword)
    if builder.status.is_none() {
        let all_statuses: Vec<String> = config
            .all_status_keywords()
            .iter()
            .map(|s| s.to_string())
            .collect();
        let hint = format!("(leave blank to skip; default: {status_default})");
        let prompt = make_prompt("Status", Some(&hint), session.is_tty);
        builder.status = session.prompt_optional(&prompt, ctx.stderr, move |v| {
            validate_status(v, &all_statuses)
        })?;
        check_cancelled!();
    }

    // Priority (non-negative integer)
    if builder.priority.is_none() {
        let prompt = make_prompt("Priority", Some("(leave blank to skip)"), session.is_tty);
        let raw = session.prompt_optional(&prompt, ctx.stderr, validate_priority)?;
        check_cancelled!();
        builder.priority = raw.and_then(|v| v.parse().ok());
    }

    // Worktime Estimate (non-negative float)
    if builder.worktime_estimate.is_none() {
        let prompt = make_prompt(
            "Worktime Estimate",
            Some("(leave blank to skip)"),
            session.is_tty,
        );
        let raw = session.prompt_optional(&prompt, ctx.stderr, validate_worktime_estimate)?;
        check_cancelled!();
        builder.worktime_estimate = raw.and_then(|v| v.parse().ok());
    }

    // Worktime Already Spent (non-negative float; defaults to 0 via format_task_string)
    if builder.worktime_spent.is_none() {
        let prompt = make_prompt(
            "Worktime Already Spent",
            Some("(leave blank to skip; default: 0)"),
            session.is_tty,
        );
        let raw = session.prompt_optional(&prompt, ctx.stderr, validate_worktime_spent)?;
        check_cancelled!();
        builder.worktime_spent = raw.and_then(|v| v.parse().ok());
    }

    // Worktime Units (must be one of the fixed allowed values)
    if builder.worktime_units.is_none() {
        let hint = format!("(leave blank to skip; default: {worktime_units_default})");
        let prompt = make_prompt("Worktime Units", Some(&hint), session.is_tty);
        builder.worktime_units =
            session.prompt_optional(&prompt, ctx.stderr, validate_worktime_units)?;
        check_cancelled!();
    }

    // Parent ID (free-form — no validation)
    if builder.pid.is_none() {
        let prompt = make_prompt("Parent ID", Some("(leave blank to skip)"), session.is_tty);
        builder.pid = session.prompt_optional(&prompt, ctx.stderr, |_| Ok(()))?;
        check_cancelled!();
    }

    let task = builder.build(config)?;
    let output = format_task_string(&task, config, fmt);
    writeln!(ctx.stdout, "{output}").map_err(RagtagError::Io)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_task_id() {
        let id = generate_task_id().unwrap();
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_generate_task_id_unique() {
        let id1 = generate_task_id().unwrap();
        let id2 = generate_task_id().unwrap();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_format_task_string() {
        let config = TaskConfig::default();
        let mut builder = TaskTagBuilder::new();
        builder.id = Some("abc123def456789a".to_string());
        builder.title = Some("Test Task".to_string());
        builder.worktime_estimate = Some(4.5);
        let task = builder.build(&config).unwrap();

        let output = format_task_string(&task, &config, TagFormat::Multiline);
        assert!(output.starts_with("@task("));
        assert!(output.contains("id=\"abc123def456789a\""));
        assert!(output.contains("title=\"Test Task\""));
        assert!(output.contains("worktime_estimate=4.5"));
        assert!(output.contains("worktime_units=\"hours\""));
        assert!(output.contains("type=\"item\""));
        assert!(output.ends_with(")\n") || output.ends_with(')'));
    }

    #[test]
    fn test_format_task_string_uses_canonical_project_type() {
        let config = TaskConfig::default();
        let mut builder = TaskTagBuilder::new();
        builder.id = Some("abc123def456789a".to_string());
        builder.title = Some("Project".to_string());
        builder.task_type = Some(TaskType::Project);
        let task = builder.build(&config).unwrap();

        let output = format_task_string(&task, &config, TagFormat::Oneline);
        assert!(output.contains("type=\"project\""));
    }

    #[test]
    fn test_format_task_string_worktime_spent_default_zero() {
        // worktime_spent should always appear in the output, defaulting to 0
        let config = TaskConfig::default();
        let mut builder = TaskTagBuilder::new();
        builder.id = Some("abc123def456789a".to_string());
        builder.title = Some("Test Task".to_string());
        // worktime_spent intentionally not set
        let task = builder.build(&config).unwrap();

        let output = format_task_string(&task, &config, TagFormat::Multiline);
        assert!(output.contains("worktime_spent=0"));
    }

    #[test]
    fn test_format_task_string_worktime_spent_explicit_value() {
        // worktime_spent should reflect the explicitly provided value
        let config = TaskConfig::default();
        let mut builder = TaskTagBuilder::new();
        builder.id = Some("abc123def456789a".to_string());
        builder.title = Some("Test Task".to_string());
        builder.worktime_spent = Some(3.5);
        let task = builder.build(&config).unwrap();

        let output = format_task_string(&task, &config, TagFormat::Multiline);
        assert!(output.contains("worktime_spent=3.5"));
    }

    #[test]
    fn test_format_task_string_multiline() {
        // Multiline format: each attr on its own indented line
        let config = TaskConfig::default();
        let mut builder = TaskTagBuilder::new();
        builder.id = Some("abc123def456789a".to_string());
        builder.title = Some("My Task".to_string());
        let task = builder.build(&config).unwrap();

        let output = format_task_string(&task, &config, TagFormat::Multiline);
        assert!(output.starts_with("@task(\n"));
        assert!(output.ends_with("\n)"));
        // Each attribute line should be indented
        assert!(output.contains("    id=\"abc123def456789a\""));
        assert!(output.contains("    title=\"My Task\""));
    }

    #[test]
    fn test_format_task_string_oneline() {
        // Oneline format: everything on a single line, no newlines inside
        let config = TaskConfig::default();
        let mut builder = TaskTagBuilder::new();
        builder.id = Some("abc123def456789a".to_string());
        builder.title = Some("My Task".to_string());
        let task = builder.build(&config).unwrap();

        let output = format_task_string(&task, &config, TagFormat::Oneline);
        assert!(output.starts_with("@task("));
        assert!(output.ends_with(')'));
        // Must be a single line (no embedded newlines)
        assert!(!output.contains('\n'));
        // Attributes should be comma-space separated
        assert!(output.contains("id=\"abc123def456789a\", title=\"My Task\""));
    }

    #[test]
    fn test_format_task_string_oneline_contains_required_fields() {
        let config = TaskConfig::default();
        let mut builder = TaskTagBuilder::new();
        builder.id = Some("abc123def456789a".to_string());
        builder.title = Some("My Task".to_string());
        builder.worktime_estimate = Some(2.0);
        let task = builder.build(&config).unwrap();

        let output = format_task_string(&task, &config, TagFormat::Oneline);
        assert!(output.contains("worktime_spent=0"));
        assert!(output.contains("worktime_estimate=2"));
        assert!(output.contains("worktime_units=\"hours\""));
        // No indentation
        assert!(!output.contains("    "));
    }

    #[test]
    fn test_task_format_exact_compatibility_fixtures() {
        let config = TaskConfig::default();
        let mut builder = TaskTagBuilder::new();
        builder.id = Some("0123456789abcdef".to_string());
        builder.title = Some("Exact \"task\"".to_string());
        builder.worktime_estimate = Some(2.0);
        builder.time_created = Some("2026-09-29T12:00:00Z".to_string());
        builder.time_last_updated = Some("2026-09-29T12:00:00Z".to_string());
        let task = builder.build(&config).unwrap();

        assert_eq!(
            format_task_string(&task, &config, TagFormat::Oneline),
            "@task(id=\"0123456789abcdef\", title=\"Exact \\\"task\\\"\", owner=\"me\", status=\"new\", type=\"item\", worktime_spent=0, worktime_estimate=2, time_created=\"2026-09-29T12:00:00Z\", time_last_updated=\"2026-09-29T12:00:00Z\", worktime_units=\"hours\")"
        );
        assert_eq!(
            format_task_string(&task, &config, TagFormat::Multiline),
            "@task(\n    id=\"0123456789abcdef\",\n    title=\"Exact \\\"task\\\"\",\n    owner=\"me\",\n    status=\"new\",\n    type=\"item\",\n    worktime_spent=0,\n    worktime_estimate=2,\n    time_created=\"2026-09-29T12:00:00Z\",\n    time_last_updated=\"2026-09-29T12:00:00Z\",\n    worktime_units=\"hours\"\n)"
        );
    }

    #[test]
    fn test_format_task_string_with_optional_fields() {
        let config = TaskConfig::default();
        let mut builder = TaskTagBuilder::new();
        builder.id = Some("abc123def456789a".to_string());
        builder.title = Some("Test".to_string());
        builder.worktime_estimate = Some(2.0);
        builder.description = Some("A description".to_string());
        builder.priority = Some(0);
        builder.time_created = Some("2026-06-12T09:00:00Z".to_string());
        builder.time_last_updated = Some("2026-06-12T10:00:00Z".to_string());
        let task = builder.build(&config).unwrap();

        let output = format_task_string(&task, &config, TagFormat::Multiline);
        assert!(output.contains("description=\"A description\""));
        assert!(output.contains("priority=0"));
        assert!(output.contains("time_created=\"2026-06-12T09:00:00Z\""));
        assert!(output.contains("time_last_updated=\"2026-06-12T10:00:00Z\""));
    }

    #[test]
    fn test_format_task_string_with_pid() {
        let config = TaskConfig::default();
        let mut builder = TaskTagBuilder::new();
        builder.id = Some("abc123def456789a".to_string());
        builder.title = Some("Child Task".to_string());
        builder.worktime_estimate = Some(2.0);
        builder.pid = Some("parent0000000000".to_string());
        let task = builder.build(&config).unwrap();

        let output = format_task_string(&task, &config, TagFormat::Multiline);
        assert!(output.contains("pid=\"parent0000000000\""));
    }

    #[test]
    fn test_escape_for_tag_quotes() {
        assert_eq!(escape_for_tag(r#"Say "hello""#), r#"Say \"hello\""#);
    }

    #[test]
    fn test_escape_for_tag_backslashes() {
        assert_eq!(escape_for_tag(r"path\to\file"), r"path\\to\\file");
    }

    #[test]
    fn test_escape_for_tag_combined() {
        assert_eq!(escape_for_tag(r#"a "b\" c"#), r#"a \"b\\\" c"#);
    }

    #[test]
    fn test_format_task_string_with_special_chars() {
        let config = TaskConfig::default();
        let mut builder = TaskTagBuilder::new();
        builder.id = Some("abc123def456789a".to_string());
        builder.title = Some("Say \"hello\"".to_string());
        builder.worktime_estimate = Some(1.0);
        let task = builder.build(&config).unwrap();

        let output = format_task_string(&task, &config, TagFormat::Multiline);
        // The output should contain escaped quotes
        assert!(output.contains(r#"title="Say \"hello\"""#));
    }

    // -----------------------------------------------------------------------
    // Validator unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_priority_valid() {
        assert!(validate_priority("0").is_ok());
        assert!(validate_priority("1").is_ok());
        assert!(validate_priority("42").is_ok());
        assert!(validate_priority("4294967295").is_ok()); // u32::MAX
    }

    #[test]
    fn test_validate_priority_invalid() {
        let err = validate_priority("m").unwrap_err();
        assert!(
            err.contains("non-negative whole number"),
            "unexpected message: {err}"
        );
        assert!(validate_priority("-1").is_err());
        assert!(validate_priority("1.5").is_err());
        assert!(validate_priority("").is_err());
        assert!(validate_priority("abc").is_err());
    }

    #[test]
    fn test_validate_worktime_estimate_valid() {
        assert!(validate_worktime_estimate("0").is_ok());
        assert!(validate_worktime_estimate("0.0").is_ok());
        assert!(validate_worktime_estimate("3.5").is_ok());
        assert!(validate_worktime_estimate("100").is_ok());
    }

    #[test]
    fn test_validate_worktime_estimate_invalid() {
        let err = validate_worktime_estimate("abc").unwrap_err();
        assert!(
            err.contains("non-negative number"),
            "unexpected message: {err}"
        );
        assert!(validate_worktime_estimate("-1").is_err());
        assert!(validate_worktime_estimate("-0.5").is_err());
        assert!(validate_worktime_estimate("").is_err());
    }

    #[test]
    fn test_validate_worktime_spent_valid() {
        assert!(validate_worktime_spent("0").is_ok());
        assert!(validate_worktime_spent("2.5").is_ok());
        assert!(validate_worktime_spent("10").is_ok());
    }

    #[test]
    fn test_validate_worktime_spent_invalid() {
        let err = validate_worktime_spent("oops").unwrap_err();
        assert!(
            err.contains("non-negative number"),
            "unexpected message: {err}"
        );
        assert!(validate_worktime_spent("-1").is_err());
        assert!(validate_worktime_spent("").is_err());
    }

    #[test]
    fn test_validate_status_valid() {
        let config = TaskConfig::default();
        let allowed: Vec<String> = config
            .all_status_keywords()
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(validate_status("new", &allowed).is_ok());
        assert!(validate_status("active", &allowed).is_ok());
        assert!(validate_status("done", &allowed).is_ok());
        assert!(validate_status("blocked", &allowed).is_ok());
    }

    #[test]
    fn test_validate_status_invalid() {
        let config = TaskConfig::default();
        let allowed: Vec<String> = config
            .all_status_keywords()
            .iter()
            .map(|s| s.to_string())
            .collect();
        let err = validate_status("banana", &allowed).unwrap_err();
        assert!(err.contains("Invalid status"), "unexpected message: {err}");
        assert!(
            err.contains("allowed values:"),
            "expected allowed list: {err}"
        );
        // Spot-check that actual keywords appear in the error
        assert!(err.contains("new"), "expected 'new' in: {err}");
    }

    #[test]
    fn test_validate_worktime_units_valid() {
        assert!(validate_worktime_units("hours").is_ok());
        assert!(validate_worktime_units("days").is_ok());
        assert!(validate_worktime_units("weeks").is_ok());
    }

    #[test]
    fn test_validate_worktime_units_invalid() {
        let err = validate_worktime_units("fortnights").unwrap_err();
        assert!(
            err.contains("Invalid worktime units"),
            "unexpected message: {err}"
        );
        assert!(err.contains("hours"), "expected 'hours' in: {err}");
        assert!(validate_worktime_units("minutes").is_err());
        assert!(validate_worktime_units("").is_err());
    }
}

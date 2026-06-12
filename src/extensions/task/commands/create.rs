//! Task create command.
//!
//! Generates a new `@task(...)` string and prints it to stdout
//! for the user to copy into their files.

use std::io::BufRead;

use chrono::Utc;

use super::super::config::TaskConfig;
use super::super::models::{TaskTag, TaskTagBuilder};
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;

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
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Formats a `TaskTag` as an `@task(...)` string.
pub fn format_task_string(task: &TaskTag, config: &TaskConfig) -> String {
    let mut attrs = Vec::new();

    attrs.push(format!("    id=\"{}\"", escape_for_tag(&task.id)));
    attrs.push(format!("    title=\"{}\"", escape_for_tag(&task.title)));

    if let Some(ref pid) = task.pid {
        attrs.push(format!("    pid=\"{}\"", escape_for_tag(pid)));
    }

    if let Some(ref desc) = task.description {
        attrs.push(format!("    description=\"{}\"", escape_for_tag(desc)));
    }

    attrs.push(format!("    owner=\"{}\"", escape_for_tag(&task.owner)));
    attrs.push(format!("    status=\"{}\"", escape_for_tag(&task.status)));

    if let Some(priority) = task.priority {
        attrs.push(format!("    priority={priority}"));
    }

    if let Some(worktime_spent) = task.worktime_spent {
        attrs.push(format!("    worktime_spent={worktime_spent}"));
    }

    if let Some(worktime_estimate) = task.worktime_estimate {
        attrs.push(format!("    worktime_estimate={worktime_estimate}"));
    }

    if let Some(ref time_created) = task.time_created {
        attrs.push(format!(
            "    time_created=\"{}\"",
            escape_for_tag(time_created)
        ));
    }

    if let Some(ref time_last_updated) = task.time_last_updated {
        attrs.push(format!(
            "    time_last_updated=\"{}\"",
            escape_for_tag(time_last_updated)
        ));
    }

    attrs.push(format!(
        "    worktime_units=\"{}\"",
        escape_for_tag(&task.worktime_units)
    ));

    format!("@{}(\n{}\n)", config.tag_name, attrs.join(",\n"))
}

/// Runs the create command.
pub fn run(
    matches: &clap::ArgMatches,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(), RagtagError> {
    let id = generate_task_id()?;

    let mut builder = TaskTagBuilder::new();
    builder.id = Some(id);
    builder.title = matches.get_one::<String>("title").cloned();
    builder.description = matches.get_one::<String>("description").cloned();
    builder.owner = matches.get_one::<String>("owner").cloned();
    builder.status = matches.get_one::<String>("status").cloned();
    builder.priority = matches
        .get_one::<String>("priority")
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
        return run_interactive(config, ctx, builder);
    }

    let task = builder.build(config)?;
    let output = format_task_string(&task, config);
    writeln!(ctx.stdout, "{output}").map_err(RagtagError::Io)?;

    Ok(())
}

/// Runs interactive task creation, prompting for any fields not already set in the builder.
fn run_interactive(
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
    mut builder: TaskTagBuilder,
) -> Result<(), RagtagError> {
    if builder.id.is_none() {
        builder.id = Some(generate_task_id()?);
    }

    // Auto-set timestamps — never user-supplied.
    let ts = now_utc();
    builder.time_created = Some(ts.clone());
    builder.time_last_updated = Some(ts);

    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();

    // Prompt for title if not already set
    if builder.title.is_none() {
        loop {
            write!(ctx.stderr, "Title: ").map_err(RagtagError::Io)?;
            ctx.stderr.flush().map_err(RagtagError::Io)?;
            if let Some(Ok(line)) = lines.next() {
                let trimmed = line.trim().to_string();
                if !trimmed.is_empty() {
                    builder.title = Some(trimmed);
                    break;
                }
                writeln!(ctx.stderr, "  Title is required. Please try again.")
                    .map_err(RagtagError::Io)?;
            } else {
                return Err(RagtagError::Io(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "unexpected end of input",
                )));
            }
        }
    }

    // Prompt for optional fields that aren't already set
    let optional_prompts: Vec<(&str, &str, bool)> = vec![
        ("Description", "description", builder.description.is_some()),
        ("Owner", "owner", builder.owner.is_some()),
        ("Status", "status", builder.status.is_some()),
        ("Priority", "priority", builder.priority.is_some()),
        (
            "Worktime Estimate",
            "worktime_estimate",
            builder.worktime_estimate.is_some(),
        ),
        (
            "Worktime Units",
            "worktime_units",
            builder.worktime_units.is_some(),
        ),
        ("Parent ID", "pid", builder.pid.is_some()),
    ];

    for (label, field, already_set) in &optional_prompts {
        if *already_set {
            continue;
        }
        write!(ctx.stderr, "{label} (leave blank to skip): ").map_err(RagtagError::Io)?;
        ctx.stderr.flush().map_err(RagtagError::Io)?;
        if let Some(Ok(line)) = lines.next() {
            let trimmed = line.trim().to_string();
            if !trimmed.is_empty() {
                match *field {
                    "description" => builder.description = Some(trimmed),
                    "owner" => builder.owner = Some(trimmed),
                    "status" => builder.status = Some(trimmed),
                    "priority" => {
                        if let Ok(p) = trimmed.parse::<u32>() {
                            builder.priority = Some(p);
                        }
                    }
                    "worktime_estimate" => {
                        if let Ok(v) = trimmed.parse::<f64>() {
                            builder.worktime_estimate = Some(v);
                        }
                    }
                    "worktime_units" => builder.worktime_units = Some(trimmed),
                    "pid" => builder.pid = Some(trimmed),
                    _ => {}
                }
            }
        }
    }

    let task = builder.build(config)?;
    let output = format_task_string(&task, config);
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

        let output = format_task_string(&task, &config);
        assert!(output.starts_with("@task("));
        assert!(output.contains("id=\"abc123def456789a\""));
        assert!(output.contains("title=\"Test Task\""));
        assert!(output.contains("worktime_estimate=4.5"));
        assert!(output.contains("worktime_units=\"hours\""));
        assert!(output.ends_with(")\n") || output.ends_with(')'));
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

        let output = format_task_string(&task, &config);
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

        let output = format_task_string(&task, &config);
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

        let output = format_task_string(&task, &config);
        // The output should contain escaped quotes
        assert!(output.contains(r#"title="Say \"hello\"""#));
    }
}

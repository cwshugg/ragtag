//! Task create command.
//!
//! Generates a new `@task(...)` string and prints it to stdout
//! for the user to copy into their files.

use std::io::{BufRead, IsTerminal, Write};

use chrono::Utc;
use owo_colors::OwoColorize;
use rustyline::error::ReadlineError;

use super::super::config::{TaskConfig, ALLOWED_WORKTIME_UNITS};
use super::super::models::{TaskTag, TaskTagBuilder};
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;

/// Controls whether `format_task_string` produces multi-line or single-line output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagFormat {
    /// Indented multi-line format (default).
    Multiline,
    /// Everything on a single line.
    Oneline,
}

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
        attrs.push(format!(
            "time_created=\"{}\"",
            escape_for_tag(time_created)
        ));
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

    match fmt {
        TagFormat::Multiline => {
            let indented: Vec<String> = attrs.iter().map(|a| format!("    {a}")).collect();
            format!("@{}(\n{}\n)", config.tag_name, indented.join(",\n"))
        }
        TagFormat::Oneline => {
            format!("@{}({})", config.tag_name, attrs.join(", "))
        }
    }
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
    builder.title = matches.get_one::<String>("title").cloned();
    builder.description = matches.get_one::<String>("description").cloned();
    builder.owner = matches.get_one::<String>("owner").cloned();
    builder.status = matches.get_one::<String>("status").cloned();
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
    v.parse::<u32>().map(|_| ()).map_err(|_| {
        "Invalid priority \u{2014} must be a non-negative whole number.".to_string()
    })
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
                Err("Invalid worktime estimate \u{2014} must be a non-negative number."
                    .to_string())
            }
        })
}

/// Validates a worktime spent string — must parse as a non-negative `f64`.
pub(crate) fn validate_worktime_spent(v: &str) -> Result<(), String> {
    v.parse::<f64>()
        .map_err(|_| {
            "Invalid worktime spent \u{2014} must be a non-negative number.".to_string()
        })
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
// Color helpers
// ---------------------------------------------------------------------------

/// Gray-blue RGB used for prompt field names  (R=140, G=170, B=210).
const FIELD_R: u8 = 140;
const FIELD_G: u8 = 170;
const FIELD_B: u8 = 210;

/// Dark gray RGB used for prompt hint text (R=128, G=128, B=128).
const HINT_R: u8 = 128;
const HINT_G: u8 = 128;
const HINT_B: u8 = 128;

/// Wraps each ANSI CSI escape sequence in rustyline-compatible `\x01`/`\x02` markers.
///
/// Without these markers, rustyline miscalculates the visible prompt width, causing
/// cursor positioning errors when the user uses arrow keys or line-editing shortcuts.
fn wrap_ansi_for_rustyline(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 32);
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // CSI begins with ESC (0x1b) followed by '['.
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            result.push('\x01'); // begin non-printing section
            result.push('\x1b');
            result.push('[');
            i += 2;
            // Consume parameter bytes and the final alphabetic terminator byte.
            while i < bytes.len() {
                let b = bytes[i];
                result.push(b as char);
                i += 1;
                if b.is_ascii_alphabetic() {
                    break;
                }
            }
            result.push('\x02'); // end non-printing section
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

/// Builds a prompt string for interactive input.
///
/// * `field`  — the field name (e.g. `"Title"`).
/// * `hint`   — optional hint text shown after the field name in parentheses
///              (e.g. `"(leave blank to skip)"`).
/// * `is_tty` — when `true`, emits ANSI colour codes and rustyline-safe wrappers;
///              when `false`, produces a plain-text prompt suitable for piped input.
fn make_prompt(field: &str, hint: Option<&str>, is_tty: bool) -> String {
    if !is_tty {
        return match hint {
            Some(h) => format!("{field} {h}: "),
            None => format!("{field}: "),
        };
    }

    let colored_field = field.truecolor(FIELD_R, FIELD_G, FIELD_B).to_string();
    let colored_colon = ": ".truecolor(FIELD_R, FIELD_G, FIELD_B).to_string();

    let plain = match hint {
        Some(h) => {
            let colored_hint = h.truecolor(HINT_R, HINT_G, HINT_B).to_string();
            format!("{colored_field} {colored_hint}{colored_colon}")
        }
        None => format!("{colored_field}{colored_colon}"),
    };

    wrap_ansi_for_rustyline(&plain)
}

// ---------------------------------------------------------------------------
// Interactive prompting
// ---------------------------------------------------------------------------

/// A prompting session for interactive task creation.
///
/// In TTY mode, uses [`rustyline`] for full line-editing support (arrow keys,
/// Ctrl+A/E, history, etc.).  In piped / non-TTY mode, falls back to plain
/// stdin reads with prompts written to stderr.
struct PromptSession {
    /// Present only when stdin is a terminal.
    tty: Option<rustyline::DefaultEditor>,
    /// Whether stdin is a TTY (mirrors `tty.is_some()`).
    pub is_tty: bool,
    /// Set to `true` when the user cancels via Ctrl+C or Ctrl+D in TTY mode.
    pub cancelled: bool,
}

impl PromptSession {
    /// Creates a new session, auto-detecting whether stdin is a TTY.
    fn new() -> Result<Self, RagtagError> {
        let tty = if std::io::stdin().is_terminal() {
            Some(rustyline::DefaultEditor::new().map_err(|e| {
                RagtagError::Io(std::io::Error::other(format!(
                    "failed to initialise line editor: {e}"
                )))
            })?)
        } else {
            None
        };
        let is_tty = tty.is_some();
        Ok(Self { tty, is_tty, cancelled: false })
    }

    /// Writes a coloured error line to `stderr`.
    ///
    /// On a TTY the `"Error: …"` prefix is rendered in red; on piped input it
    /// is written as plain text so tests and scripts see predictable output.
    fn write_error(&self, stderr: &mut dyn Write, msg: &str) -> Result<(), RagtagError> {
        if self.is_tty {
            writeln!(stderr, "  {}", format!("Error: {msg}").red())
        } else {
            writeln!(stderr, "  Error: {msg}")
        }
        .map_err(RagtagError::Io)
    }

    /// Reads one raw line from the user.
    ///
    /// Returns `Ok(Some(line))` for normal input, or `Ok(None)` when:
    /// - TTY mode: the user pressed Ctrl+C / Ctrl+D → also sets `self.cancelled`.
    /// - Piped mode: stdin reached EOF (remaining fields will be skipped).
    ///
    /// If `self.cancelled` is already `true`, returns `Ok(None)` immediately
    /// so callers can short-circuit without touching the terminal.
    fn read_line(
        &mut self,
        prompt: &str,
        stderr: &mut dyn Write,
    ) -> Result<Option<String>, RagtagError> {
        if self.cancelled {
            return Ok(None);
        }

        if let Some(ref mut rl) = self.tty {
            match rl.readline(prompt) {
                Ok(line) => Ok(Some(line)),
                Err(ReadlineError::Eof) | Err(ReadlineError::Interrupted) => {
                    // Ensure the next output starts on a fresh line.
                    writeln!(stderr, "").map_err(RagtagError::Io)?;
                    self.cancelled = true;
                    Ok(None)
                }
                Err(e) => Err(RagtagError::Io(std::io::Error::other(format!(
                    "readline error: {e}"
                )))),
            }
        } else {
            // Piped mode: write the prompt ourselves and read from stdin.
            write!(stderr, "{prompt}").map_err(RagtagError::Io)?;
            stderr.flush().map_err(RagtagError::Io)?;
            let stdin = std::io::stdin();
            let mut line = String::new();
            match stdin.lock().read_line(&mut line) {
                Ok(0) => Ok(None), // EOF — don't set cancelled; just skip remaining fields
                Ok(_) => Ok(Some(line)),
                Err(e) => Err(RagtagError::Io(e)),
            }
        }
    }

    /// Prompts for a required field (e.g. title).
    ///
    /// Re-prompts with `empty_error` whenever the user submits a blank line.
    /// Returns `Ok(Some(value))` once a non-empty string is received.
    /// Returns `Ok(None)` only if the user cancels in TTY mode.
    /// Returns `Err` on unexpected EOF in piped mode or on I/O failure.
    fn prompt_required(
        &mut self,
        prompt: &str,
        empty_error: &str,
        stderr: &mut dyn Write,
    ) -> Result<Option<String>, RagtagError> {
        loop {
            match self.read_line(prompt, stderr)? {
                None => {
                    if self.cancelled {
                        return Ok(None); // TTY cancellation — caller handles graceful exit
                    }
                    // Piped mode EOF on a required field — propagate as an error
                    return Err(RagtagError::Io(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "unexpected end of input while reading required field",
                    )));
                }
                Some(line) => {
                    let trimmed = line.trim().to_string();
                    if trimmed.is_empty() {
                        self.write_error(stderr, empty_error)?;
                    } else {
                        return Ok(Some(trimmed));
                    }
                }
            }
        }
    }

    /// Prompts for an optional field with a validation callback.
    ///
    /// - Blank input → returns `Ok(None)` (skip / use default).
    /// - Invalid non-blank input → prints the validator's error message and re-prompts.
    /// - Valid input → returns `Ok(Some(value))`.
    /// - EOF (piped) or cancellation (TTY Ctrl+C/D) → returns `Ok(None)`.
    fn prompt_optional(
        &mut self,
        prompt: &str,
        stderr: &mut dyn Write,
        validate: impl Fn(&str) -> Result<(), String>,
    ) -> Result<Option<String>, RagtagError> {
        loop {
            match self.read_line(prompt, stderr)? {
                None => return Ok(None),
                Some(line) => {
                    let trimmed = line.trim().to_string();
                    if trimmed.is_empty() {
                        return Ok(None);
                    }
                    match validate(&trimmed) {
                        Ok(()) => return Ok(Some(trimmed)),
                        Err(msg) => {
                            self.write_error(stderr, &msg)?;
                        }
                    }
                }
            }
        }
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
        let all_statuses: Vec<String> =
            config.all_status_keywords().iter().map(|s| s.to_string()).collect();
        let hint = format!("(leave blank to skip; default: {status_default})");
        let prompt = make_prompt("Status", Some(&hint), session.is_tty);
        builder.status =
            session.prompt_optional(&prompt, ctx.stderr, move |v| validate_status(v, &all_statuses))?;
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
        let prompt =
            make_prompt("Worktime Estimate", Some("(leave blank to skip)"), session.is_tty);
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
        assert!(output.ends_with(")\n") || output.ends_with(')'));
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
        assert!(err.contains("non-negative number"), "unexpected message: {err}");
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
        assert!(err.contains("non-negative number"), "unexpected message: {err}");
        assert!(validate_worktime_spent("-1").is_err());
        assert!(validate_worktime_spent("").is_err());
    }

    #[test]
    fn test_validate_status_valid() {
        let config = TaskConfig::default();
        let allowed: Vec<String> =
            config.all_status_keywords().iter().map(|s| s.to_string()).collect();
        assert!(validate_status("new", &allowed).is_ok());
        assert!(validate_status("active", &allowed).is_ok());
        assert!(validate_status("done", &allowed).is_ok());
        assert!(validate_status("blocked", &allowed).is_ok());
    }

    #[test]
    fn test_validate_status_invalid() {
        let config = TaskConfig::default();
        let allowed: Vec<String> =
            config.all_status_keywords().iter().map(|s| s.to_string()).collect();
        let err = validate_status("banana", &allowed).unwrap_err();
        assert!(err.contains("Invalid status"), "unexpected message: {err}");
        assert!(err.contains("allowed values:"), "expected allowed list: {err}");
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
        assert!(err.contains("Invalid worktime units"), "unexpected message: {err}");
        assert!(err.contains("hours"), "expected 'hours' in: {err}");
        assert!(validate_worktime_units("minutes").is_err());
        assert!(validate_worktime_units("").is_err());
    }
}

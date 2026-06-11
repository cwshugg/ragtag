//! Task set-time command.

use std::path::Path;

use super::super::config::TaskConfig;
use super::super::output::format_task_detail;
use super::{find_task_by_id, prompt_for_value};
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;

/// Parses a time string to f64 and rejects negative values.
///
/// Returns `Ok(f64)` for valid non-negative numeric strings,
/// or an appropriate `RagtagError` otherwise.
fn validate_time(s: &str) -> Result<f64, RagtagError> {
    let val = s.parse::<f64>().map_err(|_| RagtagError::ExtensionError {
        extension_name: "Task Manager".to_string(),
        message: format!("invalid time value \"{s}\" — must be numeric"),
    })?;
    if val < 0.0 {
        return Err(RagtagError::ExtensionError {
            extension_name: "Task Manager".to_string(),
            message: "time value must be non-negative".to_string(),
        });
    }
    Ok(val)
}

/// Runs the set-time command.
pub fn run(
    matches: &clap::ArgMatches,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(), RagtagError> {
    let id = matches
        .get_one::<String>("id")
        .ok_or_else(|| RagtagError::ExtensionError {
            extension_name: "Task Manager".to_string(),
            message: "missing required argument --id".to_string(),
        })?;

    let path_str = matches
        .get_one::<String>("path")
        .map(|s| s.as_str())
        .unwrap_or(".");
    let path = Path::new(path_str);

    let (mut task, _) = find_task_by_id(id, path, config, ctx)?;

    let new_time = if let Some(time_str) = matches.get_one::<String>("time") {
        validate_time(time_str)?
    } else {
        let input = prompt_for_value(ctx, &task, config, "New time_spent: ")?;
        validate_time(&input)?
    };

    ctx.editor.update_tag_attribute(
        &task.location.file_path,
        task.raw_span.clone(),
        "time_spent",
        &new_time.to_string(),
    )?;

    task.time_spent = Some(new_time);

    writeln!(
        ctx.stdout,
        "{}",
        format_task_detail(&task, config, &ctx.color_mode)
    )
    .map_err(RagtagError::Io)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_time_positive() {
        assert_eq!(validate_time("2.5").unwrap(), 2.5);
        assert_eq!(validate_time("10").unwrap(), 10.0);
        assert_eq!(validate_time("0.1").unwrap(), 0.1);
    }

    #[test]
    fn test_validate_time_zero() {
        assert_eq!(validate_time("0").unwrap(), 0.0);
        assert_eq!(validate_time("0.0").unwrap(), 0.0);
    }

    #[test]
    fn test_validate_time_negative_rejected() {
        let err = validate_time("-1.0");
        assert!(err.is_err());
        let msg = format!("{}", err.unwrap_err());
        assert!(msg.contains("non-negative"));
    }

    #[test]
    fn test_validate_time_non_numeric_rejected() {
        let err = validate_time("abc");
        assert!(err.is_err());
        let msg = format!("{}", err.unwrap_err());
        assert!(msg.contains("must be numeric"));
    }
}

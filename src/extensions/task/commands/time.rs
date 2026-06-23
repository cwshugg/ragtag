//! Task time command.
//!
//! Sets a task's `worktime_spent` attribute to a caller-supplied value,
//! auto-updates `time_last_updated`, and either edits the file in-place or
//! prints the updated `@task(...)` string when `--no-edit` is specified.
//!
//! Supports three input formats:
//! - `N`  — sets `worktime_spent` to N (absolute).
//! - `+N` — adds N to the current `worktime_spent`.
//! - `-N` — subtracts N from the current `worktime_spent` (clamped to 0).

use std::path::Path;

use chrono::Utc;

use super::super::config::TaskConfig;
use super::create::escape_for_tag;
use super::find_task_by_id;
use crate::cli;
use crate::edit::{edit_task_tag, write_file_atomically};
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;

/// Describes how the caller wants to adjust `worktime_spent`.
enum TimeAdjustment {
    /// Set to an absolute value.
    Set(f64),
    /// Add to the current value.
    Add(f64),
    /// Subtract from the current value (clamped to 0).
    Subtract(f64),
}

/// Parses a time string into a `TimeAdjustment`.
///
/// Accepts `N`, `+N`, or `-N` where N is a finite, non-negative number.
fn parse_time_adjustment(time_str: &str) -> Result<TimeAdjustment, RagtagError> {
    let make_err = || {
        RagtagError::ExtensionError {
        extension_name: "Task Manager".to_string(),
        message: format!(
            "invalid worktime_spent \"{time_str}\" — expected a number, +number, or -number (e.g., 4, +2, -1, 1.5)"
        ),
    }
    };

    if let Some(rest) = time_str.strip_prefix('+') {
        let n: f64 = rest.parse().map_err(|_| make_err())?;
        if !n.is_finite() || n < 0.0 {
            return Err(make_err());
        }
        Ok(TimeAdjustment::Add(n))
    } else if let Some(rest) = time_str.strip_prefix('-') {
        let n: f64 = rest.parse().map_err(|_| make_err())?;
        if !n.is_finite() || n < 0.0 {
            return Err(make_err());
        }
        Ok(TimeAdjustment::Subtract(n))
    } else {
        let n: f64 = time_str.parse().map_err(|_| make_err())?;
        if !n.is_finite() || n < 0.0 {
            return Err(make_err());
        }
        Ok(TimeAdjustment::Set(n))
    }
}

/// Runs the `task time` command.
pub fn run(
    matches: &clap::ArgMatches,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(), RagtagError> {
    let time_str = matches
        .get_one::<String>("worktime_spent")
        .expect("required argument");
    let id = matches.get_one::<String>("id").expect("required argument");
    let no_edit = matches.get_flag("no-edit");

    // Parse the time adjustment (absolute, relative add, or relative subtract).
    let adjustment = parse_time_adjustment(time_str)?;

    let path_str = cli::resolve_path(matches);
    let path = Path::new(&path_str);

    let (task, content) = find_task_by_id(id, path, config, ctx)?;

    // Compute the final worktime value based on the adjustment type.
    let current = task.worktime_spent.unwrap_or(0.0);
    let worktime = match adjustment {
        TimeAdjustment::Set(n) => n,
        TimeAdjustment::Add(n) => current + n,
        TimeAdjustment::Subtract(n) => (current - n).max(0.0),
    };

    // Compute the auto-updated timestamp and format attribute values.
    let now_ts = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let ts_formatted = format!("\"{}\"", escape_for_tag(&now_ts));

    // Format the worktime value: drop trailing ".0" for whole numbers.
    // Only use the integer path if the value fits in i64 range to avoid
    // silent saturation for extremely large floats.
    let wt_formatted =
        if worktime.fract() == 0.0 && worktime >= i64::MIN as f64 && worktime <= i64::MAX as f64 {
            format!("{}", worktime as i64)
        } else {
            format!("{worktime}")
        };

    let original_tag = &content[task.raw_span.clone()];

    // Apply the worktime_spent change and the auto-managed timestamp in a
    // single format-preserving edit.
    let modified_tag = edit_task_tag(
        original_tag,
        &[
            ("worktime_spent", &wt_formatted),
            ("time_last_updated", &ts_formatted),
        ],
    )?;

    if no_edit {
        writeln!(ctx.stdout, "{modified_tag}").map_err(RagtagError::Io)?;
    } else {
        let mut new_content = String::with_capacity(content.len());
        new_content.push_str(&content[..task.raw_span.start]);
        new_content.push_str(&modified_tag);
        new_content.push_str(&content[task.raw_span.end..]);
        write_file_atomically(&task.location.file_path, &new_content)?;
        writeln!(
            ctx.stdout,
            "Updated task {} (worktime_spent → {})",
            task.id, wt_formatted
        )
        .map_err(RagtagError::Io)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Valid inputs ===

    #[test]
    fn parse_absolute_integer() {
        let adj = parse_time_adjustment("5").unwrap();
        assert!(matches!(adj, TimeAdjustment::Set(n) if (n - 5.0).abs() < f64::EPSILON));
    }

    #[test]
    fn parse_add_integer() {
        let adj = parse_time_adjustment("+3").unwrap();
        assert!(matches!(adj, TimeAdjustment::Add(n) if (n - 3.0).abs() < f64::EPSILON));
    }

    #[test]
    fn parse_subtract_integer() {
        let adj = parse_time_adjustment("-2").unwrap();
        assert!(matches!(adj, TimeAdjustment::Subtract(n) if (n - 2.0).abs() < f64::EPSILON));
    }

    #[test]
    fn parse_add_zero() {
        let adj = parse_time_adjustment("+0").unwrap();
        assert!(matches!(adj, TimeAdjustment::Add(n) if n == 0.0));
    }

    #[test]
    fn parse_subtract_zero() {
        let adj = parse_time_adjustment("-0").unwrap();
        assert!(matches!(adj, TimeAdjustment::Subtract(n) if n == 0.0));
    }

    #[test]
    fn parse_absolute_float() {
        let adj = parse_time_adjustment("1.5").unwrap();
        assert!(matches!(adj, TimeAdjustment::Set(n) if (n - 1.5).abs() < f64::EPSILON));
    }

    #[test]
    fn parse_add_float() {
        let adj = parse_time_adjustment("+1.5").unwrap();
        assert!(matches!(adj, TimeAdjustment::Add(n) if (n - 1.5).abs() < f64::EPSILON));
    }

    #[test]
    fn parse_subtract_float() {
        let adj = parse_time_adjustment("-1.5").unwrap();
        assert!(matches!(adj, TimeAdjustment::Subtract(n) if (n - 1.5).abs() < f64::EPSILON));
    }

    // === Invalid inputs ===

    #[test]
    fn reject_nan() {
        assert!(parse_time_adjustment("NaN").is_err());
    }

    #[test]
    fn reject_inf() {
        assert!(parse_time_adjustment("inf").is_err());
    }

    #[test]
    fn reject_infinity() {
        assert!(parse_time_adjustment("infinity").is_err());
    }

    #[test]
    fn reject_negative_infinity() {
        assert!(parse_time_adjustment("-infinity").is_err());
    }

    #[test]
    fn reject_plus_inf() {
        assert!(parse_time_adjustment("+inf").is_err());
    }

    #[test]
    fn reject_plus_nan() {
        assert!(parse_time_adjustment("+NaN").is_err());
    }

    #[test]
    fn reject_alpha() {
        assert!(parse_time_adjustment("abc").is_err());
    }

    #[test]
    fn reject_double_prefix_plus_minus() {
        assert!(parse_time_adjustment("+-5").is_err());
    }

    #[test]
    fn reject_double_prefix_minus_minus() {
        assert!(parse_time_adjustment("--5").is_err());
    }

    #[test]
    fn reject_empty_string() {
        assert!(parse_time_adjustment("").is_err());
    }
}

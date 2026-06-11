//! Task get-attr command.
//!
//! Retrieves and prints a single attribute value from a task.

use std::path::Path;

use super::super::config::TaskConfig;
use super::super::models::TaskTag;
use super::find_task_by_id;
use crate::cli;
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;

/// Formats a float value, stripping unnecessary trailing zeros.
///
/// Produces clean output like `4.5` instead of `4.500000` and `4` instead of `4.0`.
fn format_float(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// Extracts a single attribute value from a task as a string.
///
/// Returns an empty string for `Option` fields that are `None`.
/// Returns an error for unrecognized attribute names.
fn get_task_attr_value(task: &TaskTag, attr: &str) -> Result<String, RagtagError> {
    match attr {
        "id" => Ok(task.id.clone()),
        "title" => Ok(task.title.clone()),
        "description" => Ok(task.description.clone().unwrap_or_default()),
        "owner" => Ok(task.owner.clone()),
        "status" => Ok(task.status.clone()),
        "priority" => Ok(task.priority.map(|p| p.to_string()).unwrap_or_default()),
        "time_spent" => Ok(task.time_spent.map(format_float).unwrap_or_default()),
        "ttc_estimate" => Ok(task.ttc_estimate.map(format_float).unwrap_or_default()),
        "ttc_actual" => Ok(task.ttc_actual.map(format_float).unwrap_or_default()),
        "time_units" => Ok(task.time_units.clone()),
        "pid" => Ok(task.pid.clone().unwrap_or_default()),
        _ => Err(RagtagError::ExtensionError {
            extension_name: "Task Manager".to_string(),
            message: format!("unknown attribute \"{attr}\""),
        }),
    }
}

/// Runs the get-attr command.
pub fn run(
    matches: &clap::ArgMatches,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(), RagtagError> {
    let id = matches.get_one::<String>("id").expect("required argument");
    let attr = matches
        .get_one::<String>("attr")
        .expect("required argument");

    let path_str = cli::resolve_path(matches);
    let path = Path::new(&path_str);

    let (task, _) = find_task_by_id(id, path, config, ctx)?;

    let value = get_task_attr_value(&task, attr)?;
    if !value.is_empty() {
        writeln!(ctx.stdout, "{value}").map_err(RagtagError::Io)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TagLocation;
    use std::path::PathBuf;

    fn make_task() -> TaskTag {
        TaskTag {
            id: "abc123def456789a".to_string(),
            pid: Some("parent00".to_string()),
            title: "Test Task".to_string(),
            description: Some("A description".to_string()),
            owner: "alice".to_string(),
            status: "active".to_string(),
            priority: Some(1),
            time_spent: Some(2.5),
            ttc_estimate: Some(4.0),
            ttc_actual: None,
            time_units: "hours".to_string(),
            location: TagLocation::new(PathBuf::from("test.md"), 1, 1, 0, 50),
            raw_span: 0..50,
        }
    }

    #[test]
    fn test_get_attr_id() {
        let task = make_task();
        assert_eq!(
            get_task_attr_value(&task, "id").unwrap(),
            "abc123def456789a"
        );
    }

    #[test]
    fn test_get_attr_title() {
        let task = make_task();
        assert_eq!(get_task_attr_value(&task, "title").unwrap(), "Test Task");
    }

    #[test]
    fn test_get_attr_status() {
        let task = make_task();
        assert_eq!(get_task_attr_value(&task, "status").unwrap(), "active");
    }

    #[test]
    fn test_get_attr_priority() {
        let task = make_task();
        assert_eq!(get_task_attr_value(&task, "priority").unwrap(), "1");
    }

    #[test]
    fn test_get_attr_time_spent_float() {
        let task = make_task();
        assert_eq!(get_task_attr_value(&task, "time_spent").unwrap(), "2.5");
    }

    #[test]
    fn test_get_attr_ttc_estimate_whole() {
        let task = make_task();
        assert_eq!(get_task_attr_value(&task, "ttc_estimate").unwrap(), "4");
    }

    #[test]
    fn test_get_attr_ttc_actual_none() {
        let task = make_task();
        assert_eq!(get_task_attr_value(&task, "ttc_actual").unwrap(), "");
    }

    #[test]
    fn test_get_attr_pid() {
        let task = make_task();
        assert_eq!(get_task_attr_value(&task, "pid").unwrap(), "parent00");
    }

    #[test]
    fn test_get_attr_unknown() {
        let task = make_task();
        assert!(get_task_attr_value(&task, "nonexistent").is_err());
    }

    #[test]
    fn test_format_float_whole() {
        assert_eq!(format_float(4.0), "4");
    }

    #[test]
    fn test_format_float_fractional() {
        assert_eq!(format_float(4.5), "4.5");
    }
}

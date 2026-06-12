//! Task set-attr command.
//!
//! Sets a single attribute value on a task, either editing the file
//! in-place or printing the reconstructed `@task(...)` string (for
//! editor plugin integration via `--no-edit`).

use std::path::Path;

use chrono::Utc;

use super::super::config::{TaskConfig, ALLOWED_WORKTIME_UNITS};
use super::create::escape_for_tag;
use super::find_task_by_id;
use crate::cli;
use crate::edit::{edit_task_tag, write_file_atomically};
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;

/// Validates that `attr` is a recognized, mutable attribute name and that
/// `value` is valid for that attribute.
fn validate_attr_value(attr: &str, value: &str, config: &TaskConfig) -> Result<(), RagtagError> {
    let ext_err = |msg: String| RagtagError::ExtensionError {
        extension_name: "Task Manager".to_string(),
        message: msg,
    };

    match attr {
        "id" => Err(ext_err("id is immutable and cannot be changed".to_string())),
        "status" => {
            if !config.all_status_keywords().contains(&value) {
                Err(ext_err(format!(
                    "invalid status \"{}\" — allowed statuses: {}",
                    value,
                    config.all_status_keywords().join(", ")
                )))
            } else {
                Ok(())
            }
        }
        "priority" => {
            value.parse::<u32>().map_err(|_| {
                ext_err(format!(
                    "invalid priority \"{value}\" — must be a non-negative integer"
                ))
            })?;
            Ok(())
        }
        "worktime_spent" | "worktime_estimate" => {
            let v = value.parse::<f64>().map_err(|_| {
                ext_err(format!(
                    "invalid {attr} value \"{value}\" — must be numeric"
                ))
            })?;
            if v < 0.0 {
                return Err(ext_err(format!("{attr} value must be non-negative")));
            }
            Ok(())
        }
        "worktime_units" => {
            if !ALLOWED_WORKTIME_UNITS.contains(&value) {
                Err(ext_err(format!(
                    "invalid worktime_units \"{}\" — allowed values: {}",
                    value,
                    ALLOWED_WORKTIME_UNITS.join(", ")
                )))
            } else {
                Ok(())
            }
        }
        "title" | "description" | "owner" | "pid" => Ok(()),
        "time_created" | "time_last_updated" => Err(ext_err(format!(
            "\"{attr}\" is automatically managed and cannot be set manually"
        ))),
        _ => Err(ext_err(format!("unknown attribute \"{attr}\""))),
    }
}

/// Formats the tag attribute value for use with `update_tag_attribute`.
///
/// String attributes are wrapped in quotes; numeric attributes are bare.
fn format_attr_for_update(attr: &str, value: &str) -> String {
    match attr {
        "title" | "description" | "owner" | "status" | "worktime_units" | "pid" | "time_created"
        | "time_last_updated" => {
            format!("\"{}\"", escape_for_tag(value))
        }
        _ => value.to_string(),
    }
}

/// Runs the set-attr command.
pub fn run(
    matches: &clap::ArgMatches,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(), RagtagError> {
    let id = matches.get_one::<String>("id").expect("required argument");
    let attr = matches
        .get_one::<String>("attr")
        .expect("required argument");
    let value = matches
        .get_one::<String>("value")
        .expect("required argument");
    let no_edit = matches.get_flag("no-edit");

    let path_str = cli::resolve_path(matches);
    let path = Path::new(&path_str);

    validate_attr_value(attr, value, config)?;

    let (task, content) = find_task_by_id(id, path, config, ctx)?;

    // Compute the auto-updated timestamp once for this operation.
    let now_ts = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let ts_formatted = format!("\"{}\"", escape_for_tag(&now_ts));

    let original_tag = &content[task.raw_span.clone()];
    let formatted_value = format_attr_for_update(attr, value);

    // Apply both attribute changes in a single format-preserving edit.
    let modified_tag = edit_task_tag(
        original_tag,
        &[(attr, &formatted_value), ("time_last_updated", &ts_formatted)],
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
            "Updated {attr} to \"{value}\" for task {}",
            task.id
        )
        .map_err(RagtagError::Io)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::task::models::TaskTag;
    use crate::models::TagLocation;
    use std::path::PathBuf;

    fn default_config() -> TaskConfig {
        TaskConfig::default()
    }

    /// Applies an attribute value to a `TaskTag` in memory.
    ///
    /// This is a **test-only helper** — it is not used in production code.
    /// Production attribute updates go through `modify_tag_attribute` (for
    /// `--no-edit` output) or direct in-memory + atomic write (for in-place
    /// file edits). This helper exists solely to verify
    /// `validate_attr_value` and `format_attr_for_update` behavior in
    /// isolation, without needing file I/O.
    fn apply_attr_to_task(task: &mut TaskTag, attr: &str, value: &str) {
        match attr {
            "title" => task.title = value.to_string(),
            "description" => task.description = Some(value.to_string()),
            "owner" => task.owner = value.to_string(),
            "status" => task.status = value.to_string(),
            "priority" => task.priority = value.parse::<u32>().ok(),
            "worktime_spent" => task.worktime_spent = value.parse::<f64>().ok(),
            "worktime_estimate" => task.worktime_estimate = value.parse::<f64>().ok(),
            "worktime_units" => task.worktime_units = value.to_string(),
            "pid" => task.pid = Some(value.to_string()),
            _ => {}
        }
    }

    fn make_task() -> TaskTag {
        TaskTag {
            id: "abc123def456789a".to_string(),
            pid: None,
            title: "Test Task".to_string(),
            description: None,
            owner: "me".to_string(),
            status: "new".to_string(),
            priority: None,
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
    fn test_validate_id_immutable() {
        let config = default_config();
        assert!(validate_attr_value("id", "newid", &config).is_err());
    }

    #[test]
    fn test_validate_status_valid() {
        let config = default_config();
        assert!(validate_attr_value("status", "active", &config).is_ok());
    }

    #[test]
    fn test_validate_status_invalid() {
        let config = default_config();
        let result = validate_attr_value("status", "invalid_xyz", &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_priority_valid() {
        let config = default_config();
        assert!(validate_attr_value("priority", "5", &config).is_ok());
    }

    #[test]
    fn test_validate_priority_invalid() {
        let config = default_config();
        assert!(validate_attr_value("priority", "abc", &config).is_err());
    }

    #[test]
    fn test_validate_priority_negative() {
        let config = default_config();
        assert!(validate_attr_value("priority", "-1", &config).is_err());
    }

    #[test]
    fn test_validate_worktime_spent_valid() {
        let config = default_config();
        assert!(validate_attr_value("worktime_spent", "2.5", &config).is_ok());
    }

    #[test]
    fn test_validate_worktime_spent_negative() {
        let config = default_config();
        let result = validate_attr_value("worktime_spent", "-1.0", &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_worktime_units_valid() {
        let config = default_config();
        assert!(validate_attr_value("worktime_units", "days", &config).is_ok());
    }

    #[test]
    fn test_validate_worktime_units_invalid() {
        let config = default_config();
        assert!(validate_attr_value("worktime_units", "fortnights", &config).is_err());
    }

    #[test]
    fn test_validate_unknown_attr() {
        let config = default_config();
        assert!(validate_attr_value("nonexistent", "val", &config).is_err());
    }

    #[test]
    fn test_validate_string_attrs_always_ok() {
        let config = default_config();
        assert!(validate_attr_value("title", "anything", &config).is_ok());
        assert!(validate_attr_value("description", "anything", &config).is_ok());
        assert!(validate_attr_value("owner", "anything", &config).is_ok());
        assert!(validate_attr_value("pid", "anything", &config).is_ok());
    }

    #[test]
    fn test_validate_time_created_blocked() {
        let config = default_config();
        let result = validate_attr_value("time_created", "2026-06-12T09:00:00Z", &config);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("automatically managed"));
    }

    #[test]
    fn test_validate_time_last_updated_blocked() {
        let config = default_config();
        let result = validate_attr_value("time_last_updated", "2026-06-12T10:00:00Z", &config);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("automatically managed"));
    }

    #[test]
    fn test_apply_attr_status() {
        let mut task = make_task();
        apply_attr_to_task(&mut task, "status", "active");
        assert_eq!(task.status, "active");
    }

    #[test]
    fn test_apply_attr_priority() {
        let mut task = make_task();
        apply_attr_to_task(&mut task, "priority", "5");
        assert_eq!(task.priority, Some(5));
    }

    #[test]
    fn test_apply_attr_title() {
        let mut task = make_task();
        apply_attr_to_task(&mut task, "title", "New Title");
        assert_eq!(task.title, "New Title");
    }

    #[test]
    fn test_format_attr_string() {
        assert_eq!(format_attr_for_update("status", "active"), "\"active\"");
        assert_eq!(format_attr_for_update("owner", "alice"), "\"alice\"");
    }

    #[test]
    fn test_format_attr_numeric() {
        assert_eq!(format_attr_for_update("priority", "5"), "5");
        assert_eq!(format_attr_for_update("worktime_spent", "2.5"), "2.5");
    }

    #[test]
    fn test_format_attr_string_with_quotes() {
        assert_eq!(
            format_attr_for_update("title", "Say \"hello\""),
            "\"Say \\\"hello\\\"\""
        );
    }

    // === Tests for layout-preserving attribute replacement ===

    #[test]
    fn test_modify_single_line_preserves_layout() {
        use crate::edit::modify_tag_attribute;
        let tag = "@task(id=\"abc123\", status=\"new\", title=\"Test\")";
        let result = modify_tag_attribute(tag, "status", "\"active\"").unwrap();
        // Should remain single-line
        assert!(!result.contains('\n'));
        assert!(result.contains("status=\"active\""));
        // Other attributes preserved
        assert!(result.contains("id=\"abc123\""));
        assert!(result.contains("title=\"Test\""));
    }

    #[test]
    fn test_modify_multiline_preserves_layout() {
        use crate::edit::modify_tag_attribute;
        let tag = "@task(\n    id=\"abc123\",\n    status=\"new\",\n    title=\"Test\"\n)";
        let result = modify_tag_attribute(tag, "status", "\"done\"").unwrap();
        // Should remain multi-line with same structure
        assert!(result.contains('\n'));
        assert!(result.contains("status=\"done\""));
        assert!(result.contains("    id=\"abc123\""));
        assert!(result.contains("    title=\"Test\""));
    }

    #[test]
    fn test_modify_numeric_attribute() {
        use crate::edit::modify_tag_attribute;
        let tag = "@task(id=\"abc\", priority=3)";
        let result = modify_tag_attribute(tag, "priority", "5").unwrap();
        assert!(result.contains("priority=5"));
        assert!(!result.contains("priority=3"));
    }

    #[test]
    fn test_modify_insert_missing_attribute() {
        use crate::edit::modify_tag_attribute;
        let tag = "@task(id=\"abc\", status=\"new\")";
        let result = modify_tag_attribute(tag, "priority", "5").unwrap();
        assert!(result.contains("priority=5"));
        assert!(result.contains("id=\"abc\""));
        assert!(result.contains("status=\"new\""));
    }
}

//! Task time command.
//!
//! Sets a task's `worktime_spent` attribute to a caller-supplied non-negative
//! number, auto-updates `time_last_updated`, and either edits the file
//! in-place or prints the updated `@task(...)` string when `--no-edit` is
//! specified.

use std::path::Path;

use chrono::Utc;

use super::super::config::TaskConfig;
use super::create::escape_for_tag;
use super::find_task_by_id;
use crate::cli;
use crate::edit::{edit_task_tag, write_file_atomically};
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;

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

    // Validate: worktime_spent must be a non-negative number.
    let worktime: f64 = time_str
        .parse()
        .map_err(|_| RagtagError::ExtensionError {
            extension_name: "Task Manager".to_string(),
            message: format!(
                "invalid worktime_spent \"{time_str}\" — expected a non-negative number (e.g., 0, 1.5, 8)"
            ),
        })?;
    if worktime < 0.0 {
        return Err(RagtagError::ExtensionError {
            extension_name: "Task Manager".to_string(),
            message: format!(
                "invalid worktime_spent \"{time_str}\" — value must be non-negative"
            ),
        });
    }

    let path_str = cli::resolve_path(matches);
    let path = Path::new(&path_str);

    let (task, content) = find_task_by_id(id, path, config, ctx)?;

    // Compute the auto-updated timestamp and format attribute values.
    let now_ts = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let ts_formatted = format!("\"{}\"", escape_for_tag(&now_ts));

    // Format the worktime value: drop trailing ".0" for whole numbers.
    let wt_formatted = if worktime.fract() == 0.0 {
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

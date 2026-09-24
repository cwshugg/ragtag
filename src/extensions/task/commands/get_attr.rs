//! Task get-attr command.
//!
//! Retrieves and prints a single attribute value from a task.

use std::path::Path;

use super::super::config::TaskConfig;
use super::{find_task_by_id, task_field_value};
use crate::cli;
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;

fn unknown_attribute(attr: &str) -> RagtagError {
    RagtagError::ExtensionError {
        extension_name: "Task Manager".to_string(),
        message: format!("unknown attribute \"{attr}\""),
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

    let value = task_field_value(&task, attr).ok_or_else(|| unknown_attribute(attr))?;
    if !value.is_empty() {
        writeln!(ctx.stdout, "{value}").map_err(RagtagError::Io)?;
    }

    Ok(())
}

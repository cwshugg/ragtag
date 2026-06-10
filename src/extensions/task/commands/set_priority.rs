//! Task set-priority command.

use std::io::BufRead;
use std::path::Path;

use super::super::config::TaskConfig;
use super::super::output::format_task_detail;
use super::find_task_by_id;
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;

/// Runs the set-priority command.
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

    // Get new priority
    let new_priority = if let Some(pri) = matches.get_one::<String>("priority") {
        parse_priority(pri)?
    } else {
        // Interactive mode
        writeln!(ctx.stderr, "Current task:").map_err(RagtagError::Io)?;
        writeln!(
            ctx.stderr,
            "{}",
            format_task_detail(&task, config, &ctx.color_mode)
        )
        .map_err(RagtagError::Io)?;
        write!(ctx.stderr, "New priority: ").map_err(RagtagError::Io)?;
        ctx.stderr.flush().map_err(RagtagError::Io)?;

        let stdin = std::io::stdin();
        let mut lines = stdin.lock().lines();
        match lines.next() {
            Some(Ok(line)) => parse_priority(line.trim())?,
            _ => {
                return Err(RagtagError::Io(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "no input",
                )))
            }
        }
    };

    // Edit file
    ctx.editor.update_tag_attribute(
        &task.location.file_path,
        task.raw_span.clone(),
        "priority",
        &new_priority.to_string(),
    )?;

    task.priority = Some(new_priority);

    // Print updated task detail
    writeln!(
        ctx.stdout,
        "{}",
        format_task_detail(&task, config, &ctx.color_mode)
    )
    .map_err(RagtagError::Io)?;

    Ok(())
}

/// Parses a priority string into a u32.
fn parse_priority(s: &str) -> Result<u32, RagtagError> {
    s.parse::<u32>().map_err(|_| RagtagError::ExtensionError {
        extension_name: "Task Manager".to_string(),
        message: format!("invalid priority \"{s}\" — must be a non-negative integer"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_priority_valid() {
        assert_eq!(parse_priority("0").unwrap(), 0);
        assert_eq!(parse_priority("5").unwrap(), 5);
        assert_eq!(parse_priority("100").unwrap(), 100);
    }

    #[test]
    fn test_parse_priority_invalid() {
        assert!(parse_priority("abc").is_err());
        assert!(parse_priority("-1").is_err());
        assert!(parse_priority("3.5").is_err());
        assert!(parse_priority("").is_err());
    }
}

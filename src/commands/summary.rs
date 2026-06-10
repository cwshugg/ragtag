//! Summary command implementation.
//!
//! Scans files, aggregates tags by name, and prints a summary table.
//! Consults the extension registry for enhanced per-tag-type summaries.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use crate::config::{ColorMode, Config};
use crate::discovery;
use crate::error::RagtagError;
use crate::extensions::ExtensionRegistry;
use crate::output::format::pad_right;
use crate::parser;

/// Runs the summary command.
pub fn run(
    matches: &clap::ArgMatches,
    config: &Config,
    registry: &ExtensionRegistry,
    color_mode: &ColorMode,
    stdout: &mut dyn Write,
) -> Result<(), RagtagError> {
    let path_str = matches
        .get_one::<String>("path")
        .map(|s| s.as_str())
        .unwrap_or(".");
    let path = Path::new(path_str);

    let files = discovery::walk_path(path, config)?;

    // Aggregate tags by name
    let mut tag_groups: HashMap<String, Vec<crate::models::Tag>> = HashMap::new();

    for file_path in &files {
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("skipping unreadable file {}: {}", file_path.display(), e);
                continue;
            }
        };
        let tags = parser::scan_file(&content, file_path);
        for tag in tags {
            tag_groups.entry(tag.name.clone()).or_default().push(tag);
        }
    }

    if tag_groups.is_empty() {
        writeln!(stdout, "No tags found.").map_err(RagtagError::Io)?;
        return Ok(());
    }

    // Sort by tag name for deterministic output
    let mut names: Vec<String> = tag_groups.keys().cloned().collect();
    names.sort();

    // Find the longest name for padding
    let max_name_len = names.iter().map(|n| n.len()).max().unwrap_or(4);
    let name_width = max_name_len.max(4) + 2;

    writeln!(stdout, "{} Count", pad_right("Tag", name_width)).map_err(RagtagError::Io)?;
    writeln!(stdout, "{} -----", pad_right("---", name_width)).map_err(RagtagError::Io)?;

    for name in &names {
        let tags = &tag_groups[name];
        let count = tags.len();

        // Check if an extension provides a custom summary
        let ext_summary = registry
            .get_by_tag_name(name)
            .and_then(|ext| ext.format_summary(tags, color_mode));

        if let Some(summary) = ext_summary {
            writeln!(
                stdout,
                "{} {} ({})",
                pad_right(name, name_width),
                count,
                summary
            )
            .map_err(RagtagError::Io)?;
        } else {
            writeln!(stdout, "{} {}", pad_right(name, name_width), count)
                .map_err(RagtagError::Io)?;
        }
    }

    Ok(())
}

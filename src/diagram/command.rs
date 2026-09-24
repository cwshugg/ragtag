//! End-to-end diagram command pipeline and validation gates.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::ArgMatches;

use crate::cli;
use crate::config::Config;
use crate::discovery::IgnoreWalker;
use crate::error::RagtagError;
use crate::extensions::task::semantics::TaskSemantics;

use super::backend;
use super::catalog::CatalogBuilder;
use super::diagnostics::{render, Diagnostic};
use super::document::Direction;
use super::graph::ProviderId;
use super::sink;
use super::source::build_source_index;
use super::task::provider::TaskGraphProvider;
use super::task::selection::TaskSelectionPolicy;
use super::task::{TaskBuckets, TaskTree};
use super::DiagramLimits;

pub(crate) fn run(
    matches: &ArgMatches,
    semantics: Arc<TaskSemantics>,
    config: &Config,
    startup_cwd: &Path,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<(), RagtagError> {
    let (kind, matches) = matches.subcommand().expect("diagram requires a subcommand");
    let direction = match matches
        .get_one::<String>("direction")
        .map(String::as_str)
        .unwrap_or("down")
    {
        "down" => Direction::Down,
        "right" => Direction::Right,
        "up" => Direction::Up,
        "left" => Direction::Left,
        _ => unreachable!("clap validates directions"),
    };
    let path = PathBuf::from(cli::resolve_path(matches));
    let path = if path.is_absolute() {
        path
    } else {
        startup_cwd.join(path)
    };
    let limits = DiagramLimits {
        maximum_file_bytes: usize::try_from(config.max_file_size).unwrap_or(usize::MAX),
        ..DiagramLimits::default()
    };
    let walker = IgnoreWalker::new(config)?;
    let (source, gate_a) = build_source_index(&walker, &path, &limits);
    if source.is_none() {
        render(&gate_a, stderr)?;
        return Err(RagtagError::Diagram("source validation failed".to_string()));
    }
    render_warnings(&gate_a, stderr)?;
    let catalog = CatalogBuilder::new()
        .register_forest(
            TaskGraphProvider::new(ProviderId("task-tree-provider"), Arc::clone(&semantics)),
            TaskSelectionPolicy::new(Arc::clone(&semantics)),
            TaskTree::new(limits.clone()),
        )?
        .register_forest(
            TaskGraphProvider::new(ProviderId("task-buckets-provider"), Arc::clone(&semantics)),
            TaskSelectionPolicy::new(semantics),
            TaskBuckets::new(limits.clone()),
        )?
        .build();
    let executable = catalog
        .get(kind)
        .ok_or_else(|| RagtagError::UnknownCommand(kind.to_string()))?;
    let document = executable.execute(
        source.as_ref().expect("checked as present"),
        matches,
        direction,
        &limits,
        stderr,
    )?;
    let bytes = backend::serialize(&document, limits.maximum_serialized_bytes)?;
    let output = matches
        .get_one::<String>("output")
        .map(String::as_str)
        .unwrap_or("-");
    if output == "-" {
        sink::write_stdout(&bytes, stdout)
    } else {
        let output = PathBuf::from(output);
        let output = if output.is_absolute() {
            output
        } else {
            startup_cwd.join(output)
        };
        sink::write_file(&output, &bytes)
    }
}

fn render_warnings(diagnostics: &[Diagnostic], stderr: &mut dyn Write) -> Result<(), RagtagError> {
    if diagnostics.is_empty() {
        return Ok(());
    }
    render(diagnostics, stderr)?;
    Ok(())
}

//! CLI argument parsing.
//!
//! Defines the top-level `Cli` struct and builds the clap command tree,
//! dynamically including extension subcommands.

use crate::extensions::ExtensionRegistry;
use clap::{Arg, Command};

/// Builds the complete CLI command tree.
///
/// Core commands (summary, query) are defined statically.
/// Extension commands are added dynamically from the registry.
pub fn build_cli(registry: &ExtensionRegistry) -> Command {
    let mut cmd = Command::new("ragtag")
        .version(env!("CARGO_PKG_VERSION"))
        .about("A CLI tool for parsing @tag(attr=value) from plain text files")
        .propagate_version(true)
        .arg(
            Arg::new("config")
                .long("config")
                .help("Path to config file")
                .value_name("PATH")
                .global(true),
        )
        .arg(
            Arg::new("no-color")
                .long("no-color")
                .help("Disable colored output")
                .action(clap::ArgAction::SetTrue)
                .global(true),
        )
        .subcommand(
            Command::new("summary")
                .about("Show a summary of all tags found")
                .arg(
                    Arg::new("path")
                        .long("path")
                        .help("Search path (file or directory)")
                        .value_name("PATH")
                        .default_value("."),
                ),
        )
        .subcommand(
            Command::new("query")
                .about("Search for specific tags")
                .arg(
                    Arg::new("TAG_NAME")
                        .help("Tag name to search for (without @)")
                        .required(true)
                        .index(1),
                )
                .arg(
                    Arg::new("path")
                        .long("path")
                        .help("Search path (file or directory)")
                        .value_name("PATH")
                        .default_value("."),
                )
                .arg(
                    Arg::new("filter")
                        .long("filter")
                        .help("Filter by attribute (e.g., status=active)")
                        .value_name("EXPR")
                        .action(clap::ArgAction::Append),
                )
                .arg(
                    Arg::new("count")
                        .long("count")
                        .help("Show count only")
                        .action(clap::ArgAction::SetTrue),
                ),
        );

    // Add extension commands
    for ext_cmd in registry.cli_commands() {
        cmd = cmd.subcommand(ext_cmd);
    }

    cmd
}

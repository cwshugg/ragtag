//! CLI argument parsing.
//!
//! Defines the top-level `Cli` struct and builds the clap command tree,
//! dynamically including extension subcommands. Also provides helper
//! functions for resolving CLI arguments with environment variable fallbacks.

use crate::extensions::ExtensionRegistry;
use clap::{Arg, ArgMatches, Command};

/// Environment variable name for specifying the config file path.
pub const RAGTAG_CONFIG_ENV: &str = "RAGTAG_CONFIG";

/// Environment variable name for specifying the default search path.
pub const RAGTAG_PATH_ENV: &str = "RAGTAG_PATH";

/// Resolves the search path from CLI args, falling back to `RAGTAG_PATH` env var, then `"."`.
///
/// Precedence: CLI `--path` flag > `RAGTAG_PATH` environment variable > `"."` (current directory).
pub fn resolve_path(matches: &ArgMatches) -> String {
    matches
        .get_one::<String>("path")
        .cloned()
        .or_else(|| std::env::var(RAGTAG_PATH_ENV).ok())
        .unwrap_or_else(|| ".".to_string())
}

/// Resolves the config file path from CLI args, falling back to `RAGTAG_CONFIG` env var.
///
/// Precedence: CLI `--config` flag > `RAGTAG_CONFIG` environment variable > `None` (auto-discovery).
pub fn resolve_config_path(matches: &ArgMatches) -> Option<std::path::PathBuf> {
    matches
        .get_one::<String>("config")
        .cloned()
        .or_else(|| std::env::var(RAGTAG_CONFIG_ENV).ok())
        .map(std::path::PathBuf::from)
}

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
            Command::new("config")
                .about("Inspect ragtag configuration")
                .subcommand(
                    Command::new("get")
                        .about("Print the value of a config field")
                        .arg(
                            Arg::new("key")
                                .help(
                                    "Config key in dot-notation (e.g., max_depth, tasks.tag_name)",
                                )
                                .required(true)
                                .index(1),
                        ),
                ),
        )
        .subcommand(
            Command::new("summary")
                .about("Show a summary of all tags found")
                .arg(
                    Arg::new("path")
                        .long("path")
                        .help("Search path (file or directory); falls back to RAGTAG_PATH env var, then \".\"")
                        .value_name("PATH"),
                ),
        )
        .subcommand(
            Command::new("query")
                .about("Search for specific tags")
                .arg(
                    Arg::new("TAG_NAME")
                        .help("Tag name to search for (without @); omit to list all tags")
                        .required(false)
                        .index(1),
                )
                .arg(
                    Arg::new("path")
                        .long("path")
                        .help("Search path (file or directory); falls back to RAGTAG_PATH env var, then \".\"")
                        .value_name("PATH"),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal command with a `--path` arg for testing `resolve_path`.
    fn test_cmd() -> Command {
        Command::new("test").arg(Arg::new("path").long("path").value_name("PATH"))
    }

    /// Builds a minimal command with a `--config` arg for testing `resolve_config_path`.
    fn test_config_cmd() -> Command {
        Command::new("test").arg(Arg::new("config").long("config").value_name("PATH"))
    }

    #[test]
    fn test_resolve_path_all_precedence_levels() {
        // Run all path resolution tests sequentially in one test to avoid
        // env var races with parallel test threads.

        // 1. Default: no CLI flag, no env var → "."
        std::env::remove_var(RAGTAG_PATH_ENV);
        let matches = test_cmd().get_matches_from(vec!["test"]);
        assert_eq!(resolve_path(&matches), ".");

        // 2. Env var only → uses env var.
        std::env::set_var(RAGTAG_PATH_ENV, "/env/path");
        let matches = test_cmd().get_matches_from(vec!["test"]);
        assert_eq!(resolve_path(&matches), "/env/path");

        // 3. CLI flag overrides env var.
        let matches = test_cmd().get_matches_from(vec!["test", "--path", "/cli/path"]);
        assert_eq!(resolve_path(&matches), "/cli/path");

        // Clean up.
        std::env::remove_var(RAGTAG_PATH_ENV);
    }

    #[test]
    fn test_resolve_config_path_all_precedence_levels() {
        // Run all config path resolution tests sequentially in one test.

        // 1. Default: no CLI flag, no env var → None.
        std::env::remove_var(RAGTAG_CONFIG_ENV);
        let matches = test_config_cmd().get_matches_from(vec!["test"]);
        assert_eq!(resolve_config_path(&matches), None);

        // 2. Env var only → uses env var.
        std::env::set_var(RAGTAG_CONFIG_ENV, "/env/config.yaml");
        let matches = test_config_cmd().get_matches_from(vec!["test"]);
        assert_eq!(
            resolve_config_path(&matches),
            Some(std::path::PathBuf::from("/env/config.yaml"))
        );

        // 3. CLI flag overrides env var.
        let matches =
            test_config_cmd().get_matches_from(vec!["test", "--config", "/cli/config.yaml"]);
        assert_eq!(
            resolve_config_path(&matches),
            Some(std::path::PathBuf::from("/cli/config.yaml"))
        );

        // Clean up.
        std::env::remove_var(RAGTAG_CONFIG_ENV);
    }
}

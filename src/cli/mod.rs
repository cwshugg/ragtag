//! CLI argument parsing.
//!
//! Defines the top-level `Cli` struct and builds the clap command tree,
//! dynamically including extension subcommands. Also provides helper
//! functions for resolving CLI arguments with environment variable fallbacks.

use crate::config::Alias;
use crate::extensions::ExtensionRegistry;
use clap::{Arg, ArgMatches, Command};
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

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

/// Resolves the config file path from a raw argument vector, without a full parse.
///
/// Because aliases are defined in the config file and must be registered as
/// subcommands *before* clap parses the arguments, the config path has to be
/// resolved up front. This performs a lightweight scan of `args` for a
/// `--config <PATH>` or `--config=<PATH>` occurrence anywhere on the command
/// line, falling back to the `RAGTAG_CONFIG` environment variable, then `None`.
///
/// Precedence: `--config` flag (last occurrence wins, matching clap) >
/// `RAGTAG_CONFIG` env var > `None` (auto-discovery).
pub fn resolve_config_path_from_args<I, S>(args: I) -> Option<std::path::PathBuf>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut found: Option<String> = None;
    let mut iter = args.into_iter().peekable();
    while let Some(arg) = iter.next() {
        let arg = arg.as_ref();
        // Everything after a `--` separator is a positional argument, not an
        // option, so stop scanning: a trailing `--config` (e.g. passed through
        // to an alias) must not be mistaken for ragtag's own config flag.
        if arg == "--" {
            break;
        }
        if let Some(value) = arg.strip_prefix("--config=") {
            found = Some(value.to_string());
        } else if arg == "--config" {
            if let Some(value) = iter.next() {
                found = Some(value.as_ref().to_string());
            }
        }
    }

    found
        .or_else(|| std::env::var(RAGTAG_CONFIG_ENV).ok())
        .map(std::path::PathBuf::from)
}

/// Returns the set of "real" command names: every built-in and extension
/// subcommand registered in the command tree.
///
/// The set is derived authoritatively from `build_cli` (with no aliases) so it
/// automatically covers built-ins and extension commands and cannot drift from
/// the actual command tree. Used to detect alias-name collisions at
/// config-validation time.
pub fn real_command_names(registry: &ExtensionRegistry) -> HashSet<String> {
    build_cli(registry, &[])
        .get_subcommands()
        .map(|cmd| cmd.get_name().to_string())
        .collect()
}

/// Interns an alias name into a process-lifetime pool, returning a `'static`
/// reference.
///
/// clap's `Command::new` requires a `'static` name, but `build_cli` is invoked
/// multiple times per run (initial parse, alias re-parse, help paths). Interning
/// ensures each distinct alias name is leaked at most once for the life of the
/// process, rather than leaking a fresh string on every `build_cli` call. The
/// number of distinct names is bounded by the alias-count cap enforced during
/// config validation, so the total leaked memory is bounded.
fn intern_alias_name(name: &str) -> &'static str {
    static POOL: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let pool = POOL.get_or_init(|| Mutex::new(HashSet::new()));
    let mut guard = pool.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(existing) = guard.get(name) {
        return existing;
    }
    let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
    guard.insert(leaked);
    leaked
}

/// Builds the clap subcommand that represents a single user-defined alias.
///
/// The alias is hidden from the top-level help listing (to avoid cluttering
/// `--help` with user commands that shadow the real ones) but still
/// participates in `infer_subcommands` prefix matching. A trailing var-arg
/// captures any user-supplied arguments so they can be appended after the
/// alias's own expansion.
fn build_alias_command(alias: &Alias) -> Command {
    let name = intern_alias_name(&alias.name);
    Command::new(name)
        .about(format!("Alias for `{alias}`"))
        .hide(true)
        .arg(
            Arg::new("args")
                .num_args(0..)
                .allow_hyphen_values(true)
                .trailing_var_arg(true),
        )
}

/// Builds the complete CLI command tree.
///
/// Core commands (summary, query) are defined statically.
/// Extension commands are added dynamically from the registry.
/// User-defined aliases are added as hidden subcommands so they participate
/// in `infer_subcommands` prefix matching.
pub fn build_cli(registry: &ExtensionRegistry, aliases: &[Alias]) -> Command {
    let mut cmd = Command::new("ragtag")
        .version(env!("CARGO_PKG_VERSION"))
        .about("A CLI tool for parsing @tag(attr=value) from plain text files")
        .propagate_version(true)
        .infer_subcommands(true)
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
                .infer_subcommands(true)
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
            Command::new("file")
                .about("Create files")
                .infer_subcommands(true)
                .disable_help_subcommand(true)
                .subcommand_required(true)
                .subcommand(
                    Command::new("touch")
                        .about("Create a new file and print its path; fail if the target exists")
                        .arg(
                            Arg::new("path")
                                .long("path")
                                .help("Target file; defaults to a UTC-generated file under the configured directory")
                                .value_name("FILE"),
                        )
                        .arg(
                            Arg::new("tag")
                                .long("tag")
                                .help("Tag placed at the beginning of the file; repeat once per tag")
                                .value_name("TAG")
                                .allow_hyphen_values(true)
                                .action(clap::ArgAction::Append),
                        )
                        .arg(
                            Arg::new("edit")
                                .long("edit")
                                .help("Open the newly created file using EDITOR after creation")
                                .action(clap::ArgAction::SetTrue),
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

    // Add user-defined aliases as (hidden) subcommands so clap's own
    // `infer_subcommands` handles prefix matching and ambiguity errors.
    for alias in aliases {
        cmd = cmd.subcommand(build_alias_command(alias));
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
    fn test_resolve_config_path_from_args() {
        // Consolidate every RAGTAG_CONFIG-dependent assertion into one
        // sequential test so parallel test threads never race on the shared
        // env var.
        std::env::remove_var(RAGTAG_CONFIG_ENV);

        // 1. No flag, no env → None.
        assert_eq!(resolve_config_path_from_args(["ragtag", "summary"]), None);

        // 2. `--config <PATH>` form, anywhere on the line.
        assert_eq!(
            resolve_config_path_from_args(["ragtag", "my-alias", "--config", "/a/b.yaml"]),
            Some(std::path::PathBuf::from("/a/b.yaml"))
        );

        // 3. `--config=<PATH>` form.
        assert_eq!(
            resolve_config_path_from_args(["ragtag", "--config=/c/d.yaml", "summary"]),
            Some(std::path::PathBuf::from("/c/d.yaml"))
        );

        // 4. Last occurrence wins (matching clap's override behavior).
        assert_eq!(
            resolve_config_path_from_args(["ragtag", "--config", "/first", "--config", "/last"]),
            Some(std::path::PathBuf::from("/last"))
        );

        // 5. A `--config` after a `--` separator is a positional (e.g. passed
        // to an alias's underlying command) and must not be consumed.
        assert_eq!(
            resolve_config_path_from_args(["ragtag", "my-alias", "--", "--config", "/x.yaml"]),
            None
        );
        assert_eq!(
            resolve_config_path_from_args(["ragtag", "my-alias", "--", "--config=/x.yaml"]),
            None
        );

        // 6. A `--config` before the `--` separator is still honored.
        assert_eq!(
            resolve_config_path_from_args([
                "ragtag",
                "--config",
                "/real.yaml",
                "--",
                "--config",
                "/x.yaml"
            ]),
            Some(std::path::PathBuf::from("/real.yaml"))
        );

        // 7. Falls back to env var when no flag is present.
        std::env::set_var(RAGTAG_CONFIG_ENV, "/env/config.yaml");
        assert_eq!(
            resolve_config_path_from_args(["ragtag", "summary"]),
            Some(std::path::PathBuf::from("/env/config.yaml"))
        );

        // 8. Flag overrides env var.
        assert_eq!(
            resolve_config_path_from_args(["ragtag", "--config", "/cli.yaml"]),
            Some(std::path::PathBuf::from("/cli.yaml"))
        );

        std::env::remove_var(RAGTAG_CONFIG_ENV);
    }

    #[test]
    fn test_real_command_names_includes_builtins() {
        let registry = ExtensionRegistry::new();
        let names = real_command_names(&registry);
        assert!(names.contains("summary"));
        assert!(names.contains("query"));
        assert!(names.contains("config"));
        assert!(names.contains("file"));
    }

    #[test]
    fn test_file_command_contains_only_touch_and_accepts_repeated_tags() {
        let registry = ExtensionRegistry::new();
        let command = build_cli(&registry, &[]);
        let file = command
            .get_subcommands()
            .find(|subcommand| subcommand.get_name() == "file")
            .unwrap();
        assert_eq!(
            file.get_subcommands()
                .map(clap::Command::get_name)
                .collect::<Vec<_>>(),
            vec!["touch"]
        );

        let matches = build_cli(&registry, &[])
            .try_get_matches_from([
                "ragtag", "file", "touch", "--tag", "-one", "--tag", "@-two", "--edit",
            ])
            .unwrap();
        let (_, file_matches) = matches.subcommand().unwrap();
        let (_, touch_matches) = file_matches.subcommand().unwrap();
        assert_eq!(
            touch_matches
                .get_many::<String>("tag")
                .unwrap()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["-one", "@-two"]
        );
        assert!(touch_matches.get_flag("edit"));
        assert!(build_cli(&registry, &[])
            .try_get_matches_from(["ragtag", "file", "unknown"])
            .is_err());
    }

    #[test]
    fn test_build_cli_registers_alias_subcommand() {
        let registry = ExtensionRegistry::new();
        let aliases = vec![Alias {
            name: "my-alias".to_string(),
            arguments: vec!["summary".to_string()],
        }];
        let cmd = build_cli(&registry, &aliases);
        assert!(cmd.get_subcommands().any(|sc| sc.get_name() == "my-alias"));
    }
}

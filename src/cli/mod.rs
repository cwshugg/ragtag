//! CLI argument parsing.
//!
//! Defines the top-level `Cli` struct and builds the clap command tree,
//! dynamically including extension subcommands. Also provides helper
//! functions for resolving CLI arguments with environment variable fallbacks.

use crate::application::StaticCatalog;
use clap::{Arg, ArgMatches, Command};
use std::ffi::{OsStr, OsString};

pub mod aliases;

/// Environment variable name for specifying the config file path.
pub const RAGTAG_CONFIG_ENV: &str = "RAGTAG_CONFIG";

/// Environment variable name for specifying the default search path.
pub const RAGTAG_PATH_ENV: &str = "RAGTAG_PATH";

/// Parses a deterministic query-randomization seed with ambiguity guidance.
fn parse_randomize_seed(value: &str) -> Result<u64, String> {
    value.parse::<u64>().map_err(|_| {
        format!(
            "{value:?} is not an unsigned 64-bit seed; if it is the query \
             expression, place it before --randomize or after --"
        )
    })
}

/// Classification shared by the raw config and outer-command scanners.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RootToken {
    /// The literal option/positional separator.
    Separator,
    /// The split `--config PATH` form.
    ConfigSplit,
    /// The `--config=PATH` form and its OS-native path value.
    ConfigEquals(OsString),
    /// The global `--no-color` flag.
    NoColor,
    /// An option not interpreted by the alias scanner.
    Option,
    /// A prospective outer command.
    Command,
}

/// Tests whether an OS-native token begins with an ASCII prefix.
fn os_starts_with_ascii(token: &OsStr, prefix: &str) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        token.as_bytes().starts_with(prefix.as_bytes())
    }

    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let mut encoded = token.encode_wide();
        prefix
            .encode_utf16()
            .all(|unit| encoded.next() == Some(unit))
    }

    #[cfg(not(any(unix, windows)))]
    {
        token
            .to_str()
            .is_some_and(|value| value.starts_with(prefix))
    }
}

/// Extracts an OS-native value from the `--config=PATH` form.
fn config_equals_value(token: &OsStr) -> Option<OsString> {
    const PREFIX: &str = "--config=";

    if !os_starts_with_ascii(token, PREFIX) {
        return None;
    }

    #[cfg(unix)]
    {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        Some(OsString::from_vec(
            token.as_bytes()[PREFIX.len()..].to_vec(),
        ))
    }

    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let encoded = token.encode_wide().collect::<Vec<_>>();
        Some(OsString::from_wide(
            &encoded[PREFIX.encode_utf16().count()..],
        ))
    }

    #[cfg(not(any(unix, windows)))]
    {
        token
            .to_str()
            .and_then(|value| value.strip_prefix(PREFIX))
            .map(OsString::from)
    }
}

/// Classifies one root-level token without requiring Unicode conversion.
pub(crate) fn classify_root_token(token: &OsStr) -> RootToken {
    if token == OsStr::new("--") {
        RootToken::Separator
    } else if token == OsStr::new("--config") {
        RootToken::ConfigSplit
    } else if let Some(value) = config_equals_value(token) {
        RootToken::ConfigEquals(value)
    } else if token == OsStr::new("--no-color") {
        RootToken::NoColor
    } else if os_starts_with_ascii(token, "-") {
        RootToken::Option
    } else {
        RootToken::Command
    }
}

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
/// Because aliases are defined in the selected config file, the config path
/// has to be resolved before alias expansion and the single clap parse. This
/// performs a lightweight scan of `args` for a `--config <PATH>` or
/// `--config=<PATH>` occurrence in the leading root-option prefix. Scanning
/// stops at the first command, separator, or unrecognized option so terminal
/// command values cannot redirect configuration. The result falls back to the
/// `RAGTAG_CONFIG` environment variable, then `None`.
///
/// Precedence: `--config` flag (last occurrence wins, matching clap) >
/// `RAGTAG_CONFIG` env var > `None` (auto-discovery).
pub fn resolve_config_path_from_args<I, S>(args: I) -> Option<std::path::PathBuf>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    resolve_cli_config_path_from_args(args)
        .or_else(|| std::env::var_os(RAGTAG_CONFIG_ENV).map(std::path::PathBuf::from))
}

/// Resolves only a leading root-level `--config` occurrence from raw argv.
fn resolve_cli_config_path_from_args<I, S>(args: I) -> Option<std::path::PathBuf>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut found: Option<OsString> = None;
    let mut iter = args.into_iter();
    // The first token is argv[0], not a root option.
    iter.next();
    while let Some(arg) = iter.next() {
        let arg = arg.as_ref();
        match classify_root_token(arg) {
            // Everything after a separator is positional. A separator in the
            // value position also terminates the scan rather than becoming a
            // config path.
            RootToken::Separator => break,
            RootToken::ConfigEquals(value) => found = Some(value),
            RootToken::ConfigSplit => {
                let Some(value) = iter.next() else {
                    break;
                };
                if classify_root_token(value.as_ref()) == RootToken::Separator {
                    break;
                }
                found = Some(value.as_ref().to_os_string());
            }
            RootToken::NoColor => {}
            RootToken::Option | RootToken::Command => break,
        }
    }

    found.map(std::path::PathBuf::from)
}

/// Returns the ordered "real" command names: every built-in and extension
/// subcommand registered in the command tree.
///
/// The order is derived authoritatively from `build_real_cli` so it
/// automatically covers built-ins and extension commands and cannot drift from
/// the actual command tree. Used to detect alias-name collisions at
/// config-validation time.
pub(crate) fn real_command_names(catalog: &StaticCatalog) -> Vec<String> {
    let mut command = build_real_cli(catalog);
    command.build();
    command
        .get_subcommands()
        .map(|cmd| cmd.get_name().to_string())
        .collect()
}

/// Builds the real CLI command tree.
///
/// Core commands (summary, query) are defined statically.
/// Extension commands are added dynamically from the registry.
/// Aliases remain outside clap and are expanded before the single parse.
pub(crate) fn build_real_cli(catalog: &StaticCatalog) -> Command {
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
                .value_parser(clap::value_parser!(std::path::PathBuf))
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
                )
                .arg(
                    Arg::new("limit")
                        .long("limit")
                        .help("Return at most this many final results (zero returns none)")
                        .value_name("INTEGER")
                        .allow_negative_numbers(true)
                        .value_parser(clap::value_parser!(usize)),
                )
                .arg(
                    Arg::new("randomize")
                        .long("randomize")
                        .help("Randomize results, optionally using a reproducible u64 seed")
                        .long_help(
                            "Randomize final result order before applying --limit. \
                             With no SEED, uses fresh system randomness. With SEED, \
                             produces reproducible ordering. Because SEED is optional, \
                             place TAG_NAME before --randomize or after -- when ambiguous.",
                        )
                        .value_name("SEED")
                        .num_args(0..=1)
                        .allow_negative_numbers(true)
                        .value_parser(parse_randomize_seed)
                        .action(clap::ArgAction::Set),
                ),
        )
        .subcommand(catalog.diagram_command());

    // Add extension commands
    for ext_cmd in catalog.extension_commands() {
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

        // 2. Both config forms are recognized in the leading root prefix.
        assert_eq!(
            resolve_config_path_from_args(["ragtag", "--config", "/a/b.yaml", "my-alias"]),
            Some(std::path::PathBuf::from("/a/b.yaml"))
        );
        assert_eq!(
            resolve_config_path_from_args(["ragtag", "--config=/c/d.yaml", "summary"]),
            Some(std::path::PathBuf::from("/c/d.yaml"))
        );

        // 3. Last leading occurrence wins, including interleaved globals.
        assert_eq!(
            resolve_config_path_from_args([
                "ragtag",
                "--config",
                "/first",
                "--no-color",
                "--config",
                "/last",
                "summary"
            ]),
            Some(std::path::PathBuf::from("/last"))
        );
        assert_eq!(
            resolve_config_path_from_args(["ragtag", "--config=/first", "summary", "--config=",]),
            Some(std::path::PathBuf::from("/first"))
        );
        assert_eq!(
            resolve_config_path_from_args(["ragtag", "--config", "", "summary"]),
            Some(std::path::PathBuf::from(""))
        );
        assert_eq!(
            resolve_config_path_from_args(["ragtag", "--config", "--", "summary"]),
            None
        );
        assert_eq!(
            resolve_config_path_from_args([
                "ragtag", "--config", "/first", "--config", "--", "summary"
            ]),
            Some(std::path::PathBuf::from("/first"))
        );

        // 4. Scanning stops at the outer command, separator, or unknown option
        // so terminal values cannot redirect startup configuration.
        assert_eq!(
            resolve_config_path_from_args(["ragtag", "my-alias", "--", "--config", "/x.yaml"]),
            None
        );
        assert_eq!(
            resolve_config_path_from_args(["ragtag", "my-alias", "--", "--config=/x.yaml"]),
            None
        );
        assert_eq!(
            resolve_config_path_from_args(["ragtag", "file", "touch", "--tag", "--config=/x.yaml"]),
            None
        );
        assert_eq!(
            resolve_config_path_from_args(["ragtag", "--bogus", "--config", "/x.yaml"]),
            None
        );

        // 5. Falls back to env var when no leading flag is present.
        std::env::set_var(RAGTAG_CONFIG_ENV, "/env/config.yaml");
        assert_eq!(
            resolve_config_path_from_args(["ragtag", "summary"]),
            Some(std::path::PathBuf::from("/env/config.yaml"))
        );

        // 6. A leading flag overrides the environment.
        assert_eq!(
            resolve_config_path_from_args(["ragtag", "--config", "/cli.yaml"]),
            Some(std::path::PathBuf::from("/cli.yaml"))
        );

        std::env::remove_var(RAGTAG_CONFIG_ENV);
    }

    #[cfg(unix)]
    #[test]
    fn test_config_forms_preserve_non_utf8_os_path() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let path = OsString::from_vec(vec![b'c', 0xff, b'f']);
        let split_args = vec![
            OsString::from("ragtag"),
            OsString::from("--config"),
            path.clone(),
        ];
        let split = resolve_config_path_from_args(split_args).unwrap();
        assert_eq!(split.as_os_str().as_bytes(), path.as_os_str().as_bytes());

        let mut equals = OsString::from("--config=");
        equals.push(&path);
        let equals_args = vec![OsString::from("ragtag"), equals];
        let equals = resolve_config_path_from_args(equals_args).unwrap();
        assert_eq!(equals.as_os_str().as_bytes(), path.as_os_str().as_bytes());
    }

    #[test]
    fn test_real_command_names_matches_built_tree_including_help() {
        let catalog = StaticCatalog;
        let names = real_command_names(&catalog);
        assert_eq!(
            names,
            ["config", "file", "summary", "query", "diagram", "task", "help"]
        );
    }

    #[test]
    fn test_outer_scanner_global_set_matches_real_tree() {
        let catalog = StaticCatalog;
        let command = build_real_cli(&catalog);
        let globals = command
            .get_arguments()
            .filter(|argument| argument.is_global_set())
            .filter_map(clap::Arg::get_long)
            .collect::<Vec<_>>();
        assert_eq!(globals, ["config", "no-color"]);
    }

    #[test]
    fn test_file_command_contains_only_touch_and_accepts_repeated_tags() {
        let catalog = StaticCatalog;
        let command = build_real_cli(&catalog);
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

        let matches = build_real_cli(&catalog)
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
        assert!(build_real_cli(&catalog)
            .try_get_matches_from(["ragtag", "file", "unknown"])
            .is_err());
    }
}

//! ragtag — Entry point.
//!
//! Initializes logging, registers extensions, loads config,
//! builds the CLI, and dispatches commands.

use std::process::ExitCode;

use ragtag::cli;
use ragtag::commands;
use ragtag::config;
use ragtag::discovery::IgnoreWalker;
use ragtag::edit::AtomicFileEditor;
use ragtag::error::RagtagError;
use ragtag::extensions::task::TaskExtension;
use ragtag::extensions::{DefaultTagParser, ExtensionContext, ExtensionRegistry};
use ragtag::output;

fn main() -> ExitCode {
    // Initialize logging
    env_logger::init();

    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), RagtagError> {
    // Create and register extensions
    let mut registry = ExtensionRegistry::new();
    registry.register(Box::new(TaskExtension::new()))?;

    // Resolve the config path up front (before building the CLI), because
    // aliases come from config and must be registered as subcommands before
    // clap parses the arguments.
    let raw_args: Vec<String> = std::env::args().collect();
    let config_path = cli::resolve_config_path_from_args(&raw_args);
    let cwd = std::env::current_dir().map_err(RagtagError::Io)?;
    let app_config = config::load_config(config_path.as_deref(), &cwd)?;

    // Validate aliases against the set of real command names (built-ins +
    // extension commands) so collisions are caught before anything executes.
    let command_names = cli::real_command_names(&registry);
    app_config.validate_aliases(&command_names)?;

    // Initialize extensions with config
    for ext in registry.all_mut() {
        let config_value = ext
            .config_key()
            .and_then(|key| app_config.extension_configs.get(key));
        ext.init(config_value)?;
    }

    // Build CLI (including alias subcommands) and parse args
    let matches = cli::build_cli(&registry, &app_config.aliases).get_matches();

    // Global flags (those marked `.global(true)`) are propagated by clap to the
    // root matches regardless of where they appear on the command line, so they
    // are resolved here from the top-level matches and passed explicitly into
    // dispatch. This keeps global-flag behavior identical for directly-typed
    // and alias-expanded commands, since the re-parsed expanded matches do not
    // carry the user's original global flags.
    let no_color = matches.get_flag("no-color");

    // One-shot alias expansion: if the matched subcommand is an alias, rebuild
    // the argv from the alias's expansion plus any trailing user args, re-parse,
    // and dispatch the expanded command. Expansion happens exactly once — the
    // expanded command is NOT itself treated as an alias, so aliases never
    // chain into other aliases.
    if let Some((name, sub_m)) = matches.subcommand() {
        if let Some(alias) = app_config.aliases.iter().find(|a| a.name == name) {
            let trailing: Vec<String> = sub_m
                .get_many::<String>("args")
                .map(|vals| vals.cloned().collect())
                .unwrap_or_default();
            let mut argv = Vec::with_capacity(1 + trailing.len());
            argv.push("ragtag".to_string());
            argv.extend(alias.arguments.iter().cloned());
            argv.extend(trailing);
            let expanded = cli::build_cli(&registry, &app_config.aliases).get_matches_from(argv);
            // A global flag may appear before the alias name (captured in the
            // top-level matches) or trailing after the alias's own arguments,
            // where clap folds it into the alias subcommand's trailing args and
            // it surfaces only in the re-parsed expanded matches. Resolve each
            // global flag from both sources so an alias honors it in every
            // position, identically to the fully-expanded direct command.
            let no_color = no_color || expanded.get_flag("no-color");
            return dispatch(&expanded, no_color, &app_config, &registry);
        }
    }

    dispatch(&matches, no_color, &app_config, &registry)
}

/// Dispatches a parsed set of top-level matches to the appropriate command.
///
/// This is the single dispatch path shared by directly-typed commands and by
/// alias-expanded commands. Global flags (e.g. `no-color`) are resolved by the
/// caller from the original top-level matches and passed in via `no_color`, so
/// they are honored identically regardless of whether the command was typed
/// directly or reached through an alias expansion.
///
/// Aliases are NOT expanded here — an alias name that reaches this function
/// (e.g., an alias whose arguments reference another alias) is treated as an
/// unknown command, which prevents alias chaining.
fn dispatch(
    matches: &clap::ArgMatches,
    no_color: bool,
    app_config: &config::Config,
    registry: &ExtensionRegistry,
) -> Result<(), RagtagError> {
    // Resolve color mode from the caller-supplied global flag.
    let color_mode = output::resolve_color_mode(no_color, &app_config.output.color);

    match matches.subcommand() {
        Some(("config", config_matches)) => match config_matches.subcommand() {
            Some(("get", get_matches)) => {
                let key = get_matches
                    .get_one::<String>("key")
                    .expect("key is required");
                let value = commands::config::run_get(key, app_config)?;
                println!("{value}");
                Ok(())
            }
            _ => {
                let _ = cli::build_cli(registry, &app_config.aliases)
                    .find_subcommand_mut("config")
                    .expect("config subcommand exists")
                    .print_help();
                println!();
                Ok(())
            }
        },
        Some(("summary", sub_m)) => {
            let mut stdout = std::io::stdout();
            commands::summary::run(sub_m, app_config, registry, &color_mode, &mut stdout)
        }
        Some(("query", sub_m)) => {
            let mut stdout = std::io::stdout();
            commands::query::run(sub_m, app_config, registry, &color_mode, &mut stdout)
        }
        Some((name, sub_m)) => {
            // Try extension commands
            if let Some(ext) = registry.get_by_command_name(name) {
                let walker = IgnoreWalker::new(app_config)?;
                let parser = DefaultTagParser;
                let editor = AtomicFileEditor;
                let mut stdout = std::io::stdout();
                let mut stderr = std::io::stderr();

                let mut ctx = ExtensionContext {
                    walker: &walker,
                    parser: &parser,
                    editor: &editor,
                    color_mode: color_mode.clone(),
                    config: app_config,
                    stdout: &mut stdout,
                    stderr: &mut stderr,
                };
                ext.execute(sub_m, &mut ctx)
            } else {
                Err(RagtagError::UnknownCommand(name.to_string()))
            }
        }
        None => {
            // No subcommand — print help
            let _ = cli::build_cli(registry, &app_config.aliases).print_help();
            println!();
            Ok(())
        }
    }
}

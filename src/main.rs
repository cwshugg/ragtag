//! ragtag — Entry point.
//!
//! Initializes logging, registers extensions, loads config,
//! builds the CLI, and dispatches commands.

use std::process::ExitCode;

use ragtag::cli;
use ragtag::cli::aliases::{
    assemble_terminal_argv, expand_alias, resolve_outer_command, scan_outer_command, AliasIndex,
    OuterResolution, OuterScan,
};
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
    // Preserve the process arguments as OS-native values for all startup scans
    // and for exact suffix composition.
    let original_args: Vec<std::ffi::OsString> = std::env::args_os().collect();

    // Create and register extensions
    let mut registry = ExtensionRegistry::new();
    registry.register(Box::new(TaskExtension::new()))?;

    // Select and load configuration exactly once from the immutable original
    // argv before any alias-defined tokens exist.
    let config_path = cli::resolve_config_path_from_args(&original_args);
    let cwd = std::env::current_dir().map_err(RagtagError::Io)?;
    let loaded = config::load_config(config_path.as_deref(), &cwd)?;
    let app_config = &loaded.config;
    let config_uses_environment = loaded.has_environment_interpolation();

    // Validate aliases against the registered real command tree before
    // extension initialization so config-shape errors retain startup
    // precedence over extension-specific initialization errors.
    let command_names = cli::real_command_names(&registry);
    if let Err(error) = app_config.validate_aliases(&command_names) {
        if config_uses_environment {
            return Err(RagtagError::EnvironmentDerivedConfig);
        }
        return Err(error);
    }
    let aliases = AliasIndex::new(&app_config.aliases);

    // Initialize extensions with config before terminal parsing and dispatch.
    for ext in registry.all_mut() {
        let config_value = ext
            .config_key()
            .and_then(|key| app_config.extension_configs.get(key));
        if let Err(error) = ext.init(config_value) {
            if config_uses_environment {
                return Err(RagtagError::EnvironmentDerivedConfig);
            }
            return Err(error);
        }
    }

    let mut alias_defined_config_paths = Vec::new();
    let mut environment_alias = None;
    let terminal_args = match scan_outer_command(&original_args) {
        OuterScan::Command(command_index) => match match resolve_outer_command(
            original_args[command_index].as_os_str(),
            &command_names,
            &aliases,
        ) {
            Ok(resolution) => resolution,
            Err(_) if config_uses_environment => {
                return Err(RagtagError::EnvironmentDerivedConfigCommand);
            }
            Err(error) => return Err(error),
        } {
            OuterResolution::Alias {
                definition_index,
                selected_name,
            } => {
                let expansion = match expand_alias(
                    &aliases,
                    definition_index,
                    &selected_name,
                    &original_args[command_index + 1..],
                    &command_names,
                ) {
                    Ok(expansion) => expansion,
                    Err(_) if config_uses_environment => {
                        return Err(RagtagError::EnvironmentDerivedConfigCommand);
                    }
                    Err(error) => return Err(error),
                };
                if expansion.has_environment_interpolation() {
                    environment_alias = Some(selected_name.clone());
                }
                alias_defined_config_paths = expansion.alias_defined_config_paths();
                assemble_terminal_argv(&original_args, command_index, expansion)
            }
            OuterResolution::Real | OuterResolution::None => original_args.clone(),
        },
        OuterScan::Terminal => original_args.clone(),
    };

    // This is the process's only clap parse, always against the real tree.
    let matches = match cli::build_real_cli(&registry).try_get_matches_from(terminal_args) {
        Ok(matches) => matches,
        Err(error)
            if (config_uses_environment || environment_alias.is_some()) && error.use_stderr() =>
        {
            if config_uses_environment {
                return Err(RagtagError::EnvironmentDerivedConfigCommand);
            }
            return Err(RagtagError::EnvironmentDerivedAliasCommand {
                alias: environment_alias.clone().expect("checked as present"),
            });
        }
        Err(error) => error.exit(),
    };
    reconcile_terminal_config(
        &matches,
        loaded.source_path.as_deref(),
        &alias_defined_config_paths,
        &cwd,
    )?;
    let no_color = matches.get_flag("no-color");

    let result = dispatch(
        &matches,
        no_color,
        &loaded,
        &loaded.root_dir,
        &cwd,
        &registry,
    );
    match result {
        Err(_) if config_uses_environment => Err(RagtagError::EnvironmentDerivedConfigCommand),
        Err(_) if environment_alias.is_some() => Err(RagtagError::EnvironmentDerivedAliasCommand {
            alias: environment_alias.expect("checked as present"),
        }),
        other => other,
    }
}

/// Reconciles terminal clap's config value with the startup config selection.
///
/// A selector introduced by a trusted alias is accepted as terminal syntax but
/// never reloads configuration. An original post-command selector is accepted
/// only when it resolves to the same file already loaded (using canonical
/// paths when available and lexical paths as a fallback). A different selector
/// fails with `InvalidConfig`, because aliases and extensions were initialized
/// before the process's single clap parse. This function never reads or reloads
/// a config file.
fn reconcile_terminal_config(
    matches: &clap::ArgMatches,
    loaded_path: Option<&std::path::Path>,
    alias_defined_paths: &[std::path::PathBuf],
    startup_cwd: &std::path::Path,
) -> Result<(), RagtagError> {
    let Some(parsed_path) = matches.get_one::<std::path::PathBuf>("config") else {
        return Ok(());
    };

    if alias_defined_paths.iter().any(|path| path == parsed_path) {
        return Ok(());
    }

    let parsed_resolved = if parsed_path.is_absolute() {
        parsed_path.clone()
    } else {
        startup_cwd.join(parsed_path)
    };
    let matches_loaded = loaded_path.is_some_and(|loaded_path| {
        match (
            std::fs::canonicalize(&parsed_resolved),
            std::fs::canonicalize(loaded_path),
        ) {
            (Ok(parsed), Ok(loaded)) => parsed == loaded,
            _ => parsed_resolved == loaded_path,
        }
    });
    if matches_loaded {
        return Ok(());
    }

    Err(RagtagError::InvalidConfig(
        "--config must appear before the command name so configuration is loaded before alias expansion"
            .to_string(),
    ))
}

/// Dispatches a parsed set of top-level matches to the appropriate command.
///
/// This is the alias-unaware dispatch path shared by direct and expanded argv.
fn dispatch(
    matches: &clap::ArgMatches,
    no_color: bool,
    loaded_config: &config::LoadedConfig,
    root_dir: &std::path::Path,
    startup_cwd: &std::path::Path,
    registry: &ExtensionRegistry,
) -> Result<(), RagtagError> {
    let app_config = &loaded_config.config;
    // Resolve color mode from the caller-supplied global flag.
    let color_mode = output::resolve_color_mode(no_color, &app_config.output.color);

    match matches.subcommand() {
        Some(("config", config_matches)) => match config_matches.subcommand() {
            Some(("get", get_matches)) => {
                let key = get_matches
                    .get_one::<String>("key")
                    .expect("key is required");
                let value = commands::config::run_get(key, app_config, |value| {
                    loaded_config.is_environment_derived_value(value)
                })?;
                println!("{value}");
                Ok(())
            }
            _ => {
                let _ = cli::build_real_cli(registry)
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
        Some(("file", file_matches)) => match file_matches.subcommand() {
            Some(("touch", touch_matches)) => {
                let mut stdout = std::io::stdout();
                commands::file::run_touch(
                    touch_matches,
                    app_config,
                    root_dir,
                    startup_cwd,
                    &mut stdout,
                )
            }
            _ => Err(RagtagError::UnknownCommand("file".to_string())),
        },
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
            let _ = cli::build_real_cli(registry).print_help();
            println!();
            Ok(())
        }
    }
}

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

    // Build CLI and parse args
    let cli_cmd = cli::build_cli(&registry);
    let matches = cli_cmd.get_matches();

    // Load config
    let config_path = cli::resolve_config_path(&matches);
    let cwd = std::env::current_dir().map_err(RagtagError::Io)?;
    let app_config = config::load_config(config_path.as_deref(), &cwd)?;
    app_config.validate()?;

    // Initialize extensions with config
    for ext in registry.all_mut() {
        let config_value = ext
            .config_key()
            .and_then(|key| app_config.extension_configs.get(key));
        ext.init(config_value)?;
    }

    // Resolve color mode
    let no_color = matches.get_flag("no-color");
    let color_mode = output::resolve_color_mode(no_color, &app_config.output.color);

    // Dispatch
    match matches.subcommand() {
        Some(("config", config_matches)) => match config_matches.subcommand() {
            Some(("get", get_matches)) => {
                let key = get_matches
                    .get_one::<String>("key")
                    .expect("key is required");
                let value = commands::config::run_get(key, &app_config)?;
                println!("{value}");
                Ok(())
            }
            _ => {
                let _ = cli::build_cli(&registry)
                    .find_subcommand_mut("config")
                    .expect("config subcommand exists")
                    .print_help();
                println!();
                Ok(())
            }
        },
        Some(("summary", sub_m)) => {
            let mut stdout = std::io::stdout();
            commands::summary::run(sub_m, &app_config, &registry, &color_mode, &mut stdout)
        }
        Some(("query", sub_m)) => {
            let mut stdout = std::io::stdout();
            commands::query::run(sub_m, &app_config, &registry, &color_mode, &mut stdout)
        }
        Some((name, sub_m)) => {
            // Try extension commands
            if let Some(ext) = registry.get_by_command_name(name) {
                let walker = IgnoreWalker::new(&app_config)?;
                let parser = DefaultTagParser;
                let editor = AtomicFileEditor;
                let mut stdout = std::io::stdout();
                let mut stderr = std::io::stderr();

                let mut ctx = ExtensionContext {
                    walker: &walker,
                    parser: &parser,
                    editor: &editor,
                    color_mode: color_mode.clone(),
                    config: &app_config,
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
            let _ = cli::build_cli(&registry).print_help();
            println!();
            Ok(())
        }
    }
}

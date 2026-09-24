//! Application composition, startup, parsing, and command dispatch.

use std::process::ExitCode;
use std::sync::Arc;

use crate::cli;
use crate::cli::aliases::{
    assemble_terminal_argv, expand_alias, resolve_outer_command, scan_outer_command, AliasIndex,
    OuterResolution, OuterScan,
};
use crate::commands;
use crate::config;
use crate::discovery::IgnoreWalker;
use crate::edit::AtomicFileEditor;
use crate::error::RagtagError;
use crate::extensions::task::semantics::TaskSemantics;
use crate::extensions::task::{TaskExtension, TASKS_CONFIG_KEY};
use crate::extensions::{DefaultTagParser, ExtensionContext, ExtensionRegistry};
use crate::output;

/// Config-independent descriptors used to build the authoritative command tree.
#[derive(Debug, Default)]
pub(crate) struct StaticCatalog;

impl StaticCatalog {
    /// Returns statically linked extension commands.
    pub(crate) fn extension_commands(&self) -> Vec<clap::Command> {
        vec![crate::extensions::task::cli::build_task_command()]
    }

    /// Returns the source-only diagram command.
    pub(crate) fn diagram_command(&self) -> clap::Command {
        crate::diagram::cli::build_command()
    }
}

/// Fully resolved immutable application components.
pub(crate) struct ApplicationComponents {
    pub(crate) loaded: config::LoadedConfig,
    pub(crate) task_semantics: Arc<TaskSemantics>,
    pub(crate) extensions: ExtensionRegistry,
}

/// Builds configured application components from one loaded configuration.
pub(crate) struct ApplicationFactory;

impl ApplicationFactory {
    /// Consumes raw extension configuration and constructs shared semantics once.
    pub(crate) fn build(
        loaded: config::LoadedConfig,
    ) -> Result<
        (
            ApplicationComponents,
            Vec<crate::extensions::task::config::TaskConfigWarning>,
        ),
        RagtagError,
    > {
        Self::build_with_resolver(loaded, TaskSemantics::resolve)
    }

    fn build_with_resolver(
        mut loaded: config::LoadedConfig,
        resolver: impl FnOnce(
            Option<serde_yml::Value>,
        ) -> Result<
            (
                TaskSemantics,
                Vec<crate::extensions::task::config::TaskConfigWarning>,
            ),
            RagtagError,
        >,
    ) -> Result<
        (
            ApplicationComponents,
            Vec<crate::extensions::task::config::TaskConfigWarning>,
        ),
        RagtagError,
    > {
        let task_value = loaded.config.extension_configs.remove(TASKS_CONFIG_KEY);
        if !loaded.config.extension_configs.is_empty() {
            return Err(RagtagError::InvalidConfig(
                "unknown configuration section".to_string(),
            ));
        }
        let (task_semantics, warnings) = resolver(task_value)?;
        let task_semantics = Arc::new(task_semantics);
        let mut extensions = ExtensionRegistry::new();
        extensions.register(Box::new(TaskExtension::new(Arc::clone(&task_semantics))))?;
        Ok((
            ApplicationComponents {
                loaded,
                task_semantics,
                extensions,
            },
            warnings,
        ))
    }
}

/// Runs the process against the real environment and standard streams.
pub fn run_process() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// Resolves startup state, parses once, and dispatches one command.
fn run() -> Result<(), RagtagError> {
    let original_args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let static_catalog = StaticCatalog;

    let config_path = cli::resolve_config_path_from_args(&original_args);
    let cwd = std::env::current_dir().map_err(RagtagError::Io)?;
    let loaded = config::load_config(config_path.as_deref(), &cwd)?;
    let config_uses_environment = loaded.has_environment_interpolation();

    let command_names = cli::real_command_names(&static_catalog);
    if let Err(error) = loaded.config.validate_aliases(&command_names) {
        if config_uses_environment {
            return Err(RagtagError::EnvironmentDerivedConfig);
        }
        return Err(error);
    }
    let (components, warnings) = match ApplicationFactory::build(loaded) {
        Ok(built) => built,
        Err(_) if config_uses_environment => return Err(RagtagError::EnvironmentDerivedConfig),
        Err(error) => return Err(error),
    };
    let aliases = AliasIndex::new(&components.loaded.config.aliases);
    for warning in warnings {
        eprintln!("ragtag warning[{}]: {}", warning.code, warning.message);
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

    let matches = match cli::build_real_cli(&static_catalog).try_get_matches_from(terminal_args) {
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
        components.loaded.source_path.as_deref(),
        &alias_defined_config_paths,
        &cwd,
    )?;
    let no_color = matches.get_flag("no-color");

    let result = dispatch(&matches, no_color, &components, &cwd, &static_catalog);
    match result {
        Err(_) if config_uses_environment => Err(RagtagError::EnvironmentDerivedConfigCommand),
        Err(_) if environment_alias.is_some() => Err(RagtagError::EnvironmentDerivedAliasCommand {
            alias: environment_alias.expect("checked as present"),
        }),
        other => other,
    }
}

/// Reconciles terminal clap's config selector with startup selection.
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

/// Dispatches parsed top-level matches.
fn dispatch(
    matches: &clap::ArgMatches,
    no_color: bool,
    components: &ApplicationComponents,
    startup_cwd: &std::path::Path,
    static_catalog: &StaticCatalog,
) -> Result<(), RagtagError> {
    let app_config = &components.loaded.config;
    let color_mode = output::resolve_color_mode(no_color, &app_config.output.color);

    match matches.subcommand() {
        Some(("config", config_matches)) => match config_matches.subcommand() {
            Some(("get", get_matches)) => {
                let key = get_matches
                    .get_one::<String>("key")
                    .expect("key is required");
                let value = commands::config::run_get(
                    key,
                    app_config,
                    components.task_semantics.config(),
                    |value| components.loaded.is_environment_derived_value(value),
                )?;
                println!("{value}");
                Ok(())
            }
            _ => {
                let _ = cli::build_real_cli(static_catalog)
                    .find_subcommand_mut("config")
                    .expect("config subcommand exists")
                    .print_help();
                println!();
                Ok(())
            }
        },
        Some(("summary", sub_m)) => {
            let mut stdout = std::io::stdout();
            commands::summary::run(
                sub_m,
                app_config,
                &components.extensions,
                &color_mode,
                &mut stdout,
            )
        }
        Some(("query", sub_m)) => {
            let mut stdout = std::io::stdout();
            commands::query::run(
                sub_m,
                app_config,
                &components.extensions,
                &color_mode,
                &mut stdout,
            )
        }
        Some(("diagram", sub_m)) => {
            let mut stdout = std::io::stdout();
            let mut stderr = std::io::stderr();
            crate::diagram::command::run(
                sub_m,
                Arc::clone(&components.task_semantics),
                app_config,
                startup_cwd,
                &mut stdout,
                &mut stderr,
            )
        }
        Some(("file", file_matches)) => match file_matches.subcommand() {
            Some(("touch", touch_matches)) => {
                let mut stdout = std::io::stdout();
                commands::file::run_touch(
                    touch_matches,
                    app_config,
                    &components.loaded.root_dir,
                    startup_cwd,
                    &mut stdout,
                )
            }
            _ => Err(RagtagError::UnknownCommand("file".to_string())),
        },
        Some((name, sub_m)) => {
            if let Some(extension) = components.extensions.get_by_command_name(name) {
                let walker = IgnoreWalker::new(app_config)?;
                let parser = DefaultTagParser;
                let editor = AtomicFileEditor;
                let mut stdout = std::io::stdout();
                let mut stderr = std::io::stderr();
                let mut context = ExtensionContext {
                    walker: &walker,
                    parser: &parser,
                    editor: &editor,
                    color_mode: color_mode.clone(),
                    config: app_config,
                    stdout: &mut stdout,
                    stderr: &mut stderr,
                };
                extension.execute(sub_m, &mut context)
            } else {
                Err(RagtagError::UnknownCommand(name.to_string()))
            }
        }
        None => {
            let _ = cli::build_real_cli(static_catalog).print_help();
            println!();
            Ok(())
        }
    }
}

#[cfg(test)]
mod factory_tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn task_semantics_are_resolved_once_and_shared_with_the_extension() {
        let directory = tempfile::tempdir().unwrap();
        let loaded = config::load_config(None, directory.path()).unwrap();
        let calls = Cell::new(0usize);
        let (components, _) = ApplicationFactory::build_with_resolver(loaded, |value| {
            calls.set(calls.get() + 1);
            TaskSemantics::resolve(value)
        })
        .unwrap();
        assert_eq!(calls.get(), 1);
        assert_eq!(components.extensions.all().len(), 1);
    }

    #[test]
    fn unknown_config_key_is_absent_from_display_and_debug_errors() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ragtag.yaml");
        let sentinel = "SENTINEL_CONFIG_SECRET";
        std::fs::write(
            &path,
            format!("\"bad\\n\\u001b]8;;x\\u0007\\u202e{sentinel}\": {{}}\n"),
        )
        .unwrap();
        let loaded = config::load_config(Some(&path), directory.path()).unwrap();
        let error = ApplicationFactory::build(loaded).err().unwrap();
        assert!(!error.to_string().contains(sentinel));
        assert!(!format!("{error:?}").contains(sentinel));
    }
}

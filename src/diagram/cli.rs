//! Public diagram command-line shape.

use clap::{Arg, Command};

pub(crate) fn build_command() -> Command {
    Command::new("diagram")
        .about("Generate source-only diagrams")
        .infer_subcommands(false)
        .disable_help_subcommand(true)
        .subcommand_required(true)
        .subcommand(kind("task-tree", "Generate a task parent-child tree"))
        .subcommand(kind(
            "task-buckets",
            "Generate recursively nested task buckets",
        ))
}

fn kind(name: &'static str, about: &'static str) -> Command {
    Command::new(name)
        .about(about)
        .arg(
            Arg::new("path")
                .long("path")
                .help("Search path; falls back to RAGTAG_PATH, then \".\"")
                .value_name("PATH"),
        )
        .arg(
            Arg::new("direction")
                .long("direction")
                .help("Diagram layout direction")
                .value_parser(["down", "right", "up", "left"])
                .default_value("down"),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .help("Write D2 source atomically to FILE; '-' selects stdout")
                .value_name("FILE")
                .default_value("-"),
        )
        .arg(
            Arg::new("filter")
                .long("filter")
                .help("Strict task filter expression")
                .value_name("EXPR"),
        )
        .arg(
            Arg::new("all")
                .long("all")
                .help("Include statuses excluded by task configuration")
                .action(clap::ArgAction::SetTrue),
        )
}

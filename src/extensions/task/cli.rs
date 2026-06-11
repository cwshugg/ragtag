//! Task extension CLI definitions.
//!
//! Builds the clap `Command` tree for the `task` subcommand.

use clap::{Arg, Command};

/// Builds the complete `task` subcommand tree.
pub fn build_task_command() -> Command {
    Command::new("task")
        .about("Track and manage tasks embedded in plain text files")
        .subcommand_required(true)
        .subcommand(
            Command::new("create")
                .about("Create a new task and print the @task(...) string to stdout")
                .arg(
                    Arg::new("title")
                        .long("title")
                        .help("Task title")
                        .value_name("STR"),
                )
                .arg(
                    Arg::new("description")
                        .long("description")
                        .help("Task description")
                        .value_name("STR"),
                )
                .arg(
                    Arg::new("owner")
                        .long("owner")
                        .help("Task owner")
                        .value_name("STR"),
                )
                .arg(
                    Arg::new("status")
                        .long("status")
                        .help("Task status")
                        .value_name("STR"),
                )
                .arg(
                    Arg::new("priority")
                        .long("priority")
                        .help("Task priority (0 = highest)")
                        .value_name("NUM"),
                )
                .arg(
                    Arg::new("ttc-estimate")
                        .long("ttc-estimate")
                        .help("Estimated time to complete")
                        .value_name("NUM"),
                )
                .arg(
                    Arg::new("time-units")
                        .long("time-units")
                        .help("Time units (hours, days, weeks)")
                        .value_name("STR"),
                )
                .arg(
                    Arg::new("pid")
                        .long("pid")
                        .help("Parent task ID")
                        .value_name("STR"),
                ),
        )
        .subcommand(
            Command::new("list")
                .about("List tasks found in files")
                .arg(
                    Arg::new("path")
                        .long("path")
                        .help("Search path (file or directory); falls back to RAGTAG_PATH env var, then \".\"")
                        .value_name("PATH"),
                )
                .arg(
                    Arg::new("filter")
                        .long("filter")
                        .help("Filter tasks by attribute (e.g., status=active)")
                        .value_name("EXPR")
                        .action(clap::ArgAction::Append),
                )
                .arg(
                    Arg::new("sort")
                        .long("sort")
                        .help("Sort by field (e.g., priority, status, title)")
                        .value_name("FIELD"),
                )
                .arg(
                    Arg::new("reverse")
                        .long("reverse")
                        .help("Reverse sort order")
                        .action(clap::ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("all")
                        .long("all")
                        .short('a')
                        .help("Show all tasks, including excluded status categories (done, abandoned)")
                        .action(clap::ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("get")
                .about("Look up a task by ID or title")
                .arg(
                    Arg::new("search")
                        .help("Task ID or title search string")
                        .required(true)
                        .index(1),
                )
                .arg(
                    Arg::new("path")
                        .long("path")
                        .help("Search path (file or directory); falls back to RAGTAG_PATH env var, then \".\"")
                        .value_name("PATH"),
                )
                .arg(
                    Arg::new("all")
                        .long("all")
                        .short('a')
                        .help("Show all tasks, including excluded status categories (done, abandoned)")
                        .action(clap::ArgAction::SetTrue),
                ),
        )
        .subcommand(
            Command::new("summary")
                .about("Display a table-like summary of tasks grouped by field")
                .arg(
                    Arg::new("path")
                        .long("path")
                        .help("Search path (file or directory); falls back to RAGTAG_PATH env var, then \".\"")
                        .value_name("PATH"),
                )
                .arg(
                    Arg::new("group")
                        .long("group")
                        .help("Group tasks by field (status, owner, priority)")
                        .value_name("FIELD")
                        .default_value("status"),
                )
                .arg(
                    Arg::new("sort")
                        .long("sort")
                        .help("Sort tasks within each group by field")
                        .value_name("FIELD"),
                )
                .arg(
                    Arg::new("filter")
                        .long("filter")
                        .help("Filter tasks by attribute (e.g., status=active)")
                        .value_name("EXPR")
                        .action(clap::ArgAction::Append),
                )
                .arg(
                    Arg::new("all")
                        .long("all")
                        .short('a')
                        .help("Show all tasks, including excluded status categories (done, abandoned)")
                        .action(clap::ArgAction::SetTrue),
                ),
        )
        .subcommand(build_set_command(
            "set-status",
            "Update a task's status",
            "status",
            "New status",
        ))
        .subcommand(build_set_command(
            "set-priority",
            "Update a task's priority",
            "priority",
            "New priority (non-negative integer)",
        ))
        .subcommand(build_set_command(
            "set-time",
            "Update a task's time_spent",
            "time",
            "New time_spent value",
        ))
        .subcommand(build_set_command(
            "set-owner",
            "Update a task's owner",
            "owner",
            "New owner",
        ))
        .subcommand(build_set_command(
            "set-parent",
            "Update a task's parent ID",
            "pid",
            "New parent task ID",
        ))
}

/// Helper to build a set-* subcommand with common arguments.
fn build_set_command(
    name: &'static str,
    about: &'static str,
    value_arg: &'static str,
    value_help: &'static str,
) -> Command {
    Command::new(name)
        .about(about)
        .arg(
            Arg::new("id")
                .long("id")
                .help("Task ID")
                .value_name("ID")
                .required(true),
        )
        .arg(
            Arg::new("path")
                .long("path")
                .help("Search path (file or directory); falls back to RAGTAG_PATH env var, then \".\"")
                .value_name("PATH"),
        )
        .arg(
            Arg::new(value_arg)
                .long(value_arg)
                .help(value_help)
                .value_name("VALUE"),
        )
}

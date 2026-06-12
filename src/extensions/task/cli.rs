//! Task extension CLI definitions.
//!
//! Builds the clap `Command` tree for the `task` subcommand.

use clap::{Arg, ArgAction, Command};

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
                    Arg::new("worktime-estimate")
                        .long("worktime-estimate")
                        .help("Estimated time to complete")
                        .value_name("NUM"),
                )
                .arg(
                    Arg::new("worktime-units")
                        .long("worktime-units")
                        .help("Worktime units (hours, days, weeks)")
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
                        .help("Filter expression (e.g., \"status=active AND priority<=2\")")
                        .value_name("EXPR"),
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
                )
                .arg(
                    Arg::new("format")
                        .long("format")
                        .help("Output format (default or raw)")
                        .value_parser(["default", "raw"])
                        .default_value("default"),
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
                        .default_value("priority"),
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
                        .help("Filter expression (e.g., \"status=active AND priority<=2\")")
                        .value_name("EXPR"),
                )
                .arg(
                    Arg::new("all")
                        .long("all")
                        .short('a')
                        .help("Show all tasks, including excluded status categories (done, abandoned)")
                        .action(clap::ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("format")
                        .long("format")
                        .help("Output format (table or list)")
                        .value_parser(["table", "list"])
                        .default_value("table"),
                ),
        )
        .subcommand(
            Command::new("get-attr")
                .about("Get the value of a task attribute")
                .arg(
                    Arg::new("id")
                        .help("Task ID or ID prefix")
                        .required(true)
                        .index(1),
                )
                .arg(
                    Arg::new("attr")
                        .help("Attribute name (e.g., status, priority, owner)")
                        .required(true)
                        .index(2),
                )
                .arg(
                    Arg::new("path")
                        .long("path")
                        .help("Search path (file or directory); falls back to RAGTAG_PATH env var, then \".\"")
                        .value_name("PATH"),
                ),
        )
        .subcommand(
            Command::new("set-attr")
                .about("Set the value of a task attribute")
                .arg(
                    Arg::new("id")
                        .help("Task ID or ID prefix")
                        .required(true)
                        .index(1),
                )
                .arg(
                    Arg::new("attr")
                        .help("Attribute name (e.g., status, priority, owner)")
                        .required(true)
                        .index(2),
                )
                .arg(
                    Arg::new("value")
                        .help("New value for the attribute")
                        .allow_hyphen_values(true)
                        .required(true)
                        .index(3),
                )
                .arg(
                    Arg::new("path")
                        .long("path")
                        .help("Search path (file or directory); falls back to RAGTAG_PATH env var, then \".\"")
                        .value_name("PATH"),
                )
                .arg(
                    Arg::new("no-edit")
                        .long("no-edit")
                        .action(ArgAction::SetTrue)
                        .help("Don't modify the file; print the updated @task(...) string instead"),
                ),
        )
}

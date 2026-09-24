//! Task command dispatcher.
//!
//! Routes task sub-subcommands (create, list, get-attr, set-attr, etc.)
//! to their respective implementations.

pub mod abandon;
pub mod activate;
pub mod block;
pub mod complete;
pub mod create;
pub mod deactivate;
pub mod get;
pub mod get_attr;
pub mod list;
pub mod prioritize;
pub mod set_attr;
pub mod status_change;
pub mod summary;
pub mod time;

use std::borrow::Cow;
use std::path::Path;

use super::config::TaskConfig;
use super::filter::{references_field, FilterExpr};
use super::models::{TaskTag, TaskType};
use crate::edit::{
    edit_task_tag, read_file_snapshot, verify_file_snapshot,
    write_file_atomically_if_unchanged_with_hook, FileSnapshot,
};
use crate::error::RagtagError;
use crate::extensions::ExtensionContext;
use crate::output::format::terminal_safe;

/// Dispatches to the appropriate task subcommand.
pub fn dispatch(
    matches: &clap::ArgMatches,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(), RagtagError> {
    match matches.subcommand() {
        Some(("abandon", sub_m)) => abandon::run(sub_m, config, ctx),
        Some(("activate", sub_m)) => activate::run(sub_m, config, ctx),
        Some(("block", sub_m)) => block::run(sub_m, config, ctx),
        Some(("complete", sub_m)) => complete::run(sub_m, config, ctx),
        Some(("create", sub_m)) => create::run(sub_m, config, ctx),
        Some(("deactivate", sub_m)) => deactivate::run(sub_m, config, ctx),
        Some(("get", sub_m)) => get::run(sub_m, config, ctx),
        Some(("list", sub_m)) => list::run(sub_m, config, ctx),
        Some(("prioritize", sub_m)) => prioritize::run(sub_m, config, ctx),
        Some(("summary", sub_m)) => summary::run(sub_m, config, ctx),
        Some(("get-attr", sub_m)) => get_attr::run(sub_m, config, ctx),
        Some(("set-attr", sub_m)) => set_attr::run(sub_m, config, ctx),
        Some(("time", sub_m)) => time::run(sub_m, config, ctx),
        _ => Err(RagtagError::UnknownCommand(
            "unknown task subcommand".to_string(),
        )),
    }
}

/// Collects all tasks from discovered files.
///
/// Walks the file tree, parses tags, and returns all valid `TaskTag` instances.
/// Invalid or unreadable files are skipped with a warning.
pub fn collect_tasks(
    path: &Path,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<Vec<TaskTag>, RagtagError> {
    let files = ctx.walker.walk(path)?;
    let mut tasks: Vec<TaskTag> = Vec::new();

    for file_path in &files {
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(error) => {
                log::warn!("skipping an unreadable input file ({:?})", error.kind());
                continue;
            }
        };
        let tags = ctx.parser.parse_file(&content, file_path);
        for tag in &tags {
            if tag.name == config.tag_name {
                match TaskTag::from_tag(tag, config) {
                    Ok(task) => tasks.push(task),
                    Err(_) => continue,
                }
            }
        }
    }

    Ok(tasks)
}

/// Validates that a filter expression is parseable.
///
/// A valid filter must contain one of the comparison operators
/// `!=`, `>=`, `<=`, `>`, `<`, or `=` outside any quoted span.
pub fn validate_task_filter(filter: &str) -> Result<(), RagtagError> {
    crate::filter::ensure_condition_has_operator(filter)
}

/// Applies a simple filter expression to a task.
///
/// Supports `=`, `!=`, `>`, `<`, `>=`, and `<=` operators.
/// Numeric fields are compared numerically; string fields are compared
/// lexicographically.
pub fn apply_task_filter(task: &TaskTag, filter: &str) -> bool {
    let Some((field, op, value)) = crate::filter::split_condition(filter) else {
        // Should not reach here since we validate above.
        return false;
    };
    let field_str = task_field_value(task, field).unwrap_or_else(|| {
        log::warn!("filter expression references an unknown task field");
        Cow::Borrowed("")
    });
    crate::filter::apply_operator(op, &field_str, value)
}

/// Collects, filters, and applies the shared list/summary visibility rules.
pub fn select_tasks(
    path: &Path,
    filter_source: Option<&str>,
    show_all: bool,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<Vec<TaskTag>, RagtagError> {
    let tasks = collect_tasks(path, config, ctx)?;
    select_collected_tasks(tasks, filter_source, show_all, config)
}

fn select_collected_tasks(
    mut tasks: Vec<TaskTag>,
    filter_source: Option<&str>,
    show_all: bool,
    config: &TaskConfig,
) -> Result<Vec<TaskTag>, RagtagError> {
    let parsed_filter = filter_source
        .map(|source| {
            let parsed = super::filter::parse_filter_expr(source)?;
            super::filter::validate_filter_expr(&parsed)?;
            Ok::<FilterExpr, RagtagError>(parsed)
        })
        .transpose()?;

    if let Some(expression) = &parsed_filter {
        tasks.retain(|task| super::filter::evaluate_filter(expression, task));
    }
    apply_default_visibility(
        &mut tasks,
        config,
        show_all,
        filter_source,
        parsed_filter.as_ref(),
    );
    Ok(tasks)
}

/// Applies the default list/summary exclusions while preserving filter compatibility.
fn apply_default_visibility(
    tasks: &mut Vec<TaskTag>,
    config: &TaskConfig,
    show_all: bool,
    filter_source: Option<&str>,
    parsed_filter: Option<&FilterExpr>,
) {
    if show_all {
        return;
    }

    let filter_mentions_status =
        filter_source.is_some_and(|expression| expression.contains("status"));
    if !filter_mentions_status {
        let excluded = config.get_excluded_keywords();
        tasks.retain(|task| !excluded.contains(&task.status));
    }

    let filter_mentions_type =
        parsed_filter.is_some_and(|expression| references_field(expression, "type"));
    if !filter_mentions_type {
        tasks.retain(|task| !task.task_type.is_project());
    }
}

/// Ensures a complete task tag contains exactly one canonical semantic type.
pub fn normalize_task_type_attribute(
    tag: &str,
    task_type: &TaskType,
) -> Result<String, RagtagError> {
    let formatted = format!("\"{}\"", create::escape_for_tag(task_type.as_str()));
    crate::edit::upsert_unique_named_attribute(tag, "type", &formatted)
}

/// Gets a semantic task field value without imposing caller-specific errors.
pub fn task_field_value<'a>(task: &'a TaskTag, field: &str) -> Option<Cow<'a, str>> {
    let value = match field {
        "id" => Cow::Borrowed(task.id.as_str()),
        "pid" => Cow::Borrowed(task.pid.as_deref().unwrap_or("")),
        "title" => Cow::Borrowed(task.title.as_str()),
        "description" => Cow::Borrowed(task.description.as_deref().unwrap_or("")),
        "owner" => Cow::Borrowed(task.owner.as_str()),
        "status" => Cow::Borrowed(task.status.as_str()),
        "type" => Cow::Borrowed(task.task_type.as_str()),
        // Empty optional values intentionally sort before numeric values.
        "priority" => Cow::Owned(
            task.priority
                .map(|value| value.to_string())
                .unwrap_or_default(),
        ),
        "worktime_spent" => Cow::Owned(
            task.worktime_spent
                .map(|value| value.to_string())
                .unwrap_or_default(),
        ),
        "worktime_estimate" => Cow::Owned(
            task.worktime_estimate
                .map(|value| value.to_string())
                .unwrap_or_default(),
        ),
        "time_created" => Cow::Borrowed(task.time_created.as_deref().unwrap_or("")),
        "time_last_updated" => Cow::Borrowed(task.time_last_updated.as_deref().unwrap_or("")),
        "worktime_units" => Cow::Borrowed(task.worktime_units.as_str()),
        _ => return None,
    };
    Some(value)
}

/// A complete semantic change to apply through the shared mutation transaction.
pub(super) struct TaskMutation {
    edits: Vec<(String, String)>,
    task_type: TaskType,
    confirmation: String,
}

impl TaskMutation {
    pub(super) fn new(
        edits: Vec<(String, String)>,
        task_type: TaskType,
        confirmation: String,
    ) -> Self {
        Self {
            edits,
            task_type,
            confirmation,
        }
    }
}

/// Applies a task mutation, enforces invariants, and atomically persists it.
pub(super) fn mutate_task(
    id: &str,
    path: &Path,
    no_edit: bool,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
    build: impl FnOnce(&TaskTag) -> Result<TaskMutation, RagtagError>,
) -> Result<(), RagtagError> {
    mutate_task_with_hooks(id, path, no_edit, config, ctx, build, (|| {}, || {}))
}

/// Mutation implementation with deterministic seams around both conflict checks.
fn mutate_task_with_hooks(
    id: &str,
    path: &Path,
    no_edit: bool,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
    build: impl FnOnce(&TaskTag) -> Result<TaskMutation, RagtagError>,
    hooks: (impl FnOnce(), impl FnOnce()),
) -> Result<(), RagtagError> {
    let (before_apply, before_persist) = hooks;
    let (task, snapshot) = find_task_by_id(id, path, config, ctx)?;
    let mutation = build(&task)?;
    before_apply();
    verify_file_snapshot(&task.location.file_path, &snapshot)?;
    verify_selected_task_source(&task, &snapshot, config, ctx)?;

    let content = snapshot.content();
    let range = task.location.byte_range();
    let original_tag = content
        .get(range.clone())
        .ok_or_else(|| RagtagError::ParseError {
            file: task.location.file_path.clone(),
            line: task.location.line,
            message: "task source range is out of bounds or not on UTF-8 boundaries".to_string(),
        })?;
    let edits: Vec<(&str, &str)> = mutation
        .edits
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    let modified_tag = edit_task_tag(original_tag, &edits)?;
    let modified_tag = normalize_task_type_attribute(&modified_tag, &mutation.task_type)?;

    if no_edit {
        writeln!(ctx.stdout, "{modified_tag}").map_err(RagtagError::Io)?;
        return Ok(());
    }

    let mut new_content = String::with_capacity(content.len() - range.len() + modified_tag.len());
    new_content.push_str(&content[..range.start]);
    new_content.push_str(&modified_tag);
    new_content.push_str(&content[range.end..]);
    write_file_atomically_if_unchanged_with_hook(
        &task.location.file_path,
        &snapshot,
        &new_content,
        before_persist,
    )?;
    writeln!(ctx.stdout, "{}", terminal_safe(&mutation.confirmation)).map_err(RagtagError::Io)
}

/// Re-parses the selected snapshot and confirms one exact task occupies its span.
fn verify_selected_task_source(
    task: &TaskTag,
    snapshot: &FileSnapshot,
    config: &TaskConfig,
    ctx: &ExtensionContext,
) -> Result<(), RagtagError> {
    let range = task.location.byte_range();
    let exact_matches = ctx
        .parser
        .parse_file(snapshot.content(), &task.location.file_path)
        .into_iter()
        .filter(|tag| {
            tag.name == config.tag_name
                && tag.location.byte_range() == range
                && TaskTag::from_tag(tag, config).is_ok_and(|candidate| candidate.id == task.id)
        })
        .count();
    if exact_matches != 1 {
        return Err(RagtagError::SourceConflict(task.location.file_path.clone()));
    }
    Ok(())
}

/// Finds a task by ID (exact or prefix) across all discovered files.
///
/// Returns the task and the file content it was found in.
/// Errors if no task is found, or if multiple tasks match the prefix.
///
/// Title search is intentionally excluded here. Mutation commands
/// (`set-attr`) must operate by ID only for safety — matching by title
/// substring could inadvertently modify the wrong task when titles
/// are ambiguous. Read-only lookup by title is available via
/// `search_tasks` in the `get` module.
///
/// NOTE: This function intentionally duplicates the file-walking logic
/// from `collect_tasks`. The duplication is deliberate because
/// `find_task_by_id` tracks the source file path for each task and
/// performs a targeted file re-read when the match is found, whereas
/// `collect_tasks` only collects `TaskTag` values. The two functions
/// have different return types and ownership needs, so merging them
/// would add complexity without meaningful benefit.
pub(crate) fn find_task_by_id(
    id: &str,
    path: &Path,
    config: &TaskConfig,
    ctx: &mut ExtensionContext,
) -> Result<(TaskTag, FileSnapshot), RagtagError> {
    let files = ctx.walker.walk(path)?;

    // Keep one authoritative snapshot per file and refer to it by index from
    // candidates, avoiding one content clone per task.
    let mut snapshots = Vec::new();
    let mut all_tasks: Vec<(TaskTag, usize)> = Vec::new();

    for file_path in &files {
        let snapshot = match read_file_snapshot(file_path) {
            Ok(snapshot) => snapshot,
            Err(RagtagError::FileRead { source, .. }) => {
                log::warn!("skipping an unreadable input file ({:?})", source.kind());
                continue;
            }
            Err(_) => {
                log::warn!("skipping an unreadable input file");
                continue;
            }
        };
        let snapshot_index = snapshots.len();
        let tags = ctx.parser.parse_file(snapshot.content(), file_path);
        for tag in &tags {
            if tag.name == config.tag_name {
                match TaskTag::from_tag(tag, config) {
                    Ok(task) => all_tasks.push((task, snapshot_index)),
                    Err(_) => {
                        log::warn!("failed to parse a task tag at line {}", tag.location.line);
                    }
                }
            }
        }
        snapshots.push(snapshot);
    }

    // Try exact match first
    let exact_idx: Vec<usize> = all_tasks
        .iter()
        .enumerate()
        .filter(|(_, (t, _))| t.id == id)
        .map(|(i, _)| i)
        .collect();
    if exact_idx.len() == 1 {
        let (task, snapshot_index) = all_tasks
            .into_iter()
            .nth(exact_idx[0])
            .expect("guaranteed by check");
        return Ok((task, snapshots.swap_remove(snapshot_index)));
    }

    // Try prefix match
    let prefix_idx: Vec<usize> = all_tasks
        .iter()
        .enumerate()
        .filter(|(_, (t, _))| t.id.starts_with(id))
        .map(|(i, _)| i)
        .collect();

    match prefix_idx.len() {
        0 => Err(RagtagError::ExtensionError {
            extension_name: "Task Manager".to_string(),
            message: format!(
                "task not found with id \"{}\"\nhint: run 'ragtag task list' to see all tasks",
                terminal_safe(id)
            ),
        }),
        1 => {
            let (task, snapshot_index) = all_tasks
                .into_iter()
                .nth(prefix_idx[0])
                .expect("guaranteed by match arm");
            Ok((task, snapshots.swap_remove(snapshot_index)))
        }
        _ => {
            let mut details = format!(
                "Multiple tasks match id prefix \"{}\". Please provide a longer ID string.\n",
                terminal_safe(id)
            );
            for &i in &prefix_idx {
                let (ref t, _) = all_tasks[i];
                details.push_str(&format!(
                    "{} {} {}\n",
                    terminal_safe(&t.id),
                    terminal_safe(&t.location.file_path.display().to_string()),
                    terminal_safe(&t.title)
                ));
            }
            Err(RagtagError::ExtensionError {
                extension_name: "Task Manager".to_string(),
                message: details,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ColorMode, Config};
    use crate::discovery::FileWalker;
    use crate::edit::FileEditor;
    use crate::extensions::DefaultTagParser;
    use crate::models::TagLocation;
    use std::ops::Range;
    use std::path::PathBuf;

    struct StaticWalker {
        file: PathBuf,
    }

    impl FileWalker for StaticWalker {
        fn walk(&self, _: &Path) -> Result<Vec<PathBuf>, RagtagError> {
            Ok(vec![self.file.clone()])
        }
    }

    struct UnusedEditor;

    impl FileEditor for UnusedEditor {
        fn update_tag_attribute(
            &self,
            _: &Path,
            _: Range<usize>,
            _: &str,
            _: &str,
        ) -> Result<(), RagtagError> {
            panic!("shared task mutation must not use ExtensionContext::editor")
        }
    }

    /// Runs the shared mutation transaction with deterministic conflict hooks.
    fn run_hooked_mutation(
        file: &Path,
        no_edit: bool,
        before_apply: impl FnOnce(),
        before_persist: impl FnOnce(),
    ) -> (Result<(), RagtagError>, Vec<u8>) {
        let walker = StaticWalker {
            file: file.to_path_buf(),
        };
        let parser = DefaultTagParser;
        let editor = UnusedEditor;
        let app_config = Config::default();
        let task_config = TaskConfig::default();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let result = {
            let mut ctx = ExtensionContext {
                walker: &walker,
                parser: &parser,
                editor: &editor,
                color_mode: ColorMode::Never,
                config: &app_config,
                stdout: &mut stdout,
                stderr: &mut stderr,
            };
            mutate_task_with_hooks(
                "abc",
                file,
                no_edit,
                &task_config,
                &mut ctx,
                |task| {
                    Ok(TaskMutation::new(
                        vec![("status".to_string(), "\"done\"".to_string())],
                        task.task_type.clone(),
                        "updated".to_string(),
                    ))
                },
                (before_apply, before_persist),
            )
        };
        (result, stdout)
    }

    fn make_task(id: &str, status: &str, owner: &str, priority: Option<u32>) -> TaskTag {
        TaskTag {
            id: id.to_string(),
            pid: None,
            title: format!("Task {id}"),
            description: None,
            owner: owner.to_string(),
            status: status.to_string(),
            task_type: TaskType::Item,
            priority,
            worktime_spent: None,
            worktime_estimate: Some(4.0),
            time_created: None,
            time_last_updated: None,
            worktime_units: "hours".to_string(),
            location: TagLocation::new(PathBuf::from("test.md"), 1, 1, 0, 50),
        }
    }

    #[test]
    fn test_validate_task_filter_valid_expressions() {
        assert!(validate_task_filter("status=active").is_ok());
        assert!(validate_task_filter("status!=done").is_ok());
        assert!(validate_task_filter("priority>0").is_ok());
        assert!(validate_task_filter("priority<5").is_ok());
        assert!(validate_task_filter("priority>=1").is_ok());
        assert!(validate_task_filter("priority<=3").is_ok());
    }

    #[test]
    fn test_validate_task_filter_invalid_expression() {
        assert!(validate_task_filter("nooperator").is_err());
    }

    #[test]
    fn test_apply_task_filter_equality() {
        let task = make_task("abc", "active", "alice", Some(1));
        assert!(apply_task_filter(&task, "status=active"));
        assert!(!apply_task_filter(&task, "status=done"));
        assert!(apply_task_filter(&task, "owner=alice"));
        assert!(!apply_task_filter(&task, "owner=bob"));
    }

    #[test]
    fn test_apply_task_filter_not_equal() {
        let task = make_task("abc", "active", "alice", Some(1));
        assert!(apply_task_filter(&task, "status!=done"));
        assert!(!apply_task_filter(&task, "status!=active"));
    }

    #[test]
    fn test_apply_task_filter_numeric_comparison() {
        let task = make_task("abc", "active", "alice", Some(2));
        assert!(apply_task_filter(&task, "priority>1"));
        assert!(!apply_task_filter(&task, "priority>2"));
        assert!(apply_task_filter(&task, "priority>=2"));
        assert!(apply_task_filter(&task, "priority<3"));
        assert!(!apply_task_filter(&task, "priority<2"));
        assert!(apply_task_filter(&task, "priority<=2"));
    }

    #[test]
    fn test_task_field_value_all_fields_and_unknown() {
        let mut task = make_task("abc123", "active", "alice", Some(1));
        task.pid = Some("parent".to_string());
        task.description = Some("description".to_string());
        task.worktime_spent = Some(2.5);
        task.time_created = Some("created".to_string());
        task.time_last_updated = Some("updated".to_string());
        let expected = [
            ("id", "abc123"),
            ("pid", "parent"),
            ("title", "Task abc123"),
            ("description", "description"),
            ("owner", "alice"),
            ("status", "active"),
            ("type", "item"),
            ("priority", "1"),
            ("worktime_spent", "2.5"),
            ("worktime_estimate", "4"),
            ("time_created", "created"),
            ("time_last_updated", "updated"),
            ("worktime_units", "hours"),
        ];

        for (field, value) in expected {
            assert_eq!(task_field_value(&task, field).as_deref(), Some(value));
        }
        assert!(task_field_value(&task, "nonexistent").is_none());

        task.priority = None;
        task.pid = None;
        assert_eq!(task_field_value(&task, "priority").as_deref(), Some(""));
        assert_eq!(task_field_value(&task, "pid").as_deref(), Some(""));
    }

    #[test]
    fn test_shared_selection_status_substring_and_exact_type_matrix() {
        let config = TaskConfig::default();
        let mut active_project = make_task("active-project", "active", "alice", Some(1));
        active_project.task_type = TaskType::Project;
        let mut done_project = make_task("done-project", "done", "alice", Some(1));
        done_project.task_type = TaskType::Project;
        let all_tasks = vec![
            make_task("active-item", "active", "alice", Some(1)),
            make_task("done-item", "done", "alice", Some(1)),
            active_project,
            done_project,
        ];
        let cases = [
            (None, false, vec!["active-item"]),
            (
                None,
                true,
                vec!["active-item", "done-item", "active-project", "done-project"],
            ),
            (
                Some("title!=status"),
                false,
                vec!["active-item", "done-item"],
            ),
            (Some("prototype!=x"), false, vec!["active-item"]),
            (Some("type=project"), false, vec!["active-project"]),
            (
                Some("status=done AND type=project"),
                false,
                vec!["done-project"],
            ),
        ];

        for (filter, show_all, expected) in cases {
            let selected =
                select_collected_tasks(all_tasks.clone(), filter, show_all, &config).unwrap();
            assert_eq!(
                selected
                    .iter()
                    .map(|task| task.id.as_str())
                    .collect::<Vec<_>>(),
                expected,
                "filter={filter:?}, show_all={show_all}"
            );
        }
    }

    #[test]
    fn mutation_rejects_same_length_target_change_before_applying() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("tasks.md");
        let original = "@task(id=\"abc\", title=\"Old\", status=\"new\")\n";
        let concurrent = "@task(id=\"abc\", title=\"New\", status=\"new\")\n";
        std::fs::write(&file, original).unwrap();

        let (result, stdout) = run_hooked_mutation(
            &file,
            false,
            || std::fs::write(&file, concurrent).unwrap(),
            || {},
        );

        assert!(matches!(result, Err(RagtagError::SourceConflict(_))));
        assert!(stdout.is_empty());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), concurrent);
    }

    #[test]
    fn no_edit_mutation_rejects_a_stale_source() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("tasks.md");
        let original = "@task(id=\"abc\", title=\"Old\", status=\"new\")\n";
        let concurrent = "@task(id=\"abc\", title=\"New\", status=\"new\")\n";
        std::fs::write(&file, original).unwrap();

        let (result, stdout) = run_hooked_mutation(
            &file,
            true,
            || std::fs::write(&file, concurrent).unwrap(),
            || panic!("no-edit must not enter persistence"),
        );

        assert!(matches!(result, Err(RagtagError::SourceConflict(_))));
        assert!(stdout.is_empty());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), concurrent);
    }

    #[test]
    fn mutation_rejects_duplicate_exact_ids_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("tasks.md");
        let duplicate = concat!(
            "@task(id=\"abc\", title=\"First\", status=\"new\")\n",
            "@task(id=\"abc\", title=\"Second\", status=\"new\")\n"
        );
        std::fs::write(&file, duplicate).unwrap();

        let (result, stdout) = run_hooked_mutation(
            &file,
            false,
            || panic!("ambiguous selection must fail before mutation"),
            || panic!("ambiguous selection must fail before persistence"),
        );

        assert!(matches!(result, Err(RagtagError::ExtensionError { .. })));
        assert!(stdout.is_empty());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), duplicate);
    }
}

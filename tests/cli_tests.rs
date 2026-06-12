//! CLI integration tests using assert_cmd.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

fn ragtag() -> Command {
    Command::cargo_bin("ragtag").unwrap()
}

fn fixtures_dir() -> String {
    format!("{}/tests/fixtures", env!("CARGO_MANIFEST_DIR"))
}

// === Version and Help ===

#[test]
fn test_version() {
    ragtag()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("ragtag"));
}

#[test]
fn test_help() {
    ragtag()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("ragtag"))
        .stdout(predicate::str::contains("summary"))
        .stdout(predicate::str::contains("query"))
        .stdout(predicate::str::contains("task"));
}

#[test]
fn test_tasks_help() {
    ragtag()
        .args(["task", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("create"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("set-attr"))
        .stdout(predicate::str::contains("get-attr"));
}

// === Summary ===

#[test]
fn test_summary() {
    ragtag()
        .args(["summary", "--path", &fixtures_dir()])
        .assert()
        .success()
        .stdout(predicate::str::contains("tag"))
        .stdout(predicate::str::contains("task"));
}

#[test]
fn test_summary_single_file() {
    let path = format!("{}/simple_tags.txt", fixtures_dir());
    ragtag()
        .args(["summary", "--path", &path])
        .assert()
        .success()
        .stdout(predicate::str::contains("note"));
}

// === Query ===

#[test]
fn test_query_tag() {
    let path = format!("{}/simple_tags.txt", fixtures_dir());
    ragtag()
        .args(["query", "tag", "--path", &path])
        .assert()
        .success()
        .stdout(predicate::str::contains("@tag"));
}

#[test]
fn test_query_count() {
    let path = format!("{}/simple_tags.txt", fixtures_dir());
    ragtag()
        .args(["query", "tag", "--path", &path, "--count"])
        .assert()
        .success()
        .stdout(predicate::str::contains("3")); // 3 @tag entries
}

#[test]
fn test_query_filter() {
    let path = format!("{}/simple_tags.txt", fixtures_dir());
    ragtag()
        .args(["query", "tag", "--path", &path, "--filter", "key=value"])
        .assert()
        .success();
}

// === Tasks Create ===

#[test]
fn test_tasks_create() {
    ragtag()
        .args([
            "task",
            "create",
            "--title",
            "Test Task",
            "--worktime-estimate",
            "4",
            "--worktime-units",
            "hours",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("@task("))
        .stdout(predicate::str::contains("title=\"Test Task\""))
        .stdout(predicate::str::contains("worktime_estimate=4"));
}

#[test]
fn test_tasks_create_with_all_fields() {
    ragtag()
        .args([
            "task",
            "create",
            "--title",
            "Full Task",
            "--description",
            "A full task",
            "--owner",
            "alice",
            "--status",
            "active",
            "--priority",
            "1",
            "--worktime-estimate",
            "8.5",
            "--worktime-units",
            "days",
            "--pid",
            "parent123",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("owner=\"alice\""))
        .stdout(predicate::str::contains("status=\"active\""))
        .stdout(predicate::str::contains("priority=1"));
}

#[test]
fn test_tasks_create_includes_timestamps() {
    // Newly created tasks must include auto-generated time_created and time_last_updated.
    let output = ragtag()
        .args([
            "task",
            "create",
            "--title",
            "Timestamped Task",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // Both fields must appear in the output tag string.
    assert!(
        output_str.contains("time_created="),
        "Expected time_created in output, got:\n{output_str}"
    );
    assert!(
        output_str.contains("time_last_updated="),
        "Expected time_last_updated in output, got:\n{output_str}"
    );
    // Values should look like an ISO 8601 UTC timestamp (basic pattern check).
    assert!(
        output_str.contains("time_created=\"20"),
        "Expected time_created to be an ISO-like timestamp"
    );
    assert!(
        output_str.contains("time_last_updated=\"20"),
        "Expected time_last_updated to be an ISO-like timestamp"
    );
}

#[test]
fn test_tasks_set_attr_auto_updates_time_last_updated() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"testid1234567890\", title=\"Test\", worktime_estimate=4, worktime_units=\"hours\", status=\"new\")",
    )
    .unwrap();

    // After set-attr, time_last_updated must be present in the file.
    ragtag()
        .args([
            "--no-color",
            "task",
            "set-attr",
            "testid1234567890",
            "status",
            "active",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success();

    let content = fs::read_to_string(&file).unwrap();
    assert!(
        content.contains("time_last_updated=\"20"),
        "Expected auto-populated time_last_updated after set-attr, file content:\n{content}"
    );
    assert!(
        content.contains("status=\"active\""),
        "Expected updated status in file content:\n{content}"
    );
}

#[test]
fn test_tasks_set_attr_no_edit_auto_updates_time_last_updated() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    let original =
        "@task(id=\"testid1234567890\", title=\"Test\", worktime_estimate=4, worktime_units=\"hours\", status=\"new\")";
    fs::write(&file, original).unwrap();

    // --no-edit should print the tag with both the new attr value AND updated time_last_updated.
    let output = ragtag()
        .args([
            "--no-color",
            "task",
            "set-attr",
            "testid1234567890",
            "status",
            "active",
            "--no-edit",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    assert!(
        output_str.contains("status=\"active\""),
        "Expected updated status in --no-edit output"
    );
    assert!(
        output_str.contains("time_last_updated=\"20"),
        "Expected auto-populated time_last_updated in --no-edit output"
    );
    // The original file must NOT be modified.
    let file_content = fs::read_to_string(&file).unwrap();
    assert_eq!(file_content, original, "File should be unchanged with --no-edit");
}

// === Tasks List ===

#[test]
fn test_tasks_list() {
    let path = format!("{}/tasks.md", fixtures_dir());
    ragtag()
        .args(["--no-color", "task", "list", "--path", &path])
        .assert()
        .success()
        .stdout(predicate::str::contains("Design API"))
        .stdout(predicate::str::contains("[alice]"))
        .stdout(predicate::str::contains("[1/active]"));
}

#[test]
fn test_tasks_list_sort() {
    let path = format!("{}/tasks.md", fixtures_dir());
    let output = ragtag()
        .args([
            "--no-color",
            "task",
            "list",
            "--path",
            &path,
            "--sort",
            "priority",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    // Verify that tasks appear in priority order in the output
    let output_str = String::from_utf8(output).unwrap();
    let lines: Vec<&str> = output_str.lines().collect();
    assert!(
        !lines.is_empty(),
        "Expected sorted task output, got empty output"
    );
    // Tasks should be sorted by priority ascending; verify by checking
    // that the first task in output has a lower or equal priority number
    // than the last task.
    if lines.len() >= 2 {
        // Extract priority numbers from the bracket notation [N/status]
        let extract_priority = |line: &str| -> Option<u32> {
            let start = line.find('[')? + 1;
            let slash = line[start..].find('/')? + start;
            line[start..slash].parse().ok()
        };
        let priorities: Vec<u32> = lines.iter().filter_map(|l| extract_priority(l)).collect();
        for window in priorities.windows(2) {
            assert!(
                window[0] <= window[1],
                "Tasks not sorted by priority: {} should come before {}",
                window[0],
                window[1]
            );
        }
    }
}

#[test]
fn test_tasks_list_filter() {
    let path = format!("{}/tasks.md", fixtures_dir());
    ragtag()
        .args([
            "--no-color",
            "task",
            "list",
            "--path",
            &path,
            "--filter",
            "status=active",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Design API"));
}

// === Tasks Set Commands ===

#[test]
fn test_tasks_set_attr_status() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"testid1234567890\", title=\"Test\", worktime_estimate=4, worktime_units=\"hours\", status=\"new\")",
    ).unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "set-attr",
            "testid1234567890",
            "status",
            "active",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated status to \"active\""));

    // Verify file was actually modified
    let content = fs::read_to_string(&file).unwrap();
    assert!(content.contains("\"active\""));
}

#[test]
fn test_tasks_set_attr_worktime_spent() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"testid1234567890\", title=\"Test\", worktime_estimate=4, worktime_units=\"hours\", status=\"new\")",
    ).unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "set-attr",
            "testid1234567890",
            "worktime_spent",
            "2.5",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Updated worktime_spent to \"2.5\"",
        ));
}

#[test]
fn test_tasks_set_attr_time_created_blocked() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"testid1234567890\", title=\"Test\", worktime_estimate=4, worktime_units=\"hours\", status=\"new\")",
    )
    .unwrap();

    // time_created is automatically managed — manual set-attr must be rejected.
    ragtag()
        .args([
            "--no-color",
            "task",
            "set-attr",
            "testid1234567890",
            "time_created",
            "2026-06-12T09:00:00Z",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("automatically managed"));
}

#[test]
fn test_tasks_set_attr_time_last_updated_blocked() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"testid1234567890\", title=\"Test\", worktime_estimate=4, worktime_units=\"hours\", status=\"new\")",
    )
    .unwrap();

    // time_last_updated is automatically managed — manual set-attr must be rejected.
    ragtag()
        .args([
            "--no-color",
            "task",
            "set-attr",
            "testid1234567890",
            "time_last_updated",
            "2026-06-12T10:00:00Z",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("automatically managed"));
}

#[test]
fn test_tasks_set_attr_owner() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"testid1234567890\", title=\"Test\", worktime_estimate=4, worktime_units=\"hours\", status=\"new\")",
    ).unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "set-attr",
            "testid1234567890",
            "owner",
            "alice",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated owner to \"alice\""));
}

#[test]
fn test_tasks_set_attr_parent() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"testid1234567890\", title=\"Test\", worktime_estimate=4, worktime_units=\"hours\", status=\"new\")",
    ).unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "set-attr",
            "testid1234567890",
            "pid",
            "parent123",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated pid to \"parent123\""));
}

#[test]
fn test_tasks_set_attr_priority() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"testid1234567890\", title=\"Test\", worktime_estimate=4, worktime_units=\"hours\", status=\"new\")",
    ).unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "set-attr",
            "testid1234567890",
            "priority",
            "2",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated priority to \"2\""));

    // Verify file was actually modified
    let content = fs::read_to_string(&file).unwrap();
    assert!(content.contains("priority=2"));
}

// === Tasks Get ===

#[test]
fn test_tasks_get_by_id() {
    let path = format!("{}/tasks.md", fixtures_dir());
    ragtag()
        .args([
            "--no-color",
            "task",
            "get",
            "a1b2c3d4e5f67890",
            "--path",
            &path,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Title: Design API"))
        .stdout(predicate::str::contains("Owner: alice"));
}

#[test]
fn test_tasks_get_by_title() {
    let path = format!("{}/tasks.md", fixtures_dir());
    ragtag()
        .args(["--no-color", "task", "get", "Design", "--path", &path])
        .assert()
        .success()
        .stdout(predicate::str::contains("Title: Design API"));
}

#[test]
fn test_tasks_get_by_prefix() {
    let path = format!("{}/tasks.md", fixtures_dir());
    ragtag()
        .args(["--no-color", "task", "get", "a1b2c3", "--path", &path])
        .assert()
        .success()
        .stdout(predicate::str::contains("Title: Design API"))
        .stdout(predicate::str::contains("ID: a1b2c3d4e5f67890"));
}

#[test]
fn test_tasks_get_no_match() {
    let path = format!("{}/tasks.md", fixtures_dir());
    ragtag()
        .args([
            "--no-color",
            "task",
            "get",
            "nonexistent_xyz",
            "--path",
            &path,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("No task found for"));
}

#[test]
fn test_tasks_get_empty_search_rejected() {
    let path = format!("{}/tasks.md", fixtures_dir());
    ragtag()
        .args(["--no-color", "task", "get", "", "--path", &path])
        .assert()
        .failure()
        .stderr(predicate::str::contains("search string must not be empty"));
}

#[test]
fn test_tasks_get_whitespace_search_rejected() {
    let path = format!("{}/tasks.md", fixtures_dir());
    ragtag()
        .args(["--no-color", "task", "get", "   ", "--path", &path])
        .assert()
        .failure()
        .stderr(predicate::str::contains("search string must not be empty"));
}

// === Error Cases ===

#[test]
fn test_nonexistent_path() {
    ragtag()
        .args(["summary", "--path", "/nonexistent/path/xyz"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("path not found"));
}

#[test]
fn test_invalid_status() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    let original_content = "@task(id=\"testid1234567890\", title=\"Test\", worktime_estimate=4, worktime_units=\"hours\", status=\"new\")";
    fs::write(&file, original_content).unwrap();

    ragtag()
        .args([
            "task",
            "set-attr",
            "testid1234567890",
            "status",
            "invalid_status_xyz",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid status"));

    // Verify the file was NOT modified by the invalid status attempt
    let after_content = fs::read_to_string(&file).unwrap();
    assert_eq!(
        after_content, original_content,
        "File should not be modified when an invalid status is provided"
    );
}

#[test]
fn test_task_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(&file, "No tasks here.").unwrap();

    ragtag()
        .args([
            "task",
            "set-attr",
            "nonexistent1234567",
            "status",
            "done",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("task not found"));
}

#[test]
fn test_no_color_flag() {
    let path = format!("{}/tasks.md", fixtures_dir());
    let output = ragtag()
        .args(["--no-color", "task", "list", "--path", &path])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // Verify no ANSI escape codes
    assert!(!output_str.contains("\x1b["));
}

#[test]
fn test_explicit_config() {
    let config = format!("{}/tests/fixtures/.ragtag.yaml", env!("CARGO_MANIFEST_DIR"));
    ragtag()
        .args(["--config", &config, "summary", "--path", &fixtures_dir()])
        .assert()
        .success();
}

// === Multi-file scanning ===

#[test]
fn test_multi_file_summary() {
    let path = format!("{}/multi_file", fixtures_dir());
    ragtag()
        .args(["summary", "--path", &path])
        .assert()
        .success()
        .stdout(predicate::str::contains("note"))
        .stdout(predicate::str::contains("tag"));
}

// === Duplicate task IDs ===

#[test]
fn test_duplicate_task_ids() {
    let path = format!("{}/duplicate_ids.md", fixtures_dir());
    ragtag()
        .args([
            "task",
            "set-attr",
            "dupeid123456789a",
            "status",
            "done",
            "--path",
            &path,
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Multiple tasks match id prefix"));
}

// === Full workflow ===

#[test]
fn test_full_workflow() {
    let dir = tempfile::tempdir().unwrap();

    // Create a task
    let create_output = ragtag()
        .args([
            "task",
            "create",
            "--title",
            "Workflow Test",
            "--worktime-estimate",
            "4",
            "--worktime-units",
            "hours",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let task_str = String::from_utf8(create_output).unwrap();
    assert!(task_str.contains("@task("));

    // Write it to a file
    let file = dir.path().join("workflow.md");
    fs::write(&file, &task_str).unwrap();

    // List tasks from that file
    ragtag()
        .args([
            "--no-color",
            "task",
            "list",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Workflow Test"));

    // Extract the task ID from the created output
    let id_start = task_str.find("id=\"").unwrap() + 4;
    let id_end = task_str[id_start..].find('"').unwrap() + id_start;
    let task_id = &task_str[id_start..id_end];

    // Set status
    ragtag()
        .args([
            "--no-color",
            "task",
            "set-attr",
            task_id,
            "status",
            "active",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated status to \"active\""));

    // Set time
    ragtag()
        .args([
            "--no-color",
            "task",
            "set-attr",
            task_id,
            "worktime_spent",
            "2.5",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Updated worktime_spent to \"2.5\"",
        ));

    // Set owner
    ragtag()
        .args([
            "--no-color",
            "task",
            "set-attr",
            task_id,
            "owner",
            "alice",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated owner to \"alice\""));

    // Verify final file state
    let final_content = fs::read_to_string(&file).unwrap();
    assert!(final_content.contains("\"active\""));
    assert!(final_content.contains("\"alice\""));
}

// === Flag coverage ===

#[test]
fn test_task_list_all_shows_done() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"done1234567890ab\", title=\"Done Task\", status=\"done\", worktime_estimate=1, worktime_units=\"hours\")\n\
         @task(id=\"active12345678ab\", title=\"Active Task\", status=\"active\", worktime_estimate=2, worktime_units=\"hours\")",
    ).unwrap();

    // Without --all, done task should be excluded
    ragtag()
        .args([
            "--no-color",
            "task",
            "list",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Active Task"))
        .stdout(predicate::str::contains("Done Task").not());

    // With --all, done task should be included
    ragtag()
        .args([
            "--no-color",
            "task",
            "list",
            "--all",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Done Task"))
        .stdout(predicate::str::contains("Active Task"));
}

#[test]
fn test_task_list_filter_status() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"aaa1234567890ab\", title=\"Active Task\", status=\"active\", worktime_estimate=1, worktime_units=\"hours\")\n\
         @task(id=\"bbb1234567890ab\", title=\"Blocked Task\", status=\"blocked\", worktime_estimate=2, worktime_units=\"hours\")",
    ).unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "list",
            "--filter",
            "status=active",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Active Task"))
        .stdout(predicate::str::contains("Blocked Task").not());
}

#[test]
fn test_task_list_sort_title() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"zzz1234567890ab\", title=\"Zebra\", status=\"active\", worktime_estimate=1, worktime_units=\"hours\")\n\
         @task(id=\"aaa1234567890ab\", title=\"Apple\", status=\"active\", worktime_estimate=2, worktime_units=\"hours\")",
    ).unwrap();

    let output = ragtag()
        .args([
            "--no-color",
            "task",
            "list",
            "--sort",
            "title",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let apple_pos = output_str.find("Apple").expect("should contain Apple");
    let zebra_pos = output_str.find("Zebra").expect("should contain Zebra");
    assert!(
        apple_pos < zebra_pos,
        "Apple should appear before Zebra when sorted by title"
    );
}

#[test]
fn test_task_list_reverse() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"aaa1234567890ab\", title=\"Apple\", status=\"active\", priority=1, worktime_estimate=1, worktime_units=\"hours\")\n\
         @task(id=\"zzz1234567890ab\", title=\"Zebra\", status=\"active\", priority=2, worktime_estimate=2, worktime_units=\"hours\")",
    ).unwrap();

    let output = ragtag()
        .args([
            "--no-color",
            "task",
            "list",
            "--sort",
            "title",
            "--reverse",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    let apple_pos = output_str.find("Apple").expect("should contain Apple");
    let zebra_pos = output_str.find("Zebra").expect("should contain Zebra");
    assert!(
        zebra_pos < apple_pos,
        "Zebra should appear before Apple when sorted by title reversed"
    );
}

#[test]
fn test_task_summary_default_group_by_priority() {
    let path = format!("{}/tasks.md", fixtures_dir());
    // Without --group, the default grouping should be by priority (shows "Priority:" headers)
    ragtag()
        .args([
            "--no-color",
            "task",
            "summary",
            "--all",
            "--path",
            &path,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Priority:"));
}

#[test]
fn test_task_summary_group_owner() {
    let path = format!("{}/tasks.md", fixtures_dir());
    ragtag()
        .args([
            "--no-color",
            "task",
            "summary",
            "--group",
            "owner",
            "--all",
            "--path",
            &path,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Owner: alice"))
        .stdout(predicate::str::contains("Owner: bob"));
}

#[test]
fn test_task_summary_format_table() {
    let path = format!("{}/tasks.md", fixtures_dir());
    // --format table should produce the same output as the default (table headers)
    ragtag()
        .args([
            "--no-color",
            "task",
            "summary",
            "--format",
            "table",
            "--all",
            "--path",
            &path,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Path"))
        .stdout(predicate::str::contains("Title"))
        .stdout(predicate::str::contains("Owner"))
        .stdout(predicate::str::contains("ID"));
}

#[test]
fn test_task_summary_format_list() {
    let path = format!("{}/tasks.md", fixtures_dir());
    let output = ragtag()
        .args([
            "--no-color",
            "task",
            "summary",
            "--format",
            "list",
            "--all",
            "--path",
            &path,
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let output_str = String::from_utf8(output).unwrap();

    // List format should NOT have table column headers
    assert!(
        !output_str.contains("Path  "),
        "list format should not have table headers"
    );

    // Should have group headers
    assert!(
        output_str.contains("Priority: ") || output_str.contains("Owner: "),
        "list format should have group headers"
    );

    // Should have task details in bracket format: [owner] [priority/status]
    assert!(
        output_str.contains("[alice]") || output_str.contains("[bob]"),
        "list format should show owner in brackets"
    );
}

#[test]
fn test_task_summary_format_list_grouped() {
    let path = format!("{}/tasks.md", fixtures_dir());
    let output = ragtag()
        .args([
            "--no-color",
            "task",
            "summary",
            "--format",
            "list",
            "--group",
            "owner",
            "--all",
            "--path",
            &path,
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let output_str = String::from_utf8(output).unwrap();

    // Should have owner group headers
    assert!(output_str.contains("Owner: alice"));
    assert!(output_str.contains("Owner: bob"));
}

#[test]
fn test_set_attr_negative_time_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"testid1234567890\", title=\"Test\", worktime_estimate=4, worktime_units=\"hours\", status=\"new\")",
    ).unwrap();

    ragtag()
        .args([
            "task",
            "set-attr",
            "testid1234567890",
            "worktime_spent",
            "-5",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("non-negative"));
}

// === get-attr tests ===

#[test]
fn test_get_attr_status() {
    let path = format!("{}/tasks.md", fixtures_dir());
    ragtag()
        .args([
            "task",
            "get-attr",
            "a1b2c3d4e5f67890",
            "status",
            "--path",
            &path,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("active"));
}

#[test]
fn test_get_attr_title() {
    let path = format!("{}/tasks.md", fixtures_dir());
    ragtag()
        .args([
            "task",
            "get-attr",
            "a1b2c3d4e5f67890",
            "title",
            "--path",
            &path,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Design API"));
}

#[test]
fn test_get_attr_priority() {
    let path = format!("{}/tasks.md", fixtures_dir());
    ragtag()
        .args([
            "task",
            "get-attr",
            "a1b2c3d4e5f67890",
            "priority",
            "--path",
            &path,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("1"));
}

#[test]
fn test_get_attr_unknown() {
    let path = format!("{}/tasks.md", fixtures_dir());
    ragtag()
        .args([
            "task",
            "get-attr",
            "a1b2c3d4e5f67890",
            "nonexistent",
            "--path",
            &path,
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown attribute"));
}

#[test]
fn test_get_attr_by_prefix() {
    let path = format!("{}/tasks.md", fixtures_dir());
    ragtag()
        .args(["task", "get-attr", "a1b2c3", "status", "--path", &path])
        .assert()
        .success()
        .stdout(predicate::str::contains("active"));
}

#[test]
fn test_get_attr_time_last_updated() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"testid1234567890\", title=\"Test\", worktime_estimate=4, time_last_updated=\"2026-06-12T10:00:00Z\", worktime_units=\"hours\", status=\"new\")",
    )
    .unwrap();

    ragtag()
        .args([
            "task",
            "get-attr",
            "testid1234567890",
            "time_last_updated",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("2026-06-12T10:00:00Z"));
}

#[test]
fn test_set_attr_no_edit() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    let original_content =
        "@task(id=\"testid1234567890\", title=\"Test\", worktime_estimate=4, worktime_units=\"hours\", status=\"new\")";
    fs::write(&file, original_content).unwrap();

    let output = ragtag()
        .args([
            "task",
            "set-attr",
            "testid1234567890",
            "status",
            "active",
            "--no-edit",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("@task("))
        .stdout(predicate::str::contains("status=\"active\""))
        .get_output()
        .stdout
        .clone();

    let stdout_str = String::from_utf8(output).unwrap();

    // Output should be single-line (preserving the original layout)
    let tag_line = stdout_str.trim();
    assert!(
        !tag_line.contains('\n'),
        "single-line task should produce single-line output, got: {tag_line}"
    );

    // Verify the file was NOT modified
    let after_content = fs::read_to_string(&file).unwrap();
    assert_eq!(
        after_content, original_content,
        "File should not be modified when --no-edit is used"
    );
}

#[test]
fn test_set_attr_no_edit_multiline_preserves_layout() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    let original_content = "@task(\n    id=\"testid1234567890\",\n    title=\"Test\",\n    worktime_estimate=4,\n    worktime_units=\"hours\",\n    status=\"new\"\n)";
    fs::write(&file, original_content).unwrap();

    let output = ragtag()
        .args([
            "task",
            "set-attr",
            "testid1234567890",
            "status",
            "active",
            "--no-edit",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("status=\"active\""))
        .get_output()
        .stdout
        .clone();

    let stdout_str = String::from_utf8(output).unwrap();

    // Output should be multi-line (preserving the original layout)
    let tag_text = stdout_str.trim();
    assert!(
        tag_text.contains('\n'),
        "multi-line task should produce multi-line output, got: {tag_text}"
    );

    // Verify indentation is preserved
    assert!(
        tag_text.contains("    id=\"testid1234567890\""),
        "indentation should be preserved"
    );

    // Verify the file was NOT modified
    let after_content = fs::read_to_string(&file).unwrap();
    assert_eq!(
        after_content, original_content,
        "File should not be modified when --no-edit is used"
    );
}

#[test]
fn test_set_attr_id_immutable() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"testid1234567890\", title=\"Test\", worktime_estimate=4, worktime_units=\"hours\", status=\"new\")",
    )
    .unwrap();

    ragtag()
        .args([
            "task",
            "set-attr",
            "testid1234567890",
            "id",
            "newid",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("immutable"));
}

// === Environment Variable Tests ===

#[test]
fn test_ragtag_path_env_var() {
    // When RAGTAG_PATH is set and --path is not provided, the env var path should be used.
    let path = format!("{}/simple_tags.txt", fixtures_dir());
    ragtag()
        .env("RAGTAG_PATH", &path)
        .args(["summary"])
        .assert()
        .success()
        .stdout(predicate::str::contains("tag"));
}

#[test]
fn test_ragtag_path_cli_overrides_env() {
    // When both RAGTAG_PATH and --path are provided, --path should take precedence.
    let fixtures = fixtures_dir();
    let correct_path = format!("{}/simple_tags.txt", fixtures);

    // Set env var to a path that would produce different results (the whole fixtures dir).
    ragtag()
        .env("RAGTAG_PATH", &fixtures)
        .args(["query", "tag", "--path", &correct_path, "--count"])
        .assert()
        .success()
        .stdout(predicate::str::contains("3")); // 3 @tag entries in simple_tags.txt
}

#[test]
fn test_ragtag_path_env_var_task_list() {
    // RAGTAG_PATH should work with task subcommands too.
    let path = format!("{}/tasks.md", fixtures_dir());
    ragtag()
        .env("RAGTAG_PATH", &path)
        .args(["--no-color", "task", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Design API"));
}

#[test]
fn test_ragtag_config_env_var() {
    // When RAGTAG_CONFIG is set, it should load the specified config file.
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("custom.ragtag.yaml");
    fs::write(
        &config_path,
        "skip_hidden: false\noutput:\n  color: \"never\"\n",
    )
    .unwrap();

    let fixtures = fixtures_dir();
    ragtag()
        .env("RAGTAG_CONFIG", config_path.to_str().unwrap())
        .args(["summary", "--path", &fixtures])
        .assert()
        .success();
}

#[test]
fn test_ragtag_config_cli_overrides_env() {
    // When both RAGTAG_CONFIG and --config are provided, --config should take precedence.
    let dir = tempfile::tempdir().unwrap();

    // Create a valid config that --config points to.
    let cli_config = dir.path().join("cli.ragtag.yaml");
    fs::write(&cli_config, "output:\n  color: \"never\"\n").unwrap();

    // Create a config at the env var path that would cause a validation error (bad data).
    let env_config = dir.path().join("env.ragtag.yaml");
    fs::write(&env_config, "output:\n  color: \"never\"\n").unwrap();

    let fixtures = fixtures_dir();
    ragtag()
        .env("RAGTAG_CONFIG", env_config.to_str().unwrap())
        .args([
            "--config",
            cli_config.to_str().unwrap(),
            "summary",
            "--path",
            &fixtures,
        ])
        .assert()
        .success();
}

#[test]
fn test_ragtag_config_env_var_missing_file() {
    // When RAGTAG_CONFIG points to a nonexistent file, it should error.
    ragtag()
        .env("RAGTAG_CONFIG", "/nonexistent/path/.ragtag.yaml")
        .args(["summary"])
        .assert()
        .failure();
}

// === Config Command ===

#[test]
fn test_config_help_shown() {
    ragtag()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("config"));
}

#[test]
fn test_config_get_max_file_size() {
    ragtag()
        .args(["config", "get", "max_file_size"])
        .assert()
        .success()
        .stdout(predicate::str::contains("10485760"));
}

#[test]
fn test_config_get_respect_gitignore() {
    ragtag()
        .args(["config", "get", "respect_gitignore"])
        .assert()
        .success()
        .stdout(predicate::str::contains("true"));
}

#[test]
fn test_config_get_output_color() {
    ragtag()
        .args(["config", "get", "output.color"])
        .assert()
        .success()
        .stdout(predicate::str::contains("auto"));
}

#[test]
fn test_config_get_tasks_tag_name() {
    ragtag()
        .args(["config", "get", "tasks.tag_name"])
        .assert()
        .success()
        .stdout(predicate::str::contains("task"));
}

#[test]
fn test_config_get_tasks_status_keywords_done() {
    ragtag()
        .args(["config", "get", "tasks.status_keywords.done"])
        .assert()
        .success()
        .stdout(predicate::str::contains("done"))
        .stdout(predicate::str::contains("finished"))
        .stdout(predicate::str::contains("complete"))
        .stdout(predicate::str::contains("completed"));
}

#[test]
fn test_config_get_unknown_key() {
    ragtag()
        .args(["config", "get", "nonexistent_field"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown config key"));
}

#[test]
fn test_config_get_with_custom_config() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join(".ragtag.yaml");
    fs::write(
        &config_path,
        "tasks:\n  tag_name: \"custom_tag\"\n  default_owner: \"alice\"\n",
    )
    .unwrap();
    ragtag()
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "config",
            "get",
            "tasks.tag_name",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("custom_tag"));
}

// === Special characters in set-attr values ===

#[test]
fn test_set_attr_value_with_comma_no_edit() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"testid1234567890\", title=\"Test\", description=\"old\", worktime_estimate=4, worktime_units=\"hours\", status=\"new\")",
    ).unwrap();

    ragtag()
        .args([
            "task",
            "set-attr",
            "testid1234567890",
            "description",
            "First, second",
            "--no-edit",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("First, second"));
}

#[test]
fn test_set_attr_value_with_parens_no_edit() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"testid1234567890\", title=\"old\", worktime_estimate=4, worktime_units=\"hours\", status=\"new\")",
    ).unwrap();

    ragtag()
        .args([
            "task",
            "set-attr",
            "testid1234567890",
            "title",
            "Fix bug (urgent)",
            "--no-edit",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Fix bug (urgent)"));
}

#[test]
fn test_set_attr_value_with_comma_file_edit() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"testid1234567890\", title=\"Test\", description=\"old\", worktime_estimate=4, worktime_units=\"hours\", status=\"new\")",
    ).unwrap();

    ragtag()
        .args([
            "task",
            "set-attr",
            "testid1234567890",
            "description",
            "First, second",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success();

    let content = fs::read_to_string(&file).unwrap();
    assert!(content.contains("First, second"));
}

#[test]
fn test_set_attr_value_with_parens_file_edit() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"testid1234567890\", title=\"old\", worktime_estimate=4, worktime_units=\"hours\", status=\"new\")",
    ).unwrap();

    ragtag()
        .args([
            "task",
            "set-attr",
            "testid1234567890",
            "title",
            "Fix bug (urgent)",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success();

    let content = fs::read_to_string(&file).unwrap();
    assert!(content.contains("Fix bug (urgent)"));
}

// === Filter Mode Tests ===

#[test]
fn test_task_list_filter_mode_and_default() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"aaa1234567890ab\", title=\"Alice Active\", status=\"active\", owner=\"alice\", worktime_estimate=1, worktime_units=\"hours\")\n\
         @task(id=\"bbb1234567890ab\", title=\"Bob Active\", status=\"active\", owner=\"bob\", worktime_estimate=2, worktime_units=\"hours\")\n\
         @task(id=\"ccc1234567890ab\", title=\"Alice Blocked\", status=\"blocked\", owner=\"alice\", worktime_estimate=3, worktime_units=\"hours\")",
    ).unwrap();

    // AND expression: must match both status=active AND owner=alice
    ragtag()
        .args([
            "--no-color",
            "task",
            "list",
            "--filter",
            "status=active AND owner=alice",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Alice Active"))
        .stdout(predicate::str::contains("Bob Active").not())
        .stdout(predicate::str::contains("Alice Blocked").not());
}

#[test]
fn test_task_list_filter_mode_or() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"aaa1234567890ab\", title=\"Alice Active\", status=\"active\", owner=\"alice\", worktime_estimate=1, worktime_units=\"hours\")\n\
         @task(id=\"bbb1234567890ab\", title=\"Bob Blocked\", status=\"blocked\", owner=\"bob\", worktime_estimate=2, worktime_units=\"hours\")\n\
         @task(id=\"ccc1234567890ab\", title=\"Alice Blocked\", status=\"blocked\", owner=\"alice\", worktime_estimate=3, worktime_units=\"hours\")",
    ).unwrap();

    // OR expression: match status=active OR owner=alice
    ragtag()
        .args([
            "--no-color",
            "task",
            "list",
            "--filter",
            "status=active OR owner=alice",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Alice Active"))
        .stdout(predicate::str::contains("Alice Blocked"))
        .stdout(predicate::str::contains("Bob Blocked").not());
}

#[test]
fn test_task_list_filter_mode_and_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"aaa1234567890ab\", title=\"Alice Active\", status=\"active\", owner=\"alice\", worktime_estimate=1, worktime_units=\"hours\")\n\
         @task(id=\"bbb1234567890ab\", title=\"Bob Active\", status=\"active\", owner=\"bob\", worktime_estimate=2, worktime_units=\"hours\")",
    ).unwrap();

    // Parenthesized expression: (status=active OR status=blocked) AND owner=alice
    ragtag()
        .args([
            "--no-color",
            "task",
            "list",
            "--filter",
            "(status=active OR status=blocked) AND owner=alice",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Alice Active"))
        .stdout(predicate::str::contains("Bob Active").not());
}

#[test]
fn test_task_summary_filter_applied() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"aaa1234567890ab\", title=\"Active Task\", status=\"active\", worktime_estimate=1, worktime_units=\"hours\")\n\
         @task(id=\"bbb1234567890ab\", title=\"Blocked Task\", status=\"blocked\", worktime_estimate=2, worktime_units=\"hours\")",
    ).unwrap();

    // Filter should be applied in summary (this was the bug)
    ragtag()
        .args([
            "--no-color",
            "task",
            "summary",
            "--format",
            "table",
            "--filter",
            "status=active",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Active Task"))
        .stdout(predicate::str::contains("Blocked Task").not());
}

#[test]
fn test_task_summary_filter_mode_or() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"aaa1234567890ab\", title=\"Alice Active\", status=\"active\", owner=\"alice\", worktime_estimate=1, worktime_units=\"hours\")\n\
         @task(id=\"bbb1234567890ab\", title=\"Bob Blocked\", status=\"blocked\", owner=\"bob\", worktime_estimate=2, worktime_units=\"hours\")\n\
         @task(id=\"ccc1234567890ab\", title=\"Alice Blocked\", status=\"blocked\", owner=\"alice\", worktime_estimate=3, worktime_units=\"hours\")",
    ).unwrap();

    // OR expression in summary: match status=active OR owner=alice
    ragtag()
        .args([
            "--no-color",
            "task",
            "summary",
            "--format",
            "table",
            "--filter",
            "status=active OR owner=alice",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Alice Active"))
        .stdout(predicate::str::contains("Alice Blocked"))
        .stdout(predicate::str::contains("Bob Blocked").not());
}

// === Task List --format raw ===

#[test]
fn test_tasks_list_format_raw() {
    let path = format!("{}/tasks.md", fixtures_dir());
    let output = ragtag()
        .args([
            "--no-color",
            "task",
            "list",
            "--format",
            "raw",
            "--path",
            &path,
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // Verify key=value format
    assert!(output_str.contains("id=a1b2c3d4e5f67890"));
    assert!(output_str.contains("title=Design API"));
    assert!(output_str.contains("owner=alice"));
    assert!(output_str.contains("status=active"));
    assert!(output_str.contains("priority=1"));
    // Verify multiple tasks are separated by blank lines
    let blocks: Vec<&str> = output_str.split("\n\n").collect();
    assert!(
        blocks.len() >= 2,
        "Expected multiple task blocks separated by blank lines, got {}",
        blocks.len()
    );
}

#[test]
fn test_tasks_list_format_raw_with_filter() {
    let path = format!("{}/tasks.md", fixtures_dir());
    let output = ragtag()
        .args([
            "--no-color",
            "task",
            "list",
            "--format",
            "raw",
            "--path",
            &path,
            "--filter",
            "status=active",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    // Should only contain the active task
    assert!(output_str.contains("id=a1b2c3d4e5f67890"));
    assert!(output_str.contains("status=active"));
    // Should NOT contain blocked task
    assert!(!output_str.contains("id=fedcba0987654321"));
}

// === Task Complete ===

#[test]
fn test_task_complete_marks_done() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"compltest1234567a\", title=\"Finish me\", worktime_estimate=2, worktime_units=\"hours\", status=\"active\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "complete",
            "compltest1234567a",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Completed task"))
        .stdout(predicate::str::contains("compltest1234567a"))
        .stdout(predicate::str::contains("done"));

    let content = fs::read_to_string(&file).unwrap();
    assert!(
        content.contains("status=\"done\""),
        "Expected status=\"done\" in file after complete, got:\n{content}"
    );
}

#[test]
fn test_task_complete_auto_updates_time_last_updated() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    // Task does NOT have time_last_updated — it should be added.
    fs::write(
        &file,
        "@task(id=\"compltest2345678b\", title=\"Add timestamp\", worktime_estimate=1, worktime_units=\"hours\", status=\"new\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "complete",
            "compltest2345678b",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success();

    let content = fs::read_to_string(&file).unwrap();
    assert!(
        content.contains("time_last_updated=\"20"),
        "Expected time_last_updated to be added, got:\n{content}"
    );
}

#[test]
fn test_task_complete_updates_existing_time_last_updated() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    // Task already has time_last_updated — it should be updated.
    fs::write(
        &file,
        "@task(id=\"compltest3456789c\", title=\"Update TS\", worktime_estimate=1, worktime_units=\"hours\", status=\"active\", time_last_updated=\"2025-01-01T00:00:00Z\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "complete",
            "compltest3456789c",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success();

    let content = fs::read_to_string(&file).unwrap();
    assert!(
        !content.contains("2025-01-01"),
        "Old timestamp should have been replaced, got:\n{content}"
    );
    assert!(
        content.contains("time_last_updated=\"20"),
        "Expected updated time_last_updated, got:\n{content}"
    );
}

#[test]
fn test_task_complete_no_edit() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    let original =
        "@task(id=\"compltest4567890d\", title=\"No edit\", worktime_estimate=1, worktime_units=\"hours\", status=\"new\")";
    fs::write(&file, original).unwrap();

    let output = ragtag()
        .args([
            "--no-color",
            "task",
            "complete",
            "compltest4567890d",
            "--no-edit",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    assert!(
        output_str.contains("status=\"done\""),
        "Expected status=\"done\" in --no-edit output: {output_str}"
    );
    assert!(
        output_str.contains("time_last_updated=\"20"),
        "Expected time_last_updated in --no-edit output: {output_str}"
    );
    // File must be unchanged.
    let file_content = fs::read_to_string(&file).unwrap();
    assert_eq!(
        file_content, original,
        "File should be unchanged with --no-edit"
    );
}

#[test]
fn test_task_complete_prefix_match() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"prefixtest567890ef\", title=\"Prefix test\", worktime_estimate=1, worktime_units=\"hours\", status=\"active\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "complete",
            "prefixtest",  // prefix only
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success();

    let content = fs::read_to_string(&file).unwrap();
    assert!(
        content.contains("status=\"done\""),
        "Prefix match should have completed the task: {content}"
    );
}

#[test]
fn test_task_complete_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"existingtask1234\", title=\"Real task\", worktime_estimate=1, worktime_units=\"hours\", status=\"new\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "complete",
            "doesnotexist9999",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("task not found").or(predicate::str::contains("not found")));
}

#[test]
fn test_tasks_help_includes_complete() {
    ragtag()
        .args(["task", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("complete"));
}

#[test]
fn test_tasks_help_includes_status_change_commands() {
    ragtag()
        .args(["task", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("activate"))
        .stdout(predicate::str::contains("deactivate"))
        .stdout(predicate::str::contains("block"))
        .stdout(predicate::str::contains("abandon"));
}

// =========================================================================
// task activate
// =========================================================================

#[test]
fn test_task_activate_sets_active_status() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"acttest1234567aa\", title=\"Activate me\", worktime_estimate=1, worktime_units=\"hours\", status=\"new\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "activate",
            "acttest1234567aa",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Activated task"))
        .stdout(predicate::str::contains("acttest1234567aa"))
        .stdout(predicate::str::contains("active"));

    let content = fs::read_to_string(&file).unwrap();
    assert!(
        content.contains("status=\"active\""),
        "Expected status=\"active\" in file after activate, got:\n{content}"
    );
}

#[test]
fn test_task_activate_auto_updates_time_last_updated() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"acttest2345678bb\", title=\"Add TS\", worktime_estimate=1, worktime_units=\"hours\", status=\"new\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "activate",
            "acttest2345678bb",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success();

    let content = fs::read_to_string(&file).unwrap();
    assert!(
        content.contains("time_last_updated=\"20"),
        "Expected time_last_updated to be added, got:\n{content}"
    );
}

#[test]
fn test_task_activate_no_edit() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    let original = "@task(id=\"acttest3456789cc\", title=\"No edit\", worktime_estimate=1, worktime_units=\"hours\", status=\"new\")";
    fs::write(&file, original).unwrap();

    let output = ragtag()
        .args([
            "--no-color",
            "task",
            "activate",
            "acttest3456789cc",
            "--no-edit",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    assert!(
        output_str.contains("status=\"active\""),
        "Expected status=\"active\" in --no-edit output: {output_str}"
    );
    assert!(
        output_str.contains("time_last_updated=\"20"),
        "Expected time_last_updated in --no-edit output: {output_str}"
    );
    let file_content = fs::read_to_string(&file).unwrap();
    assert_eq!(file_content, original, "File should be unchanged with --no-edit");
}

#[test]
fn test_task_activate_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"realacttask12345\", title=\"Real\", worktime_estimate=1, worktime_units=\"hours\", status=\"new\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "activate",
            "doesnotexist9999",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("task not found").or(predicate::str::contains("not found")));
}

// =========================================================================
// task deactivate
// =========================================================================

#[test]
fn test_task_deactivate_sets_inactive_status() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"deacttest1234aaa\", title=\"Deactivate me\", worktime_estimate=1, worktime_units=\"hours\", status=\"active\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "deactivate",
            "deacttest1234aaa",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Deactivated task"))
        .stdout(predicate::str::contains("deacttest1234aaa"))
        .stdout(predicate::str::contains("inactive"));

    let content = fs::read_to_string(&file).unwrap();
    assert!(
        content.contains("status=\"inactive\""),
        "Expected status=\"inactive\" in file after deactivate, got:\n{content}"
    );
}

#[test]
fn test_task_deactivate_auto_updates_time_last_updated() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"deacttest2345bbb\", title=\"Add TS\", worktime_estimate=1, worktime_units=\"hours\", status=\"active\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "deactivate",
            "deacttest2345bbb",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success();

    let content = fs::read_to_string(&file).unwrap();
    assert!(
        content.contains("time_last_updated=\"20"),
        "Expected time_last_updated to be added, got:\n{content}"
    );
}

#[test]
fn test_task_deactivate_no_edit() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    let original = "@task(id=\"deacttest3456ccc\", title=\"No edit\", worktime_estimate=1, worktime_units=\"hours\", status=\"active\")";
    fs::write(&file, original).unwrap();

    let output = ragtag()
        .args([
            "--no-color",
            "task",
            "deactivate",
            "deacttest3456ccc",
            "--no-edit",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    assert!(
        output_str.contains("status=\"inactive\""),
        "Expected status=\"inactive\" in --no-edit output: {output_str}"
    );
    let file_content = fs::read_to_string(&file).unwrap();
    assert_eq!(file_content, original, "File should be unchanged with --no-edit");
}

#[test]
fn test_task_deactivate_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"realdeacttask123\", title=\"Real\", worktime_estimate=1, worktime_units=\"hours\", status=\"active\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "deactivate",
            "doesnotexist9999",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("task not found").or(predicate::str::contains("not found")));
}

// =========================================================================
// task block
// =========================================================================

#[test]
fn test_task_block_command_sets_blocked_status() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"blkcmd12345678aa\", title=\"Block me\", worktime_estimate=1, worktime_units=\"hours\", status=\"active\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "block",
            "blkcmd12345678aa",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Blocked task"))
        .stdout(predicate::str::contains("blkcmd12345678aa"))
        .stdout(predicate::str::contains("blocked"));

    let content = fs::read_to_string(&file).unwrap();
    assert!(
        content.contains("status=\"blocked\""),
        "Expected status=\"blocked\" in file after block, got:\n{content}"
    );
}

#[test]
fn test_task_block_auto_updates_time_last_updated() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"blkcmd23456789bb\", title=\"Add TS\", worktime_estimate=1, worktime_units=\"hours\", status=\"active\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "block",
            "blkcmd23456789bb",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success();

    let content = fs::read_to_string(&file).unwrap();
    assert!(
        content.contains("time_last_updated=\"20"),
        "Expected time_last_updated to be added, got:\n{content}"
    );
}

#[test]
fn test_task_block_no_edit() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    let original = "@task(id=\"blkcmd3456789ccc\", title=\"No edit\", worktime_estimate=1, worktime_units=\"hours\", status=\"active\")";
    fs::write(&file, original).unwrap();

    let output = ragtag()
        .args([
            "--no-color",
            "task",
            "block",
            "blkcmd3456789ccc",
            "--no-edit",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    assert!(
        output_str.contains("status=\"blocked\""),
        "Expected status=\"blocked\" in --no-edit output: {output_str}"
    );
    let file_content = fs::read_to_string(&file).unwrap();
    assert_eq!(file_content, original, "File should be unchanged with --no-edit");
}

#[test]
fn test_task_block_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"realblkcmd123456\", title=\"Real\", worktime_estimate=1, worktime_units=\"hours\", status=\"active\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "block",
            "doesnotexist9999",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("task not found").or(predicate::str::contains("not found")));
}

// =========================================================================
// task abandon
// =========================================================================

#[test]
fn test_task_abandon_sets_abandoned_status() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"abdtest1234567aa\", title=\"Abandon me\", worktime_estimate=1, worktime_units=\"hours\", status=\"active\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "abandon",
            "abdtest1234567aa",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Abandoned task"))
        .stdout(predicate::str::contains("abdtest1234567aa"))
        .stdout(predicate::str::contains("abandoned"));

    let content = fs::read_to_string(&file).unwrap();
    assert!(
        content.contains("status=\"abandoned\""),
        "Expected status=\"abandoned\" in file after abandon, got:\n{content}"
    );
}

#[test]
fn test_task_abandon_auto_updates_time_last_updated() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"abdtest2345678bb\", title=\"Add TS\", worktime_estimate=1, worktime_units=\"hours\", status=\"active\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "abandon",
            "abdtest2345678bb",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success();

    let content = fs::read_to_string(&file).unwrap();
    assert!(
        content.contains("time_last_updated=\"20"),
        "Expected time_last_updated to be added, got:\n{content}"
    );
}

#[test]
fn test_task_abandon_no_edit() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    let original = "@task(id=\"abdtest3456789cc\", title=\"No edit\", worktime_estimate=1, worktime_units=\"hours\", status=\"active\")";
    fs::write(&file, original).unwrap();

    let output = ragtag()
        .args([
            "--no-color",
            "task",
            "abandon",
            "abdtest3456789cc",
            "--no-edit",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    assert!(
        output_str.contains("status=\"abandoned\""),
        "Expected status=\"abandoned\" in --no-edit output: {output_str}"
    );
    let file_content = fs::read_to_string(&file).unwrap();
    assert_eq!(file_content, original, "File should be unchanged with --no-edit");
}

#[test]
fn test_task_abandon_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"realabdtask12345\", title=\"Real\", worktime_estimate=1, worktime_units=\"hours\", status=\"active\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "abandon",
            "doesnotexist9999",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("task not found").or(predicate::str::contains("not found")));
}

#[test]
fn test_task_abandon_prefix_match() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"abdprefix567890ef\", title=\"Prefix abandon\", worktime_estimate=1, worktime_units=\"hours\", status=\"active\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "abandon",
            "abdprefix",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success();

    let content = fs::read_to_string(&file).unwrap();
    assert!(
        content.contains("status=\"abandoned\""),
        "Prefix match should have abandoned the task: {content}"
    );
}

#[test]
fn test_query_all_tags() {
    ragtag()
        .args(["--no-color", "query", "--path", &fixtures_dir()])
        .assert()
        .success()
        // Should contain tags from multiple files/types
        .stdout(predicate::str::contains("@task"))
        .stdout(predicate::str::contains("@note"))
        .stdout(predicate::str::contains("@todo"));
}

#[test]
fn test_query_specific_tag_still_works() {
    ragtag()
        .args(["--no-color", "query", "task", "--path", &fixtures_dir()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Design API"))
        .stdout(predicate::str::contains("Write tests"))
        // Should NOT contain non-task tags
        .stdout(predicate::str::contains("@note").not())
        .stdout(predicate::str::contains("@todo").not());
}

// === Interactive mode — piped stdin with validation ===
//
// These tests exercise `task create` without a `--title` flag so that the
// interactive (`run_interactive`) code path is triggered.  stdin is piped, so
// `PromptSession` uses the plain BufRead path and prompts go to stderr.
// Each test verifies that:
//   - Invalid input causes an error message on stderr and re-prompting.
//   - The subsequent valid value (or blank to skip) is used correctly.

/// Pipes lines as if the user typed them one by one.
// ---- Title re-prompt -------------------------------------------------------

#[test]
fn test_interactive_title_blank_reprompt() {
    // Blank line first → error on stderr → second line accepted as title.
    let assert = ragtag()
        .args(["task", "create"])
        .write_stdin("\nMy Task\n\n\n\n\n\n\n\n")
        .assert()
        .success();

    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let err = String::from_utf8(assert.get_output().stderr.clone()).unwrap();

    assert!(out.contains("title=\"My Task\""), "stdout: {out}");
    assert!(err.contains("Title is required."), "stderr: {err}");
}

// ---- Priority validation ---------------------------------------------------

#[test]
fn test_interactive_invalid_priority_reprompt() {
    // Pipe: title, blank description, blank owner, blank status,
    //       invalid priority "m", then valid priority "5", then blanks for the rest.
    let stdin = "My Task\n\n\n\nm\n5\n\n\n\n";
    let assert = ragtag()
        .args(["task", "create"])
        .write_stdin(stdin)
        .assert()
        .success();

    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let err = String::from_utf8(assert.get_output().stderr.clone()).unwrap();

    assert!(out.contains("title=\"My Task\""), "stdout: {out}");
    assert!(out.contains("priority=5"), "stdout should have priority=5: {out}");
    assert!(
        err.contains("non-negative whole number"),
        "stderr should have priority error: {err}"
    );
}

#[test]
fn test_interactive_negative_priority_reprompt() {
    // "-1" is rejected (not a u32), then "3" is accepted.
    let stdin = "Neg Priority Task\n\n\n\n-1\n3\n\n\n\n";
    let assert = ragtag()
        .args(["task", "create"])
        .write_stdin(stdin)
        .assert()
        .success();

    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let err = String::from_utf8(assert.get_output().stderr.clone()).unwrap();

    assert!(out.contains("priority=3"), "stdout: {out}");
    assert!(err.contains("non-negative whole number"), "stderr: {err}");
}

// ---- Worktime estimate validation ------------------------------------------

#[test]
fn test_interactive_invalid_worktime_estimate_reprompt() {
    // "abc" rejected; then "2.5" accepted.
    let stdin = "Estimate Task\n\n\n\n\nabc\n2.5\n\n\n\n";
    let assert = ragtag()
        .args(["task", "create"])
        .write_stdin(stdin)
        .assert()
        .success();

    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let err = String::from_utf8(assert.get_output().stderr.clone()).unwrap();

    assert!(out.contains("worktime_estimate=2.5"), "stdout: {out}");
    assert!(
        err.contains("non-negative number"),
        "stderr should have worktime estimate error: {err}"
    );
}

#[test]
fn test_interactive_negative_worktime_estimate_reprompt() {
    // "-3" rejected; then "1" accepted.
    let stdin = "Neg Estimate Task\n\n\n\n\n-3\n1\n\n\n\n";
    let assert = ragtag()
        .args(["task", "create"])
        .write_stdin(stdin)
        .assert()
        .success();

    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let err = String::from_utf8(assert.get_output().stderr.clone()).unwrap();

    assert!(out.contains("worktime_estimate=1"), "stdout: {out}");
    assert!(err.contains("non-negative number"), "stderr: {err}");
}

// ---- Worktime spent validation ---------------------------------------------

#[test]
fn test_interactive_invalid_worktime_spent_reprompt() {
    // "oops" rejected; then "0.5" accepted.
    let stdin = "Spent Task\n\n\n\n\n\noops\n0.5\n\n\n";
    let assert = ragtag()
        .args(["task", "create"])
        .write_stdin(stdin)
        .assert()
        .success();

    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let err = String::from_utf8(assert.get_output().stderr.clone()).unwrap();

    assert!(out.contains("worktime_spent=0.5"), "stdout: {out}");
    assert!(
        err.contains("non-negative number"),
        "stderr should have worktime spent error: {err}"
    );
}

// ---- Status validation -----------------------------------------------------

#[test]
fn test_interactive_invalid_status_reprompt() {
    // "banana" rejected; then "active" accepted.
    let stdin = "Status Task\n\n\nbanana\nactive\n\n\n\n\n\n";
    let assert = ragtag()
        .args(["task", "create"])
        .write_stdin(stdin)
        .assert()
        .success();

    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let err = String::from_utf8(assert.get_output().stderr.clone()).unwrap();

    assert!(out.contains("status=\"active\""), "stdout: {out}");
    assert!(
        err.contains("Invalid status"),
        "stderr should have status error: {err}"
    );
    assert!(
        err.contains("allowed values:"),
        "stderr should list allowed values: {err}"
    );
}

// ---- Worktime units validation ---------------------------------------------

#[test]
fn test_interactive_invalid_worktime_units_reprompt() {
    // "fortnights" rejected; then "days" accepted.
    let stdin = "Units Task\n\n\n\n\n\n\nfortnights\ndays\n\n";
    let assert = ragtag()
        .args(["task", "create"])
        .write_stdin(stdin)
        .assert()
        .success();

    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let err = String::from_utf8(assert.get_output().stderr.clone()).unwrap();

    assert!(out.contains("worktime_units=\"days\""), "stdout: {out}");
    assert!(
        err.contains("Invalid worktime units"),
        "stderr should have units error: {err}"
    );
    assert!(err.contains("hours"), "stderr should list allowed units: {err}");
}

// ---- Skip all optional fields (blank) after title --------------------------

#[test]
fn test_interactive_title_only_all_blanks() {
    // Provide title, then blank for every optional field.
    let stdin = "Blank Fields Task\n\n\n\n\n\n\n\n\n";
    let assert = ragtag()
        .args(["task", "create"])
        .write_stdin(stdin)
        .assert()
        .success();

    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.contains("title=\"Blank Fields Task\""), "stdout: {out}");
    assert!(out.contains("worktime_spent=0"), "stdout: {out}");
    assert!(out.contains("worktime_units=\"hours\""), "stdout: {out}");
}

// ---- Valid input for every field interactively -----------------------------

#[test]
fn test_interactive_all_fields_valid() {
    // Supply all fields via piped stdin in order:
    // title, description, owner, status, priority, wt_estimate, wt_spent, wt_units, pid
    let stdin = "Interactive Task\nDoes things\nalice\nactive\n2\n4.0\n1.0\nhours\nparent123\n";
    let assert = ragtag()
        .args(["task", "create"])
        .write_stdin(stdin)
        .assert()
        .success();

    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(out.contains("title=\"Interactive Task\""), "stdout: {out}");
    assert!(out.contains("description=\"Does things\""), "stdout: {out}");
    assert!(out.contains("owner=\"alice\""), "stdout: {out}");
    assert!(out.contains("status=\"active\""), "stdout: {out}");
    assert!(out.contains("priority=2"), "stdout: {out}");
    assert!(out.contains("worktime_estimate=4"), "stdout: {out}");
    assert!(out.contains("worktime_spent=1"), "stdout: {out}");
    assert!(out.contains("worktime_units=\"hours\""), "stdout: {out}");
    assert!(out.contains("pid=\"parent123\""), "stdout: {out}");
}

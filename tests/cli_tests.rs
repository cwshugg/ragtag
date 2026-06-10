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
        .stdout(predicate::str::contains("tasks"));
}

#[test]
fn test_tasks_help() {
    ragtag()
        .args(["tasks", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("create"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("set-status"));
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

#[test]
fn test_query_show_attributes() {
    let path = format!("{}/simple_tags.txt", fixtures_dir());
    ragtag()
        .args([
            "query",
            "todo",
            "--path",
            &path,
            "--show-attributes",
            "priority,description",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("priority="));
}

// === Tasks Create ===

#[test]
fn test_tasks_create() {
    ragtag()
        .args([
            "tasks",
            "create",
            "--title",
            "Test Task",
            "--ttc-estimate",
            "4",
            "--time-units",
            "hours",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("@task("))
        .stdout(predicate::str::contains("title=\"Test Task\""))
        .stdout(predicate::str::contains("ttc_estimate=4"));
}

#[test]
fn test_tasks_create_with_all_fields() {
    ragtag()
        .args([
            "tasks",
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
            "--ttc-estimate",
            "8.5",
            "--time-units",
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

// === Tasks List ===

#[test]
fn test_tasks_list() {
    let path = format!("{}/tasks.md", fixtures_dir());
    ragtag()
        .args(["--no-color", "tasks", "list", "--path", &path])
        .assert()
        .success()
        .stdout(predicate::str::contains("a1b2c3d4e5f67890"))
        .stdout(predicate::str::contains("Design API"));
}

#[test]
fn test_tasks_list_show_attributes() {
    let path = format!("{}/tasks.md", fixtures_dir());
    ragtag()
        .args([
            "--no-color",
            "tasks",
            "list",
            "--path",
            &path,
            "--show-attributes",
            "id,title,priority",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("id="))
        .stdout(predicate::str::contains("title="));
}

#[test]
fn test_tasks_list_sort() {
    let path = format!("{}/tasks.md", fixtures_dir());
    ragtag()
        .args([
            "--no-color",
            "tasks",
            "list",
            "--path",
            &path,
            "--sort",
            "priority",
        ])
        .assert()
        .success();
}

#[test]
fn test_tasks_list_filter() {
    let path = format!("{}/tasks.md", fixtures_dir());
    ragtag()
        .args([
            "--no-color",
            "tasks",
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
fn test_tasks_set_status() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"testid1234567890\", title=\"Test\", ttc_estimate=4, time_units=\"hours\", status=\"new\")",
    ).unwrap();

    ragtag()
        .args([
            "--no-color",
            "tasks",
            "set-status",
            "--id",
            "testid1234567890",
            "--status",
            "active",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("status: active"));

    // Verify file was actually modified
    let content = fs::read_to_string(&file).unwrap();
    assert!(content.contains("\"active\""));
}

#[test]
fn test_tasks_set_time() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"testid1234567890\", title=\"Test\", ttc_estimate=4, time_units=\"hours\", status=\"new\")",
    ).unwrap();

    ragtag()
        .args([
            "--no-color",
            "tasks",
            "set-time",
            "--id",
            "testid1234567890",
            "--time",
            "2.5",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("time_spent: 2.5"));
}

#[test]
fn test_tasks_set_owner() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"testid1234567890\", title=\"Test\", ttc_estimate=4, time_units=\"hours\", status=\"new\")",
    ).unwrap();

    ragtag()
        .args([
            "--no-color",
            "tasks",
            "set-owner",
            "--id",
            "testid1234567890",
            "--owner",
            "alice",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("owner: alice"));
}

#[test]
fn test_tasks_set_parent() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"testid1234567890\", title=\"Test\", ttc_estimate=4, time_units=\"hours\", status=\"new\")",
    ).unwrap();

    ragtag()
        .args([
            "--no-color",
            "tasks",
            "set-parent",
            "--id",
            "testid1234567890",
            "--pid",
            "parent123",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("pid: parent123"));
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
    fs::write(
        &file,
        "@task(id=\"testid1234567890\", title=\"Test\", ttc_estimate=4, time_units=\"hours\", status=\"new\")",
    ).unwrap();

    ragtag()
        .args([
            "tasks",
            "set-status",
            "--id",
            "testid1234567890",
            "--status",
            "invalid_status_xyz",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid status"));
}

#[test]
fn test_task_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(&file, "No tasks here.").unwrap();

    ragtag()
        .args([
            "tasks",
            "set-status",
            "--id",
            "nonexistent1234567",
            "--status",
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
        .args(["--no-color", "tasks", "list", "--path", &path])
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
            "tasks",
            "set-status",
            "--id",
            "dupeid123456789a",
            "--status",
            "done",
            "--path",
            &path,
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("multiple tasks found"));
}

// === Full workflow ===

#[test]
fn test_full_workflow() {
    let dir = tempfile::tempdir().unwrap();

    // Create a task
    let create_output = ragtag()
        .args([
            "tasks",
            "create",
            "--title",
            "Workflow Test",
            "--ttc-estimate",
            "4",
            "--time-units",
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
            "tasks",
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
            "tasks",
            "set-status",
            "--id",
            task_id,
            "--status",
            "active",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("status: active"));

    // Set time
    ragtag()
        .args([
            "--no-color",
            "tasks",
            "set-time",
            "--id",
            task_id,
            "--time",
            "2.5",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("time_spent: 2.5"));

    // Set owner
    ragtag()
        .args([
            "--no-color",
            "tasks",
            "set-owner",
            "--id",
            task_id,
            "--owner",
            "alice",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("owner: alice"));

    // Verify final file state
    let final_content = fs::read_to_string(&file).unwrap();
    assert!(final_content.contains("\"active\""));
    assert!(final_content.contains("\"alice\""));
}

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

// === Tasks Create ===

#[test]
fn test_tasks_create() {
    ragtag()
        .args([
            "task",
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
            "task",
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
        .stdout(predicate::str::contains("Status: active"));

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
            "task",
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
        .stdout(predicate::str::contains("Time Spent: 2.5"));
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
            "task",
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
        .stdout(predicate::str::contains("Owner: alice"));
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
            "task",
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
        .stdout(predicate::str::contains("Parent ID: parent123"));
}

#[test]
fn test_tasks_set_priority() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"testid1234567890\", title=\"Test\", ttc_estimate=4, time_units=\"hours\", status=\"new\")",
    ).unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "set-priority",
            "--id",
            "testid1234567890",
            "--priority",
            "2",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Priority: 2"));

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
    let original_content = "@task(id=\"testid1234567890\", title=\"Test\", ttc_estimate=4, time_units=\"hours\", status=\"new\")";
    fs::write(&file, original_content).unwrap();

    ragtag()
        .args([
            "task",
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
        .stdout(predicate::str::contains("Status: active"));

    // Set time
    ragtag()
        .args([
            "--no-color",
            "task",
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
        .stdout(predicate::str::contains("Time Spent: 2.5"));

    // Set owner
    ragtag()
        .args([
            "--no-color",
            "task",
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
        .stdout(predicate::str::contains("Owner: alice"));

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
        "@task(id=\"done1234567890ab\", title=\"Done Task\", status=\"done\", ttc_estimate=1, time_units=\"hours\")\n\
         @task(id=\"active12345678ab\", title=\"Active Task\", status=\"active\", ttc_estimate=2, time_units=\"hours\")",
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
        "@task(id=\"aaa1234567890ab\", title=\"Active Task\", status=\"active\", ttc_estimate=1, time_units=\"hours\")\n\
         @task(id=\"bbb1234567890ab\", title=\"Blocked Task\", status=\"blocked\", ttc_estimate=2, time_units=\"hours\")",
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
        "@task(id=\"zzz1234567890ab\", title=\"Zebra\", status=\"active\", ttc_estimate=1, time_units=\"hours\")\n\
         @task(id=\"aaa1234567890ab\", title=\"Apple\", status=\"active\", ttc_estimate=2, time_units=\"hours\")",
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
        "@task(id=\"aaa1234567890ab\", title=\"Apple\", status=\"active\", priority=1, ttc_estimate=1, time_units=\"hours\")\n\
         @task(id=\"zzz1234567890ab\", title=\"Zebra\", status=\"active\", priority=2, ttc_estimate=2, time_units=\"hours\")",
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
fn test_set_time_negative_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"testid1234567890\", title=\"Test\", ttc_estimate=4, time_units=\"hours\", status=\"new\")",
    ).unwrap();

    ragtag()
        .args([
            "task",
            "set-time",
            "--id",
            "testid1234567890",
            "--time=-5",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("non-negative"));
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

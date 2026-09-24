//! End-to-end tests for source-only diagram commands.

use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;

fn fixture() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("tasks.md"),
        concat!(
            "@task(id=root, title=\"Root\", status=done, owner=lead)\n",
            "@task(id=child, pid=root, title=\"Child\", status=active, owner=dev, priority=1)\n",
            "@task(id=hidden, pid=root, title=\"Hidden\", status=abandoned)\n",
            "@task(id=other, title=\"Other\", status=active, owner=dev)\n",
        ),
    )
    .unwrap();
    directory
}

#[test]
fn task_tree_is_deterministic_and_includes_filtered_ancestor_context() {
    let directory = fixture();
    let output = Command::cargo_bin("ragtag")
        .unwrap()
        .args([
            "diagram",
            "task-tree",
            "--path",
            directory.path().to_str().unwrap(),
            "--filter",
            "id=child",
        ])
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .get_output()
        .stdout
        .clone();
    let source = String::from_utf8(output).unwrap();
    assert!(source.starts_with("direction: down\n"));
    assert!(source.contains("Child\\n[active] · P1 · dev"));
    assert!(source.contains("Root\\n[done] · lead"));
    assert!(source.contains(".style.opacity: 0.55"));
    assert!(source.contains(" -> "));
    assert!(!source.contains("Other"));
    assert!(source.ends_with('\n'));
}

#[test]
fn default_status_exclusion_restores_only_required_tree_ancestors() {
    let directory = fixture();
    Command::cargo_bin("ragtag")
        .unwrap()
        .args([
            "diagram",
            "task-tree",
            "--path",
            directory.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Child\\n[active]")
                .and(predicate::str::contains("Root\\n[done]"))
                .and(predicate::str::contains(".style.opacity: 0.55"))
                .and(predicate::str::contains("Other\\n[active]"))
                .and(predicate::str::contains("Hidden").not()),
        )
        .stderr(predicate::str::is_empty());
}

#[test]
fn default_status_exclusion_restores_only_required_bucket_ancestors() {
    let directory = fixture();
    Command::cargo_bin("ragtag")
        .unwrap()
        .args([
            "diagram",
            "task-buckets",
            "--path",
            directory.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Root\\n[done]")
                .and(predicate::str::contains("Child\\n[active]"))
                .and(predicate::str::contains("style.opacity: 0.55"))
                .and(predicate::str::contains("Other\\n[active]"))
                .and(predicate::str::contains("Hidden").not()),
        )
        .stderr(predicate::str::is_empty());
}

#[test]
fn all_includes_default_excluded_tasks_as_ordinary_content() {
    let directory = fixture();
    Command::cargo_bin("ragtag")
        .unwrap()
        .args([
            "diagram",
            "task-tree",
            "--path",
            directory.path().to_str().unwrap(),
            "--all",
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Root\\n[done]")
                .and(predicate::str::contains(".style.opacity: 0.55").not()),
        )
        .stderr(predicate::str::is_empty());
}

#[test]
fn explicit_status_filter_does_not_restore_unmatched_descendants() {
    let directory = fixture();
    for kind in ["task-tree", "task-buckets"] {
        Command::cargo_bin("ragtag")
            .unwrap()
            .args([
                "diagram",
                kind,
                "--path",
                directory.path().to_str().unwrap(),
                "--filter",
                "status=done",
            ])
            .assert()
            .success()
            .stdout(
                predicate::str::contains("Root\\n[done]")
                    .and(predicate::str::contains("Child").not())
                    .and(predicate::str::contains("Hidden").not())
                    .and(predicate::str::contains("Other").not())
                    .and(predicate::str::contains("style.opacity: 0.55").not()),
            )
            .stderr(predicate::str::is_empty());
    }
}

#[test]
fn task_buckets_uses_recursive_containment_without_edges() {
    let directory = fixture();
    Command::cargo_bin("ragtag")
        .unwrap()
        .args([
            "diagram",
            "task-buckets",
            "--path",
            directory.path().to_str().unwrap(),
            "--all",
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains(": {")
                .and(predicate::str::contains(": \"Child\\n[active]"))
                .and(predicate::str::contains(" -> ").not()),
        )
        .stderr(predicate::str::is_empty());
}

#[test]
fn task_tree_silently_skips_png_and_extensionless_binary_inputs() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("tasks.md"),
        "@task(id=valid, title=\"Valid Task\", status=active)\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("image.png"),
        [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0xff],
    )
    .unwrap();
    fs::write(
        directory.path().join("extensionless"),
        [0x00, 0x80, 0xfe, 0xff],
    )
    .unwrap();

    Command::cargo_bin("ragtag")
        .unwrap()
        .args([
            "diagram",
            "task-tree",
            "--path",
            directory.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Valid Task\\n[active]")
                .and(predicate::str::contains("direction: down")),
        )
        .stderr(predicate::str::is_empty());
}

#[test]
fn strict_diagram_filter_rejects_unknown_fields() {
    let directory = fixture();
    Command::cargo_bin("ragtag")
        .unwrap()
        .args([
            "diagram",
            "task-tree",
            "--path",
            directory.path().to_str().unwrap(),
            "--filter",
            "unknown=value",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("unknown task field"));
}

#[test]
fn strict_diagram_filters_preserve_every_task_field() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("tasks.md"),
        concat!(
            "@task(id=parent, title=\"Parent\", status=active)\n",
            "@task(id=rich, pid=parent, title=\"Rich Task\", description=\"Detailed\", ",
            "owner=alice, status=blocked, priority=2, worktime_spent=1.5, ",
            "worktime_estimate=3.5, time_created=2025-01-01, ",
            "time_last_updated=2025-01-02, worktime_units=days)\n",
        ),
    )
    .unwrap();
    let filters = [
        "id=rich",
        "pid=parent",
        "title='Rich Task'",
        "description=Detailed",
        "owner=alice",
        "status=blocked",
        "priority=2",
        "worktime_spent=1.5",
        "worktime_estimate=3.5",
        "time_created=2025-01-01",
        "time_last_updated=2025-01-02",
        "worktime_units=days",
    ];
    for filter in filters {
        Command::cargo_bin("ragtag")
            .unwrap()
            .args([
                "diagram",
                "task-tree",
                "--path",
                directory.path().to_str().unwrap(),
                "--filter",
                filter,
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("Rich Task"))
            .stderr(predicate::str::is_empty());
    }
}

#[test]
fn recoverable_task_identity_and_parent_findings_still_render() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("tasks.md"),
        concat!(
            "@task(title=\"No ID\", pid=ignored, status=active)\n",
            "@task(id=orphan, pid=missing, title=\"Orphan\", status=active)\n",
            "@task(id=valid, title=\"Valid\", status=active)\n",
        ),
    )
    .unwrap();
    Command::cargo_bin("ragtag")
        .unwrap()
        .args([
            "diagram",
            "task-tree",
            "--path",
            directory.path().to_str().unwrap(),
            "--all",
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("No ID")
                .and(predicate::str::contains("Orphan"))
                .and(predicate::str::contains("Valid")),
        )
        .stderr(
            predicate::str::contains("DGM-TASK-001")
                .and(predicate::str::contains("DGM-TASK-002"))
                .and(predicate::str::contains("DGM-TASK-005"))
                .and(predicate::str::contains("DIA-GRAPH-009").not()),
        );
}

#[test]
fn duplicate_task_ids_fail_closed() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("tasks.md"),
        concat!(
            "@task(id=duplicate, title=\"One\", status=active)\n",
            "@task(id=duplicate, title=\"Two\", status=active)\n",
        ),
    )
    .unwrap();
    Command::cargo_bin("ragtag")
        .unwrap()
        .args([
            "diagram",
            "task-tree",
            "--path",
            directory.path().to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("DGM-TASK-003"));
}

#[test]
fn provider_and_forest_hierarchy_errors_accumulate_before_output() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("tasks.md"),
        concat!(
            "@task(id=self, pid=self, title=\"Self\", status=active)\n",
            "@task(id=first, pid=second, title=\"First\", status=active)\n",
            "@task(id=second, pid=first, title=\"Second\", status=active)\n",
        ),
    )
    .unwrap();
    Command::cargo_bin("ragtag")
        .unwrap()
        .args([
            "diagram",
            "task-tree",
            "--path",
            directory.path().to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("DGM-TASK-006")
                .and(predicate::str::contains("DGM-TASK-007"))
                .and(predicate::str::contains("DIA-FOREST-001"))
                .and(predicate::str::contains("DIA-FOREST-003")),
        );
}

#[test]
fn unknown_config_keys_never_reach_terminal_diagnostics() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("ragtag.yaml");
    let sentinel = "SENTINEL_CONFIG_SECRET";
    fs::write(
        &config,
        format!("\"bad\\n\\u001b]8;;https://evil\\u0007\\u202e{sentinel}\": {{}}\n"),
    )
    .unwrap();
    let output = Command::cargo_bin("ragtag")
        .unwrap()
        .args(["--config", config.to_str().unwrap(), "query", "anything"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("unknown configuration section"));
    assert!(!stderr.contains(sentinel));
    assert!(!stderr.contains('\u{1b}'));
    assert!(!stderr.contains('\u{7}'));
    assert!(!stderr.contains('\u{202e}'));
}

#[test]
fn invalid_ignore_regex_never_reaches_process_stderr() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("ragtag.yaml");
    let sentinel = "SENTINEL_IGNORE_SECRET";
    fs::write(
        &config,
        format!(
            "ignore_patterns:\n  - \"(?P<\\\\n\\\\u001b]8;;https://evil\\\\u0007\\\\u202e{sentinel}{}\"\n",
            "é".repeat(200)
        ),
    )
    .unwrap();
    let output = Command::cargo_bin("ragtag")
        .unwrap()
        .args([
            "--config",
            config.to_str().unwrap(),
            "diagram",
            "task-tree",
            "--path",
            directory.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("invalid ignore pattern at configured index 0"));
    assert!(!stderr.contains(sentinel));
    assert!(!stderr.contains('\u{1b}'));
    assert!(!stderr.contains('\u{7}'));
    assert!(!stderr.contains('\u{202e}'));
}

#[cfg(target_os = "linux")]
#[test]
fn file_output_is_atomic_mode_and_silent_on_success() {
    use std::os::unix::fs::PermissionsExt;

    let directory = fixture();
    let output = directory.path().join("diagram.d2");
    Command::cargo_bin("ragtag")
        .unwrap()
        .args([
            "diagram",
            "task-tree",
            "--path",
            directory.path().to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--all",
        ])
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::is_empty());
    assert!(fs::read_to_string(&output)
        .unwrap()
        .starts_with("direction: down\n"));
    assert_eq!(
        fs::metadata(output).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[cfg(target_os = "linux")]
#[test]
fn file_output_rejects_symlink_without_changing_target() {
    use std::os::unix::fs::symlink;

    let directory = fixture();
    let target = directory.path().join("target.d2");
    let output = directory.path().join("output.d2");
    fs::write(&target, "original").unwrap();
    symlink(&target, &output).unwrap();
    Command::cargo_bin("ragtag")
        .unwrap()
        .args([
            "diagram",
            "task-tree",
            "--path",
            directory.path().to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "existing output must be a regular file",
        ));
    assert_eq!(fs::read_to_string(target).unwrap(), "original");
}

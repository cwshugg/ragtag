//! Focused real-process coverage for task-type CLI semantics.

use predicates::prelude::*;
use std::fs;

mod support;
use support::ragtag;

#[test]
fn task_type_cli_preserves_defaults_custom_values_and_mutation_normalization() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("types.md");
    fs::write(
        &file,
        concat!(
            "@task(id=\"item\", title=\"Item\", status=\"active\")\n",
            "@task(id=\"project\", title=\"Project\", status=\"active\", type=\"PROJECT\")\n",
            "@task(id=\"custom\", title=\"Custom\", status=\"active\", type=\"ProjectX\")\n",
            "@task(id=\"spaced\", title=\"Spaced\", status=\"active\", type=\" Custom/阶段! \")\n",
            "@task(id=\"duplicate\", title=\"Duplicate\", status=\"active\", type=\"Old\", type=\"Hostile\")\n",
        ),
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "list",
            "--sort",
            "appearance",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("item"))
        .stdout(predicate::str::contains("custom"))
        .stdout(predicate::str::contains("project").not());

    for (filter, included, excluded) in [
        ("type=project", "project", "custom"),
        ("type=ProjectX", "custom", "project"),
        ("type=projectx", "", "custom"),
    ] {
        let output = ragtag()
            .args([
                "--no-color",
                "task",
                "list",
                "--filter",
                filter,
                "--format",
                "raw",
                "--path",
                file.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "stderr: {:?}", output.stderr);
        let output = String::from_utf8(output.stdout).unwrap();
        assert!(
            included.is_empty() || output.contains(included),
            "filter={filter:?}, output={output:?}"
        );
        assert!(
            !output.contains(excluded),
            "filter={filter:?}, output={output:?}"
        );
    }

    let output = ragtag()
        .args([
            "--no-color",
            "task",
            "list",
            "--all",
            "--sort",
            "appearance",
            "--format",
            "jsonl",
            "--path",
            file.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    let types: Vec<String> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line).unwrap()["type"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        types,
        ["item", "project", "ProjectX", " Custom/阶段! ", "Old"]
    );

    ragtag()
        .args([
            "--no-color",
            "task",
            "set-attr",
            "duplicate",
            "type",
            " Custom/阶段! ",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success();
    let updated = fs::read_to_string(&file).unwrap();
    let duplicate = updated
        .lines()
        .find(|line| line.contains("duplicate"))
        .unwrap();
    assert_eq!(duplicate.matches("type=").count(), 1, "{duplicate:?}");
    assert!(duplicate.contains("type=\" Custom/阶段! \""));
}

#[cfg(unix)]
#[test]
fn task_mutation_messages_are_terminal_safe() {
    let dir = tempfile::tempdir().unwrap();
    let controls = "\u{1b}]52;c;payload\u{7}\u{1b}[2J\r\n\u{202e}\u{85}";
    let prefix = format!("hostile{controls}id");
    let first_id = format!("{prefix}-a");
    let second_id = format!("{prefix}-b");
    let file = dir.path().join(format!("hostile{controls}.md"));
    fs::write(
        &file,
        format!(
            "@task(id=\"{first_id}\", title=\"First\", status=\"active\")\n\
             @task(id=\"{second_id}\", title=\"Second\", status=\"active\")\n"
        ),
    )
    .unwrap();

    let confirmation = ragtag()
        .args([
            "--no-color",
            "task",
            "set-attr",
            &first_id,
            "owner",
            controls,
            "--path",
            file.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(confirmation.status.success());
    let confirmation = String::from_utf8(confirmation.stdout).unwrap();
    assert_eq!(confirmation.lines().count(), 1, "{confirmation:?}");
    for control in ['\r', '\u{1b}', '\u{7}', '\u{202e}', '\u{85}'] {
        assert!(!confirmation.contains(control), "{confirmation:?}");
    }

    let ambiguity = ragtag()
        .args([
            "--no-color",
            "task",
            "set-attr",
            &prefix,
            "owner",
            "alice",
            "--path",
            file.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!ambiguity.status.success());
    let diagnostic = String::from_utf8(ambiguity.stderr).unwrap();
    for control in ['\r', '\u{1b}', '\u{7}', '\u{202e}', '\u{85}'] {
        assert!(!diagnostic.contains(control), "{diagnostic:?}");
    }
    for escaped in ["\\n", "\\u{1b}", "\\u{7}", "\\u{202e}", "\\u{85}"] {
        assert!(diagnostic.contains(escaped), "{diagnostic:?}");
    }
}

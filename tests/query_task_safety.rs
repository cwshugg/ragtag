//! Real-process coverage for task-aware query safety.

use std::fs;

mod support;
use support::ragtag;

#[test]
fn malformed_tasks_are_skipped_without_changing_generic_query_formatting() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("malformed.md");
    fs::write(
        &file,
        "@task(id=\"broken\", status=\"active\", type=\"\u{1b}]0;OSC\u{7}\")\n\
         @note(type=\"item\", title=\"Generic remains\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "query",
            "task",
            "--filter",
            "type=item",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout("");

    let output = ragtag()
        .args([
            "--no-color",
            "query",
            "--filter",
            "type=item",
            "--path",
            file.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    let output = String::from_utf8(output.stdout).unwrap();
    assert!(output.ends_with("@note(type=item, title=\"Generic remains\")\n"));
    assert_eq!(output.lines().count(), 1, "{output:?}");
    assert!(!output.contains('\u{1b}'), "{output:?}");
    assert!(!output.contains('\u{7}'), "{output:?}");
}

#[cfg(unix)]
#[test]
fn task_queries_escape_human_fields_while_jsonl_preserves_data() {
    let dir = tempfile::tempdir().unwrap();
    let hostile = "line\ncarriage\rCSI\x1b[2JOSC\x1b]52;c;payload\x07bidi\u{202e}control\u{85}";
    let file = dir.path().join(format!("{hostile}.md"));
    let config = dir.path().join("ragtag.yaml");
    let yaml = serde_yml::to_string(&serde_json::json!({
        "tasks": {
            "default_status": hostile,
            "status_keywords": {"active": [hostile]},
            "exclude_status_categories": []
        }
    }))
    .unwrap();
    fs::write(&config, yaml).unwrap();
    fs::write(
        &file,
        format!(
            "@task(id=\"{hostile}\", pid=\"{hostile}\", title=\"{hostile}\", \
             description=\"{hostile}\", owner=\"{hostile}\", status=\"{hostile}\", \
             type=\"PROJECT\u{202e}\", time_created=\"{hostile}\", \
             time_last_updated=\"{hostile}\")"
        ),
    )
    .unwrap();

    for scope in [Some("task"), None] {
        let mut arguments = vec!["--config", config.to_str().unwrap(), "--no-color", "query"];
        if let Some(tag_name) = scope {
            arguments.push(tag_name);
        }
        arguments.extend(["--path", file.to_str().unwrap()]);
        let output = ragtag().args(arguments).output().unwrap();
        assert!(output.status.success(), "stderr: {:?}", output.stderr);
        let output = String::from_utf8(output.stdout).unwrap();
        assert_eq!(output.lines().count(), 1, "{output:?}");
        for control in ['\r', '\u{1b}', '\u{7}', '\u{202e}', '\u{85}'] {
            assert!(!output.contains(control), "{output:?}");
        }
        for escaped in ["\\n", "\\r", "\\u{1b}", "\\u{7}", "\\u{202e}", "\\u{85}"] {
            assert!(output.contains(escaped), "{output:?}");
        }
    }

    let output = ragtag()
        .args([
            "--config",
            config.to_str().unwrap(),
            "--no-color",
            "task",
            "list",
            "--all",
            "--format",
            "jsonl",
            "--path",
            file.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    let record: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    for field in [
        "id",
        "pid",
        "title",
        "description",
        "owner",
        "status",
        "time_created",
        "time_last_updated",
    ] {
        assert_eq!(record[field], hostile, "field={field}");
    }
    assert_eq!(record["type"], "PROJECT\u{202e}");
    assert_eq!(record["source"]["file"], file.to_string_lossy().as_ref());
}

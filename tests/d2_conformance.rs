//! Golden and semantic conformance for D2 v0.9.0 source.

use std::path::Path;
use std::process::{Command, Stdio};

use assert_cmd::cargo::cargo_bin;

const FIXTURE_DIRECTORY: &str = "tests/fixtures/diagram";

#[test]
fn representative_task_documents_match_committed_goldens() {
    let cases: &[(&[&str], &str)] = &[
        (
            &["diagram", "task-tree", "--path", FIXTURE_DIRECTORY, "--all"],
            "task-tree.d2",
        ),
        (
            &[
                "diagram",
                "task-buckets",
                "--path",
                FIXTURE_DIRECTORY,
                "--all",
            ],
            "task-buckets.d2",
        ),
        (
            &[
                "diagram",
                "task-tree",
                "--path",
                FIXTURE_DIRECTORY,
                "--filter",
                "id=active",
                "--direction",
                "right",
            ],
            "task-tree-filter-right.d2",
        ),
        (
            &["diagram", "task-tree", "--path", FIXTURE_DIRECTORY],
            "task-tree-default-context.d2",
        ),
        (
            &["diagram", "task-buckets", "--path", FIXTURE_DIRECTORY],
            "task-buckets-default-context.d2",
        ),
    ];
    for (arguments, golden) in cases {
        let output = Command::new(cargo_bin("ragtag"))
            .args(*arguments)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            output.stdout,
            std::fs::read(Path::new(FIXTURE_DIRECTORY).join(golden)).unwrap(),
            "golden mismatch for {golden}"
        );
    }
}

#[test]
#[ignore = "requires pinned D2 v0.9.0 and tests/tools/d2inspect/d2inspect"]
fn representative_goldens_have_exact_pinned_d2_semantics() {
    const ROOT: &str = "n7461736b732e6d643a303a30";
    const ACTIVE: &str = "n7461736b732e6d643a36363a31";
    const BLOCKED: &str = "n7461736b732e6d643a3134373a32";
    let cases = [
        ("task-tree.d2", "down", 5usize, 5usize),
        ("task-buckets.d2", "down", 5, 5),
        ("task-tree-filter-right.d2", "right", 2, 2),
        ("task-tree-default-context.d2", "down", 4, 4),
        ("task-buckets-default-context.d2", "down", 4, 4),
    ];

    for (golden, direction, nodes, newlines) in cases {
        let path = Path::new(FIXTURE_DIRECTORY).join(golden);
        let status = Command::new("d2")
            .args([path.to_str().unwrap(), "-"])
            .stdout(Stdio::null())
            .status()
            .expect("pinned d2 executable must be on PATH");
        assert!(status.success(), "D2 rejected {golden}");
        let output = Command::new("tests/tools/d2inspect/d2inspect")
            .arg(&path)
            .output()
            .expect("pinned d2inspect helper must be built");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let facts = String::from_utf8(output.stdout).unwrap();
        assert!(facts.contains(&format!("\"direction\":\"{direction}\"")));
        assert_eq!(facts.matches("\"id\":").count(), nodes, "{golden}: {facts}");
        assert!(facts.contains(&format!("\"label_newlines\":{newlines}")));
        for expected in [
            "\"has_import\":false",
            "\"has_substitution\":false",
            "\"has_link\":false",
            "\"has_class\":false",
            "\"safe_styles\":true",
            "\"shape\":\"rectangle\",\"classes\":[]",
        ] {
            assert!(facts.contains(expected), "{golden}: missing {expected}");
        }

        if golden.contains("buckets") {
            assert!(facts.contains("\"edges\":[]"));
            assert!(facts.contains(&format!("\"id\":\"{ROOT}.{ACTIVE}\",\"parent\":\"{ROOT}\"")));
            assert!(facts.contains(&format!(
                "\"id\":\"{ROOT}.{BLOCKED}\",\"parent\":\"{ROOT}\""
            )));
        } else {
            assert!(facts.contains(&format!(
                "\"source\":\"{ROOT}\",\"destination\":\"{ACTIVE}\",\"source_arrow\":false,\"target_arrow\":true"
            )));
            if golden != "task-tree-filter-right.d2" {
                assert!(facts.contains(&format!(
                    "\"source\":\"{ROOT}\",\"destination\":\"{BLOCKED}\",\"source_arrow\":false,\"target_arrow\":true"
                )));
            }
        }
        if golden.contains("context") || golden.contains("filter") {
            assert!(facts.contains("\"opacity\":\"0.55\""));
            assert!(facts.contains("\"stroke_dash\":\"4\""));
        }
    }
}

#[test]
#[ignore = "requires pinned D2 v0.9.0 and tests/tools/d2inspect/d2inspect"]
fn generated_hostile_source_passes_pinned_d2_tools() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("hostile-${path}.md");
    let config_path = directory.path().join("ragtag.yaml");
    let matrix = concat!(
        "CR\rLF\nCRLF\r\nNEL\u{85}LS\u{2028}PS\u{2029}",
        "ANSI\u{1b}[31mOSC\u{1b}]8;;https://evil\u{7}",
        "BIDI\u{202e}SUB${inject}IMPORT@import LINKlink: ",
        "PROPstyle.fill: red CLASSclass: bad"
    );
    let status_matrix = concat!(
        "CR\rLF\nCRLF\r\nNEL\u{85}LS\u{2028}PS\u{2029}",
        "ANSI\u{1b}[31mOSC\u{1b}]8;;https://evil\u{7}",
        "BIDI\u{202e}IMPORT@import LINKlink: PROPstyle.fill: red CLASSclass: bad"
    );
    let hostile_status = format!("STATUS_SENTINEL {status_matrix}");
    std::fs::write(
        &config_path,
        format!(
            "tasks:\n  status_keywords:\n    active:\n      - {}\n",
            yaml_string(&hostile_status)
        ),
    )
    .unwrap();
    let root_id = format!("ROOT_ID_SENTINEL {matrix}");
    let child_id = format!("CHILD_ID_SENTINEL {matrix}");
    let root_title = format!("TITLE_SENTINEL quote\" {matrix}");
    let root_owner = format!("OWNER_SENTINEL {matrix}");
    let source = format!(
        concat!(
            "@task(id={root_id}, title={root_title}, description={root_description}, ",
            "owner={root_owner}, status={hostile_status}, ",
            "priority=0, worktime_spent=-1.5, worktime_estimate=1.25, ",
            "time_created={root_created}, time_last_updated={root_updated}, ",
            "worktime_units=days)\n",
            "@task(id={child_id}, pid={root_id}, title={child_title}, ",
            "description={child_description}, owner={child_owner}, ",
            "status={hostile_status}, priority=4294967295, ",
            "worktime_spent=0, worktime_estimate=999999.5, ",
            "time_created={child_created}, time_last_updated={child_updated}, ",
            "worktime_units=hours)\n",
        ),
        root_id = tag_string(&root_id),
        child_id = tag_string(&child_id),
        root_title = tag_string(&root_title),
        root_description = tag_string(&format!("DESCRIPTION_SENTINEL {matrix}")),
        root_owner = tag_string(&root_owner),
        hostile_status = tag_string(&hostile_status),
        root_created = tag_string(&format!("CREATED_SENTINEL {matrix}")),
        root_updated = tag_string(&format!("UPDATED_SENTINEL {matrix}")),
        child_title = tag_string(&format!("CHILD_TITLE_SENTINEL {matrix}")),
        child_description = tag_string(&format!("CHILD_DESCRIPTION_SENTINEL {matrix}")),
        child_owner = tag_string(&format!("CHILD_OWNER_SENTINEL {matrix}")),
        child_created = tag_string(&format!("CHILD_CREATED_SENTINEL {matrix}")),
        child_updated = tag_string(&format!("CHILD_UPDATED_SENTINEL {matrix}")),
    );
    std::fs::write(&source_path, &source).unwrap();
    let root_offset = source.find("@task").unwrap();
    let child_offset = source.rfind("@task").unwrap();
    let root_node = generated_id(&format!(":{root_offset}:0"));
    let child_node = generated_id(&format!(":{child_offset}:1"));
    let root_label = format!(
        "{}\n[{}] · P0 · {}",
        sanitized_field(&root_title),
        sanitized_field(&hostile_status),
        sanitized_field(&root_owner)
    );
    let child_label = format!(
        "{}\n[{}] · P4294967295 · {}",
        sanitized_field(&format!("CHILD_TITLE_SENTINEL {matrix}")),
        sanitized_field(&hostile_status),
        sanitized_field(&format!("CHILD_OWNER_SENTINEL {matrix}"))
    );

    for kind in ["task-tree", "task-buckets"] {
        let output = Command::new(cargo_bin("ragtag"))
            .args([
                "--config",
                config_path.to_str().unwrap(),
                "diagram",
                kind,
                "--path",
                source_path.to_str().unwrap(),
                "--all",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let d2_path = directory.path().join(format!("{kind}.d2"));
        std::fs::write(&d2_path, &output.stdout).unwrap();

        let status = Command::new("d2")
            .args([d2_path.to_str().unwrap(), "-"])
            .stdout(Stdio::null())
            .status()
            .expect("pinned d2 executable must be on PATH");
        assert!(status.success());

        let result = Command::new("tests/tools/d2inspect/d2inspect")
            .arg(&d2_path)
            .output()
            .expect("pinned d2inspect helper must be built");
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let facts = String::from_utf8(result.stdout).unwrap();
        for expected in [
            "\"direction\":\"down\"",
            "\"has_import\":false",
            "\"has_substitution\":false",
            "\"has_link\":false",
            "\"has_class\":false",
            "\"safe_styles\":true",
            "\"label_newlines\":2",
            "\"shape\":\"rectangle\",\"classes\":[]",
            &format!("\"label\":{}", json_string(&root_label)),
            &format!("\"label\":{}", json_string(&child_label)),
            "\"style\":{\"fill\":\"#fff3bf\",\"stroke\":\"#e67700\",\"stroke_width\":\"4\",\"border_radius\":\"6\"}",
            "\"style\":{\"fill\":\"#fff3bf\",\"stroke\":\"#e67700\",\"stroke_width\":\"1\",\"border_radius\":\"6\"}",
        ] {
            assert!(facts.contains(expected), "missing {expected} in {facts}");
        }
        assert_eq!(facts.matches("\"id\":").count(), 2, "{facts}");
        for unrendered in [
            "ROOT_ID_SENTINEL",
            "CHILD_ID_SENTINEL",
            "DESCRIPTION_SENTINEL",
            "CHILD_DESCRIPTION_SENTINEL",
            "CREATED_SENTINEL",
            "UPDATED_SENTINEL",
            "CHILD_CREATED_SENTINEL",
            "CHILD_UPDATED_SENTINEL",
            "999999.5",
        ] {
            assert!(
                !facts.contains(unrendered),
                "unrendered task field leaked into D2 facts: {unrendered}: {facts}"
            );
        }
        if kind == "task-tree" {
            let edge = format!(
                "{{\"source\":\"{root_node}\",\"destination\":\"{child_node}\",\"source_arrow\":false,\"target_arrow\":true}}"
            );
            assert!(
                facts.contains(&edge),
                "missing exact edge {edge} in {facts}"
            );
            assert!(facts.contains(&format!("\"id\":\"{root_node}\"")));
            assert!(facts.contains(&format!("\"id\":\"{child_node}\"")));
            assert!(!facts.contains("\"parent\":"));
        } else {
            let nested_child = format!("{root_node}.{child_node}");
            assert!(facts.contains("\"edges\":[]"));
            assert!(facts.contains(&format!(
                "\"id\":\"{nested_child}\",\"parent\":\"{root_node}\""
            )));
        }
    }
}

fn tag_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn yaml_string(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character
                if character.is_control()
                    || matches!(
                        character,
                        '\u{2028}'
                            | '\u{2029}'
                            | '\u{202a}'..='\u{202e}'
                            | '\u{2066}'..='\u{2069}'
                    ) =>
            {
                use std::fmt::Write;
                write!(output, "\\u{:04x}", character as u32).unwrap();
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

fn sanitized_field(value: &str) -> String {
    let mut output = String::new();
    for character in value.chars() {
        if character.is_control()
            || matches!(
                character,
                '\u{2028}' | '\u{2029}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
            )
        {
            output.extend(character.escape_unicode());
        } else {
            output.push(character);
        }
    }
    output
}

fn generated_id(value: &str) -> String {
    let mut output = String::from("n");
    for byte in value.as_bytes() {
        use std::fmt::Write;
        write!(output, "{byte:02x}").unwrap();
    }
    output
}

fn json_string(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                use std::fmt::Write;
                write!(output, "\\u{:04x}", character as u32).unwrap();
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

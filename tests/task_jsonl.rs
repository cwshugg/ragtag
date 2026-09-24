//! Real-process coverage for the stable task JSONL protocol.

use std::fs;

mod support;
use support::ragtag;

#[test]
fn task_list_jsonl_is_framed_normalized_and_source_bound() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("ragtag.yaml");
    let file = dir.path().join("snapshot.md");
    fs::write(&config, "tasks:\n  tag_name: work\n").unwrap();

    let hostile_description =
        "first line\n\nid=forged-parent\ntype=project\nfile=forged.md\nline=1\n\u{1b}[2J\u{1b}]0;OSC\u{7}\u{202e}";
    let multiline = format!(
        "@work(\n  id=\"duplicate-id\",\n  title=\"Duplicate title\",\n  description=\"{hostile_description}\",\n  status=\"active\",\n  type=\"PrOjEcT\"\n)"
    );
    let same_line_first =
        "@work(id=\"duplicate-id\", title=\"Duplicate title\", status=\"active\", type=\"ITEM\")";
    let same_line_second =
        "@work(id=\"third-id\", title=\"Third\", status=\"active\", description=\"a=b\")";
    let source = format!("π snapshot {multiline} {same_line_first} text {same_line_second}\n");
    fs::write(&file, &source).unwrap();

    let output = ragtag()
        .args([
            "--config",
            config.to_str().unwrap(),
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
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output = String::from_utf8(output).unwrap();
    let records: Vec<serde_json::Value> = output
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), 3, "output: {output:?}");
    assert_eq!(records[0].as_object().unwrap().len(), 14);
    assert!(
        !output.contains("\nid=forged-parent\n"),
        "JSONL framing was broken: {output:?}"
    );

    let occurrences = [multiline.as_str(), same_line_first, same_line_second];
    for (record, occurrence) in records.iter().zip(occurrences) {
        let source_record = &record["source"];
        assert_eq!(source_record["tag_name"], "work");
        assert_eq!(source_record["file"], file.to_str().unwrap());

        let byte_start = source_record["byte_start"].as_u64().unwrap() as usize;
        let byte_end = source_record["byte_end"].as_u64().unwrap() as usize;
        assert_eq!(&source[byte_start..byte_end], occurrence);
        assert_eq!(byte_end - byte_start, occurrence.len());
        assert_eq!(
            source_record["line"].as_u64().unwrap() as usize,
            source[..byte_start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                + 1
        );
        let line_start = source[..byte_start]
            .rfind('\n')
            .map_or(0, |newline| newline + 1);
        assert_eq!(
            source_record["column"].as_u64().unwrap() as usize,
            source[line_start..byte_start].len() + 1
        );
    }

    assert_eq!(records[0]["id"], "duplicate-id");
    assert_eq!(records[0]["title"], "Duplicate title");
    assert_eq!(records[0]["type"], "project");
    assert_eq!(records[0]["description"], hostile_description);
    assert_eq!(records[1]["id"], "duplicate-id");
    assert_eq!(records[1]["title"], "Duplicate title");
    assert_eq!(records[1]["type"], "item");
    assert_eq!(records[2]["type"], "item");
    assert_eq!(records[2]["pid"], serde_json::Value::Null);
    assert_eq!(records[2]["priority"], serde_json::Value::Null);
}

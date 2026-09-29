//! End-to-end contracts for generic create and effective config dumping.

use std::fs;
use std::path::Path;

use predicates::prelude::*;
use ragtag::models::TagAttribute;

mod support;
use support::ragtag;

/// Returns only the flat-dump records that describe tag presets.
fn preset_dump_lines(stdout: &[u8]) -> Vec<&str> {
    std::str::from_utf8(stdout)
        .unwrap()
        .lines()
        .filter(|line| line.starts_with("tags.presets"))
        .collect()
}

/// Creates an explicit config file and returns its owning temporary directory.
fn config_file(yaml: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.yaml");
    fs::write(&path, yaml).unwrap();
    (directory, path)
}

/// Parses exact command output and compares its ordered tag semantics.
fn assert_tag_semantics(actual: &str, expected: &str) {
    fn parse(input: &str) -> (String, Vec<TagAttribute>) {
        let tags = ragtag::parser::scan_file(input, Path::new("<create-test>"));
        assert_eq!(tags.len(), 1, "expected exactly one tag in {input:?}");
        let tag = tags.into_iter().next().unwrap();
        (tag.name, tag.attributes)
    }

    assert_eq!(parse(actual), parse(expected));
}

#[test]
fn create_requires_exactly_one_source() {
    ragtag()
        .arg("create")
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty());
    ragtag()
        .args(["create", "--name", "note", "--preset", "note"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty());
}

#[test]
fn create_name_repeated_attributes_and_formats_are_exact() {
    ragtag()
        .args([
            "create",
            "--name",
            "note",
            "--attribute",
            "title=`Release, notes`",
            "--attribute",
            "priority=-1",
            "--attribute",
            "title=\"Final\"",
            "--format",
            "oneline",
        ])
        .assert()
        .success()
        .stdout("@note(title=\"Final\", priority=-1)\n")
        .stderr(predicate::str::is_empty());

    ragtag()
        .args(["create", "--name", "note", "--attribute", "title=Final"])
        .assert()
        .success()
        .stdout("@note(\n    title=Final\n)\n");
}

#[test]
fn preset_empty_delimiters_are_exact_in_both_layouts() {
    let (_directory, config) = config_file(
        r#"
tags:
  presets:
    - nickname: review
      value: '@code-review(description="", url=``)'
"#,
    );
    for (format, expected) in [
        (
            "multiline",
            "@code-review(\n    description=\"\",\n    url=``\n)\n",
        ),
        ("oneline", "@code-review(description=\"\", url=``)\n"),
    ] {
        let output = ragtag()
            .args([
                "--config",
                config.to_str().unwrap(),
                "create",
                "--preset",
                "review",
                "--format",
                format,
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let actual = String::from_utf8(output.stdout).unwrap();
        assert_eq!(actual, expected);
        assert_tag_semantics(&actual, "@code-review(description='', url=\"\")");
    }
}

#[test]
fn preset_and_override_lexemes_are_preserved_without_double_escaping() {
    let (_directory, config) = config_file(
        r#"
tags:
  presets:
    - nickname: styled
      value: |-
        @styled('positional', double="a\"b", tick=`a\`b`, path='c\\d', hex=0XFF, empty=``)
"#,
    );
    let output = ragtag()
        .args([
            "--config",
            config.to_str().unwrap(),
            "create",
            "--preset",
            "styled",
            "--attribute",
            "double='changed'",
            "--format",
            "oneline",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let actual = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        actual,
        "@styled('positional', double='changed', tick=`a\\`b`, path='c\\\\d', hex=0XFF, empty=``)\n"
    );
    assert_tag_semantics(
        &actual,
        r#"@styled("positional", double="changed", tick="a`b", path="c\\d", hex=0xff, empty="")"#,
    );
}

#[test]
fn create_rejects_invalid_name_attributes_and_missing_option_values() {
    ragtag()
        .args(["create", "--name", "@note"])
        .assert()
        .code(1)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("invalid tag name"));
    ragtag()
        .args(["create", "--name", "note", "--attribute", "positional"])
        .assert()
        .code(1)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("invalid --attribute #1"));

    for next in [
        "--interactive",
        "--format",
        "--name",
        "--preset",
        "--attribute",
    ] {
        ragtag()
            .args(["create", "--name", "note", "--attribute", next])
            .assert()
            .code(2)
            .stdout(predicate::str::is_empty());
    }
}

#[test]
fn preset_lookup_ignores_ordinary_english_case_and_reports_cross_category_ambiguity() {
    let (_directory, config) = config_file(
        r#"
tags:
  presets:
    - nickname: Bug Report
      value: '@issue(priority=1)'
"#,
    );
    ragtag()
        .args([
            "--config",
            config.to_str().unwrap(),
            "create",
            "--preset",
            "BUG REPORT",
            "--format",
            "oneline",
        ])
        .assert()
        .success()
        .stdout("@issue(priority=1)\n");

    let (_directory, ambiguous) = config_file(
        r#"
tags:
  presets:
    - nickname: task
      value: '@first'
    - nickname: second
      value: '@task'
"#,
    );
    ragtag()
        .args([
            "--config",
            ambiguous.to_str().unwrap(),
            "create",
            "--preset",
            "@TASK",
        ])
        .assert()
        .code(1)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "matches tags.presets[0], tags.presets[1]",
        ));
}

#[test]
fn explicit_values_precede_interactive_edits_and_blank_preserves() {
    let (_directory, config) = config_file(
        r#"
tags:
  presets:
    - nickname: bug
      value: "@issue('positional', priority=1)"
"#,
    );
    ragtag()
        .args([
            "--config",
            config.to_str().unwrap(),
            "create",
            "--preset",
            "bug",
            "--attribute",
            "priority=`2`",
            "--interactive",
            "--format",
            "oneline",
        ])
        .write_stdin("\n'three'\n")
        .assert()
        .success()
        .stdout("@issue('positional', priority='three')\n")
        .stderr(
            predicate::str::contains("Positional 1 (current: 'positional'; Enter to keep): ").and(
                predicate::str::contains("priority (current: `2`; Enter to keep): "),
            ),
        );
}

#[test]
fn every_quoted_empty_cli_value_is_preserved_and_bare_empty_is_rejected() {
    let output = ragtag()
        .args([
            "create",
            "--name",
            "empty",
            "--attribute",
            "double=\"\"",
            "--attribute",
            "single=''",
            "--attribute",
            "backtick=``",
            "--format",
            "oneline",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let actual = String::from_utf8(output.stdout).unwrap();
    assert_eq!(actual, "@empty(double=\"\", single='', backtick=``)\n");
    assert_tag_semantics(&actual, "@empty(double='', single=`` , backtick=\"\")");

    ragtag()
        .args(["create", "--name", "empty", "--attribute", "value="])
        .assert()
        .code(1)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("invalid --attribute #1"));
}

#[test]
fn interactive_invalid_value_reprompts_without_partial_stdout() {
    ragtag()
        .args([
            "create",
            "--name",
            "note",
            "--attribute",
            "value=old",
            "--interactive",
            "--format",
            "oneline",
        ])
        .write_stdin("1.0e999\nnew\n")
        .assert()
        .success()
        .stdout("@note(value=new)\n")
        .stderr(predicate::str::contains("Error: Expected one complete"));
}

#[test]
fn invalid_preset_values_fail_at_startup_without_disclosure() {
    let (_directory, config) = config_file(
        r#"
tags:
  presets:
    - nickname: bad
      value: '@note(value=1.0e999)'
"#,
    );
    ragtag()
        .args([
            "--config",
            config.to_str().unwrap(),
            "create",
            "--preset",
            "bad",
        ])
        .assert()
        .code(1)
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("tags.presets[0].value")
                .and(predicate::str::contains("1.0e999").not()),
        );
}

#[test]
fn aggregate_attribute_limit_allows_replacement_and_rejects_append_before_prompting() {
    let attributes = (0..256)
        .map(|index| format!("a{index}={index}"))
        .collect::<Vec<_>>()
        .join(",");
    let (_directory, config) = config_file(&format!(
        "tags:\n  presets:\n    - nickname: full\n      value: '@full({attributes})'\n"
    ));

    ragtag()
        .args([
            "--config",
            config.to_str().unwrap(),
            "create",
            "--preset",
            "full",
            "--attribute",
            "a0=first",
            "--attribute",
            "a0=last",
            "--format",
            "oneline",
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::starts_with("@full(a0=last, a1=1")
                .and(predicate::str::ends_with("a255=255)\n")),
        );

    ragtag()
        .args([
            "--config",
            config.to_str().unwrap(),
            "create",
            "--preset",
            "full",
            "--attribute",
            "new_name=value",
            "--interactive",
        ])
        .write_stdin("replacement\n")
        .assert()
        .code(1)
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("maximum of 256 attributes")
                .and(predicate::str::contains("(current:").not()),
        );
}

#[test]
fn config_dump_flat_is_sorted_complete_and_json_scalar_encoded() {
    let (_directory, config) = config_file(
        r#"
ignore_patterns: ["space value"]
tags:
  presets:
    - nickname: 'a"b'
      value: '@note(value="x")'
"#,
    );
    let output = ragtag()
        .args(["--config", config.to_str().unwrap(), "config", "dump"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).unwrap();
    let lines = text.lines().collect::<Vec<_>>();
    let mut sorted = lines.clone();
    sorted.sort();
    assert_eq!(lines, sorted);
    assert!(text.contains("tags.presets[0].nickname = \"a\\\"b\"\n"));
    assert!(text.contains("tags.presets[0].value = \"@note(value=\\\"x\\\")\"\n"));
    assert!(text.contains("tasks.default_owner = \"me\"\n"));
    assert!(text.ends_with('\n'));
    assert!(!text.ends_with("\n\n"));
}

#[test]
fn config_dump_yaml_and_environment_redaction_are_complete() {
    let (_directory, config) = config_file(
        r#"
tags:
  presets:
    - nickname: "$SECRET_NICK"
      value: '@note(value="safe")'
"#,
    );
    let output = ragtag()
        .env("SECRET_NICK", "hidden-name")
        .args([
            "--config",
            config.to_str().unwrap(),
            "config",
            "dump",
            "--format",
            "yaml",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    let value: serde_yml::Value = serde_yml::from_str(&text).unwrap();
    assert_eq!(
        value["tags"]["presets"][0]["nickname"].as_str(),
        Some("<environment-derived>")
    );
    assert_eq!(value["tasks"]["default_owner"].as_str(), Some("me"));
    assert!(!text.contains("hidden-name"));
    assert!(text.ends_with('\n'));
    assert!(!text.ends_with("\n\n"));
}

#[test]
fn unsupported_dump_format_is_a_clap_error() {
    ragtag()
        .args(["config", "dump", "--format", "json"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty());
}

#[test]
fn inferred_create_and_config_prefixes_follow_the_real_command_tree() {
    ragtag()
        .arg("c")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("config").and(predicate::str::contains("create")));
    ragtag()
        .args(["cr", "--name", "note", "--format", "oneline"])
        .assert()
        .success()
        .stdout("@note\n");
    ragtag()
        .args(["co", "dump"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("aliases = []\n"));
}

#[test]
fn aliases_can_target_generic_create() {
    let (_directory, config) = config_file(
        "aliases:\n  - name: make-note\n    arguments: 'create --name note --format oneline'\n",
    );
    ragtag()
        .args(["--config", config.to_str().unwrap(), "make-note"])
        .assert()
        .success()
        .stdout("@note\n");
}

#[test]
fn task_create_command_bytes_remain_exact_in_both_formats() {
    for (format, expected) in [
        (
            "oneline",
            "@task(id=\"{id}\", title=\"Exact \\\"task\\\"\", owner=\"me\", status=\"new\", type=\"item\", worktime_spent=0, worktime_estimate=2, time_created=\"{timestamp}\", time_last_updated=\"{timestamp}\", worktime_units=\"hours\")\n".to_string(),
        ),
        (
            "multiline",
            "@task(\n    id=\"{id}\",\n    title=\"Exact \\\"task\\\"\",\n    owner=\"me\",\n    status=\"new\",\n    type=\"item\",\n    worktime_spent=0,\n    worktime_estimate=2,\n    time_created=\"{timestamp}\",\n    time_last_updated=\"{timestamp}\",\n    worktime_units=\"hours\"\n)\n".to_string(),
        ),
    ] {
        let output = ragtag()
            .args([
                "task",
                "create",
                "--title",
                "Exact \"task\"",
                "--worktime-estimate",
                "2",
                "--format",
                format,
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());

        let actual = String::from_utf8(output.stdout).unwrap();
        let id_start = actual.find("id=\"").unwrap() + 4;
        let id_end = actual[id_start..].find('"').unwrap() + id_start;
        let id = &actual[id_start..id_end];
        assert_eq!(id.len(), 16);
        assert!(id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));

        let timestamp_start = actual.find("time_created=\"").unwrap() + 14;
        let timestamp_end = actual[timestamp_start..].find('"').unwrap() + timestamp_start;
        let timestamp = &actual[timestamp_start..timestamp_end];
        assert_eq!(timestamp.len(), 20);
        chrono::DateTime::parse_from_rfc3339(timestamp).unwrap();

        assert_eq!(
            actual,
            expected.replace("{id}", id).replace("{timestamp}", timestamp),
            "{format} task-create bytes changed"
        );
    }
}

#[test]
fn flat_dump_empty_and_escaped_preset_fixtures_are_exact() {
    let empty = ragtag()
        .args(["config", "dump"])
        .output()
        .expect("run default config dump");
    assert!(empty.status.success());
    assert!(empty.stderr.is_empty());
    assert_eq!(preset_dump_lines(&empty.stdout), ["tags.presets = []"]);

    let (_directory, config) = config_file(
        r#"
tags:
  presets:
    - nickname: 'quoted"name'
      value: '@note(value="C:\\notes")'
"#,
    );
    let escaped = ragtag()
        .args(["--config", config.to_str().unwrap(), "config", "dump"])
        .output()
        .expect("run configured config dump");
    assert!(escaped.status.success());
    assert!(escaped.stderr.is_empty());
    assert_eq!(
        preset_dump_lines(&escaped.stdout),
        [
            "tags.presets[0].nickname = \"quoted\\\"name\"",
            "tags.presets[0].value = \"@note(value=\\\"C:\\\\\\\\notes\\\")\"",
        ]
    );
}

#[test]
fn flat_dump_presets_preserve_lexical_lines_and_numeric_index_order() {
    let presets = (0..12)
        .map(|index| format!("    - nickname: preset-{index}\n      value: '@tag-{index}'\n"))
        .collect::<String>();
    let (_directory, config) = config_file(&format!("tags:\n  presets:\n{presets}"));
    let output = ragtag()
        .args(["--config", config.to_str().unwrap(), "config", "dump"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());

    let lines = preset_dump_lines(&output.stdout);
    let mut expected_lexical = (0..12)
        .flat_map(|index| {
            [
                format!("tags.presets[{index}].nickname = \"preset-{index}\""),
                format!("tags.presets[{index}].value = \"@tag-{index}\""),
            ]
        })
        .collect::<Vec<_>>();
    expected_lexical.sort();
    assert_eq!(lines, expected_lexical);

    let mut reconstructed = lines
        .chunks_exact(2)
        .map(|pair| {
            let index = pair[0]
                .strip_prefix("tags.presets[")
                .unwrap()
                .split_once(']')
                .unwrap()
                .0
                .parse::<usize>()
                .unwrap();
            (index, pair[0], pair[1])
        })
        .collect::<Vec<_>>();
    reconstructed.sort_by_key(|(index, _, _)| *index);
    for (expected_index, (actual_index, nickname, value)) in reconstructed.into_iter().enumerate() {
        assert_eq!(actual_index, expected_index);
        assert_eq!(
            nickname,
            format!("tags.presets[{expected_index}].nickname = \"preset-{expected_index}\"")
        );
        assert_eq!(
            value,
            format!("tags.presets[{expected_index}].value = \"@tag-{expected_index}\"")
        );
    }
}

#[test]
fn flat_dump_preset_redaction_fixtures_cover_each_field_combination() {
    let (_directory, config) = config_file(
        r#"
tags:
  presets:
    - nickname: safe
      value: '@safe'
    - nickname: "$SECRET_NICKNAME"
      value: '@nickname-redacted'
    - nickname: value-redacted
      value: "$SECRET_VALUE"
    - nickname: "$SECRET_BOTH_NICKNAME"
      value: "$SECRET_BOTH_VALUE"
"#,
    );
    let output = ragtag()
        .env("SECRET_NICKNAME", "private-nickname")
        .env("SECRET_VALUE", "@private-value")
        .env("SECRET_BOTH_NICKNAME", "private-both-nickname")
        .env("SECRET_BOTH_VALUE", "@private-both-value")
        .args(["--config", config.to_str().unwrap(), "config", "dump"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        preset_dump_lines(&output.stdout),
        [
            "tags.presets[0].nickname = \"safe\"",
            "tags.presets[0].value = \"@safe\"",
            "tags.presets[1].nickname = \"<environment-derived>\"",
            "tags.presets[1].value = \"@nickname-redacted\"",
            "tags.presets[2].nickname = \"value-redacted\"",
            "tags.presets[2].value = \"<environment-derived>\"",
            "tags.presets[3].nickname = \"<environment-derived>\"",
            "tags.presets[3].value = \"<environment-derived>\"",
        ]
    );
    let text = std::str::from_utf8(&output.stdout).unwrap();
    assert!(!text.contains("private-nickname"));
    assert!(!text.contains("@private-value"));
    assert!(!text.contains("private-both-nickname"));
    assert!(!text.contains("@private-both-value"));
}

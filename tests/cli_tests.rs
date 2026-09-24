//! CLI integration tests using assert_cmd.

use predicates::prelude::*;
use std::fs;

mod support;
use support::{assert_output_equivalent, fixtures_dir, ragtag};

// === File Touch ===

#[test]
fn test_file_touch_help_exposes_only_touch_and_creation_options() {
    ragtag()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("file"));
    ragtag()
        .args(["file", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("touch"))
        .stdout(predicate::str::contains("  help").not())
        .stdout(predicate::str::contains("create").not())
        .stdout(predicate::str::contains("list").not());
    ragtag()
        .args(["file", "touch", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Create a new file and print its path; fail if the target exists",
        ))
        .stdout(predicate::str::contains("--path <FILE>"))
        .stdout(predicate::str::contains("--tag <TAG>"))
        .stdout(predicate::str::contains("--edit"));
    ragtag().args(["file", "list"]).assert().failure();
}

#[test]
fn test_file_touch_default_uses_config_root_and_utc_filename() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join(".ragtag.yaml");
    fs::write(
        &config,
        "files:\n  default_directory: notes\n  filename_format: \"%Y-%m-%d_%H-%M-%S.md\"\n",
    )
    .unwrap();
    let elsewhere = tempfile::tempdir().unwrap();

    let assert = ragtag()
        .current_dir(elsewhere.path())
        .args(["--config", config.to_str().unwrap(), "file", "touch"])
        .env_remove("EDITOR")
        .assert()
        .success()
        .stderr(predicate::str::is_empty());

    let entries = fs::read_dir(dir.path().join("notes"))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(entries.len(), 1);
    let name = entries[0].file_name();
    let name = name.to_str().unwrap();
    assert_eq!(name.len(), "2026-08-21_12-33-52.md".len());
    assert!(name.ends_with(".md"));
    assert!(name.chars().enumerate().all(|(index, value)| match index {
        4 | 7 => value == '-',
        10 => value == '_',
        13 | 16 => value == '-',
        19 => value == '.',
        20 => value == 'm',
        21 => value == 'd',
        _ => value.is_ascii_digit(),
    }));
    assert_eq!(fs::read(entries[0].path()).unwrap(), b"");
    assert_eq!(
        assert.get_output().stdout,
        format!("{}\n", entries[0].path().display()).as_bytes()
    );
}

#[test]
fn test_file_touch_default_dot_directory_prints_clean_absolute_path() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join(".ragtag.yaml");
    fs::write(
        &config,
        "files:\n  default_directory: \".\"\n  filename_format: \"fixed.md\"\n",
    )
    .unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let expected = dir.path().join("fixed.md");

    let assert = ragtag()
        .current_dir(elsewhere.path())
        .args(["--config", config.to_str().unwrap(), "file", "touch"])
        .assert()
        .success()
        .stderr(predicate::str::is_empty());

    assert!(expected.is_absolute());
    assert!(expected.is_file());
    assert!(!String::from_utf8_lossy(&assert.get_output().stdout).contains("/./"));
    assert_eq!(
        assert.get_output().stdout,
        format!("{}\n", expected.display()).as_bytes()
    );
}

#[test]
fn test_file_touch_explicit_nested_absolute_and_parent_paths() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().join("cwd");
    fs::create_dir(&cwd).unwrap();

    ragtag()
        .current_dir(&cwd)
        .args(["file", "touch", "--path", "nested/deep/note.md"])
        .assert()
        .success()
        .stdout(format!("{}\n", cwd.join("nested/deep/note.md").display()));
    assert!(cwd.join("nested/deep/note.md").is_file());

    let external = dir.path().join("external/path/note.md");
    ragtag()
        .current_dir(&cwd)
        .args(["file", "touch", "--path", external.to_str().unwrap()])
        .assert()
        .success()
        .stdout(format!("{}\n", external.display()));
    assert!(external.is_file());

    ragtag()
        .current_dir(&cwd)
        .args(["file", "touch", "--path", "../parent.md"])
        .assert()
        .success()
        .stdout(format!("{}\n", cwd.join("../parent.md").display()));
    assert!(dir.path().join("parent.md").is_file());
}

#[test]
fn test_file_touch_tags_preserve_order_spelling_and_exact_deduplication() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("tags.md");
    ragtag()
        .args([
            "file",
            "touch",
            "--path",
            target.to_str().unwrap(),
            "--tag",
            " todo ",
            "--tag",
            "@task(owner=\"A B\", priority=1)",
            "--tag",
            "@todo",
            "--tag",
            "@task(owner=\"C\", priority=1)",
        ])
        .assert()
        .success();
    assert_eq!(
        fs::read(&target).unwrap(),
        b"@todo\n@task(owner=\"A B\", priority=1)\n@task(owner=\"C\", priority=1)\n"
    );
}

#[test]
fn test_file_touch_hyphen_leading_tags_are_repeatable_before_adjacent_flags() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("hyphen-tags.md");
    ragtag()
        .args([
            "file",
            "touch",
            "--tag",
            "-todo",
            "--tag",
            "@-doing",
            "--path",
            target.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert_eq!(fs::read(target).unwrap(), b"@-todo\n@-doing\n");

    let edit_target = dir.path().join("edit-after-hyphen-tags.md");
    ragtag()
        .args([
            "file",
            "touch",
            "--tag",
            "-todo",
            "--tag",
            "@-doing",
            "--edit",
            "--path",
            edit_target.to_str().unwrap(),
        ])
        .env("EDITOR", " ")
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("invalid EDITOR"));
    assert!(!edit_target.exists());
}

#[test]
fn test_file_touch_invalid_tag_has_no_filesystem_effects() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("uncreated/invalid.md");
    ragtag()
        .args([
            "file",
            "touch",
            "--path",
            target.to_str().unwrap(),
            "--tag",
            "@one trailing prose",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid tag"));
    assert!(!dir.path().join("uncreated").exists());
}

#[test]
fn test_file_touch_invalid_terminal_components_have_no_filesystem_effects() {
    let dir = tempfile::tempdir().unwrap();
    for path in ["new-dot/.", "new-parent/..", "new-trailing/"] {
        ragtag()
            .current_dir(dir.path())
            .args(["file", "touch", "--path", path])
            .assert()
            .failure()
            .stderr(predicate::str::contains("usable filename"));
    }
    assert!(!dir.path().join("new-dot").exists());
    assert!(!dir.path().join("new-parent").exists());
    assert!(!dir.path().join("new-trailing").exists());
}

#[cfg(unix)]
#[test]
fn test_file_touch_allows_literal_backslashes_before_terminal_dots() {
    let dir = tempfile::tempdir().unwrap();
    for path in [r"note\.", r"note\.."] {
        ragtag()
            .current_dir(dir.path())
            .args(["file", "touch", "--path", path])
            .assert()
            .success();
        assert!(dir.path().join(path).is_file());
    }
}

#[test]
fn test_file_touch_rejects_existing_file_and_directory_without_changes() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("existing.md");
    fs::write(&file, b"original").unwrap();
    ragtag()
        .args([
            "file",
            "touch",
            "--path",
            file.to_str().unwrap(),
            "--tag",
            "replacement",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("target already exists"));
    assert_eq!(fs::read(&file).unwrap(), b"original");
    ragtag()
        .env("EDITOR", "/bin/false")
        .args(["file", "touch", "--edit", "--path", file.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("target already exists"));
    assert_eq!(fs::read(&file).unwrap(), b"original");

    let directory = dir.path().join("existing-directory");
    fs::create_dir(&directory).unwrap();
    ragtag()
        .args(["file", "touch", "--path", directory.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("target already exists"));
    assert!(directory.is_dir());
}

#[cfg(unix)]
#[test]
fn test_file_touch_rejects_existing_symlink_and_dangling_symlink() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("original.md");
    fs::write(&original, b"original").unwrap();
    for (name, destination) in [
        ("link.md", original.clone()),
        ("dangling.md", dir.path().join("missing.md")),
    ] {
        let link = dir.path().join(name);
        symlink(destination, &link).unwrap();
        ragtag()
            .args(["file", "touch", "--path", link.to_str().unwrap()])
            .assert()
            .failure()
            .stderr(predicate::str::contains("target already exists"));
        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
    }
    assert_eq!(fs::read(original).unwrap(), b"original");
}

#[test]
fn test_file_touch_same_generated_name_collision_fails_without_suffix() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join(".ragtag.yaml");
    fs::write(
        &config,
        "files:\n  default_directory: notes\n  filename_format: \"fixed.md\"\n",
    )
    .unwrap();
    let args = ["--config", config.to_str().unwrap(), "file", "touch"];
    ragtag().args(args).assert().success();
    ragtag()
        .args(args)
        .assert()
        .failure()
        .stderr(predicate::str::contains("target already exists"));
    let entries = fs::read_dir(dir.path().join("notes"))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].file_name(), "fixed.md");
}

#[test]
fn test_file_touch_editor_is_opt_in_and_prevalidated() {
    let dir = tempfile::tempdir().unwrap();
    let no_edit = dir.path().join("no-edit.md");
    ragtag()
        .env_remove("EDITOR")
        .args(["file", "touch", "--path", no_edit.to_str().unwrap()])
        .assert()
        .success();
    assert!(no_edit.is_file());

    for (parent, editor) in [
        ("unset", None),
        ("blank", Some("   ")),
        ("malformed", Some("'unterminated")),
        ("empty-program", Some("''")),
        ("empty-program-with-argument", Some("'' --wait")),
    ] {
        let parent = dir.path().join(parent);
        let target = parent.join("note.md");
        let mut command = ragtag();
        command.args([
            "file",
            "touch",
            "--edit",
            "--path",
            target.to_str().unwrap(),
        ]);
        match editor {
            Some(value) => {
                command.env("EDITOR", value);
            }
            None => {
                command.env_remove("EDITOR");
            }
        }
        command
            .assert()
            .failure()
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains("invalid EDITOR"));
        assert!(!parent.exists());
    }
}

#[cfg(unix)]
#[test]
fn test_file_touch_editor_success_failure_and_retention() {
    let dir = tempfile::tempdir().unwrap();
    let success = dir.path().join("success.md");
    ragtag()
        .env("EDITOR", "/usr/bin/test -f")
        .args([
            "file",
            "touch",
            "--edit",
            "--path",
            success.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(format!("{}\n", success.display()));
    assert!(success.is_file());

    let cwd = dir.path().join("cwd");
    fs::create_dir(&cwd).unwrap();
    let lexical_target = cwd.join("../lexical.md");
    ragtag()
        .current_dir(&cwd)
        .env(
            "EDITOR",
            "/bin/sh -c 'test \"$0\" = \"quoted arg\" && test \"$1\" = \"--ordered\" && test \"$2\" = \"$EXPECTED_TARGET\"' 'quoted arg' --ordered",
        )
        .env("EXPECTED_TARGET", lexical_target.as_os_str())
        .args(["file", "touch", "--edit", "--path", "../lexical.md"])
        .assert()
        .success()
        .stdout(format!("{}\n", lexical_target.display()));
    assert!(dir.path().join("lexical.md").is_file());

    let nonzero = dir.path().join("nonzero.md");
    ragtag()
        .env("EDITOR", "/bin/false")
        .args([
            "file",
            "touch",
            "--edit",
            "--path",
            nonzero.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("created file remains"));
    assert!(nonzero.is_file());

    let missing = dir.path().join("missing-editor.md");
    ragtag()
        .env("EDITOR", "/definitely/missing/editor")
        .args([
            "file",
            "touch",
            "--edit",
            "--path",
            missing.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("failed to launch editor"))
        .stderr(predicate::str::contains("created file remains"));
    assert!(missing.is_file());

    let signaled = dir.path().join("signaled.md");
    ragtag()
        .env("EDITOR", "/bin/sh -c 'kill -TERM $$'")
        .args([
            "file",
            "touch",
            "--edit",
            "--path",
            signaled.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("editor exited unsuccessfully"))
        .stderr(predicate::str::contains("created file remains"));
    assert!(signaled.is_file());
}

#[test]
fn test_file_touch_alias_collision_and_expansion_with_trailing_options() {
    let dir = tempfile::tempdir().unwrap();
    let collision = dir.path().join("collision.yaml");
    fs::write(
        &collision,
        "aliases:\n  - name: file\n    arguments: \"summary\"\n",
    )
    .unwrap();
    ragtag()
        .args([
            "--config",
            collision.to_str().unwrap(),
            "summary",
            "--path",
            &fixtures_dir(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("collides"));

    let alias_config = dir.path().join("alias.yaml");
    fs::write(
        &alias_config,
        "aliases:\n  - name: new-note\n    arguments: \"file touch\"\n",
    )
    .unwrap();
    let target = dir.path().join("alias-created.md");
    ragtag()
        .args([
            "--config",
            alias_config.to_str().unwrap(),
            "new-note",
            "--tag",
            "aliased",
            "--path",
            target.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert_eq!(fs::read(target).unwrap(), b"@aliased\n");
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

#[test]
fn test_query_help_documents_limit_and_randomize() {
    ragtag()
        .args(["query", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--limit <INTEGER>"))
        .stdout(predicate::str::contains("--randomize [<SEED>]"))
        .stdout(predicate::str::contains(
            "With no SEED, uses fresh system randomness",
        ))
        .stdout(predicate::str::contains(
            "place TAG_NAME before --randomize or after --",
        ));
}

#[test]
fn test_query_limit_truncates_output_and_count_and_zero_is_empty() {
    let path = format!("{}/simple_tags.txt", fixtures_dir());

    let limited = ragtag()
        .args([
            "--no-color",
            "query",
            "tag",
            "--path",
            &path,
            "--limit",
            "2",
        ])
        .output()
        .unwrap();
    assert!(limited.status.success());
    assert_eq!(
        String::from_utf8(limited.stdout).unwrap().lines().count(),
        2
    );

    ragtag()
        .args(["query", "tag", "--path", &path, "--limit", "2", "--count"])
        .assert()
        .success()
        .stdout("2\n");

    ragtag()
        .args(["query", "tag", "--path", &path, "--limit", "0"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
    ragtag()
        .args(["query", "tag", "--path", &path, "--count", "--limit", "0"])
        .assert()
        .success()
        .stdout("0\n");
}

#[test]
fn test_query_limit_rejects_negative_and_non_integer_values() {
    for invalid in ["-1", "one", "1.5"] {
        ragtag()
            .args(["query", "tag", "--limit", invalid])
            .assert()
            .failure()
            .stderr(predicate::str::contains("invalid value"));
    }
}

#[test]
fn test_query_limit_missing_value_does_not_consume_following_options() {
    let following_options: &[&[&str]] = &[
        &["--count"],
        &["--randomize"],
        &["--help"],
        &["--path", "somewhere"],
        &["--filter", "key=value"],
    ];

    for options in following_options {
        ragtag()
            .args(["query", "tag", "--limit"])
            .args(*options)
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "a value is required for '--limit <INTEGER>'",
            ))
            .stderr(predicate::str::contains("invalid value").not());
    }
}

#[test]
fn test_query_randomize_preserves_all_matching_results() {
    let path = format!("{}/simple_tags.txt", fixtures_dir());
    let baseline = ragtag()
        .args(["--no-color", "query", "tag", "--path", &path])
        .output()
        .unwrap();
    let randomized = ragtag()
        .args(["--no-color", "query", "tag", "--path", &path, "--randomize"])
        .output()
        .unwrap();
    assert!(baseline.status.success());
    assert!(randomized.status.success());

    let mut baseline_lines: Vec<_> = String::from_utf8(baseline.stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    let mut randomized_lines: Vec<_> = String::from_utf8(randomized.stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    baseline_lines.sort_unstable();
    randomized_lines.sort_unstable();
    assert_eq!(randomized_lines, baseline_lines);
}

#[test]
fn test_query_seeded_randomize_forms_and_limits_are_deterministic() {
    let path = format!("{}/simple_tags.txt", fixtures_dir());
    let spaced = ragtag()
        .args([
            "--no-color",
            "query",
            "tag",
            "--path",
            &path,
            "--randomize",
            "42",
        ])
        .output()
        .unwrap();
    let equals = ragtag()
        .args([
            "--no-color",
            "query",
            "tag",
            "--path",
            &path,
            "--randomize=42",
        ])
        .output()
        .unwrap();
    let seed_before_query = ragtag()
        .args([
            "--no-color",
            "query",
            "--randomize",
            "42",
            "tag",
            "--path",
            &path,
        ])
        .output()
        .unwrap();

    assert!(spaced.status.success());
    assert_output_equivalent(&equals, &spaced);
    assert_output_equivalent(&seed_before_query, &spaced);

    let full_lines = String::from_utf8(spaced.stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    for options in [
        ["--randomize=42", "--limit", "2"],
        ["--limit", "2", "--randomize=42"],
    ] {
        let output = ragtag()
            .args(["--no-color", "query", "tag", "--path", &path])
            .args(options)
            .output()
            .unwrap();
        assert!(output.status.success());
        let lines: Vec<_> = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect();
        assert_eq!(lines, full_lines[..2]);
    }
}

#[test]
fn test_query_seeded_randomize_accepts_u64_boundaries() {
    let path = format!("{}/simple_tags.txt", fixtures_dir());
    for seed in ["0", "18446744073709551615"] {
        let first = ragtag()
            .args([
                "--no-color",
                "query",
                "tag",
                "--path",
                &path,
                "--randomize",
                seed,
            ])
            .output()
            .unwrap();
        let second = ragtag()
            .args([
                "--no-color",
                "query",
                "tag",
                "--path",
                &path,
                &format!("--randomize={seed}"),
            ])
            .output()
            .unwrap();
        assert!(first.status.success());
        assert_output_equivalent(&second, &first);
    }
}

#[test]
fn test_query_randomize_rejects_invalid_seeds_with_ambiguity_guidance() {
    for invalid in ["-1", "18446744073709551616", "1.5", "not-a-seed"] {
        ragtag()
            .args(["query", "tag", "--randomize", invalid])
            .assert()
            .failure()
            .stderr(predicate::str::contains("is not an unsigned 64-bit seed"))
            .stderr(predicate::str::contains(
                "place it before --randomize or after --",
            ));
    }
}

#[test]
fn test_query_unseeded_randomize_handles_options_and_positional_boundaries() {
    let path = format!("{}/simple_tags.txt", fixtures_dir());
    ragtag()
        .args(["query", "tag", "--randomize", "--path", &path, "--count"])
        .assert()
        .success()
        .stdout("3\n");
    ragtag()
        .args([
            "query",
            "tag",
            "--randomize",
            "--limit",
            "2",
            "--count",
            "--path",
            &path,
        ])
        .assert()
        .success()
        .stdout("2\n");
    ragtag()
        .args([
            "query",
            "tag",
            "--randomize",
            "--filter",
            "key=value",
            "--path",
            &path,
            "--count",
        ])
        .assert()
        .success()
        .stdout("1\n");
    ragtag()
        .args(["query", "tag", "--randomize", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--randomize [<SEED>]"));

    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("tags.md"), "@tag(id=1)\n").unwrap();
    ragtag()
        .current_dir(directory.path())
        .args(["query", "--randomize", "--", "tag"])
        .assert()
        .success()
        .stdout(predicate::str::contains("@tag(id=1)"));

    ragtag()
        .args(["query", "--randomize", "tag"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "\"tag\" is not an unsigned 64-bit seed",
        ))
        .stderr(predicate::str::contains(
            "place it before --randomize or after --",
        ));
}

#[test]
fn test_query_randomize_rejects_repeated_occurrences() {
    for arguments in [
        vec!["query", "tag", "--randomize", "--randomize"],
        vec!["query", "tag", "--randomize=1", "--randomize=2"],
    ] {
        ragtag()
            .args(arguments)
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "the argument '--randomize [<SEED>]' cannot be used multiple times",
            ));
    }
}

#[test]
fn test_query_limit_and_seeded_randomize_apply_before_all_output_modes() {
    let path = format!("{}/tasks.md", fixtures_dir());
    let first = ragtag()
        .args([
            "--no-color",
            "query",
            "task",
            "--path",
            &path,
            "--randomize=42",
            "--limit",
            "1",
        ])
        .output()
        .unwrap();
    let second = ragtag()
        .args([
            "--no-color",
            "query",
            "task",
            "--path",
            &path,
            "--randomize",
            "42",
            "--limit",
            "1",
        ])
        .output()
        .unwrap();
    assert!(first.status.success());
    assert_output_equivalent(&second, &first);
    assert_eq!(String::from_utf8(first.stdout).unwrap().lines().count(), 1);

    ragtag()
        .args([
            "query",
            "task",
            "--path",
            &path,
            "--randomize=42",
            "--limit",
            "1",
            "--count",
        ])
        .assert()
        .success()
        .stdout("1\n");
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
        .stdout(predicate::str::contains("type=\"item\""))
        .stdout(predicate::str::contains("worktime_estimate=4"));
}

#[test]
fn test_tasks_create_empty_title_enters_interactive_mode() {
    // `--title ""` must be treated the same as omitting --title and route to
    // interactive mode.  We supply the title via stdin to confirm that the
    // interactive prompt was actually reached and used.
    ragtag()
        .args(["task", "create", "--title", ""])
        // Provide title via stdin; remaining optional fields left blank.
        .write_stdin("Interactive Title\n\n\n\n\n\n\n\n\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("title=\"Interactive Title\""));
}

#[test]
fn test_tasks_create_whitespace_title_enters_interactive_mode() {
    // `--title "   "` (whitespace-only) must also fall through to interactive mode.
    ragtag()
        .args(["task", "create", "--title", "   "])
        .write_stdin("Whitespace Title\n\n\n\n\n\n\n\n\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("title=\"Whitespace Title\""));
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
        .args(["task", "create", "--title", "Timestamped Task"])
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
    assert_eq!(
        file_content, original,
        "File should be unchanged with --no-edit"
    );
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
        .args(["--no-color", "task", "summary", "--all", "--path", &path])
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

#[test]
fn test_task_summary_filter_spaced_operators() {
    // Spaces around comparison operators inside a boolean filter expression
    // are accepted.
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"aaa1234567890ab\", title=\"Alice Active\", status=\"active\", owner=\"alice\", priority=1, worktime_estimate=1, worktime_units=\"hours\")\n\
         @task(id=\"bbb1234567890ab\", title=\"Bob Zero\", status=\"blocked\", owner=\"bob\", priority=0, worktime_estimate=2, worktime_units=\"hours\")\n\
         @task(id=\"ccc1234567890ab\", title=\"Carol Done\", status=\"done\", owner=\"carol\", priority=4, worktime_estimate=3, worktime_units=\"hours\")",
    ).unwrap();

    // A boolean expression with spaces around operators.
    ragtag()
        .args([
            "--no-color",
            "task",
            "summary",
            "--format",
            "table",
            "--filter",
            "(status = active OR priority = 0) AND (status != done OR status != inactive)",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        // Alice: status=active AND not done -> matches.
        .stdout(predicate::str::contains("Alice Active"))
        // Bob: priority=0 AND not done -> matches.
        .stdout(predicate::str::contains("Bob Zero"))
        // Carol: status=done and priority!=0 -> left group false -> excluded.
        .stdout(predicate::str::contains("Carol Done").not());
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
            "prefixtest", // prefix only
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
        .stderr(
            predicate::str::contains("task not found").or(predicate::str::contains("not found")),
        );
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
    assert_eq!(
        file_content, original,
        "File should be unchanged with --no-edit"
    );
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
        .stderr(
            predicate::str::contains("task not found").or(predicate::str::contains("not found")),
        );
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
    assert_eq!(
        file_content, original,
        "File should be unchanged with --no-edit"
    );
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
        .stderr(
            predicate::str::contains("task not found").or(predicate::str::contains("not found")),
        );
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
    assert_eq!(
        file_content, original,
        "File should be unchanged with --no-edit"
    );
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
        .stderr(
            predicate::str::contains("task not found").or(predicate::str::contains("not found")),
        );
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
    assert_eq!(
        file_content, original,
        "File should be unchanged with --no-edit"
    );
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
        .stderr(
            predicate::str::contains("task not found").or(predicate::str::contains("not found")),
        );
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
        .stdout(predicate::str::contains("Design API"))
        .stdout(predicate::str::contains("@task").not())
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
    assert!(
        out.contains("priority=5"),
        "stdout should have priority=5: {out}"
    );
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
    assert!(
        err.contains("hours"),
        "stderr should list allowed units: {err}"
    );
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

// === task prioritize ===

#[test]
fn test_task_prioritize_sets_priority() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"priotest1234567a\", title=\"Priority Task\", worktime_estimate=2, worktime_units=\"hours\", status=\"active\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "prioritize",
            "3",
            "priotest1234567a",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Prioritized task"))
        .stdout(predicate::str::contains("priotest1234567a"))
        .stdout(predicate::str::contains("3"));

    let content = fs::read_to_string(&file).unwrap();
    assert!(
        content.contains("priority=3"),
        "Expected priority=3 in file after prioritize, got:\n{content}"
    );
}

#[test]
fn test_task_prioritize_auto_updates_time_last_updated() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    // Task does NOT have time_last_updated — it should be added.
    fs::write(
        &file,
        "@task(id=\"priotest2345678b\", title=\"Add timestamp\", worktime_estimate=1, worktime_units=\"hours\", status=\"new\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "prioritize",
            "0",
            "priotest2345678b",
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
    assert!(
        content.contains("priority=0"),
        "Expected priority=0 in file, got:\n{content}"
    );
}

#[test]
fn test_task_prioritize_no_edit() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    let original =
        "@task(id=\"priotest3456789c\", title=\"No edit\", worktime_estimate=1, worktime_units=\"hours\", status=\"new\")";
    fs::write(&file, original).unwrap();

    let output = ragtag()
        .args([
            "--no-color",
            "task",
            "prioritize",
            "5",
            "priotest3456789c",
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
        output_str.contains("priority=5"),
        "Expected priority=5 in --no-edit output, got:\n{output_str}"
    );
    assert!(
        output_str.contains("time_last_updated=\"20"),
        "Expected auto-populated time_last_updated in --no-edit output, got:\n{output_str}"
    );

    let file_content = fs::read_to_string(&file).unwrap();
    assert_eq!(
        file_content, original,
        "File should be unchanged with --no-edit"
    );
}

#[test]
fn test_task_prioritize_invalid_priority_non_numeric() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"priotest4567890d\", title=\"Bad priority\", worktime_estimate=1, worktime_units=\"hours\", status=\"new\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "prioritize",
            "abc",
            "priotest4567890d",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid priority"));
}

#[test]
fn test_task_prioritize_invalid_priority_negative() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"priotest5678901e\", title=\"Negative priority\", worktime_estimate=1, worktime_units=\"hours\", status=\"new\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "prioritize",
            "-1",
            "priotest5678901e",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .failure();
}

#[test]
fn test_task_prioritize_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"priotest6789012f\", title=\"Existing\", worktime_estimate=1, worktime_units=\"hours\", status=\"new\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "prioritize",
            "1",
            "nonexistentid999",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("task not found"));
}

#[test]
fn test_task_prioritize_prefix_match() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.md");
    fs::write(
        &file,
        "@task(id=\"priopfx7890123ab\", title=\"Prefix test\", worktime_estimate=1, worktime_units=\"hours\", status=\"active\")",
    )
    .unwrap();

    ragtag()
        .args([
            "--no-color",
            "task",
            "prioritize",
            "2",
            "priopfx",
            "--path",
            file.to_str().unwrap(),
        ])
        .assert()
        .success();

    let content = fs::read_to_string(&file).unwrap();
    assert!(
        content.contains("priority=2"),
        "Prefix match should have set priority=2: {content}"
    );
}

#[test]
fn test_tasks_help_includes_prioritize() {
    ragtag()
        .args(["task", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("prioritize"));
}

// === Infer Subcommands (Prefix Matching) ===

/// `ragtag su` should resolve unambiguously to `ragtag summary`.
#[test]
fn test_prefix_ragtag_su_resolves_to_summary() {
    ragtag()
        .args(["su", "--path", &fixtures_dir()])
        .assert()
        .success()
        .stdout(predicate::str::contains("task")); // summary output always lists tags
}

/// `ragtag q` should resolve unambiguously to `ragtag query`.
#[test]
fn test_prefix_ragtag_q_resolves_to_query() {
    ragtag()
        .args(["q", "--path", &fixtures_dir()])
        .assert()
        .success();
}

/// `ragtag t l` should resolve to `ragtag task list`.
#[test]
fn test_prefix_ragtag_t_l_resolves_to_task_list() {
    ragtag()
        .args(["--no-color", "t", "l", "--path", &fixtures_dir()])
        .assert()
        .success();
}

/// `ragtag task su` should resolve unambiguously to `ragtag task summary`.
#[test]
fn test_prefix_task_su_resolves_to_task_summary() {
    ragtag()
        .args(["--no-color", "task", "su", "--path", &fixtures_dir()])
        .assert()
        .success();
}

/// `ragtag task p` should resolve unambiguously to `ragtag task prioritize`.
/// It requires arguments so we just verify the error is about missing args, not unknown subcommand.
#[test]
fn test_prefix_task_p_resolves_to_prioritize_not_unknown() {
    let output = ragtag().args(["task", "p"]).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should not say "unknown subcommand" — clap resolved it but found missing required args.
    assert!(
        !stderr.contains("unrecognized subcommand") && !stderr.contains("unknown subcommand"),
        "Expected prefix 'p' to resolve to 'prioritize', but got: {stderr}"
    );
}

/// `ragtag task c` is ambiguous between `create` and `complete` — clap should error.
#[test]
fn test_prefix_task_c_is_ambiguous() {
    ragtag().args(["task", "c"]).assert().failure().stderr(
        predicate::str::contains("ambiguous")
            .or(predicate::str::contains("create").and(predicate::str::contains("complete"))),
    );
}

/// `ragtag task a` is ambiguous between `activate` and `abandon` — clap should error.
#[test]
fn test_prefix_task_a_is_ambiguous() {
    ragtag().args(["task", "a"]).assert().failure().stderr(
        predicate::str::contains("ambiguous")
            .or(predicate::str::contains("activate").and(predicate::str::contains("abandon"))),
    );
}

/// `ragtag config g` should resolve unambiguously to `ragtag config get`.
/// It requires a key argument, so verify the error is about missing args, not unknown subcommand.
#[test]
fn test_prefix_config_g_resolves_to_get_not_unknown() {
    let output = ragtag().args(["config", "g"]).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unrecognized subcommand") && !stderr.contains("unknown subcommand"),
        "Expected prefix 'g' to resolve to 'get', but got: {stderr}"
    );
}

// === Task Time (relative adjustments) ===

#[test]
fn test_task_time_set_absolute() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("tasks.md");
    fs::write(
        &file,
        "@task(id=\"timeset123456789\", title=\"Time Test\", worktime_estimate=4, worktime_units=\"hours\", status=\"active\")",
    )
    .unwrap();

    ragtag()
        .args([
            "task",
            "time",
            "3.5",
            "timeset123456789",
            "--path",
            dir.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("worktime_spent → 3.5"));

    let updated = fs::read_to_string(&file).unwrap();
    assert!(
        updated.contains("worktime_spent=3.5"),
        "Expected worktime_spent=3.5, got: {updated}"
    );
}

#[test]
fn test_task_time_add_relative() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("tasks.md");
    fs::write(
        &file,
        "@task(id=\"timeadd123456789\", title=\"Time Test\", worktime_spent=2, worktime_estimate=4, worktime_units=\"hours\", status=\"active\")",
    )
    .unwrap();

    ragtag()
        .args([
            "task",
            "time",
            "+1.5",
            "timeadd123456789",
            "--path",
            dir.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("worktime_spent → 3.5"));

    let updated = fs::read_to_string(&file).unwrap();
    assert!(
        updated.contains("worktime_spent=3.5"),
        "Expected worktime_spent=3.5, got: {updated}"
    );
}

#[test]
fn test_task_time_subtract_relative() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("tasks.md");
    fs::write(
        &file,
        "@task(id=\"timesub123456789\", title=\"Time Test\", worktime_spent=5, worktime_estimate=4, worktime_units=\"hours\", status=\"active\")",
    )
    .unwrap();

    ragtag()
        .args([
            "task",
            "time",
            "-2",
            "timesub123456789",
            "--path",
            dir.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("worktime_spent → 3"));

    let updated = fs::read_to_string(&file).unwrap();
    assert!(
        updated.contains("worktime_spent=3"),
        "Expected worktime_spent=3, got: {updated}"
    );
}

#[test]
fn test_task_time_subtract_clamps_to_zero() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("tasks.md");
    fs::write(
        &file,
        "@task(id=\"timeclamp1234567\", title=\"Time Test\", worktime_spent=1, worktime_estimate=4, worktime_units=\"hours\", status=\"active\")",
    )
    .unwrap();

    ragtag()
        .args([
            "task",
            "time",
            "-10",
            "timeclamp1234567",
            "--path",
            dir.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("worktime_spent → 0"));

    let updated = fs::read_to_string(&file).unwrap();
    assert!(
        updated.contains("worktime_spent=0"),
        "Expected worktime_spent=0, got: {updated}"
    );
}

#[test]
fn test_task_time_add_when_no_prior_worktime() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("tasks.md");
    fs::write(
        &file,
        "@task(id=\"timenone12345678\", title=\"Time Test\", worktime_estimate=4, worktime_units=\"hours\", status=\"active\")",
    )
    .unwrap();

    ragtag()
        .args([
            "task",
            "time",
            "+2",
            "timenone12345678",
            "--path",
            dir.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("worktime_spent → 2"));

    let updated = fs::read_to_string(&file).unwrap();
    assert!(
        updated.contains("worktime_spent=2"),
        "Expected worktime_spent=2, got: {updated}"
    );
}

#[test]
fn test_task_time_no_edit_prints_tag_without_modifying_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("tasks.md");
    let original =
        "@task(id=\"timenoedit123456\", title=\"Time Test\", worktime_spent=1, worktime_estimate=4, worktime_units=\"hours\", status=\"active\")";
    fs::write(&file, original).unwrap();

    let output = ragtag()
        .args([
            "task",
            "time",
            "+2",
            "timenoedit123456",
            "--path",
            dir.path().to_str().unwrap(),
            "--no-edit",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8(output).unwrap();
    assert!(
        output_str.contains("worktime_spent=3"),
        "Expected worktime_spent=3 in --no-edit output, got: {output_str}"
    );
    assert!(
        output_str.contains("@task("),
        "Expected @task tag in --no-edit output, got: {output_str}"
    );

    // File must remain unchanged.
    let content = fs::read_to_string(&file).unwrap();
    assert_eq!(
        content, original,
        "File should not be modified with --no-edit"
    );
}

#[test]
fn test_task_time_rejects_abc() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("tasks.md");
    fs::write(
        &file,
        "@task(id=\"timeerr123456789\", title=\"Time Test\", worktime_estimate=4, worktime_units=\"hours\", status=\"active\")",
    )
    .unwrap();

    ragtag()
        .args([
            "task",
            "time",
            "abc",
            "timeerr123456789",
            "--path",
            dir.path().to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid worktime_spent"));
}

#[test]
fn test_task_time_rejects_nan() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("tasks.md");
    fs::write(
        &file,
        "@task(id=\"timenan123456789\", title=\"Time Test\", worktime_estimate=4, worktime_units=\"hours\", status=\"active\")",
    )
    .unwrap();

    ragtag()
        .args([
            "task",
            "time",
            "NaN",
            "timenan123456789",
            "--path",
            dir.path().to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid worktime_spent"));
}

#[test]
fn test_task_time_rejects_inf() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("tasks.md");
    fs::write(
        &file,
        "@task(id=\"timeinf123456789\", title=\"Time Test\", worktime_estimate=4, worktime_units=\"hours\", status=\"active\")",
    )
    .unwrap();

    ragtag()
        .args([
            "task",
            "time",
            "inf",
            "timeinf123456789",
            "--path",
            dir.path().to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid worktime_spent"));
}

// === Aliases ===

/// Writes a config file containing the given YAML into a temp dir and returns
/// (the TempDir guard, the config file path as a String).
fn alias_config(yaml: &str) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".ragtag.yaml");
    fs::write(&path, yaml).unwrap();
    let path_str = path.to_str().unwrap().to_string();
    (dir, path_str)
}

#[test]
fn test_terminal_command_values_cannot_redirect_startup_config() {
    let dir = tempfile::tempdir().unwrap();
    let first_target = dir.path().join("first.md");
    ragtag()
        .env_remove("RAGTAG_CONFIG")
        .current_dir(dir.path())
        .args(["file", "touch", "--path"])
        .arg(&first_target)
        .args(["--tag", "--config=/definitely/not/selected.yaml"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid tag"))
        .stderr(predicate::str::contains("config file not found").not());
    assert!(!first_target.exists());

    let second_target = dir.path().join("second.md");
    ragtag()
        .env_remove("RAGTAG_CONFIG")
        .current_dir(dir.path())
        .args(["file", "touch", "--tag", "--config", "--path"])
        .arg(&second_target)
        .assert()
        .code(0)
        .stdout(predicate::str::contains(
            second_target.display().to_string(),
        ))
        .stderr(predicate::str::contains("config file not found").not());
    assert_eq!(fs::read_to_string(&second_target).unwrap(), "@--config\n");

    ragtag()
        .env_remove("RAGTAG_CONFIG")
        .current_dir(dir.path())
        .args([
            "task",
            "set-attr",
            "a1b2c3d4e5f67890",
            "owner",
            "--config=/definitely/not/selected.yaml",
            "--path",
            &fixtures_dir(),
            "--no-edit",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "required arguments were not provided",
        ))
        .stderr(predicate::str::contains("config file not found").not());
}

#[test]
fn test_root_config_selection_and_terminal_reconciliation() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.yaml");
    let second = dir.path().join("second.yaml");
    fs::write(&first, "output:\n  color: always\n").unwrap();
    fs::write(&second, "output:\n  color: never\n").unwrap();
    let equals = format!("--config={}", second.display());

    ragtag()
        .env_remove("RAGTAG_CONFIG")
        .args(["--config"])
        .arg(&first)
        .args(["config", "get", "output.color"])
        .assert()
        .code(0)
        .stdout("always\n");
    ragtag()
        .env_remove("RAGTAG_CONFIG")
        .arg(&equals)
        .args(["config", "get", "output.color"])
        .assert()
        .code(0)
        .stdout("never\n");

    ragtag()
        .env("RAGTAG_CONFIG", &first)
        .args(["config", "get", "output.color", "--config"])
        .arg(&second)
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "--config must appear before the command name",
        ));
}

#[cfg(unix)]
#[test]
fn test_unbounded_special_file_is_rejected_as_config() {
    if !std::path::Path::new("/dev/zero").exists() {
        return;
    }

    ragtag()
        .args(["--config", "/dev/zero", "summary"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "config file \"/dev/zero\" must be a regular file",
        ));
}

/// Runs ragtag with owned arguments and an optional startup config environment.
fn alias_case_output(args: &[String], config_environment: Option<&str>) -> std::process::Output {
    let mut command = ragtag();
    command.env_remove("RAGTAG_CONFIG");
    if let Some(config) = config_environment {
        command.env("RAGTAG_CONFIG", config);
    }
    command.args(args).output().unwrap()
}

/// Compares an alias invocation with its explicit terminal argv.
fn assert_alias_case_equivalent(
    actual_args: &[String],
    expected_args: &[String],
    config_environment: Option<&str>,
) {
    let actual = alias_case_output(actual_args, config_environment);
    let expected = alias_case_output(expected_args, config_environment);
    assert_output_equivalent(&actual, &expected);
}

#[test]
fn test_config_environment_interpolation_reaches_typed_core_and_extension_values() {
    const SECRET_OWNER: &str = "sentinel-config-output-must-not-leak";
    let (_guard, config) = alias_config(
        r#"
ignore_patterns: ["$IGNORE_PATTERN"]
output:
  color: "$COLOR_MODE"
files:
  default_directory: "${NOTES_DIRECTORY}"
  filename_format: "$FILENAME_FORMAT"
tasks:
  default_owner: "$DEFAULT_OWNER"
"#,
    );

    for key in [
        "ignore_patterns",
        "output.color",
        "files.default_directory",
        "files.filename_format",
        "tasks.default_owner",
    ] {
        ragtag()
            .args(["--config", &config, "config", "get", key])
            .env("IGNORE_PATTERN", "target/")
            .env("COLOR_MODE", "never")
            .env("NOTES_DIRECTORY", "notes")
            .env("FILENAME_FORMAT", "fixed.md")
            .env("DEFAULT_OWNER", SECRET_OWNER)
            .assert()
            .success()
            .stdout(predicate::str::contains("<environment-derived>"))
            .stdout(predicate::str::contains("target/").not())
            .stdout(predicate::str::contains(SECRET_OWNER).not());
    }

    let (_guard, config) = alias_config("tasks:\n  default_owner: \"$UNDEFINED_OWNER\"\n");
    ragtag()
        .args(["--config", &config, "config", "get", "tasks.default_owner"])
        .env_remove("UNDEFINED_OWNER")
        .assert()
        .success()
        .stdout("<environment-derived>\n");
}

#[test]
fn test_alias_environment_interpolation_is_deferred_quoted_and_recursive() {
    let notes = tempfile::tempdir().unwrap();
    let spaced = notes.path().join("notes with spaces");
    fs::create_dir(&spaced).unwrap();
    fs::write(
        spaced.join("one.md"),
        "@task(title=\"Alias target\", status=active)\n",
    )
    .unwrap();
    let (_guard, config) = alias_config(
        r#"
aliases:
  - name: dynamic
    arguments: '$INNER --count'
  - name: inner
    arguments: 'query $TAG --path "$NOTES_PATH"'
"#,
    );

    ragtag()
        .args(["--config", &config, "config", "get", "aliases"])
        .env("INNER", "inner")
        .env("TAG", "task")
        .env("NOTES_PATH", &spaced)
        .assert()
        .success()
        .stdout(predicate::str::contains("arguments: '$INNER' --count"))
        .stdout(predicate::str::contains("arguments: query '$TAG'"))
        .stdout(predicate::str::contains(spaced.display().to_string()).not());

    ragtag()
        .args(["--config", &config, "dynamic"])
        .env("INNER", "inner")
        .env("TAG", "task")
        .env("NOTES_PATH", &spaced)
        .assert()
        .success()
        .stdout("1\n");
}

#[test]
fn test_command_line_arguments_are_not_interpolated_by_ragtag() {
    let dir = tempfile::tempdir().unwrap();

    ragtag()
        .current_dir(dir.path())
        .args(["file", "touch", "--path", "$TARGET"])
        .env("TARGET", "expanded.md")
        .assert()
        .success()
        .stdout(format!("{}\n", dir.path().join("$TARGET").display()));

    assert!(dir.path().join("$TARGET").is_file());
    assert!(!dir.path().join("expanded.md").exists());
}

#[test]
fn test_environment_derived_values_are_absent_from_config_errors_and_alias_failures() {
    const SECRET: &str = "sentinel-secret-must-not-leak";

    for (yaml, value) in [
        ("output:\n  color: \"$SECRET_VALUE\"\n", SECRET),
        (
            "files:\n  filename_format: \"$SECRET_VALUE\"\n",
            "sentinel-secret-must-not-leak%",
        ),
        ("tasks:\n  default_status: \"$SECRET_VALUE\"\n", SECRET),
    ] {
        let (_guard, config) = alias_config(yaml);
        ragtag()
            .args(["--config", &config, "--help"])
            .env("SECRET_VALUE", value)
            .assert()
            .code(1)
            .stderr(predicate::str::contains(SECRET).not())
            .stderr(predicate::str::contains("environment").or(predicate::str::contains("config")));
    }

    let (_guard, config) = alias_config(
        "aliases:\n  - name: secret-key\n    arguments: 'config get $SECRET_KEY'\n  - name: secret-option\n    arguments: 'query --randomize=$SECRET_OPTION task'\n",
    );
    for (alias, variable) in [
        ("secret-key", "SECRET_KEY"),
        ("secret-option", "SECRET_OPTION"),
    ] {
        ragtag()
            .args(["--config", &config, alias])
            .env(variable, SECRET)
            .assert()
            .code(1)
            .stderr(predicate::str::contains(
                "command failed after environment interpolation",
            ))
            .stderr(predicate::str::contains(SECRET).not());
    }
}

#[test]
fn test_environment_derived_task_validation_never_prints_resolved_values() {
    const SECRET: &str = "sentinel-task-validation-secret";
    let (_guard, category_config) =
        alias_config("tasks:\n  exclude_status_categories:\n    - \"$SECRET_CATEGORY\"\n");
    let category_output = ragtag()
        .args(["--config", &category_config, "--help"])
        .env("SECRET_CATEGORY", SECRET)
        .env("RUST_LOG", "trace")
        .output()
        .unwrap();
    assert!(category_output.status.success());
    assert!(!String::from_utf8_lossy(&category_output.stdout).contains(SECRET));
    let category_stderr = String::from_utf8_lossy(&category_output.stderr);
    assert!(!category_stderr.contains(SECRET));
    assert!(category_stderr.contains("tasks.exclude_status_categories[0]"));

    let (_guard, status_config) = alias_config("tasks:\n  default_status: \"$SECRET_STATUS\"\n");
    let status_output = ragtag()
        .args(["--config", &status_config, "--help"])
        .env("SECRET_STATUS", SECRET)
        .env("RUST_LOG", "trace")
        .output()
        .unwrap();
    assert!(!status_output.status.success());
    assert!(!String::from_utf8_lossy(&status_output.stdout).contains(SECRET));
    assert!(!String::from_utf8_lossy(&status_output.stderr).contains(SECRET));
}

#[test]
fn test_tasks_config_drives_inspection_and_runtime_behavior() {
    let (_guard, config) = alias_config("tasks:\n  default_owner: canonical-owner\n");

    ragtag()
        .args(["--config", &config, "config", "get", "tasks.default_owner"])
        .assert()
        .success()
        .stdout("canonical-owner\n");

    ragtag()
        .args([
            "--config",
            &config,
            "task",
            "create",
            "--title",
            "Configured owner",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("owner=\"canonical-owner\""));
}

#[test]
fn test_alias_expands_to_same_output() {
    // `ragtag my-alias` must produce byte-identical output to the expanded
    // `ragtag task summary`.
    let (_guard, config) =
        alias_config("aliases:\n  - name: \"my-alias\"\n    arguments: \"task summary\"\n");
    let fixtures = fixtures_dir();

    let expanded = ragtag()
        .env_remove("RAGTAG_CONFIG")
        .args(["--config", &config, "task", "summary", "--path", &fixtures])
        .output()
        .unwrap();

    let aliased = ragtag()
        .env_remove("RAGTAG_CONFIG")
        .args(["--config", &config, "my-alias", "--path", &fixtures])
        .output()
        .unwrap();

    assert!(expanded.status.success());
    assert!(aliased.status.success());
    assert_eq!(aliased.stdout, expanded.stdout);
}

#[test]
fn test_alias_propagates_global_no_color_flag() {
    // A global flag (`--no-color`) must be honored through an alias in both
    // positions, identically to the direct command. `output.color: always`
    // forces color on so its suppression is observable regardless of TTY.
    let (_guard, config) = alias_config(
        "output:\n  color: always\naliases:\n  - name: \"my-alias\"\n    arguments: \"task summary\"\n",
    );
    let fixtures = fixtures_dir();

    // Sanity check: without `--no-color`, the aliased command is colored, so
    // the assertions below are meaningful.
    let colored = ragtag()
        .env_remove("RAGTAG_CONFIG")
        .env_remove("NO_COLOR")
        .args(["--config", &config, "my-alias", "--path", &fixtures])
        .output()
        .unwrap();
    assert!(colored.status.success());
    assert!(String::from_utf8(colored.stdout).unwrap().contains("\x1b["));

    // `--no-color` before the alias name.
    let before = ragtag()
        .env_remove("RAGTAG_CONFIG")
        .env_remove("NO_COLOR")
        .args([
            "--config",
            &config,
            "--no-color",
            "my-alias",
            "--path",
            &fixtures,
        ])
        .output()
        .unwrap();
    assert!(before.status.success());
    assert!(!String::from_utf8(before.stdout).unwrap().contains("\x1b["));

    // `--no-color` after the alias name.
    let after = ragtag()
        .env_remove("RAGTAG_CONFIG")
        .env_remove("NO_COLOR")
        .args([
            "--config",
            &config,
            "my-alias",
            "--no-color",
            "--path",
            &fixtures,
        ])
        .output()
        .unwrap();
    assert!(after.status.success());
    assert!(!String::from_utf8(after.stdout).unwrap().contains("\x1b["));

    // `--no-color` trailing after other alias arguments. clap folds this flag
    // into the alias's trailing args, so it must still be honored to match the
    // fully-expanded direct command.
    let trailing = ragtag()
        .env_remove("RAGTAG_CONFIG")
        .env_remove("NO_COLOR")
        .args([
            "--config",
            &config,
            "my-alias",
            "--path",
            &fixtures,
            "--no-color",
        ])
        .output()
        .unwrap();
    assert!(trailing.status.success());
    assert!(!String::from_utf8(trailing.stdout)
        .unwrap()
        .contains("\x1b["));
}

#[test]
fn test_alias_trailing_args_appended() {
    // `ragtag qt task` must behave like `ragtag query task`.
    let (_guard, config) = alias_config("aliases:\n  - name: \"qt\"\n    arguments: \"query\"\n");
    let fixtures = fixtures_dir();

    let expanded = ragtag()
        .env_remove("RAGTAG_CONFIG")
        .args(["--config", &config, "query", "task", "--path", &fixtures])
        .output()
        .unwrap();

    let aliased = ragtag()
        .env_remove("RAGTAG_CONFIG")
        .args(["--config", &config, "qt", "task", "--path", &fixtures])
        .output()
        .unwrap();

    assert!(expanded.status.success());
    assert!(aliased.status.success());
    assert_eq!(aliased.stdout, expanded.stdout);
}

#[test]
fn test_alias_quoted_arguments() {
    // Shell-like quoting: an alias whose arguments contain a quoted --path
    // value with a trailing slash should still resolve correctly.
    let fixtures = fixtures_dir();
    let yaml = format!(
        "aliases:\n  - name: \"fx\"\n    arguments: \"summary --path \\\"{fixtures}\\\"\"\n"
    );
    let (_guard, config) = alias_config(&yaml);

    let expanded = ragtag()
        .env_remove("RAGTAG_CONFIG")
        .args(["--config", &config, "summary", "--path", &fixtures])
        .output()
        .unwrap();

    let aliased = ragtag()
        .env_remove("RAGTAG_CONFIG")
        .args(["--config", &config, "fx"])
        .output()
        .unwrap();

    assert!(expanded.status.success());
    assert!(aliased.status.success());
    assert_eq!(aliased.stdout, expanded.stdout);
}

#[test]
fn test_alias_prefix_inference() {
    // `ragtag my` should infer the `my-alias` subcommand when unambiguous.
    let (_guard, config) =
        alias_config("aliases:\n  - name: \"my-alias\"\n    arguments: \"task summary\"\n");
    let fixtures = fixtures_dir();

    let inferred = ragtag()
        .env_remove("RAGTAG_CONFIG")
        .args(["--config", &config, "my", "--path", &fixtures])
        .output()
        .unwrap();

    let full = ragtag()
        .env_remove("RAGTAG_CONFIG")
        .args(["--config", &config, "my-alias", "--path", &fixtures])
        .output()
        .unwrap();

    assert!(inferred.status.success());
    assert_eq!(inferred.stdout, full.stdout);
}

#[test]
fn test_alias_ambiguous_prefix_errors() {
    // `sum` is ambiguous between the built-in `summary` and the alias
    // `sumtotal`, so the combined alias resolver reports its typed error.
    let (_guard, config) =
        alias_config("aliases:\n  - name: \"sumtotal\"\n    arguments: \"summary\"\n");

    ragtag()
        .env_remove("RAGTAG_CONFIG")
        .args(["--config", &config, "sum"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("sumtotal"))
        .stderr(predicate::str::contains("summary"));
}

#[test]
fn test_alias_help_target_collision_and_shared_prefixes() {
    let (_target_guard, target_config) =
        alias_config("aliases:\n  - name: show-help\n    arguments: help\n");
    let alias_help = alias_case_output(
        &[
            "--config".to_string(),
            target_config.clone(),
            "show-help".to_string(),
        ],
        None,
    );
    let direct_help = alias_case_output(
        &["--config".to_string(), target_config, "help".to_string()],
        None,
    );
    assert_output_equivalent(&alias_help, &direct_help);
    assert_eq!(alias_help.status.code(), Some(0));

    let (_collision_guard, collision_config) =
        alias_config("aliases:\n  - name: help\n    arguments: summary\n");
    ragtag()
        .args(["--config", &collision_config, "--help"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "alias \"help\" collides with an existing command name",
        ));

    let (_prefix_guard, prefix_config) =
        alias_config("aliases:\n  - name: hello\n    arguments: summary\n");
    for prefix in ["h", "hel"] {
        ragtag()
            .args(["--config", &prefix_config, prefix])
            .assert()
            .code(1)
            .stderr(predicate::str::contains(format!(
                "alias command \"{prefix}\" is ambiguous"
            )))
            .stderr(predicate::str::contains("help, hello"));
    }
}

#[test]
fn test_alias_synonym_prefix_resolves_once_per_definition() {
    let (_guard, config) =
        alias_config("aliases:\n  - names: [active, act]\n    arguments: summary\n");
    let fixtures = fixtures_dir();
    let inferred = alias_case_output(
        &[
            "--config".to_string(),
            config.clone(),
            "ac".to_string(),
            "--path".to_string(),
            fixtures.clone(),
        ],
        None,
    );
    let canonical = alias_case_output(
        &[
            "--config".to_string(),
            config,
            "active".to_string(),
            "--path".to_string(),
            fixtures,
        ],
        None,
    );
    assert_output_equivalent(&inferred, &canonical);
    assert_eq!(inferred.status.code(), Some(0));
}

#[test]
fn test_alias_composes_recursively() {
    let (_guard, config) = alias_config(
        "aliases:\n  - name: \"chain-a\"\n    arguments: \"chain-b --count\"\n  - name: \"chain-b\"\n    arguments: \"chain-c task\"\n  - name: \"chain-c\"\n    arguments: \"query\"\n",
    );
    let fixtures = fixtures_dir();

    let actual = ragtag()
        .env_remove("RAGTAG_CONFIG")
        .args(["--config", &config, "chain-a", "--path", &fixtures])
        .output()
        .unwrap();
    let expected = ragtag()
        .env_remove("RAGTAG_CONFIG")
        .args([
            "--config", &config, "query", "task", "--count", "--path", &fixtures,
        ])
        .output()
        .unwrap();
    assert_output_equivalent(&actual, &expected);
}

#[test]
fn test_alias_collision_with_builtin_is_load_error() {
    let (_guard, config) =
        alias_config("aliases:\n  - name: \"summary\"\n    arguments: \"query\"\n");

    ragtag()
        .env_remove("RAGTAG_CONFIG")
        .args(["--config", &config, "--help"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("collides"))
        .stderr(predicate::str::contains("summary"));
}

#[test]
fn test_alias_collision_with_extension_is_load_error() {
    let (_guard, config) = alias_config("aliases:\n  - name: \"task\"\n    arguments: \"query\"\n");

    ragtag()
        .env_remove("RAGTAG_CONFIG")
        .args(["--config", &config, "summary", "--path", &fixtures_dir()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("collides"))
        .stderr(predicate::str::contains("task"));
}

#[test]
fn test_alias_duplicate_name_is_load_error() {
    let (_guard, config) = alias_config(
        "aliases:\n  - name: \"dup\"\n    arguments: \"summary\"\n  - name: \"dup\"\n    arguments: \"query\"\n",
    );

    ragtag()
        .env_remove("RAGTAG_CONFIG")
        .args(["--config", &config, "--help"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("duplicate alias name"));
}

#[test]
fn test_unknown_command_still_errors_with_aliases_defined() {
    // Defining aliases must not change behavior for genuinely unknown commands.
    let (_guard, config) =
        alias_config("aliases:\n  - name: \"my-alias\"\n    arguments: \"summary\"\n");

    ragtag()
        .env_remove("RAGTAG_CONFIG")
        .args(["--config", &config, "definitely-not-a-command"])
        .assert()
        .failure();
}

#[test]
fn test_aliases_are_absent_from_top_level_help() {
    // Aliases are never registered as clap subcommands, so the top-level help
    // contains only commands from the real command tree.
    let (_guard, config) =
        alias_config("aliases:\n  - name: \"my-alias\"\n    arguments: \"summary\"\n");

    ragtag()
        .env_remove("RAGTAG_CONFIG")
        .args(["--config", &config, "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("my-alias").not());
}

#[test]
fn test_every_extension_alias_synonym_matches_direct_dispatch() {
    let (_guard, config) = alias_config(
        "aliases:\n  - names: [task-view, tv, tasks-now]\n    arguments: \"task summary --all\"\n",
    );
    let fixtures = fixtures_dir();
    let expected = ragtag()
        .env_remove("RAGTAG_CONFIG")
        .args([
            "--config", &config, "task", "summary", "--all", "--path", &fixtures,
        ])
        .output()
        .unwrap();

    for name in ["task-view", "tv", "tasks-now"] {
        let actual = ragtag()
            .env_remove("RAGTAG_CONFIG")
            .args(["--config", &config, name, "--path", &fixtures])
            .output()
            .unwrap();
        assert_output_equivalent(&actual, &expected);
    }
}

#[test]
fn test_alias_leading_global_scanner_preserves_spelling_order_and_boundary() {
    let (_guard, config) =
        alias_config("aliases:\n  - name: a\n    arguments: \"query task --count\"\n");
    let fixtures = fixtures_dir();

    for (actual, expected) in [
        (
            vec![
                "--no-color",
                "--config",
                config.as_str(),
                "a",
                "--path",
                fixtures.as_str(),
            ],
            vec![
                "--no-color",
                "--config",
                config.as_str(),
                "query",
                "task",
                "--count",
                "--path",
                fixtures.as_str(),
            ],
        ),
        (
            vec![
                "--config",
                config.as_str(),
                "--no-color",
                "a",
                "--path",
                fixtures.as_str(),
            ],
            vec![
                "--config",
                config.as_str(),
                "--no-color",
                "query",
                "task",
                "--count",
                "--path",
                fixtures.as_str(),
            ],
        ),
    ] {
        let actual = ragtag()
            .env_remove("RAGTAG_CONFIG")
            .args(actual)
            .output()
            .unwrap();
        let expected = ragtag()
            .env_remove("RAGTAG_CONFIG")
            .args(expected)
            .output()
            .unwrap();
        assert_output_equivalent(&actual, &expected);
    }

    let (_plain_guard, plain_config) = alias_config("");
    let after_boundary =
        alias_case_output(&["--".to_string(), "a".to_string()], Some(config.as_str()));
    let direct = alias_case_output(
        &["--".to_string(), "a".to_string()],
        Some(plain_config.as_str()),
    );
    assert_output_equivalent(&after_boundary, &direct);
    assert_eq!(after_boundary.status.code(), Some(2));
    assert!(String::from_utf8(after_boundary.stderr)
        .unwrap()
        .contains("unrecognized subcommand 'a'"));
}

#[test]
fn test_config_split_form_stops_at_separator_for_both_raw_scanners() {
    let (_guard, config) = alias_config("aliases:\n  - name: a\n    arguments: summary\n");
    ragtag()
        .env("RAGTAG_CONFIG", &config)
        .args(["--config", "--", "a"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("config file not found").not())
        .stderr(predicate::str::contains("unrecognized subcommand 'a'"));
}

#[cfg(unix)]
#[test]
fn test_non_utf8_config_paths_survive_alias_expansion_and_terminal_clap() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let dir = tempfile::tempdir().unwrap();
    let path = dir
        .path()
        .join(OsString::from_vec(b"config-\xff.yaml".to_vec()));
    fs::write(&path, "aliases:\n  - name: a\n    arguments: summary\n").unwrap();
    let fixtures = fixtures_dir();

    let split_alias = ragtag()
        .env_remove("RAGTAG_CONFIG")
        .arg("--config")
        .arg(&path)
        .args(["a", "--path", &fixtures])
        .output()
        .unwrap();
    let split_direct = ragtag()
        .env_remove("RAGTAG_CONFIG")
        .arg("--config")
        .arg(&path)
        .args(["summary", "--path", &fixtures])
        .output()
        .unwrap();
    assert_output_equivalent(&split_alias, &split_direct);
    assert_eq!(split_alias.status.code(), Some(0));

    let mut equals = OsString::from("--config=");
    equals.push(path.as_os_str());
    let equals_alias = ragtag()
        .env_remove("RAGTAG_CONFIG")
        .arg(&equals)
        .args(["a", "--path", &fixtures])
        .output()
        .unwrap();
    let equals_direct = ragtag()
        .env_remove("RAGTAG_CONFIG")
        .arg(&equals)
        .args(["summary", "--path", &fixtures])
        .output()
        .unwrap();
    assert_output_equivalent(&equals_alias, &equals_direct);
    assert_eq!(equals_alias.status.code(), Some(0));
}

#[test]
fn test_empty_suffix_token_reaches_terminal_clap_unchanged() {
    let (_guard, config) = alias_config("aliases:\n  - name: q\n    arguments: query\n");
    let fixtures = fixtures_dir();
    let actual = ragtag()
        .args(["--config", &config, "q"])
        .arg("")
        .args(["--path", &fixtures])
        .output()
        .unwrap();
    let direct = ragtag()
        .args(["--config", &config, "query"])
        .arg("")
        .args(["--path", &fixtures])
        .output()
        .unwrap();
    assert_output_equivalent(&actual, &direct);
    assert_eq!(actual.status.code(), Some(0));
}

#[test]
fn test_alias_leading_global_matrix_matches_direct_terminal_argv() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.yaml");
    let last = dir.path().join("last.yaml");
    let yaml = "aliases:\n  - name: a\n    arguments: \"query task --count\"\n";
    fs::write(&first, yaml).unwrap();
    fs::write(&last, yaml).unwrap();
    let first = first.to_str().unwrap().to_string();
    let last = last.to_str().unwrap().to_string();
    let first_equals = format!("--config={first}");
    let last_equals = format!("--config={last}");
    let fixtures = fixtures_dir();

    let cases = [
        (
            vec![
                "--no-color".to_string(),
                "--config".to_string(),
                first.clone(),
                "a".to_string(),
                "--path".to_string(),
                fixtures.clone(),
            ],
            vec![
                "--no-color".to_string(),
                "--config".to_string(),
                first.clone(),
                "query".to_string(),
                "task".to_string(),
                "--count".to_string(),
                "--path".to_string(),
                fixtures.clone(),
            ],
        ),
        (
            vec![
                first_equals.clone(),
                "a".to_string(),
                "--path".to_string(),
                fixtures.clone(),
            ],
            vec![
                first_equals.clone(),
                "query".to_string(),
                "task".to_string(),
                "--count".to_string(),
                "--path".to_string(),
                fixtures.clone(),
            ],
        ),
        (
            vec![
                "--config".to_string(),
                first.clone(),
                "--no-color".to_string(),
                last_equals.clone(),
                "a".to_string(),
                "--path".to_string(),
                fixtures.clone(),
            ],
            vec![
                "--config".to_string(),
                first.clone(),
                "--no-color".to_string(),
                last_equals,
                "query".to_string(),
                "task".to_string(),
                "--count".to_string(),
                "--path".to_string(),
                fixtures,
            ],
        ),
        (
            vec!["--bogus".to_string(), "a".to_string()],
            vec![
                "--bogus".to_string(),
                "query".to_string(),
                "task".to_string(),
                "--count".to_string(),
            ],
        ),
    ];

    for (actual, expected) in cases {
        assert_alias_case_equivalent(&actual, &expected, Some(&last));
    }
}

#[test]
fn test_alias_repeated_and_suffix_config_tokens_match_direct_clap_behavior() {
    let (_guard, config) =
        alias_config("aliases:\n  - name: a\n    arguments: \"query task --count\"\n");
    let fixtures = fixtures_dir();
    let equals = format!("--config={config}");

    let actual = ragtag()
        .args([
            "--config",
            &config,
            "--no-color",
            &equals,
            "a",
            "--path",
            &fixtures,
        ])
        .output()
        .unwrap();
    let expected = ragtag()
        .args([
            "--config",
            &config,
            "--no-color",
            &equals,
            "query",
            "task",
            "--count",
            "--path",
            &fixtures,
        ])
        .output()
        .unwrap();
    assert_output_equivalent(&actual, &expected);

    let actual = ragtag()
        .env("RAGTAG_CONFIG", &config)
        .args(["a", &equals, "--path", &fixtures])
        .output()
        .unwrap();
    let expected = ragtag()
        .env("RAGTAG_CONFIG", &config)
        .args(["query", "task", "--count", &equals, "--path", &fixtures])
        .output()
        .unwrap();
    assert_output_equivalent(&actual, &expected);
}

#[test]
fn test_alias_suffix_global_and_positional_order_matrix_matches_direct_argv() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.yaml");
    let second = dir.path().join("second.yaml");
    let yaml =
        "aliases:\n  - name: a\n    arguments: query\n  - name: p\n    arguments: \"query task\"\n";
    fs::write(&first, yaml).unwrap();
    fs::write(&second, yaml).unwrap();
    let first = first.to_str().unwrap().to_string();
    let second = second.to_str().unwrap().to_string();
    let second_equals = format!("--config={second}");

    let replacements = [
        (vec!["a", "--no-color"], vec!["query", "--no-color"]),
        (
            vec!["a", "--config", first.as_str()],
            vec!["query", "--config", first.as_str()],
        ),
        (
            vec!["a", second_equals.as_str()],
            vec!["query", second_equals.as_str()],
        ),
        (
            vec![
                "a",
                "--no-color",
                "--config",
                first.as_str(),
                second_equals.as_str(),
            ],
            vec![
                "query",
                "--no-color",
                "--config",
                first.as_str(),
                second_equals.as_str(),
            ],
        ),
        (
            vec!["a", "task", "--no-color"],
            vec!["query", "task", "--no-color"],
        ),
        (
            vec!["p", "--no-color", "item"],
            vec!["query", "task", "--no-color", "item"],
        ),
        (
            vec!["p", "item", "--no-color"],
            vec!["query", "task", "item", "--no-color"],
        ),
        (
            vec![
                "--no-color",
                "a",
                second_equals.as_str(),
                "task",
                "--no-color",
            ],
            vec![
                "--no-color",
                "query",
                second_equals.as_str(),
                "task",
                "--no-color",
            ],
        ),
    ];

    for (actual, expected) in replacements {
        let actual = actual.into_iter().map(str::to_string).collect::<Vec<_>>();
        let expected = expected.into_iter().map(str::to_string).collect::<Vec<_>>();
        assert_alias_case_equivalent(&actual, &expected, Some(&first));
    }
}

#[test]
fn test_alias_suffix_globals_help_version_and_separator_match_direct_argv() {
    let (_guard, config) = alias_config("aliases:\n  - name: a\n    arguments: \"query task\"\n");
    let fixtures = fixtures_dir();
    for suffix in [
        vec!["--no-color", "--path", fixtures.as_str()],
        vec!["--path", fixtures.as_str(), "--no-color"],
        vec!["--help"],
        vec!["--version"],
        vec!["--", "--no-color"],
        vec!["--", "--help"],
        vec!["--", "--config", "literal.yaml"],
    ] {
        let mut actual_args = vec!["--config", config.as_str(), "a"];
        actual_args.extend(suffix.iter().copied());
        let mut expected_args = vec!["--config", config.as_str(), "query", "task"];
        expected_args.extend(suffix.iter().copied());
        let actual = ragtag()
            .env_remove("RAGTAG_CONFIG")
            .args(actual_args)
            .output()
            .unwrap();
        let expected = ragtag()
            .env_remove("RAGTAG_CONFIG")
            .args(expected_args)
            .output()
            .unwrap();
        assert_output_equivalent(&actual, &expected);
    }
}

#[test]
fn test_alias_separator_and_help_version_matrix_matches_terminal_clap() {
    let (_guard, config) = alias_config(
        "aliases:\n  - name: a\n    arguments: \"query task\"\n  - name: configured-help\n    arguments: \"query --help\"\n  - name: configured-version\n    arguments: \"query --version\"\n",
    );

    for (actual, expected) in [
        (vec!["a", "-h"], vec!["query", "task", "-h"]),
        (vec!["a", "--help"], vec!["query", "task", "--help"]),
        (
            vec!["a", "item", "--help"],
            vec!["query", "task", "item", "--help"],
        ),
        (vec!["a", "--version"], vec!["query", "task", "--version"]),
        (
            vec!["a", "item", "--version"],
            vec!["query", "task", "item", "--version"],
        ),
        (
            vec!["a", "--", "--help"],
            vec!["query", "task", "--", "--help"],
        ),
        (
            vec!["a", "item", "--", "--help"],
            vec!["query", "task", "item", "--", "--help"],
        ),
        (vec!["configured-help"], vec!["query", "--help"]),
        (vec!["configured-version"], vec!["query", "--version"]),
    ] {
        let actual = actual.into_iter().map(str::to_string).collect::<Vec<_>>();
        let expected = expected.into_iter().map(str::to_string).collect::<Vec<_>>();
        assert_alias_case_equivalent(&actual, &expected, Some(&config));
    }

    for separator_args in [
        vec!["--".to_string(), "a".to_string()],
        vec![
            "--config".to_string(),
            config.clone(),
            "--".to_string(),
            "a".to_string(),
        ],
    ] {
        let output = alias_case_output(&separator_args, Some(&config));
        assert!(!output.status.success());
    }

    let after_boundary = vec![
        "--config".to_string(),
        config.clone(),
        "a".to_string(),
        "--".to_string(),
        "--config=/definitely/not/selected.yaml".to_string(),
    ];
    let direct_after_boundary = vec![
        "--config".to_string(),
        config.clone(),
        "query".to_string(),
        "task".to_string(),
        "--".to_string(),
        "--config=/definitely/not/selected.yaml".to_string(),
    ];
    assert_alias_case_equivalent(&after_boundary, &direct_after_boundary, None);
}

#[test]
fn test_top_level_help_and_version_are_unchanged_by_configured_aliases() {
    let (_alias_guard, alias_config_path) =
        alias_config("aliases:\n  - name: hidden-alias\n    arguments: summary\n");
    let (_plain_guard, plain_config) = alias_config("");

    for terminal in ["-h", "--help", "--version"] {
        let actual = alias_case_output(
            &[
                "--config".to_string(),
                alias_config_path.clone(),
                terminal.to_string(),
            ],
            None,
        );
        let expected = alias_case_output(
            &[
                "--config".to_string(),
                plain_config.clone(),
                terminal.to_string(),
            ],
            None,
        );
        assert_output_equivalent(&actual, &expected);
    }
}

#[test]
fn test_alias_defined_config_token_does_not_reload_startup_config() {
    let dir = tempfile::tempdir().unwrap();
    let startup = dir.path().join("startup.yaml");
    let second = dir.path().join("second.yaml");
    fs::write(&second, "output:\n  color: never\n").unwrap();
    fs::write(
        &startup,
        format!(
            "output:\n  color: always\naliases:\n  - name: one-load\n    arguments: \"config get output.color --config {}\"\n",
            second.display()
        ),
    )
    .unwrap();

    ragtag()
        .env("RAGTAG_CONFIG", &startup)
        .arg("one-load")
        .assert()
        .code(0)
        .stdout("always\n");
}

#[test]
fn test_alias_option_leading_valid_and_unknown_vectors_reach_final_clap() {
    let (_guard, config) = alias_config(
        "aliases:\n  - name: valid-option\n    arguments: \"--no-color task summary --all\"\n  - name: bad-option\n    arguments: \"--bogus task summary\"\n",
    );
    let fixtures = fixtures_dir();
    for (alias, direct) in [
        (
            "valid-option",
            vec!["--no-color", "task", "summary", "--all"],
        ),
        ("bad-option", vec!["--bogus", "task", "summary"]),
    ] {
        let mut actual_args = vec!["--config", config.as_str(), alias];
        let mut expected_args = vec!["--config", config.as_str()];
        expected_args.extend(direct);
        if alias == "valid-option" {
            actual_args.extend(["--path", fixtures.as_str()]);
            expected_args.extend(["--path", fixtures.as_str()]);
        }
        let actual = ragtag().args(actual_args).output().unwrap();
        let expected = ragtag().args(expected_args).output().unwrap();
        assert_output_equivalent(&actual, &expected);
    }
}

#[test]
fn test_alias_cycles_unknown_inner_prefix_and_combined_ambiguity_are_typed() {
    let (_guard, config) = alias_config(
        "aliases:\n  - names: [a, alt-a]\n    arguments: alt-a\n  - name: prefix-target\n    arguments: al\n  - name: sum-all\n    arguments: summary\n  - name: pair-one\n    arguments: summary\n  - name: pair-two\n    arguments: summary\n",
    );
    ragtag()
        .args(["--config", &config, "a"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("a -> alt-a"));
    ragtag()
        .args(["--config", &config, "prefix-target"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("alias target is not a command"))
        .stderr(predicate::str::contains("prefix-target"));
    ragtag()
        .args(["--config", &config, "sum"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("summary, sum-all"));
    ragtag()
        .args(["--config", &config, "pair"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("pair-one, pair-two"));
}

#[test]
fn test_alias_schema_errors_and_every_synonym_collision_fail_before_help() {
    for (yaml, expected) in [
        (
            "aliases:\n  - name: a\n    names: [b]\n    arguments: summary\n",
            "exactly one of \"name\" or \"names\", not both",
        ),
        (
            "aliases:\n  - arguments: summary\n",
            "exactly one of \"name\" or \"names\"",
        ),
        (
            "aliases:\n  - names: []\n    arguments: summary\n",
            "\"names\" must contain at least one name",
        ),
        (
            "aliases:\n  - names: [ok, summary]\n    arguments: query\n",
            "alias \"summary\" collides with an existing command name",
        ),
        (
            "aliases:\n  - names: [ok, task]\n    arguments: query\n",
            "alias \"task\" collides with an existing command name",
        ),
        (
            "aliases:\n  - names: [dup, dup]\n    arguments: summary\n",
            "duplicate alias name \"dup\"",
        ),
        (
            "aliases:\n  - names: [ok, \"\"]\n    arguments: summary\n",
            "alias name must not be empty",
        ),
        (
            "aliases:\n  - name: [a]\n    arguments: summary\n",
            "invalid type: sequence",
        ),
        (
            "aliases:\n  - names: a\n    arguments: summary\n",
            "expected a sequence",
        ),
        (
            "aliases:\n  - name: first\n    arguments: summary\n  - names: [second, first]\n    arguments: query\n",
            "duplicate alias name \"first\"",
        ),
        (
            "aliases:\n  - name: blank\n    arguments: \"   \"\n",
            "alias \"blank\" has empty arguments",
        ),
    ] {
        let (_guard, config) = alias_config(yaml);
        ragtag()
            .args(["--config", &config, "--help"])
            .assert()
            .code(1)
            .stderr(predicate::str::contains(expected));
    }

    let (_guard, config) =
        alias_config("aliases:\n  - name: quoted\n    arguments: 'query \"$SECRET'\n");
    ragtag()
        .args(["--config", &config, "--help"])
        .env("SECRET", "sensitive-value")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid arguments string"))
        .stderr(predicate::str::contains("sensitive-value").not());
}

#[test]
fn test_alias_unknown_metadata_fields_are_ignored() {
    let (_guard, config) = alias_config(
        "aliases:\n  - name: compatible\n    arguments: summary\n    description: handy\n    future_metadata:\n      category: reporting\n",
    );
    let fixtures = fixtures_dir();
    let actual = ragtag()
        .args(["--config", &config, "compatible", "--path", &fixtures])
        .output()
        .unwrap();
    let direct = ragtag()
        .args(["--config", &config, "summary", "--path", &fixtures])
        .output()
        .unwrap();
    assert_output_equivalent(&actual, &direct);
    assert_eq!(actual.status.code(), Some(0));
}

#[test]
fn test_alias_validation_precedes_extension_initialization_errors() {
    let (_guard, config) = alias_config(
        "tasks:\n  default_status: definitely-invalid\naliases:\n  - name: help\n    arguments: summary\n",
    );
    ragtag()
        .args(["--config", &config, "--help"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "alias \"help\" collides with an existing command name",
        ))
        .stderr(predicate::str::contains("invalid default_status").not());
}

#[test]
fn test_config_get_aliases_emits_canonical_name_and_ordered_names() {
    let (_guard, config) = alias_config(
        "aliases:\n  - names: [single]\n    arguments: summary\n  - names: [active, a]\n    arguments: \"query 'two words'\"\n",
    );
    ragtag()
        .args(["--config", &config, "config", "get", "aliases"])
        .assert()
        .success()
        .stdout(predicate::str::contains("name: single"))
        .stdout(predicate::str::contains(r#"names: ["active", "a"]"#))
        .stdout(predicate::str::contains("query 'two words'"));
}

#[test]
fn test_alias_depth_and_argument_limits_report_stable_errors() {
    let mut depth_yaml = String::from("aliases:\n");
    for index in 0..33 {
        let target = if index == 32 {
            "summary".to_string()
        } else {
            format!("a{}", index + 1)
        };
        depth_yaml.push_str(&format!("  - name: a{index}\n    arguments: {target}\n"));
    }
    let (_guard, config) = alias_config(&depth_yaml);
    ragtag()
        .args(["--config", &config, "a0"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("maximum depth of 32"));

    let wide = std::iter::repeat_n("x", 4096).collect::<Vec<_>>().join(" ");
    let yaml = format!("aliases:\n  - name: wide\n    arguments: \"summary {wide}\"\n");
    let (_guard, config) = alias_config(&yaml);
    ragtag()
        .args(["--config", &config, "wide"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("projected count: 4097"))
        .stderr(predicate::str::contains("maximum of 4096"));
}

#[test]
fn test_alias_depth_32_and_argument_count_4096_reach_terminal_clap() {
    let mut depth_yaml = String::from("aliases:\n");
    for index in 0..32 {
        let target = if index == 31 {
            "summary".to_string()
        } else {
            format!("a{}", index + 1)
        };
        depth_yaml.push_str(&format!("  - name: a{index}\n    arguments: {target}\n"));
    }
    let (_depth_guard, depth_config) = alias_config(&depth_yaml);
    let fixtures = fixtures_dir();
    assert_alias_case_equivalent(
        &["a0".to_string(), "--path".to_string(), fixtures.clone()],
        &["summary".to_string(), "--path".to_string(), fixtures],
        Some(&depth_config),
    );

    let remainder = std::iter::repeat_n("x", 4095).collect::<Vec<_>>().join(" ");
    let yaml = format!("aliases:\n  - name: wide\n    arguments: \"summary {remainder}\"\n");
    let (_wide_guard, wide_config) = alias_config(&yaml);
    let mut direct = vec!["summary".to_string()];
    direct.extend(std::iter::repeat_n("x".to_string(), 4095));
    assert_alias_case_equivalent(&["wide".to_string()], &direct, Some(&wide_config));
}

#[test]
fn test_alias_definition_and_name_caps_fail_deterministically_at_257() {
    let mut definitions = String::from("aliases:\n");
    for index in 0..257 {
        definitions.push_str(&format!("  - name: a{index}\n    arguments: summary\n"));
    }
    let names = (0..257)
        .map(|index| format!("n{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let aggregate_names = format!("aliases:\n  - names: [{names}]\n    arguments: summary\n");

    for (yaml, expected) in [
        (definitions.as_str(), "too many aliases (257)"),
        (aggregate_names.as_str(), "too many alias names (257)"),
    ] {
        let (_guard, config) = alias_config(yaml);
        ragtag()
            .args(["--config", &config, "--help"])
            .assert()
            .failure()
            .stderr(predicate::str::contains(expected));
    }
}

// === Query boolean filters (shared filter engine) ===

/// Creates a temp dir with a file of `@item(a=.., b=.., c=..)` tags for query
/// filter tests. Returns the guard (kept alive by the caller) and the file path.
fn query_items_file() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("items.txt");
    fs::write(
        &file,
        "@item(a=1, b=9, c=9)\n\
         @item(a=9, b=2, c=9)\n\
         @item(a=9, b=9, c=3)\n\
         @item(a=1, b=9, c=3)\n",
    )
    .unwrap();
    let path = file.to_str().unwrap().to_string();
    (dir, path)
}

#[test]
fn test_query_filter_boolean_and_or_parens() {
    // (a = 1 OR b = 2) AND c != 3 matches:
    //   row1 (a=1, c=9): true
    //   row2 (b=2, c=9): true
    //   row3 (a=9,b=9): OR false
    //   row4 (a=1, c=3): OR true but c!=3 false
    // → 2 matches.
    let (_dir, path) = query_items_file();
    ragtag()
        .args([
            "query",
            "item",
            "--path",
            &path,
            "--count",
            "--filter",
            "(a = 1 OR b = 2) AND c != 3",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("2"));
}

#[test]
fn test_query_filter_whitespace_matches_spaceless() {
    // The same expression with and without whitespace around operators yields
    // the same count.
    let (_dir, path) = query_items_file();
    for expr in ["a = 1 AND c != 3", "a=1 AND c!=3"] {
        ragtag()
            .args([
                "query", "item", "--path", &path, "--count", "--filter", expr,
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("1"));
    }
}

#[test]
fn test_query_multiple_filters_are_and_combined() {
    // Two --filter flags are AND-combined: a=1 AND c!=3 → only row1.
    let (_dir, path) = query_items_file();
    ragtag()
        .args([
            "query", "item", "--path", &path, "--count", "--filter", "a=1", "--filter", "c!=3",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("1"));
}

#[test]
fn test_query_filter_no_operator_errors() {
    // A condition with no comparison operator is rejected with a clear message.
    let (_dir, path) = query_items_file();
    ragtag()
        .args([
            "query",
            "item",
            "--path",
            &path,
            "--filter",
            "statusinvalid",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("expected format"));
}

#[test]
fn test_query_filter_lexicographic_ordering_preserved() {
    // Non-numeric values fall back to lexicographic comparison, unchanged.
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("items.txt");
    fs::write(&file, "@item(status=\"draft\")\n").unwrap();
    let path = file.to_str().unwrap().to_string();

    // "draft" > "active" lexicographically → 1 match.
    ragtag()
        .args([
            "query",
            "item",
            "--path",
            &path,
            "--count",
            "--filter",
            "status>active",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("1"));

    // "draft" > "final" is false → 0 matches.
    ragtag()
        .args([
            "query",
            "item",
            "--path",
            &path,
            "--count",
            "--filter",
            "status>final",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("0"));
}

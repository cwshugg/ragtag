#![allow(dead_code)]

use assert_cmd::Command;

pub fn ragtag() -> Command {
    Command::cargo_bin("ragtag").unwrap()
}

pub fn fixtures_dir() -> String {
    format!("{}/tests/fixtures", env!("CARGO_MANIFEST_DIR"))
}

/// Asserts process status, stdout, and stderr are byte-identical.
pub fn assert_output_equivalent(actual: &std::process::Output, expected: &std::process::Output) {
    assert_eq!(
        actual.status.code(),
        expected.status.code(),
        "status differs"
    );
    assert_eq!(actual.stdout, expected.stdout, "stdout differs");
    assert_eq!(actual.stderr, expected.stderr, "stderr differs");
}

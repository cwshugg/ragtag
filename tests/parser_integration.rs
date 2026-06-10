//! Parser integration tests using fixture files.

use std::path::PathBuf;

use ragtag::parser::scan_file;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn parse_fixture(name: &str) -> Vec<ragtag::models::Tag> {
    let path = fixture_path(name);
    let content = std::fs::read_to_string(&path).unwrap();
    scan_file(&content, &path)
}

#[test]
fn test_simple_tags_count() {
    let tags = parse_fixture("simple_tags.txt");
    // @note, @todo, @bookmark, @tag x3, @_private_tag, @-hyphen-tag = 8
    assert_eq!(tags.len(), 8, "Expected exactly 8 tags, got {}", tags.len());
}

#[test]
fn test_simple_tags_names() {
    let tags = parse_fixture("simple_tags.txt");
    let names: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"note"));
    assert!(names.contains(&"todo"));
    assert!(names.contains(&"bookmark"));
    assert!(names.contains(&"tag"));
    assert!(names.contains(&"_private_tag"));
    assert!(names.contains(&"-hyphen-tag"));
}

#[test]
fn test_email_not_parsed() {
    let tags = parse_fixture("simple_tags.txt");
    // No tag named "example" from email@example.com
    let names: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
    assert!(!names.contains(&"example"));
}

#[test]
fn test_edge_cases_multiline() {
    let tags = parse_fixture("edge_cases.txt");
    let multiline: Vec<_> = tags.iter().filter(|t| t.name == "multiline").collect();
    assert_eq!(multiline.len(), 1);
    assert_eq!(
        multiline[0].get_named_attribute("key1").unwrap().as_str(),
        Some("value 1")
    );
    assert_eq!(
        multiline[0]
            .get_named_attribute("key3")
            .unwrap()
            .as_integer(),
        Some(42)
    );
}

#[test]
fn test_edge_cases_numeric_bases() {
    let tags = parse_fixture("edge_cases.txt");
    let numbers: Vec<_> = tags.iter().filter(|t| t.name == "numbers").collect();
    assert_eq!(numbers.len(), 1);
    assert_eq!(
        numbers[0].get_named_attribute("dec").unwrap().as_integer(),
        Some(42)
    );
    assert_eq!(
        numbers[0].get_named_attribute("hex").unwrap().as_integer(),
        Some(255)
    );
    assert_eq!(
        numbers[0].get_named_attribute("oct").unwrap().as_integer(),
        Some(63)
    );
    assert_eq!(
        numbers[0].get_named_attribute("bin").unwrap().as_integer(),
        Some(10)
    );
    assert_eq!(
        numbers[0].get_named_attribute("float").unwrap().as_float(),
        Some(3.14)
    );
    assert_eq!(
        numbers[0].get_named_attribute("neg").unwrap().as_integer(),
        Some(-5)
    );
}

#[test]
fn test_edge_cases_quoted() {
    let tags = parse_fixture("edge_cases.txt");
    let quoted: Vec<_> = tags.iter().filter(|t| t.name == "quoted").collect();
    assert_eq!(quoted.len(), 1);
    assert_eq!(
        quoted[0].get_named_attribute("double").unwrap().as_str(),
        Some("hello world")
    );
    assert_eq!(
        quoted[0].get_named_attribute("single").unwrap().as_str(),
        Some("single quoted")
    );
    assert_eq!(
        quoted[0].get_named_attribute("escaped").unwrap().as_str(),
        Some("has \"quotes\"")
    );
}

#[test]
fn test_edge_cases_invalid_start() {
    let tags = parse_fixture("edge_cases.txt");
    // @1invalid_start should NOT be parsed
    let names: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
    assert!(!names.contains(&"1invalid_start"));
}

#[test]
fn test_edge_cases_tag_termination() {
    let tags = parse_fixture("edge_cases.txt");
    // @terminated::extra should parse as @terminated, and "is just @terminated" adds another
    let terminated: Vec<_> = tags.iter().filter(|t| t.name == "terminated").collect();
    assert_eq!(terminated.len(), 2);
}

#[test]
fn test_edge_cases_overflow() {
    let tags = parse_fixture("edge_cases.txt");
    let overflow: Vec<_> = tags.iter().filter(|t| t.name == "overflow").collect();
    assert_eq!(overflow.len(), 1);
    // Large number should fall back to string
    assert!(overflow[0]
        .get_named_attribute("n")
        .unwrap()
        .as_str()
        .is_some());
}

#[test]
fn test_tasks_parse() {
    let tags = parse_fixture("tasks.md");
    let tasks: Vec<_> = tags.iter().filter(|t| t.name == "task").collect();
    assert_eq!(tasks.len(), 3);
}

#[test]
fn test_empty_file() {
    let tags = parse_fixture("empty.txt");
    assert!(tags.is_empty());
}

#[test]
fn test_no_tags_file() {
    let tags = parse_fixture("no_tags.txt");
    assert!(tags.is_empty());
}

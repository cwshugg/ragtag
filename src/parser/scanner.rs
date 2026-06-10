//! File-level tag scanner.
//!
//! Scans an entire file (as a `&str`) looking for `@` characters that
//! pass the pre-check (preceded by whitespace or start of file), then
//! attempts to parse each one as a tag. Malformed tags are silently skipped.

use std::path::Path;

use super::cursor::Cursor;
use super::tag::parse_tag;
use crate::models::Tag;

/// Scans a file's contents and returns all successfully parsed tags.
///
/// The `@` pre-check ensures that `email@address.com` is NOT treated
/// as a tag. Only `@` characters at the start of input or preceded by
/// whitespace are considered tag candidates.
pub fn scan_file(input: &str, file_path: &Path) -> Vec<Tag> {
    let mut tags = Vec::new();
    let mut cursor = Cursor::new(input);

    while !cursor.is_eof() {
        // Scan for '@'
        if cursor.peek() != Some('@') {
            cursor.advance();
            continue;
        }

        // Pre-check: '@' must be at start of input or preceded by whitespace
        if cursor.pos > 0 {
            let prev_byte = input.as_bytes()[cursor.pos - 1];
            if !matches!(prev_byte, b' ' | b'\t' | b'\n' | b'\r') {
                cursor.advance();
                continue;
            }
        }

        // Save state for error recovery
        let state = cursor.save();

        // Attempt to parse a tag
        match parse_tag(&mut cursor, file_path) {
            Some(tag) => {
                tags.push(tag);
            }
            None => {
                // Parse failed — restore to one byte past the '@' and continue
                cursor.restore(state);
                cursor.advance(); // skip past the '@'
            }
        }
    }

    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scan(input: &str) -> Vec<Tag> {
        scan_file(input, &PathBuf::from("test.md"))
    }

    #[test]
    fn test_single_tag_in_text() {
        let tags = scan("hello @tag world");
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "tag");
    }

    #[test]
    fn test_multiple_tags() {
        let tags = scan("@a @b(x=1) @c");
        assert_eq!(tags.len(), 3);
        assert_eq!(tags[0].name, "a");
        assert_eq!(tags[1].name, "b");
        assert_eq!(tags[2].name, "c");
    }

    #[test]
    fn test_email_rejection() {
        let tags = scan("email@address.com");
        assert!(tags.is_empty());
    }

    #[test]
    fn test_at_start_of_file() {
        let tags = scan("@tag rest of text");
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "tag");
    }

    #[test]
    fn test_at_start_of_line() {
        let tags = scan("line1\n@tag line2");
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "tag");
        assert_eq!(tags[0].location.line, 2);
    }

    #[test]
    fn test_at_after_tab() {
        let tags = scan("\t@tag");
        assert_eq!(tags.len(), 1);
    }

    #[test]
    fn test_at_after_non_whitespace() {
        let tags = scan("abc@tag");
        assert!(tags.is_empty());
    }

    #[test]
    fn test_error_recovery() {
        // First tag has unmatched paren, second is valid
        let tags = scan("@tag( @valid");
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "valid");
    }

    #[test]
    fn test_multiple_lines() {
        let tags = scan("@a\n@b\n@c");
        assert_eq!(tags.len(), 3);
    }

    #[test]
    fn test_no_at_characters() {
        let tags = scan("hello world no tags here");
        assert!(tags.is_empty());
    }

    #[test]
    fn test_empty_file() {
        let tags = scan("");
        assert!(tags.is_empty());
    }

    #[test]
    fn test_tag_with_multiline_attrs() {
        let input = "before @task(\n    id=\"abc\",\n    title=\"test\"\n) after";
        let tags = scan(input);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "task");
        assert_eq!(
            tags[0].get_named_attribute("id").unwrap().as_str(),
            Some("abc")
        );
    }

    #[test]
    fn test_invalid_tag_name_skip() {
        // @123 should be skipped (starts with digit)
        let tags = scan("@123 @valid");
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "valid");
    }

    #[test]
    fn test_line_numbers() {
        let tags = scan("line1\nline2\n@tag");
        assert_eq!(tags[0].location.line, 3);
        assert_eq!(tags[0].location.column, 1);
    }

    #[test]
    fn test_column_numbers() {
        let tags = scan("   @tag");
        assert_eq!(tags[0].location.column, 4);
    }

    #[test]
    fn test_tag_at_eof() {
        let tags = scan("@tag");
        assert_eq!(tags.len(), 1);
    }
}

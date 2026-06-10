//! Tag name and attribute list parsing.
//!
//! Contains the core parsing functions for tags: `parse_tag`, `parse_tag_name`,
//! `parse_attr_list`, `parse_attribute`, and `parse_attr_name`.

use std::path::Path;

use super::cursor::{skip_whitespace, Cursor};
use super::value::parse_attr_value;
use crate::models::{AttributeKind, Tag, TagAttribute, TagLocation};

/// Maximum allowed length for a tag name (in characters).
const MAX_TAG_NAME_LENGTH: usize = 256;

/// Maximum number of attributes allowed per tag.
const MAX_ATTRIBUTES_PER_TAG: usize = 256;

/// Parses a complete tag starting at the `@` character.
///
/// Expects the cursor to be positioned at `@`. Returns `None` if parsing
/// fails (invalid tag name, unmatched parenthesis, etc.).
pub fn parse_tag(cursor: &mut Cursor, file_path: &Path) -> Option<Tag> {
    let start_pos = cursor.pos;
    let start_line = cursor.line;
    let start_col = cursor.col;

    // Advance past '@'
    cursor.advance()?;

    // Parse the tag name
    let name = parse_tag_name(cursor)?;

    // Check for attribute list
    let attributes = if cursor.peek() == Some('(') {
        cursor.advance(); // consume '('
        let attrs = parse_attr_list(cursor);
        // Expect closing ')'
        if cursor.peek() == Some(')') {
            cursor.advance();
        } else {
            // Unmatched parenthesis — parsing fails
            return None;
        }
        attrs
    } else {
        vec![]
    };

    let end_pos = cursor.pos;

    let location = TagLocation::new(
        file_path.to_path_buf(),
        start_line,
        start_col,
        start_pos,
        end_pos,
    );

    Some(Tag {
        name,
        attributes,
        location,
        raw_span: start_pos..end_pos,
    })
}

/// Parses a tag name.
///
/// Tag names must start with an alphabetic character, `_`, or `-`.
/// They cannot start with a digit. Subsequent characters may be
/// alphanumeric, `_`, or `-`.
pub fn parse_tag_name(cursor: &mut Cursor) -> Option<String> {
    let first = cursor.peek()?;

    // First char: must be alphabetic, '_', or '-' (NOT a digit)
    if !first.is_ascii_alphabetic() && first != '_' && first != '-' {
        return None;
    }

    let mut name = String::new();
    name.push(first);
    cursor.advance();

    // Rest: alphanumeric, '_', or '-'
    while let Some(ch) = cursor.peek() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            name.push(ch);
            cursor.advance();
            if name.len() > MAX_TAG_NAME_LENGTH {
                return None;
            }
        } else {
            break;
        }
    }

    Some(name)
}

/// Parses an attribute list inside parentheses.
///
/// Handles comma-separated attributes with optional trailing comma.
/// Whitespace (including newlines) is freely allowed between attributes.
pub fn parse_attr_list(cursor: &mut Cursor) -> Vec<TagAttribute> {
    let mut attributes = Vec::new();

    skip_whitespace(cursor);

    // Empty attribute list
    if cursor.peek() == Some(')') {
        return attributes;
    }

    // Parse first attribute
    if let Some(attr) = parse_attribute(cursor) {
        attributes.push(attr);
    } else {
        return attributes;
    }

    // Parse remaining attributes
    loop {
        skip_whitespace(cursor);

        // Enforce attribute count limit
        if attributes.len() >= MAX_ATTRIBUTES_PER_TAG {
            break;
        }

        match cursor.peek() {
            Some(',') => {
                cursor.advance(); // consume comma
                skip_whitespace(cursor);
                // Check for trailing comma before ')'
                if cursor.peek() == Some(')') {
                    break;
                }
                // Parse next attribute
                if let Some(attr) = parse_attribute(cursor) {
                    attributes.push(attr);
                } else {
                    break;
                }
            }
            Some(')') => break,
            _ => break, // unexpected char — let caller handle
        }
    }

    attributes
}

/// Parses a single attribute (named or positional).
///
/// Tries named first (with backtracking on failure), falls back to positional.
pub fn parse_attribute(cursor: &mut Cursor) -> Option<TagAttribute> {
    let state = cursor.save();

    // Try named attribute: name '=' value
    if let Some(name) = parse_attr_name(cursor) {
        skip_whitespace(cursor);
        if cursor.peek() == Some('=') {
            cursor.advance(); // consume '='
            skip_whitespace(cursor);
            if let Some(value) = parse_attr_value(cursor) {
                return Some(TagAttribute {
                    kind: AttributeKind::Named { name, value },
                });
            }
            // Value parse failed — restore and try as positional
        }
        // No '=' found — restore and try as positional
    }

    cursor.restore(state);

    // Parse as positional attribute
    let value = parse_attr_value(cursor)?;
    Some(TagAttribute {
        kind: AttributeKind::Positional { value },
    })
}

/// Parses an attribute name.
///
/// Attribute names must start with an alphabetic character or `_` (NOT `-`).
/// Subsequent characters may be alphanumeric, `_`, or `-`.
pub fn parse_attr_name(cursor: &mut Cursor) -> Option<String> {
    let first = cursor.peek()?;

    // First char: alphabetic or '_' (NO '-')
    if !first.is_ascii_alphabetic() && first != '_' {
        return None;
    }

    let mut name = String::new();
    name.push(first);
    cursor.advance();

    while let Some(ch) = cursor.peek() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            name.push(ch);
            cursor.advance();
        } else {
            break;
        }
    }

    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::tag::{AttributeValue, NumericBase};
    use std::path::PathBuf;

    fn test_path() -> PathBuf {
        PathBuf::from("test.md")
    }

    fn parse(input: &str) -> Option<Tag> {
        let mut cursor = Cursor::new(input);
        parse_tag(&mut cursor, &test_path())
    }

    #[test]
    fn test_simple_tag() {
        let tag = parse("@tag").unwrap();
        assert_eq!(tag.name, "tag");
        assert!(tag.attributes.is_empty());
    }

    #[test]
    fn test_tag_empty_parens() {
        let tag = parse("@tag()").unwrap();
        assert_eq!(tag.name, "tag");
        assert!(tag.attributes.is_empty());
    }

    #[test]
    fn test_tag_positional_attrs() {
        let tag = parse("@tag(hello, world)").unwrap();
        assert_eq!(tag.name, "tag");
        assert_eq!(tag.attributes.len(), 2);
        assert_eq!(
            tag.get_positional_attribute(0).unwrap().as_str(),
            Some("hello")
        );
        assert_eq!(
            tag.get_positional_attribute(1).unwrap().as_str(),
            Some("world")
        );
    }

    #[test]
    fn test_tag_named_attrs() {
        let tag = parse("@tag(key=value, name=\"hello\")").unwrap();
        assert_eq!(
            tag.get_named_attribute("key").unwrap().as_str(),
            Some("value")
        );
        assert_eq!(
            tag.get_named_attribute("name").unwrap().as_str(),
            Some("hello")
        );
    }

    #[test]
    fn test_tag_mixed_attrs() {
        let tag = parse("@tag(positional, key=value)").unwrap();
        assert_eq!(tag.attributes.len(), 2);
        assert_eq!(
            tag.get_positional_attribute(0).unwrap().as_str(),
            Some("positional")
        );
        assert_eq!(
            tag.get_named_attribute("key").unwrap().as_str(),
            Some("value")
        );
    }

    #[test]
    fn test_tag_multiline() {
        let input = "@tag(\n    key=value,\n    name=\"hello\"\n)";
        let tag = parse(input).unwrap();
        assert_eq!(
            tag.get_named_attribute("key").unwrap().as_str(),
            Some("value")
        );
        assert_eq!(
            tag.get_named_attribute("name").unwrap().as_str(),
            Some("hello")
        );
    }

    #[test]
    fn test_trailing_comma() {
        let tag = parse("@tag(a, b,)").unwrap();
        assert_eq!(tag.attributes.len(), 2);
    }

    #[test]
    fn test_numeric_bases() {
        let tag = parse("@tag(0xff, 0o77, 0b1010, 42)").unwrap();
        assert_eq!(
            tag.get_positional_attribute(0).unwrap().as_integer(),
            Some(255)
        );
        assert_eq!(
            tag.get_positional_attribute(1).unwrap().as_integer(),
            Some(63)
        );
        assert_eq!(
            tag.get_positional_attribute(2).unwrap().as_integer(),
            Some(10)
        );
        assert_eq!(
            tag.get_positional_attribute(3).unwrap().as_integer(),
            Some(42)
        );
    }

    #[test]
    fn test_float_attr() {
        let tag = parse("@tag(time=4.5)").unwrap();
        assert_eq!(
            tag.get_named_attribute("time").unwrap().as_float(),
            Some(4.5)
        );
    }

    #[test]
    fn test_integer_not_float() {
        let tag = parse("@tag(count=4)").unwrap();
        let val = tag.get_named_attribute("count").unwrap();
        assert!(matches!(
            val,
            AttributeValue::Integer {
                value: 4,
                base: NumericBase::Decimal
            }
        ));
    }

    #[test]
    fn test_tag_name_with_hyphens_underscores() {
        let tag = parse("@my-tag_1").unwrap();
        assert_eq!(tag.name, "my-tag_1");
    }

    #[test]
    fn test_tag_name_starting_underscore() {
        let tag = parse("@_tag").unwrap();
        assert_eq!(tag.name, "_tag");
    }

    #[test]
    fn test_tag_name_starting_hyphen() {
        let tag = parse("@-tag").unwrap();
        assert_eq!(tag.name, "-tag");
    }

    #[test]
    fn test_invalid_tag_starts_with_digit() {
        assert!(parse("@1tag").is_none());
    }

    #[test]
    fn test_tag_name_terminated_by_colon() {
        let tag = parse("@tag::bad").unwrap();
        assert_eq!(tag.name, "tag");
    }

    #[test]
    fn test_tag_name_terminated_by_question() {
        let tag = parse("@tag?").unwrap();
        assert_eq!(tag.name, "tag");
    }

    #[test]
    fn test_unmatched_paren() {
        assert!(parse("@tag(key=value").is_none());
    }

    #[test]
    fn test_duplicate_named_attrs() {
        let tag = parse("@tag(key=a, key=b)").unwrap();
        assert_eq!(tag.attributes.len(), 2);
        assert_eq!(tag.get_named_attribute("key").unwrap().as_str(), Some("a"));
    }

    #[test]
    fn test_empty_value_after_equals() {
        // @tag(key=) — The named attribute parse fails (no value after '='),
        // backtrack treats "key" as positional, but then '=' is unexpected,
        // so the attr list breaks, and ')' is not found at cursor position → None.
        let tag = parse("@tag(key=)");
        assert!(tag.is_none());
    }

    #[test]
    fn test_adjacent_delimiters() {
        // @tag(,,) — first char after '(' is ',', which is not a valid attribute start.
        // parse_attribute returns None, so the attr list is empty.
        // Then the parser expects ')' but finds ',', so parse fails.
        let tag = parse("@tag(,,)");
        // Tag parsing may fail entirely (None) since the parser can't find ')' at the right place
        // After parse_attr_list returns empty (first attr fails), the loop sees ','
        // but there's no initial attribute. Let's check what actually happens.
        // Actually parse_attr_list returns empty immediately when first parse_attribute fails.
        // Then parse_tag checks cursor.peek() == Some(')') → but cursor is at first ','
        // So this returns None (unmatched paren from parser's perspective).
        assert!(tag.is_none());
    }

    #[test]
    fn test_whitespace_only_parens() {
        let tag = parse("@tag(   )").unwrap();
        assert!(tag.attributes.is_empty());
    }

    #[test]
    fn test_tag_location() {
        let tag = parse("@tag(x=1)").unwrap();
        assert_eq!(tag.location.line, 1);
        assert_eq!(tag.location.column, 1);
        assert_eq!(tag.location.byte_offset, 0);
    }

    #[test]
    fn test_raw_span() {
        let input = "@tag(x=1)";
        let tag = parse(input).unwrap();
        assert_eq!(tag.raw_span, 0..input.len());
    }

    #[test]
    fn test_long_tag_name() {
        // Tag names up to MAX_TAG_NAME_LENGTH (256) should parse successfully
        let name_256: String = "a".repeat(256);
        let input = format!("@{name_256}");
        let tag = parse(&input).unwrap();
        assert_eq!(tag.name.len(), 256);
    }

    #[test]
    fn test_tag_name_too_long() {
        // Tag names exceeding MAX_TAG_NAME_LENGTH should be rejected
        let name_257: String = "a".repeat(257);
        let input = format!("@{name_257}");
        assert!(parse(&input).is_none());
    }

    #[test]
    fn test_quoted_value_with_paren() {
        let tag = parse("@tag(value=\"has ) inside\")").unwrap();
        assert_eq!(
            tag.get_named_attribute("value").unwrap().as_str(),
            Some("has ) inside")
        );
    }

    #[test]
    fn test_max_attributes_limit() {
        // Build a tag with exactly MAX_ATTRIBUTES_PER_TAG attributes — should succeed
        let attrs: Vec<String> = (0..256).map(|i| format!("a{i}")).collect();
        let input = format!("@tag({})", attrs.join(", "));
        let tag = parse(&input).unwrap();
        assert_eq!(tag.attributes.len(), 256);
    }

    #[test]
    fn test_max_attributes_exceeded() {
        // Build a tag with MAX_ATTRIBUTES_PER_TAG + 1 attributes — should fail to parse
        // (parser stops accumulating at the limit and cannot find closing ')')
        let attrs: Vec<String> = (0..257).map(|i| format!("a{i}")).collect();
        let input = format!("@tag({})", attrs.join(", "));
        assert!(parse(&input).is_none());
    }
}

//! Strict parser adapters for complete command-line and configuration values.

use std::path::Path;

use super::cursor::{skip_whitespace, Cursor};
use super::tag::{parse_attribute, parse_tag, parse_tag_name};
use super::value::parse_attr_value;
use crate::models::{AttributeKind, AttributeValue, Tag, TagAttribute};

/// The parser's maximum number of attributes in one tag.
pub use super::tag::MAX_ATTRIBUTES_PER_TAG;

/// A complete-input parser failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrictParseError;

/// Trims only the ASCII whitespace recognized by the tag grammar.
fn trim_ascii(input: &str) -> &str {
    input.trim_matches(|character: char| character.is_ascii_whitespace())
}

/// Parses exactly one complete tag.
pub fn parse_complete_tag(input: &str, synthetic_path: &Path) -> Result<Tag, StrictParseError> {
    let input = trim_ascii(input);
    let mut cursor = Cursor::new(input);
    let tag = parse_tag(&mut cursor, synthetic_path).ok_or(StrictParseError)?;
    skip_whitespace(&mut cursor);
    if cursor.is_eof() {
        Ok(tag)
    } else {
        Err(StrictParseError)
    }
}

/// Validates and returns exactly one complete tag name without an `@`.
pub fn validate_complete_tag_name(input: &str) -> Result<String, StrictParseError> {
    let mut cursor = Cursor::new(input);
    let name = parse_tag_name(&mut cursor).ok_or(StrictParseError)?;
    if cursor.is_eof() {
        Ok(name)
    } else {
        Err(StrictParseError)
    }
}

/// Parses exactly one complete named attribute.
pub fn parse_complete_named_attribute(input: &str) -> Result<TagAttribute, StrictParseError> {
    let input = trim_ascii(input);
    let mut cursor = Cursor::new(input);
    let attribute = parse_attribute(&mut cursor).ok_or(StrictParseError)?;
    skip_whitespace(&mut cursor);
    if !cursor.is_eof() || !matches!(attribute.kind, AttributeKind::Named { .. }) {
        return Err(StrictParseError);
    }
    Ok(attribute)
}

/// Parses exactly one complete attribute value expression.
pub fn parse_complete_attribute_value(input: &str) -> Result<AttributeValue, StrictParseError> {
    let input = trim_ascii(input);
    let mut cursor = Cursor::new(input);
    let value = parse_attr_value(&mut cursor).ok_or(StrictParseError)?;
    skip_whitespace(&mut cursor);
    if cursor.is_eof() {
        Ok(value)
    } else {
        Err(StrictParseError)
    }
}

/// Reports whether text is outside the reversible generic-create domain.
pub fn contains_forbidden_created_text(value: &str) -> bool {
    value
        .chars()
        .any(|character| character.is_control() || matches!(character, '\u{2028}' | '\u{2029}'))
}

/// Validates a value for canonical generic tag creation.
pub fn validate_creatable_value(value: &AttributeValue) -> Result<(), StrictParseError> {
    match value {
        AttributeValue::Float(value) if !value.is_finite() => Err(StrictParseError),
        AttributeValue::Str(value) if contains_forbidden_created_text(value) => {
            Err(StrictParseError)
        }
        _ => Ok(()),
    }
}

/// Validates every value in a tag for canonical generic tag creation.
pub fn validate_creatable_tag(tag: &Tag) -> Result<(), StrictParseError> {
    tag.attributes.iter().try_for_each(|attribute| {
        let value = match &attribute.kind {
            AttributeKind::Named { value, .. } | AttributeKind::Positional { value } => value,
        };
        validate_creatable_value(value)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_parsers_reject_trailing_input_and_positionals() {
        let path = Path::new("<test>");
        assert!(parse_complete_tag(" @tag(a=1) ", path).is_ok());
        assert!(parse_complete_tag("@tag trailing", path).is_err());
        assert!(validate_complete_tag_name("name").is_ok());
        assert!(validate_complete_tag_name("@name").is_err());
        assert!(parse_complete_named_attribute("key=`a, b (c) d=e`").is_ok());
        assert!(parse_complete_named_attribute("value").is_err());
        assert!(parse_complete_named_attribute("key=1, other=2").is_err());
        assert!(parse_complete_attribute_value("\"value\"").is_ok());
        assert!(parse_complete_attribute_value("value trailing").is_err());
    }

    #[test]
    fn creatable_domain_rejects_nonfinite_and_controls() {
        assert!(validate_creatable_value(&AttributeValue::Float(f64::INFINITY)).is_err());
        for value in [
            "\r", "\n", "\t", "\u{1b}", "\u{7f}", "\u{85}", "\u{2028}", "\u{2029}",
        ] {
            assert!(validate_creatable_value(&AttributeValue::Str(value.to_string())).is_err());
        }
        assert!(validate_creatable_value(&AttributeValue::Str("safe text".to_string())).is_ok());
    }
}

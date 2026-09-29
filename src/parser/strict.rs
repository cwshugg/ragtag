//! Strict parser adapters for complete command-line and configuration values.

use std::path::Path;

use super::cursor::{skip_whitespace, Cursor};
use super::tag::{
    parse_attribute_with_value_span, parse_tag, parse_tag_name, parse_tag_with_value_spans,
};
use super::value::parse_attr_value;
use crate::models::{AttributeKind, AttributeValue, Tag, TagAttribute};

/// The parser's maximum number of attributes in one tag.
pub(crate) use super::tag::MAX_ATTRIBUTES_PER_TAG;

/// A complete-input parser failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StrictParseError;

/// Trims only the ASCII whitespace recognized by the tag grammar.
fn trim_ascii(input: &str) -> &str {
    input.trim_matches(|character: char| character.is_ascii_whitespace())
}

/// Parses exactly one complete tag.
pub(crate) fn parse_complete_tag(
    input: &str,
    synthetic_path: &Path,
) -> Result<Tag, StrictParseError> {
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

/// Parses one complete tag and returns its ordered raw value expressions.
pub(crate) fn parse_complete_tag_with_value_lexemes(
    input: &str,
    synthetic_path: &Path,
) -> Result<(Tag, Vec<String>), StrictParseError> {
    let input = trim_ascii(input);
    let mut cursor = Cursor::new(input);
    let (tag, spans) =
        parse_tag_with_value_spans(&mut cursor, synthetic_path).ok_or(StrictParseError)?;
    skip_whitespace(&mut cursor);
    if !cursor.is_eof() {
        return Err(StrictParseError);
    }
    let lexemes = spans
        .into_iter()
        .map(|span| input[span].to_string())
        .collect();
    Ok((tag, lexemes))
}

/// Validates and returns exactly one complete tag name without an `@`.
pub(crate) fn validate_complete_tag_name(input: &str) -> Result<String, StrictParseError> {
    let mut cursor = Cursor::new(input);
    let name = parse_tag_name(&mut cursor).ok_or(StrictParseError)?;
    if cursor.is_eof() {
        Ok(name)
    } else {
        Err(StrictParseError)
    }
}

/// Parses one complete named attribute and returns its raw value expression.
pub(crate) fn parse_complete_named_attribute_with_value_lexeme(
    input: &str,
) -> Result<(TagAttribute, String), StrictParseError> {
    let input = trim_ascii(input);
    let mut cursor = Cursor::new(input);
    let (attribute, span) = parse_attribute_with_value_span(&mut cursor).ok_or(StrictParseError)?;
    skip_whitespace(&mut cursor);
    if !cursor.is_eof() || !matches!(attribute.kind, AttributeKind::Named { .. }) {
        return Err(StrictParseError);
    }
    Ok((attribute, input[span].to_string()))
}

/// Parses one complete value and returns its grammar-trimmed expression.
pub(crate) fn parse_complete_attribute_value_with_lexeme(
    input: &str,
) -> Result<(AttributeValue, String), StrictParseError> {
    let input = trim_ascii(input);
    let mut cursor = Cursor::new(input);
    let value_start = cursor.pos;
    let value = parse_attr_value(&mut cursor).ok_or(StrictParseError)?;
    let value_end = cursor.pos;
    skip_whitespace(&mut cursor);
    if !cursor.is_eof() {
        return Err(StrictParseError);
    }
    Ok((value, input[value_start..value_end].to_string()))
}

/// Reports whether text is outside the reversible generic-create domain.
pub(crate) fn contains_forbidden_created_text(value: &str) -> bool {
    value
        .chars()
        .any(|character| character.is_control() || matches!(character, '\u{2028}' | '\u{2029}'))
}

/// Validates a value for canonical generic tag creation.
pub(crate) fn validate_creatable_value(value: &AttributeValue) -> Result<(), StrictParseError> {
    match value {
        AttributeValue::Float(value) if !value.is_finite() => Err(StrictParseError),
        AttributeValue::Str(value) if contains_forbidden_created_text(value) => {
            Err(StrictParseError)
        }
        _ => Ok(()),
    }
}

/// Validates every value in a tag for canonical generic tag creation.
pub(crate) fn validate_creatable_tag(tag: &Tag) -> Result<(), StrictParseError> {
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

    /// Returns the semantic value contained by either attribute shape.
    fn attribute_value(attribute: &TagAttribute) -> &AttributeValue {
        match &attribute.kind {
            AttributeKind::Named { value, .. } | AttributeKind::Positional { value } => value,
        }
    }

    #[test]
    fn complete_parsers_reject_trailing_input_and_positionals() {
        let path = Path::new("<test>");
        assert!(parse_complete_tag(" @tag(a=1) ", path).is_ok());
        assert!(parse_complete_tag("@tag trailing", path).is_err());
        assert!(validate_complete_tag_name("name").is_ok());
        assert!(validate_complete_tag_name("@name").is_err());
        assert!(parse_complete_named_attribute_with_value_lexeme("key=`a, b (c) d=e`").is_ok());
        assert!(parse_complete_named_attribute_with_value_lexeme("value").is_err());
        assert!(parse_complete_named_attribute_with_value_lexeme("key=1, other=2").is_err());
        assert!(parse_complete_attribute_value_with_lexeme("\"value\"").is_ok());
        assert!(parse_complete_attribute_value_with_lexeme("value trailing").is_err());
        assert!(parse_complete_tag_with_value_lexemes("@tag(value=1) trailing", path).is_err());
        assert!(parse_complete_named_attribute_with_value_lexeme("key=1, other=2").is_err());
        assert!(parse_complete_attribute_value_with_lexeme("\"value\" trailing").is_err());
    }

    #[test]
    fn lossless_parsers_preserve_order_escapes_whitespace_and_utf8() {
        let path = Path::new("<test>");
        let input = " \t@tag(bare, double=\"a\\\\\\\"b\", 'é', tick=`a\\`b`, path='c\\\\d') \r\n";
        let (tag, lexemes) = parse_complete_tag_with_value_lexemes(input, path).unwrap();
        assert_eq!(
            lexemes,
            [r#"bare"#, r#""a\\\"b""#, "'é'", r#"`a\`b`"#, r#"'c\\d'"#]
        );
        assert_eq!(tag.attributes.len(), lexemes.len());

        let (attribute, lexeme) =
            parse_complete_named_attribute_with_value_lexeme(" \t key = `` \n").unwrap();
        assert_eq!(attribute_value(&attribute).as_str(), Some(""));
        assert_eq!(lexeme, "``");

        let (value, lexeme) = parse_complete_attribute_value_with_lexeme(" \t 'é' \r\n").unwrap();
        assert_eq!(value.as_str(), Some("é"));
        assert_eq!(lexeme, "'é'");
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

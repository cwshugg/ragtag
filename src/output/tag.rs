//! Canonical parser-safe tag formatting primitives.

use std::path::Path;

use crate::error::RagtagError;
use crate::models::{AttributeKind, AttributeValue, NumericBase, Tag};
use crate::parser::{parse_complete_tag, validate_creatable_tag};

/// Generic tag layout selected by create commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagFormat {
    /// Indented attributes on separate lines.
    Multiline,
    /// A physically single-line tag.
    Oneline,
}

/// Escapes a string body for a double-quoted parser value.
pub fn escape_tag_string_body(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Lays out already-rendered attribute tokens without changing their bytes.
pub fn layout_tag(name: &str, attributes: &[String], format: TagFormat) -> String {
    if attributes.is_empty() {
        return format!("@{name}");
    }
    match format {
        TagFormat::Multiline => {
            let body = attributes
                .iter()
                .map(|attribute| format!("    {attribute}"))
                .collect::<Vec<_>>()
                .join(",\n");
            format!("@{name}(\n{body}\n)")
        }
        TagFormat::Oneline => format!("@{name}({})", attributes.join(", ")),
    }
}

/// Formats one generic value with a reversible parser spelling.
pub fn format_value(value: &AttributeValue) -> Result<String, RagtagError> {
    crate::parser::validate_creatable_value(value).map_err(|_| {
        RagtagError::InvalidInput("attribute value cannot be represented safely".to_string())
    })?;
    Ok(match value {
        AttributeValue::Str(value) => format!("\"{}\"", escape_tag_string_body(value)),
        AttributeValue::Integer { value, base }
            if *value < 0 && !matches!(base, NumericBase::Decimal) =>
        {
            return Err(RagtagError::InvalidInput(
                "negative non-decimal integer cannot be represented safely".into(),
            ));
        }
        AttributeValue::Integer { value, base } => match base {
            NumericBase::Decimal => value.to_string(),
            NumericBase::Hex => format!("0x{value:x}"),
            NumericBase::Octal => format!("0o{value:o}"),
            NumericBase::Binary => format!("0b{value:b}"),
        },
        AttributeValue::Float(value) if value.fract() == 0.0 => format!("{value:.1}"),
        AttributeValue::Float(value) => value.to_string(),
    })
}

/// Formats a generic tag and proves that the result reparses identically.
pub fn format_tag(tag: &Tag, format: TagFormat) -> Result<String, RagtagError> {
    validate_creatable_tag(tag).map_err(|_| {
        RagtagError::InvalidInput("tag contains a value that cannot be represented safely".into())
    })?;
    let attributes = tag
        .attributes
        .iter()
        .map(|attribute| match &attribute.kind {
            AttributeKind::Named { name, value } => {
                format_value(value).map(|value| format!("{name}={value}"))
            }
            AttributeKind::Positional { value } => format_value(value),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let rendered = layout_tag(&tag.name, &attributes, format);
    let reparsed = parse_complete_tag(&rendered, Path::new("<formatted-tag>")).map_err(|_| {
        RagtagError::InvalidInput("internal tag formatting validation failed".into())
    })?;
    if reparsed.name != tag.name || reparsed.attributes != tag.attributes {
        return Err(RagtagError::InvalidInput(
            "internal tag formatting changed tag semantics".into(),
        ));
    }
    Ok(rendered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_complete_tag;

    #[test]
    fn generic_format_round_trips_special_strings_and_numeric_types() {
        let tag = parse_complete_tag(
            r#"@note(text=`a, b (c) d=e "q" \\ path`, hex=0xff, whole=2.0, neg=-0.0)"#,
            Path::new("<test>"),
        )
        .unwrap();
        let oneline = format_tag(&tag, TagFormat::Oneline).unwrap();
        assert!(!oneline.contains('\n'));
        assert_eq!(
            parse_complete_tag(&oneline, Path::new("<test>"))
                .unwrap()
                .attributes,
            tag.attributes
        );
        assert!(oneline.contains(r#"text="a, b (c) d=e \"q\" \\ path""#));
    }

    #[test]
    fn empty_and_multiline_layout_are_exact() {
        let empty = parse_complete_tag("@note", Path::new("<test>")).unwrap();
        assert_eq!(format_tag(&empty, TagFormat::Multiline).unwrap(), "@note");
        let tag = parse_complete_tag("@note(a, key=1)", Path::new("<test>")).unwrap();
        assert_eq!(
            format_tag(&tag, TagFormat::Multiline).unwrap(),
            "@note(\n    \"a\",\n    key=1\n)"
        );
    }

    #[test]
    fn formatter_rejects_out_of_domain_and_unrepresentable_values() {
        assert!(format_value(&AttributeValue::Float(f64::NEG_INFINITY)).is_err());
        assert!(format_value(&AttributeValue::Str("line\nbreak".into())).is_err());
        assert!(format_value(&AttributeValue::Integer {
            value: -1,
            base: NumericBase::Hex,
        })
        .is_err());
    }
}

//! Core tag data models.
//!
//! Defines `Tag`, `TagAttribute`, `AttributeKind`, `AttributeValue`,
//! and `NumericBase` — the foundational types produced by the parser
//! and consumed by all downstream modules.

use std::fmt;
use std::ops::Range;

use super::location::TagLocation;

/// The numeric base of an integer attribute value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NumericBase {
    Decimal,
    Hex,
    Octal,
    Binary,
}

/// A parsed attribute value.
///
/// Values are parsed in this order: prefixed int → float → decimal int → string fallback.
#[derive(Debug, Clone, PartialEq)]
pub enum AttributeValue {
    /// A string value (either quoted or a bare word that didn't parse as numeric).
    Str(String),
    /// An integer value with its detected numeric base.
    Integer { value: i64, base: NumericBase },
    /// A floating-point value (decimal point detected in the bare word).
    Float(f64),
}

impl AttributeValue {
    /// Extracts the value as a string slice, if this is a `Str` variant.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            AttributeValue::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Extracts the integer value, if this is an `Integer` variant.
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            AttributeValue::Integer { value, .. } => Some(*value),
            _ => None,
        }
    }

    /// Extracts a float value. Returns the value for `Float` variants,
    /// and converts `Integer` to `f64` as well.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            AttributeValue::Float(f) => Some(*f),
            AttributeValue::Integer { value, .. } => Some(*value as f64),
            _ => None,
        }
    }
}

impl fmt::Display for AttributeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AttributeValue::Str(s) => {
                // Quote strings that contain whitespace or special chars
                if s.contains(|c: char| c.is_whitespace() || c == ',' || c == ')') {
                    write!(f, "\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
                } else {
                    write!(f, "{s}")
                }
            }
            AttributeValue::Integer { value, base } => match base {
                NumericBase::Decimal => write!(f, "{value}"),
                NumericBase::Hex => write!(f, "0x{value:x}"),
                NumericBase::Octal => write!(f, "0o{value:o}"),
                NumericBase::Binary => write!(f, "0b{value:b}"),
            },
            AttributeValue::Float(v) => {
                // Ensure at least one decimal place
                if v.fract() == 0.0 {
                    write!(f, "{v:.1}")
                } else {
                    write!(f, "{v}")
                }
            }
        }
    }
}

/// Describes whether an attribute is named (key=value) or positional.
#[derive(Debug, Clone, PartialEq)]
pub enum AttributeKind {
    /// A named attribute: `key=value`.
    Named { name: String, value: AttributeValue },
    /// A positional attribute: just a value.
    Positional { value: AttributeValue },
}

/// A single parsed tag attribute.
#[derive(Debug, Clone, PartialEq)]
pub struct TagAttribute {
    /// Whether this attribute is named or positional.
    pub kind: AttributeKind,
}

impl TagAttribute {
    /// Creates a new named attribute.
    pub fn named(name: impl Into<String>, value: AttributeValue) -> Self {
        Self {
            kind: AttributeKind::Named {
                name: name.into(),
                value,
            },
        }
    }

    /// Creates a new positional attribute.
    pub fn positional(value: AttributeValue) -> Self {
        Self {
            kind: AttributeKind::Positional { value },
        }
    }
}

/// A parsed tag from a source file.
///
/// Represents `@name(attr1, key=value, ...)` or simply `@name`.
#[derive(Debug, Clone, PartialEq)]
pub struct Tag {
    /// The tag name (without the `@` prefix).
    pub name: String,
    /// The list of parsed attributes.
    pub attributes: Vec<TagAttribute>,
    /// The location where this tag was found in the source file.
    pub location: TagLocation,
    /// The byte range of the entire tag in the source file.
    pub raw_span: Range<usize>,
}

impl Tag {
    /// Looks up a named attribute by key.
    ///
    /// Returns the *first* match if multiple attributes share the same name
    /// ("first match wins" semantics). Returns `None` if no attribute with
    /// the given name exists.
    pub fn get_named_attribute(&self, name: &str) -> Option<&AttributeValue> {
        self.attributes.iter().find_map(|attr| match &attr.kind {
            AttributeKind::Named {
                name: n, value: v, ..
            } if n == name => Some(v),
            _ => None,
        })
    }

    /// Looks up a positional attribute by index (0-based among positional attrs only).
    pub fn get_positional_attribute(&self, index: usize) -> Option<&AttributeValue> {
        self.attributes
            .iter()
            .filter_map(|attr| match &attr.kind {
                AttributeKind::Positional { value } => Some(value),
                _ => None,
            })
            .nth(index)
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}", self.name)?;
        if !self.attributes.is_empty() {
            write!(f, "(")?;
            for (i, attr) in self.attributes.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                match &attr.kind {
                    AttributeKind::Named { name, value } => {
                        write!(f, "{name}={value}")?;
                    }
                    AttributeKind::Positional { value } => {
                        write!(f, "{value}")?;
                    }
                }
            }
            write!(f, ")")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_location() -> TagLocation {
        TagLocation::new(PathBuf::from("test.md"), 1, 1, 0, 10)
    }

    #[test]
    fn test_tag_construction_no_attrs() {
        let tag = Tag {
            name: "todo".to_string(),
            attributes: vec![],
            location: make_location(),
            raw_span: 0..5,
        };
        assert_eq!(tag.name, "todo");
        assert!(tag.attributes.is_empty());
    }

    #[test]
    fn test_tag_named_attribute_lookup() {
        let tag = Tag {
            name: "tag".to_string(),
            attributes: vec![
                TagAttribute::named("key", AttributeValue::Str("value".to_string())),
                TagAttribute::named(
                    "num",
                    AttributeValue::Integer {
                        value: 42,
                        base: NumericBase::Decimal,
                    },
                ),
            ],
            location: make_location(),
            raw_span: 0..20,
        };
        assert_eq!(
            tag.get_named_attribute("key").unwrap().as_str(),
            Some("value")
        );
        assert_eq!(
            tag.get_named_attribute("num").unwrap().as_integer(),
            Some(42)
        );
        assert!(tag.get_named_attribute("missing").is_none());
    }

    #[test]
    fn test_tag_positional_attribute_lookup() {
        let tag = Tag {
            name: "tag".to_string(),
            attributes: vec![
                TagAttribute::positional(AttributeValue::Str("hello".to_string())),
                TagAttribute::positional(AttributeValue::Integer {
                    value: 10,
                    base: NumericBase::Decimal,
                }),
            ],
            location: make_location(),
            raw_span: 0..20,
        };
        assert_eq!(
            tag.get_positional_attribute(0).unwrap().as_str(),
            Some("hello")
        );
        assert_eq!(
            tag.get_positional_attribute(1).unwrap().as_integer(),
            Some(10)
        );
        assert!(tag.get_positional_attribute(2).is_none());
    }

    #[test]
    fn test_duplicate_named_first_wins() {
        let tag = Tag {
            name: "tag".to_string(),
            attributes: vec![
                TagAttribute::named("key", AttributeValue::Str("a".to_string())),
                TagAttribute::named("key", AttributeValue::Str("b".to_string())),
            ],
            location: make_location(),
            raw_span: 0..20,
        };
        assert_eq!(tag.get_named_attribute("key").unwrap().as_str(), Some("a"));
    }

    #[test]
    fn test_attribute_value_as_float_from_integer() {
        let v = AttributeValue::Integer {
            value: 4,
            base: NumericBase::Decimal,
        };
        assert_eq!(v.as_float(), Some(4.0));
    }

    #[test]
    fn test_attribute_value_as_float_from_float() {
        let v = AttributeValue::Float(4.5);
        assert_eq!(v.as_float(), Some(4.5));
    }

    #[test]
    fn test_attribute_value_as_str_from_non_str() {
        let v = AttributeValue::Integer {
            value: 1,
            base: NumericBase::Decimal,
        };
        assert!(v.as_str().is_none());
    }

    #[test]
    fn test_display_integer_bases() {
        assert_eq!(
            format!(
                "{}",
                AttributeValue::Integer {
                    value: 255,
                    base: NumericBase::Hex
                }
            ),
            "0xff"
        );
        assert_eq!(
            format!(
                "{}",
                AttributeValue::Integer {
                    value: 63,
                    base: NumericBase::Octal
                }
            ),
            "0o77"
        );
        assert_eq!(
            format!(
                "{}",
                AttributeValue::Integer {
                    value: 10,
                    base: NumericBase::Binary
                }
            ),
            "0b1010"
        );
        assert_eq!(
            format!(
                "{}",
                AttributeValue::Integer {
                    value: 42,
                    base: NumericBase::Decimal
                }
            ),
            "42"
        );
    }

    #[test]
    fn test_display_float() {
        assert_eq!(format!("{}", AttributeValue::Float(4.5)), "4.5");
        assert_eq!(format!("{}", AttributeValue::Float(4.0)), "4.0");
    }

    #[test]
    fn test_display_str() {
        assert_eq!(
            format!("{}", AttributeValue::Str("hello".to_string())),
            "hello"
        );
        assert_eq!(
            format!("{}", AttributeValue::Str("hello world".to_string())),
            "\"hello world\""
        );
    }

    #[test]
    fn test_tag_display() {
        let tag = Tag {
            name: "task".to_string(),
            attributes: vec![
                TagAttribute::named("id", AttributeValue::Str("abc".to_string())),
                TagAttribute::positional(AttributeValue::Integer {
                    value: 42,
                    base: NumericBase::Decimal,
                }),
            ],
            location: make_location(),
            raw_span: 0..20,
        };
        assert_eq!(format!("{tag}"), "@task(id=abc, 42)");
    }

    #[test]
    fn test_tag_display_no_attrs() {
        let tag = Tag {
            name: "note".to_string(),
            attributes: vec![],
            location: make_location(),
            raw_span: 0..5,
        };
        assert_eq!(format!("{tag}"), "@note");
    }

    #[test]
    fn test_mixed_named_positional() {
        let tag = Tag {
            name: "tag".to_string(),
            attributes: vec![
                TagAttribute::positional(AttributeValue::Str("pos".to_string())),
                TagAttribute::named("key", AttributeValue::Str("val".to_string())),
            ],
            location: make_location(),
            raw_span: 0..20,
        };
        assert_eq!(
            tag.get_positional_attribute(0).unwrap().as_str(),
            Some("pos")
        );
        assert_eq!(
            tag.get_named_attribute("key").unwrap().as_str(),
            Some("val")
        );
    }
}

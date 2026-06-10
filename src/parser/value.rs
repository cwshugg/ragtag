//! Attribute value parsing.
//!
//! Handles parsing of quoted strings, numeric literals (hex, octal, binary,
//! float, decimal), and bare word fallback.

use super::cursor::Cursor;
use crate::models::tag::{AttributeValue, NumericBase};

/// Parses an attribute value at the current cursor position.
///
/// Dispatches to quoted string parsing if the next character is a quote,
/// otherwise attempts bare value parsing.
pub fn parse_attr_value(cursor: &mut Cursor) -> Option<AttributeValue> {
    match cursor.peek()? {
        '"' => parse_quoted_string(cursor, '"'),
        '\'' => parse_quoted_string(cursor, '\''),
        _ => parse_bare_value(cursor),
    }
}

/// Parses a quoted string value, handling backslash escapes.
///
/// Backslash causes the next character to be included literally — there is
/// no special interpretation of `\n`, `\t`, etc.
pub fn parse_quoted_string(cursor: &mut Cursor, quote_char: char) -> Option<AttributeValue> {
    // Advance past the opening quote
    cursor.advance()?;
    let mut result = String::new();

    loop {
        let ch = cursor.peek()?; // EOF before closing quote → None
        cursor.advance();
        if ch == '\\' {
            // Escape: include the next character literally
            let escaped = cursor.advance()?;
            result.push(escaped);
        } else if ch == quote_char {
            return Some(AttributeValue::Str(result));
        } else {
            result.push(ch);
        }
    }
}

/// Maximum allowed length for a bare (unquoted) value (in characters).
const MAX_BARE_VALUE_LENGTH: usize = 4096;

/// Parses a bare (unquoted) value and attempts numeric conversion.
///
/// Accumulates characters that are not whitespace, `,`, `)`, `'`, `"`, or `=`.
/// Then attempts conversion in order: prefixed int → float → decimal int → string.
pub fn parse_bare_value(cursor: &mut Cursor) -> Option<AttributeValue> {
    let mut word = String::new();

    while let Some(ch) = cursor.peek() {
        if ch.is_ascii_whitespace()
            || ch == ','
            || ch == ')'
            || ch == '\''
            || ch == '"'
            || ch == '='
        {
            break;
        }
        cursor.advance();
        word.push(ch);
        if word.len() > MAX_BARE_VALUE_LENGTH {
            return None;
        }
    }

    if word.is_empty() {
        return None;
    }

    Some(convert_bare_value(&word))
}

/// Converts a bare word string to the appropriate `AttributeValue`.
///
/// Conversion order:
/// 1. Prefixed integer (0x, 0o, 0b)
/// 2. Float (contains a decimal point)
/// 3. Decimal integer
/// 4. String fallback
fn convert_bare_value(word: &str) -> AttributeValue {
    // 1. Prefixed integers
    if let Some(hex_str) = word.strip_prefix("0x").or_else(|| word.strip_prefix("0X")) {
        if let Ok(value) = i64::from_str_radix(hex_str, 16) {
            return AttributeValue::Integer {
                value,
                base: NumericBase::Hex,
            };
        }
    }
    if let Some(oct_str) = word.strip_prefix("0o").or_else(|| word.strip_prefix("0O")) {
        if let Ok(value) = i64::from_str_radix(oct_str, 8) {
            return AttributeValue::Integer {
                value,
                base: NumericBase::Octal,
            };
        }
    }
    if let Some(bin_str) = word.strip_prefix("0b").or_else(|| word.strip_prefix("0B")) {
        if let Ok(value) = i64::from_str_radix(bin_str, 2) {
            return AttributeValue::Integer {
                value,
                base: NumericBase::Binary,
            };
        }
    }

    // 2. Float (must contain a decimal point)
    if word.contains('.') {
        if let Ok(value) = word.parse::<f64>() {
            return AttributeValue::Float(value);
        }
    }

    // 3. Decimal integer
    if let Ok(value) = word.parse::<i64>() {
        return AttributeValue::Integer {
            value,
            base: NumericBase::Decimal,
        };
    }

    // 4. String fallback
    AttributeValue::Str(word.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_value(input: &str) -> Option<AttributeValue> {
        let mut cursor = Cursor::new(input);
        parse_attr_value(&mut cursor)
    }

    #[test]
    fn test_double_quoted_string() {
        assert_eq!(
            parse_value("\"hello world\""),
            Some(AttributeValue::Str("hello world".to_string()))
        );
    }

    #[test]
    fn test_single_quoted_string() {
        assert_eq!(
            parse_value("'hello world'"),
            Some(AttributeValue::Str("hello world".to_string()))
        );
    }

    #[test]
    fn test_escape_quote_in_string() {
        assert_eq!(
            parse_value("\"he said \\\"hi\\\"\""),
            Some(AttributeValue::Str("he said \"hi\"".to_string()))
        );
    }

    #[test]
    fn test_escape_backslash() {
        assert_eq!(
            parse_value("\"path\\\\here\""),
            Some(AttributeValue::Str("path\\here".to_string()))
        );
    }

    #[test]
    fn test_backslash_n_is_literal() {
        // Backslash causes the next character to be included literally.
        // \n stores just 'n' (the literal next character), NOT '\' + 'n'.
        assert_eq!(
            parse_value("\"hello\\nworld\""),
            Some(AttributeValue::Str("hellonworld".to_string()))
        );
    }

    #[test]
    fn test_unterminated_string() {
        assert_eq!(parse_value("\"unterminated"), None);
    }

    #[test]
    fn test_bare_decimal_integer() {
        assert_eq!(
            parse_value("42"),
            Some(AttributeValue::Integer {
                value: 42,
                base: NumericBase::Decimal
            })
        );
    }

    #[test]
    fn test_bare_negative_integer() {
        assert_eq!(
            parse_value("-7"),
            Some(AttributeValue::Integer {
                value: -7,
                base: NumericBase::Decimal
            })
        );
    }

    #[test]
    fn test_bare_hex() {
        assert_eq!(
            parse_value("0xff"),
            Some(AttributeValue::Integer {
                value: 255,
                base: NumericBase::Hex
            })
        );
    }

    #[test]
    fn test_bare_octal() {
        assert_eq!(
            parse_value("0o77"),
            Some(AttributeValue::Integer {
                value: 63,
                base: NumericBase::Octal
            })
        );
    }

    #[test]
    fn test_bare_binary() {
        assert_eq!(
            parse_value("0b1010"),
            Some(AttributeValue::Integer {
                value: 10,
                base: NumericBase::Binary
            })
        );
    }

    #[test]
    fn test_bare_float() {
        assert_eq!(parse_value("4.5"), Some(AttributeValue::Float(4.5)));
    }

    #[test]
    fn test_bare_negative_float() {
        assert_eq!(parse_value("-1.25"), Some(AttributeValue::Float(-1.25)));
    }

    #[test]
    fn test_integer_not_float() {
        // "4" (no decimal point) should be Integer, not Float
        assert_eq!(
            parse_value("4"),
            Some(AttributeValue::Integer {
                value: 4,
                base: NumericBase::Decimal
            })
        );
    }

    #[test]
    fn test_bare_word_string() {
        assert_eq!(
            parse_value("hello"),
            Some(AttributeValue::Str("hello".to_string()))
        );
    }

    #[test]
    fn test_bare_word_stops_at_comma() {
        let mut cursor = Cursor::new("hello,world");
        let val = parse_attr_value(&mut cursor);
        assert_eq!(val, Some(AttributeValue::Str("hello".to_string())));
        assert_eq!(cursor.peek(), Some(','));
    }

    #[test]
    fn test_bare_word_stops_at_paren() {
        let mut cursor = Cursor::new("hello)");
        let val = parse_attr_value(&mut cursor);
        assert_eq!(val, Some(AttributeValue::Str("hello".to_string())));
        assert_eq!(cursor.peek(), Some(')'));
    }

    #[test]
    fn test_0x_no_digits_is_string() {
        assert_eq!(
            parse_value("0x"),
            Some(AttributeValue::Str("0x".to_string()))
        );
    }

    #[test]
    fn test_overflow_integer_is_string() {
        assert_eq!(
            parse_value("99999999999999999999"),
            Some(AttributeValue::Str("99999999999999999999".to_string()))
        );
    }

    #[test]
    fn test_empty_input() {
        assert_eq!(parse_value(""), None);
    }

    #[test]
    fn test_bare_value_too_long() {
        // A bare value exceeding MAX_BARE_VALUE_LENGTH should return None
        let long_word: String = "a".repeat(4097);
        assert_eq!(parse_value(&long_word), None);
    }

    #[test]
    fn test_bare_value_at_max_length() {
        // A bare value exactly at MAX_BARE_VALUE_LENGTH should succeed
        let word: String = "a".repeat(4096);
        assert_eq!(parse_value(&word), Some(AttributeValue::Str(word.clone())));
    }

    #[test]
    fn test_bare_stops_at_whitespace() {
        let mut cursor = Cursor::new("done next");
        let val = parse_attr_value(&mut cursor);
        assert_eq!(val, Some(AttributeValue::Str("done".to_string())));
    }
}

//! Shared low-level scanning helpers for surgical tag-text edits.
//!
//! Both the format-preserving pipeline and the surgical value replacer
//! need to locate where an attribute's raw value ends inside the tag
//! text. Keeping that logic — and the set of accepted quote delimiters
//! — in one place prevents the two edit paths from drifting apart.

/// Byte values recognized as quote delimiters for attribute values.
///
/// Single source of truth for the accepted delimiter set on the edit
/// path: any change here applies to every edit-path scan at once.
pub(crate) const QUOTE_DELIMITERS: [u8; 3] = [b'"', b'\'', b'`'];

/// Returns whether `byte` opens or closes a quoted attribute value.
pub(crate) fn is_quote_delimiter(byte: u8) -> bool {
    QUOTE_DELIMITERS.contains(&byte)
}

/// Returns the exclusive byte offset at which the attribute value
/// beginning at `value_start` ends within `tag_text`.
///
/// A value opening with a quote delimiter is scanned with
/// backslash-escape awareness, so an escaped delimiter (or an escaped
/// backslash) does not terminate the value. Any other value is treated
/// as bare and ends at the first whitespace, `,`, or `)`.
///
/// `value_start` must be the byte offset of the first byte of the
/// value (past the `=` and any leading whitespace).
pub(crate) fn attr_value_end(tag_text: &str, value_start: usize) -> usize {
    let bytes = tag_text.as_bytes();
    if value_start < tag_text.len() && is_quote_delimiter(bytes[value_start]) {
        let quote = bytes[value_start];
        let mut end = value_start + 1;
        while end < tag_text.len() {
            if bytes[end] == b'\\' {
                end += 2;
                if end >= tag_text.len() {
                    break;
                }
                continue;
            }
            if bytes[end] == quote {
                end += 1;
                break;
            }
            end += 1;
        }
        end
    } else {
        let mut end = value_start;
        while end < tag_text.len() {
            let b = bytes[end];
            if b.is_ascii_whitespace() || b == b',' || b == b')' {
                break;
            }
            end += 1;
        }
        end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_quote_delimiter() {
        assert!(is_quote_delimiter(b'"'));
        assert!(is_quote_delimiter(b'\''));
        assert!(is_quote_delimiter(b'`'));
        assert!(!is_quote_delimiter(b'x'));
        assert!(!is_quote_delimiter(b' '));
    }

    #[test]
    fn test_double_quoted_span() {
        let s = r#""hello world" rest"#;
        assert_eq!(attr_value_end(s, 0), 13);
    }

    #[test]
    fn test_single_quoted_span() {
        let s = "'a, b')";
        assert_eq!(attr_value_end(s, 0), 6);
    }

    #[test]
    fn test_backtick_quoted_span() {
        let s = "`echo hi`, id";
        assert_eq!(attr_value_end(s, 0), 9);
    }

    #[test]
    fn test_escaped_delimiter_does_not_terminate() {
        let s = r#"`a \` b`, x"#;
        // The escaped backtick is skipped; the span ends at the real one.
        assert_eq!(attr_value_end(s, 0), 8);
    }

    #[test]
    fn test_bare_value_span() {
        let s = "42, id";
        assert_eq!(attr_value_end(s, 0), 2);
    }

    #[test]
    fn test_bare_value_to_close_paren() {
        let s = "42)";
        assert_eq!(attr_value_end(s, 0), 2);
    }

    #[test]
    fn test_trailing_backslash_no_overflow_panic() {
        // A trailing backslash escapes past the end; must not panic and
        // must report an end at or just past the string length.
        let s = r#""hello\"#;
        let end = attr_value_end(s, 0);
        assert!(end >= s.len());
    }
}

//! Generic formatting helpers.
//!
//! Provides utilities for table alignment, string truncation,
//! and other output formatting needs.

/// Right-pads a string to the specified width.
pub fn pad_right(s: &str, width: usize) -> String {
    if s.len() >= width {
        s.to_string()
    } else {
        format!("{s:width$}")
    }
}

/// Truncates a string to `max_len` characters, appending "..." if truncated.
///
/// Uses character count (Unicode code points) consistently for both
/// the length check and the truncation.
pub fn truncate(s: &str, max_len: usize) -> String {
    if max_len <= 3 {
        return s.chars().take(max_len).collect();
    }
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len - 3).collect();
        format!("{truncated}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pad_right() {
        assert_eq!(pad_right("hi", 5), "hi   ");
    }

    #[test]
    fn test_pad_right_already_wide() {
        assert_eq!(pad_right("hello", 3), "hello");
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello world", 8), "hello...");
    }

    #[test]
    fn test_truncate_short_enough() {
        assert_eq!(truncate("hi", 10), "hi");
    }

    #[test]
    fn test_truncate_exact() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_multibyte_utf8() {
        // "héllo wörld" has 11 characters but more than 11 bytes
        let s = "héllo wörld";
        assert_eq!(s.chars().count(), 11);
        assert!(s.len() > 11); // more bytes than chars
                               // Truncating to 8 chars should give 5 chars + "..."
        let result = truncate(s, 8);
        assert_eq!(result, "héllo...");
    }
}

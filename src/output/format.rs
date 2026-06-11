//! Generic formatting helpers.
//!
//! Provides utilities for table alignment, string truncation,
//! path coloring, and other output formatting needs.

use std::io::IsTerminal;
use std::path::Path;

use owo_colors::OwoColorize;

use crate::config::ColorMode;

/// Determines whether color output should be used for the given mode.
pub fn should_use_color(color_mode: &ColorMode) -> bool {
    match color_mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => std::io::stdout().is_terminal(),
    }
}

/// Generates an RGB color from a string hash.
///
/// The color is constrained to a mid-range brightness (not too dark,
/// not too bright) for readability on both light and dark terminals.
fn hash_to_color(s: &str) -> (u8, u8, u8) {
    let mut hash: u64 = 5381;
    for b in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(b as u64);
    }

    // Constrain each channel to 90..210 for comfortable brightness.
    const MIN: u8 = 90;
    const RANGE: u8 = 120;
    let r = MIN + ((hash & 0xFF) as u8 % RANGE);
    let g = MIN + (((hash >> 8) & 0xFF) as u8 % RANGE);
    let b = MIN + (((hash >> 16) & 0xFF) as u8 % RANGE);
    (r, g, b)
}

/// Strips a leading `./` or `.\` from a path string.
pub fn strip_dot_slash(s: &str) -> String {
    if s.starts_with("./") || s.starts_with(".\\") {
        s[2..].to_string()
    } else {
        s.to_string()
    }
}

/// Formats a file path with each component colored deterministically.
///
/// Each path component is hashed to produce a unique, mid-brightness
/// color. Separators are left uncolored. Leading `./` is stripped.
pub fn colorize_path(path: &Path, color_mode: &ColorMode) -> String {
    let display = strip_dot_slash(&path.display().to_string());
    if !should_use_color(color_mode) {
        return display;
    }

    let mut result = String::new();
    let has_leading_sep = display.starts_with('/') || display.starts_with('\\');

    if has_leading_sep {
        result.push(display.chars().next().unwrap_or('/'));
    }

    let parts: Vec<&str> = display
        .trim_start_matches(['/', '\\'])
        .split(['/', '\\'])
        .collect();

    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            result.push('/');
        }
        let (r, g, b) = hash_to_color(part);
        result.push_str(&part.truecolor(r, g, b).to_string());
    }

    result
}

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

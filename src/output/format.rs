//! Generic formatting helpers.
//!
//! Provides utilities for table alignment, string truncation,
//! path coloring, and other output formatting needs.

use std::fmt;
use std::io::IsTerminal;
use std::path::Path;

use owo_colors::OwoColorize;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::config::ColorMode;

/// A display wrapper that renders untrusted text without terminal control effects.
///
/// Printable text is preserved verbatim. Control characters and Unicode
/// bidirectional formatting controls are rendered as visible Rust-style escapes.
#[derive(Debug, Clone, Copy)]
pub struct TerminalSafe<'a>(&'a str);

impl fmt::Display for TerminalSafe<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for character in self.0.chars() {
            if is_terminal_control(character) {
                for escaped in character.escape_default() {
                    formatter.write_fmt(format_args!("{escaped}"))?;
                }
            } else {
                formatter.write_fmt(format_args!("{character}"))?;
            }
        }
        Ok(())
    }
}

/// Wraps repository-controlled text for safe human-readable terminal output.
pub fn terminal_safe(value: &str) -> TerminalSafe<'_> {
    TerminalSafe(value)
}

/// Identifies controls that can affect terminal state or visual ordering.
fn is_terminal_control(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\u{061c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{206f}'
        )
}

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
    let display = terminal_safe(&strip_dot_slash(&path.display().to_string())).to_string();
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

/// Returns the terminal display-cell width of a string.
pub fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Right-pads a string to the specified terminal display-cell width.
pub fn pad_right(s: &str, width: usize) -> String {
    let current_width = display_width(s);
    if current_width >= width {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(width - current_width))
    }
}

/// Truncates a string to `max_width` terminal cells without splitting graphemes.
///
/// An ellipsis is appended when it fits, and its display width counts toward
/// the limit.
pub fn truncate(s: &str, max_width: usize) -> String {
    if display_width(s) <= max_width {
        return s.to_string();
    }

    const ELLIPSIS: &str = "...";
    let ellipsis_width = display_width(ELLIPSIS);
    let suffix_width = if max_width >= ellipsis_width {
        ellipsis_width
    } else {
        0
    };
    let content_width = max_width - suffix_width;
    let mut truncated = String::new();
    let mut used_width = 0;
    for grapheme in s.graphemes(true) {
        let grapheme_width = display_width(grapheme);
        if used_width + grapheme_width > content_width {
            break;
        }
        truncated.push_str(grapheme);
        used_width += grapheme_width;
    }
    if suffix_width > 0 {
        truncated.push_str(ELLIPSIS);
    }
    truncated
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
    fn test_pad_right_uses_terminal_cells() {
        assert_eq!(display_width("阶段"), 4);
        assert_eq!(display_width("e\u{301}"), 1);
        assert_eq!(pad_right("阶段", 6), "阶段  ");
        assert_eq!(pad_right("e\u{301}", 3), "e\u{301}  ");
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

    #[test]
    fn test_truncate_uses_cells_and_preserves_graphemes() {
        assert_eq!(truncate("阶段任务", 7), "阶段...");
        assert_eq!(display_width(&truncate("阶段任务", 7)), 7);
        assert_eq!(
            truncate("e\u{301}e\u{301}e\u{301}e\u{301}e\u{301}e\u{301}", 5),
            "e\u{301}e\u{301}..."
        );
        assert_eq!(
            display_width(&truncate(
                "e\u{301}e\u{301}e\u{301}e\u{301}e\u{301}e\u{301}",
                5
            )),
            5
        );
        assert_eq!(truncate("👩‍💻👩‍💻👩‍💻", 5), "👩‍💻...");
        assert_eq!(truncate("界", 1), "");
        assert_eq!(truncate("abc", 2), "ab");
    }

    #[test]
    fn terminal_safe_preserves_printable_text_and_escapes_controls() {
        let input = "normal\n\r\t\u{1b}[2J\u{1b}]52;c;payload\u{7}\u{202e}\u{206a}\u{0085}";
        assert_eq!(
            terminal_safe(input).to_string(),
            "normal\\n\\r\\t\\u{1b}[2J\\u{1b}]52;c;payload\\u{7}\\u{202e}\\u{206a}\\u{85}"
        );
    }
}

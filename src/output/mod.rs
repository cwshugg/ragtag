//! Output formatting and color handling.
//!
//! Centralizes terminal detection, color mode resolution,
//! and basic formatting utilities.

pub mod format;

use crate::config::ColorMode;

/// Encodes untrusted text for one bounded terminal-safe line.
pub(crate) fn terminal_safe(value: &str, maximum: usize) -> String {
    let mut output = String::new();
    for character in value.chars() {
        let encoded = if character.is_control()
            || matches!(
                character,
                '\u{2028}' | '\u{2029}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
            ) {
            character.escape_unicode().to_string()
        } else {
            character.to_string()
        };
        if output.len().saturating_add(encoded.len()) > maximum {
            return "<value omitted: exceeds output limit>".to_string();
        }
        output.push_str(&encoded);
    }
    output
}

/// Resolves the effective color mode from CLI flags, config, and environment.
///
/// Priority: CLI `--no-color` flag > `NO_COLOR` env var > config setting.
pub fn resolve_color_mode(cli_no_color: bool, config_color: &ColorMode) -> ColorMode {
    if cli_no_color {
        return ColorMode::Never;
    }
    if std::env::var("NO_COLOR").is_ok() {
        return ColorMode::Never;
    }
    config_color.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_no_color_overrides() {
        assert_eq!(
            resolve_color_mode(true, &ColorMode::Always),
            ColorMode::Never
        );
    }

    #[test]
    fn test_config_color_used() {
        assert_eq!(
            resolve_color_mode(false, &ColorMode::Always),
            ColorMode::Always
        );
    }

    #[test]
    fn test_default_auto() {
        assert_eq!(resolve_color_mode(false, &ColorMode::Auto), ColorMode::Auto);
    }

    #[test]
    fn terminal_safe_encodes_controls_and_bidi() {
        let value = terminal_safe("x\n\u{1b}]8;;bad\u{7}\u{202e}", 1024);
        assert!(!value.contains('\n'));
        assert!(!value.contains('\u{1b}'));
        assert!(!value.contains('\u{7}'));
        assert!(!value.contains('\u{202e}'));
    }
}

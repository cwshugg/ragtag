//! Output formatting and color handling.
//!
//! Centralizes terminal detection, color mode resolution,
//! and basic formatting utilities.

pub mod format;

use crate::config::ColorMode;

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
}

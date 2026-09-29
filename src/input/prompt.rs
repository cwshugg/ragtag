//! Terminal-safe interactive prompting shared by create commands.

use std::io::{BufRead, IsTerminal, Write};

use owo_colors::OwoColorize;
use rustyline::error::ReadlineError;

use crate::error::RagtagError;

/// Gray-blue RGB used for prompt field names.
const FIELD_COLOR: (u8, u8, u8) = (140, 170, 210);
/// Dark gray RGB used for prompt hint text.
const HINT_COLOR: (u8, u8, u8) = (128, 128, 128);

/// Produces visible, terminal-safe text before any styling is applied.
pub(crate) fn escape_prompt_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '\r' => escaped.push_str("\\r"),
            '\n' => escaped.push_str("\\n"),
            '\t' => escaped.push_str("\\t"),
            '\u{1b}' => escaped.push_str("\\x1b"),
            '\u{2028}' => escaped.push_str("\\u{2028}"),
            '\u{2029}' => escaped.push_str("\\u{2029}"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\u{{{:X}}}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped
}

/// Wraps ANSI CSI sequences in markers understood by rustyline.
fn wrap_ansi_for_rustyline(value: &str) -> String {
    let mut result = String::with_capacity(value.len() + 32);
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == 0x1b && index + 1 < bytes.len() && bytes[index + 1] == b'[' {
            result.push('\x01');
            result.push('\x1b');
            result.push('[');
            index += 2;
            while index < bytes.len() {
                let byte = bytes[index];
                result.push(byte as char);
                index += 1;
                if byte.is_ascii_alphabetic() {
                    break;
                }
            }
            result.push('\x02');
        } else {
            result.push(bytes[index] as char);
            index += 1;
        }
    }
    result
}

/// Builds a safe prompt, adding color only for a terminal session.
pub(crate) fn make_prompt(field: &str, hint: Option<&str>, is_tty: bool) -> String {
    let field = escape_prompt_text(field);
    let hint = hint.map(escape_prompt_text);
    if !is_tty {
        return match hint {
            Some(hint) => format!("{field} {hint}: "),
            None => format!("{field}: "),
        };
    }
    let field = field
        .truecolor(FIELD_COLOR.0, FIELD_COLOR.1, FIELD_COLOR.2)
        .to_string();
    let colon = ": "
        .truecolor(FIELD_COLOR.0, FIELD_COLOR.1, FIELD_COLOR.2)
        .to_string();
    let prompt = match hint {
        Some(hint) => format!(
            "{field} {}{colon}",
            hint.truecolor(HINT_COLOR.0, HINT_COLOR.1, HINT_COLOR.2)
        ),
        None => format!("{field}{colon}"),
    };
    wrap_ansi_for_rustyline(&prompt)
}

/// A stdin/stderr prompting session with TTY cancellation semantics.
pub(crate) struct PromptSession {
    tty: Option<rustyline::DefaultEditor>,
    /// Whether the session uses a terminal line editor.
    pub(crate) is_tty: bool,
    /// Whether Ctrl+C or Ctrl+D cancelled a terminal session.
    pub(crate) cancelled: bool,
}

impl PromptSession {
    /// Creates a session using the process stdin.
    pub(crate) fn new() -> Result<Self, RagtagError> {
        let tty = if std::io::stdin().is_terminal() {
            Some(rustyline::DefaultEditor::new().map_err(|error| {
                RagtagError::Io(std::io::Error::other(format!(
                    "failed to initialise line editor: {error}"
                )))
            })?)
        } else {
            None
        };
        Ok(Self {
            is_tty: tty.is_some(),
            tty,
            cancelled: false,
        })
    }

    /// Writes one escaped validation error.
    pub(crate) fn write_error(
        &self,
        stderr: &mut dyn Write,
        message: &str,
    ) -> Result<(), RagtagError> {
        let message = escape_prompt_text(message);
        if self.is_tty {
            writeln!(stderr, "  {}", format!("Error: {message}").red())
        } else {
            writeln!(stderr, "  Error: {message}")
        }
        .map_err(RagtagError::Io)
    }

    /// Reads one line, returning `None` for cancellation or piped EOF.
    pub(crate) fn read_line(
        &mut self,
        prompt: &str,
        stderr: &mut dyn Write,
    ) -> Result<Option<String>, RagtagError> {
        if self.cancelled {
            return Ok(None);
        }
        if let Some(editor) = &mut self.tty {
            return match editor.readline(prompt) {
                Ok(line) => Ok(Some(line)),
                Err(ReadlineError::Eof | ReadlineError::Interrupted) => {
                    writeln!(stderr).map_err(RagtagError::Io)?;
                    self.cancelled = true;
                    Ok(None)
                }
                Err(error) => Err(RagtagError::Io(std::io::Error::other(format!(
                    "readline error: {error}"
                )))),
            };
        }

        write!(stderr, "{prompt}").map_err(RagtagError::Io)?;
        stderr.flush().map_err(RagtagError::Io)?;
        let mut line = String::new();
        match std::io::stdin().lock().read_line(&mut line) {
            Ok(0) => Ok(None),
            Ok(_) => Ok(Some(line)),
            Err(error) => Err(RagtagError::Io(error)),
        }
    }

    /// Prompts until a required nonblank value is supplied.
    pub(crate) fn prompt_required(
        &mut self,
        prompt: &str,
        empty_error: &str,
        stderr: &mut dyn Write,
    ) -> Result<Option<String>, RagtagError> {
        loop {
            match self.read_line(prompt, stderr)? {
                None if self.cancelled => return Ok(None),
                None => {
                    return Err(RagtagError::Io(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "unexpected end of input while reading required field",
                    )));
                }
                Some(line) if line.trim().is_empty() => self.write_error(stderr, empty_error)?,
                Some(line) => return Ok(Some(line.trim().to_string())),
            }
        }
    }

    /// Prompts for an optional validated value.
    pub(crate) fn prompt_optional(
        &mut self,
        prompt: &str,
        stderr: &mut dyn Write,
        validate: impl Fn(&str) -> Result<(), String>,
    ) -> Result<Option<String>, RagtagError> {
        loop {
            let Some(line) = self.read_line(prompt, stderr)? else {
                return Ok(None);
            };
            let value = line.trim();
            if value.is_empty() {
                return Ok(None);
            }
            match validate(value) {
                Ok(()) => return Ok(Some(value.to_string())),
                Err(message) => self.write_error(stderr, &message)?,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_text_escapes_controls_before_layout() {
        assert_eq!(
            escape_prompt_text("\\\r\n\t\u{1b}\u{7f}\u{85}\u{2028}\u{2029}"),
            "\\\\\\r\\n\\t\\x1b\\u{7F}\\u{85}\\u{2028}\\u{2029}"
        );
        assert_eq!(
            make_prompt("Field\n", Some("(x\t)"), false),
            "Field\\n (x\\t): "
        );
    }
}

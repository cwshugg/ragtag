//! Bounded structured diagnostics for diagram generation.

use std::io::Write;
use std::path::PathBuf;

use crate::output::terminal_safe;

const MAXIMUM_PARAMETERS: usize = 64;
const MAXIMUM_AGGREGATE_PARAMETERS: usize = 4_096;
const MAXIMUM_PARAMETER_BYTES: usize = 2 * 1024 * 1024;
const MAXIMUM_RENDERED_BYTES: usize = 8 * 1024 * 1024;

/// Diagram diagnostic severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Severity {
    #[allow(dead_code)]
    Warning,
    Error,
}

/// A source reference attached to a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceRef {
    pub(crate) path: PathBuf,
    pub(crate) line: usize,
    pub(crate) column: usize,
}

/// A structured diagnostic with trusted code/message and bounded parameters.
#[derive(Debug, Clone)]
pub(crate) struct Diagnostic {
    pub(crate) severity: Severity,
    pub(crate) code: &'static str,
    pub(crate) message: &'static str,
    pub(crate) parameters: Vec<String>,
    pub(crate) source: Option<SourceRef>,
}

impl Diagnostic {
    pub(crate) fn error(code: &'static str, message: &'static str) -> Self {
        Self {
            severity: Severity::Error,
            code,
            message,
            parameters: Vec::new(),
            source: None,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn warning(code: &'static str, message: &'static str) -> Self {
        Self {
            severity: Severity::Warning,
            code,
            message,
            parameters: Vec::new(),
            source: None,
        }
    }

    pub(crate) fn parameter(mut self, value: impl AsRef<str>) -> Self {
        if self.parameters.len() >= MAXIMUM_PARAMETERS {
            return self;
        }
        let value = sanitize_parameter(value.as_ref());
        let current = self.parameters.iter().map(String::len).sum::<usize>();
        if current.saturating_add(value.len()) <= MAXIMUM_PARAMETER_BYTES {
            self.parameters.push(value);
        }
        self
    }
}

/// A deterministic bounded diagnostic collection.
#[derive(Debug)]
pub(crate) struct DiagnosticBag {
    diagnostics: Vec<Diagnostic>,
    maximum: usize,
    count_truncated: bool,
    parameters_truncated: bool,
    parameter_count: usize,
    parameter_bytes: usize,
}

impl DiagnosticBag {
    pub(crate) fn new(maximum: usize) -> Self {
        Self {
            diagnostics: Vec::with_capacity(maximum.min(64)),
            maximum,
            count_truncated: false,
            parameters_truncated: false,
            parameter_count: 0,
            parameter_bytes: 0,
        }
    }

    pub(crate) fn push(&mut self, mut diagnostic: Diagnostic) {
        let next_count = self
            .parameter_count
            .saturating_add(diagnostic.parameters.len());
        let next_bytes = self
            .parameter_bytes
            .saturating_add(diagnostic.parameters.iter().map(String::len).sum::<usize>());
        if next_count > MAXIMUM_AGGREGATE_PARAMETERS || next_bytes > MAXIMUM_PARAMETER_BYTES {
            diagnostic.parameters.clear();
            if !self.parameters_truncated {
                self.parameters_truncated = true;
                self.retain_limit(Diagnostic::error(
                    "DIA-LIMIT-002",
                    "additional diagnostic parameters were omitted because their limit was reached",
                ));
            }
        } else {
            self.parameter_count = next_count;
            self.parameter_bytes = next_bytes;
        }
        self.retain(diagnostic);
    }

    fn retain(&mut self, diagnostic: Diagnostic) {
        if self.maximum == 0 {
            self.count_truncated = true;
            return;
        }
        if self.diagnostics.len() < self.maximum {
            self.diagnostics.push(diagnostic);
        } else if !self.count_truncated {
            self.count_truncated = true;
            let replace = self
                .diagnostics
                .iter()
                .rposition(|item| !item.code.starts_with("DIA-LIMIT-"));
            if let Some(index) = replace {
                self.diagnostics.remove(index);
            } else {
                self.diagnostics.pop();
            }
            self.diagnostics.push(Diagnostic::error(
                "DIA-LIMIT-001",
                "additional diagnostics were omitted because the diagnostic limit was reached",
            ));
        }
    }

    fn retain_limit(&mut self, diagnostic: Diagnostic) {
        if self.maximum == 0 {
            return;
        }
        if self.diagnostics.len() < self.maximum {
            self.diagnostics.push(diagnostic);
            return;
        }
        if let Some(index) = self
            .diagnostics
            .iter()
            .rposition(|item| !item.code.starts_with("DIA-LIMIT-"))
        {
            self.diagnostics.remove(index);
            self.diagnostics.push(diagnostic);
        }
    }

    pub(crate) fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
    }

    pub(crate) fn into_vec(self) -> Vec<Diagnostic> {
        self.diagnostics
    }
}

/// Renders diagnostics without emitting terminal controls from untrusted values.
pub(crate) fn render(diagnostics: &[Diagnostic], writer: &mut dyn Write) -> std::io::Result<()> {
    const RENDER_SENTINEL: &[u8] = b"ragtag diagram error[DIA-LIMIT-003]: additional diagnostic output was omitted because the rendered byte limit was reached\n";
    let mut output = Vec::new();
    let mut line_starts = Vec::new();
    let mut rendered_truncated = false;
    for diagnostic in diagnostics {
        let mut line = Vec::new();
        let level = match diagnostic.severity {
            Severity::Warning => "warning",
            Severity::Error => "error",
        };
        write!(
            line,
            "ragtag diagram {level}[{}]: {}",
            diagnostic.code, diagnostic.message
        )?;
        for parameter in &diagnostic.parameters {
            write!(line, " [{parameter}]")?;
        }
        if let Some(source) = &diagnostic.source {
            write!(
                line,
                " ({}:{}:{})",
                terminal_safe(&source.path.to_string_lossy(), 16 * 1024),
                source.line,
                source.column
            )?;
        }
        writeln!(line)?;
        if output.len().saturating_add(line.len()) > MAXIMUM_RENDERED_BYTES {
            while output.len().saturating_add(RENDER_SENTINEL.len()) > MAXIMUM_RENDERED_BYTES {
                let Some(start) = line_starts.pop() else {
                    break;
                };
                output.truncate(start);
            }
            output.extend_from_slice(RENDER_SENTINEL);
            rendered_truncated = true;
            break;
        }
        line_starts.push(output.len());
        output.extend_from_slice(&line);
    }
    writer.write_all(&output)?;
    if rendered_truncated {
        return Err(std::io::Error::other(
            "rendered diagnostic byte limit exceeded",
        ));
    }
    Ok(())
}

fn sanitize_parameter(value: &str) -> String {
    const MAXIMUM: usize = 16 * 1024;
    terminal_safe(value, MAXIMUM)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_bag_reserves_a_truncation_sentinel() {
        let mut bag = DiagnosticBag::new(2);
        bag.push(Diagnostic::warning("ONE", "one"));
        bag.push(Diagnostic::warning("TWO", "two"));
        bag.push(Diagnostic::warning("THREE", "three"));
        let values = bag.into_vec();
        assert_eq!(values.len(), 2);
        assert_eq!(
            values
                .iter()
                .filter(|diagnostic| diagnostic.code == "DIA-LIMIT-001")
                .count(),
            1
        );
    }

    #[test]
    fn rendering_makes_terminal_controls_visible() {
        let diagnostic = Diagnostic::error("DIA", "invalid").parameter("bad\u{1b}[31m\u{202e}");
        let mut output = Vec::new();
        render(&[diagnostic], &mut output).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(!output.contains('\u{1b}'));
        assert!(output.contains("\\u{1b}"));
    }

    #[test]
    fn parameters_and_rendered_output_are_bounded() {
        let mut diagnostic = Diagnostic::error("DIA", "bounded");
        for index in 0..100 {
            diagnostic = diagnostic.parameter(index.to_string());
        }
        assert_eq!(diagnostic.parameters.len(), MAXIMUM_PARAMETERS);

        let value = "x".repeat(16 * 1024);
        let diagnostics = (0..600)
            .map(|_| Diagnostic::error("DIA", "bounded").parameter(&value))
            .collect::<Vec<_>>();
        let mut output = Vec::new();
        assert!(render(&diagnostics, &mut output).is_err());
        let output = String::from_utf8(output).unwrap();
        assert_eq!(output.matches("DIA-LIMIT-003").count(), 1);

        let mut bag = DiagnosticBag::new(1_000);
        for _ in 0..500 {
            bag.push(Diagnostic::error("DIA", "bounded").parameter(&value));
        }
        let retained = bag.into_vec();
        assert_eq!(
            retained
                .iter()
                .filter(|diagnostic| diagnostic.code == "DIA-LIMIT-002")
                .count(),
            1
        );
        let retained_bytes = retained
            .iter()
            .flat_map(|diagnostic| &diagnostic.parameters)
            .map(String::len)
            .sum::<usize>();
        assert!(retained_bytes <= MAXIMUM_PARAMETER_BYTES);
    }
}

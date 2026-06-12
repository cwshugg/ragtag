//! Tag format analysis and regeneration.
//!
//! When editing a tag's attributes in a source file, we want to preserve
//! the original formatting (multiline vs. oneline layout, indentation,
//! attribute order) instead of repeatedly performing surgical string
//! replacements. This module provides a higher-level pipeline that:
//!
//! 1. **Parses** the original tag text to extract formatting metadata.
//! 2. **Applies** a batch of attribute changes to an in-memory model.
//! 3. **Regenerates** the entire tag string, preserving the original
//!    format.
//!
//! The single entry point used by callers is [`edit_task_tag`].

use std::ops::Range;
use std::path::Path;

use crate::error::RagtagError;
use crate::models::{AttributeKind, Tag};
use crate::parser;

/// Describes the formatting of a tag as found in a source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagFormatInfo {
    /// The tag name (e.g., `"task"`).
    pub tag_name: String,
    /// Whether the tag spans multiple lines.
    pub is_multiline: bool,
    /// The indentation string used for each attribute line in a
    /// multiline tag (e.g., `"    "`). Empty for oneline tags.
    pub attr_indent: String,
    /// The indentation string used for the closing `)` in a multiline
    /// tag. Empty for oneline tags.
    pub close_indent: String,
    /// The ordered list of *named* attribute names as they appear in
    /// the original tag.
    pub attr_order: Vec<String>,
    /// Separator placed between attributes when regenerating.
    /// `", "` for oneline; `",\n<attr_indent>"` for multiline.
    pub attr_separator: String,
}

/// Analyzes the formatting of `tag_text` (the raw, in-file text of a
/// single tag) using the already-parsed `Tag` for attribute info.
fn analyze_format(tag_text: &str, tag: &Tag) -> TagFormatInfo {
    let tag_name = tag.name.clone();
    let mut is_multiline = false;
    let mut attr_indent = String::new();
    let mut close_indent = String::new();
    let mut attr_separator = ", ".to_string();

    if let (Some(ps), Some(pe)) = (tag_text.find('('), tag_text.rfind(')')) {
        if ps < pe {
            let inner = &tag_text[ps + 1..pe];
            is_multiline = inner.contains('\n');
            if is_multiline {
                // Indentation of the first attribute line: characters
                // after the first `\n` up to the first non-whitespace
                // (excluding newlines).
                if let Some(nl) = inner.find('\n') {
                    let after_nl = &inner[nl + 1..];
                    attr_indent = after_nl
                        .chars()
                        .take_while(|c| *c == ' ' || *c == '\t')
                        .collect();
                }
                // Indentation of the closing `)`: characters between
                // the last `\n` of `inner` and the `)`.
                if let Some(last_nl) = inner.rfind('\n') {
                    close_indent = inner[last_nl + 1..]
                        .chars()
                        .take_while(|c| *c == ' ' || *c == '\t')
                        .collect();
                }
                attr_separator = format!(",\n{attr_indent}");
            }
        }
    }

    let attr_order: Vec<String> = tag
        .attributes
        .iter()
        .filter_map(|a| match &a.kind {
            AttributeKind::Named { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();

    TagFormatInfo {
        tag_name,
        is_multiline,
        attr_indent,
        close_indent,
        attr_order,
        attr_separator,
    }
}

/// Regenerates a complete tag string from a list of named attribute
/// `(name, raw_value)` pairs, preserving the layout described by
/// `format_info`.
///
/// `attributes` is taken as the authoritative final list (order and
/// contents). Callers are expected to have already merged "new
/// attributes go at the end" / "existing attributes keep their slot"
/// semantics before invoking this function.
pub fn regenerate_tag(format_info: &TagFormatInfo, attributes: &[(String, String)]) -> String {
    let mut out = String::new();
    out.push('@');
    out.push_str(&format_info.tag_name);

    if attributes.is_empty() {
        // Preserve a bare `@tag` form when there are no attributes.
        return out;
    }

    out.push('(');
    if format_info.is_multiline {
        out.push('\n');
        for (i, (name, value)) in attributes.iter().enumerate() {
            out.push_str(&format_info.attr_indent);
            out.push_str(name);
            out.push('=');
            out.push_str(value);
            if i + 1 < attributes.len() {
                out.push(',');
                out.push('\n');
            }
        }
        out.push('\n');
        out.push_str(&format_info.close_indent);
        out.push(')');
    } else {
        for (i, (name, value)) in attributes.iter().enumerate() {
            if i > 0 {
                out.push_str(&format_info.attr_separator);
            }
            out.push_str(name);
            out.push('=');
            out.push_str(value);
        }
        out.push(')');
    }
    out
}

/// Locates the byte span of an attribute's raw value substring within
/// `tag_text`. Returns the half-open range covering exactly the value
/// text (quotes included for string values, no leading/trailing
/// whitespace).
///
/// Returns `None` if the attribute is not found in the text.
fn find_attr_value_span(tag_text: &str, attr_name: &str) -> Option<Range<usize>> {
    let bytes = tag_text.as_bytes();
    let mut search_pos = 0;

    while search_pos < tag_text.len() {
        let rel = tag_text[search_pos..].find(attr_name)?;
        let abs_idx = search_pos + rel;
        let after_name = abs_idx + attr_name.len();

        // Reject substring matches inside longer identifiers.
        let before_ok = abs_idx == 0
            || (!bytes[abs_idx - 1].is_ascii_alphanumeric()
                && bytes[abs_idx - 1] != b'_'
                && bytes[abs_idx - 1] != b'-');

        if before_ok && after_name <= tag_text.len() {
            // Skip whitespace then expect `=`.
            let mut eq_pos = after_name;
            while eq_pos < tag_text.len() && bytes[eq_pos].is_ascii_whitespace() {
                eq_pos += 1;
            }
            if eq_pos < tag_text.len() && bytes[eq_pos] == b'=' {
                let mut vs = eq_pos + 1;
                while vs < tag_text.len() && bytes[vs].is_ascii_whitespace() {
                    vs += 1;
                }
                let value_end = if vs < tag_text.len() && (bytes[vs] == b'"' || bytes[vs] == b'\'')
                {
                    let quote = bytes[vs];
                    let mut end = vs + 1;
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
                    let mut end = vs;
                    while end < tag_text.len() {
                        let b = bytes[end];
                        if b.is_ascii_whitespace() || b == b',' || b == b')' {
                            break;
                        }
                        end += 1;
                    }
                    end
                };
                return Some(vs..value_end);
            }
        }
        search_pos = abs_idx + 1;
    }
    None
}

/// Edits a task tag in-memory by applying a batch of attribute changes
/// then regenerating the entire tag string, preserving the original
/// formatting.
///
/// `changes` is a list of `(attr_name, formatted_value)` pairs. Values
/// must already be in their final on-disk form (e.g., string values
/// wrapped in quotes, numeric values bare).
///
/// * Existing attributes are updated in place (preserving their slot).
/// * New attributes (not present in the original) are appended at the
///   end, using the same separator/indentation as the rest.
/// * The tag's original layout (multiline vs. oneline, indentation,
///   attribute order) is preserved.
pub fn edit_task_tag(
    original_tag_text: &str,
    changes: &[(&str, &str)],
) -> Result<String, RagtagError> {
    // A tag without parens — synthesize one. This matches the
    // historical behavior of `modify_tag_attribute`.
    if !original_tag_text.contains('(') {
        let tag_name_end = original_tag_text
            .find(|c: char| c == '(' || c.is_whitespace())
            .unwrap_or(original_tag_text.len());
        let tag_name = original_tag_text
            .get(1..tag_name_end)
            .unwrap_or("")
            .to_string();
        let format_info = TagFormatInfo {
            tag_name,
            is_multiline: false,
            attr_indent: String::new(),
            close_indent: String::new(),
            attr_order: Vec::new(),
            attr_separator: ", ".to_string(),
        };
        let attrs: Vec<(String, String)> = changes
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        return Ok(regenerate_tag(&format_info, &attrs));
    }

    // Parse the original tag for structural info.
    let tags = parser::scan_file(original_tag_text, Path::new(""));
    if tags.is_empty() {
        return Err(RagtagError::ParseError {
            file: "".into(),
            line: 0,
            message: "failed to re-parse tag for editing".to_string(),
        });
    }
    let tag = &tags[0];
    let format_info = analyze_format(original_tag_text, tag);

    // Build the ordered list of current named attributes, extracting
    // each attribute's *raw* value substring from the original text so
    // formatting (quotes, numeric base, trailing zeros) is preserved.
    let mut attributes: Vec<(String, String)> = Vec::with_capacity(tag.attributes.len());
    for attr in &tag.attributes {
        if let AttributeKind::Named { name, value } = &attr.kind {
            let raw_value = match find_attr_value_span(original_tag_text, name) {
                Some(span) => original_tag_text[span].to_string(),
                // Fallback to `Display` if we somehow couldn't locate
                // the span in the source text.
                None => value.to_string(),
            };
            attributes.push((name.clone(), raw_value));
        }
    }

    // Apply changes: replace existing in-slot, append new at the end.
    for (name, new_value) in changes {
        if let Some(existing) = attributes.iter_mut().find(|(n, _)| n == name) {
            existing.1 = (*new_value).to_string();
        } else {
            attributes.push(((*name).to_string(), (*new_value).to_string()));
        }
    }

    Ok(regenerate_tag(&format_info, &attributes))
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------
    // analyze_format
    // -------------------------------------------------------------------

    fn analyze(tag_text: &str) -> TagFormatInfo {
        let tags = parser::scan_file(tag_text, Path::new(""));
        analyze_format(tag_text, &tags[0])
    }

    #[test]
    fn test_analyze_oneline() {
        let info = analyze(r#"@task(id="abc", status="new")"#);
        assert!(!info.is_multiline);
        assert_eq!(info.tag_name, "task");
        assert_eq!(info.attr_order, vec!["id", "status"]);
        assert_eq!(info.attr_separator, ", ");
    }

    #[test]
    fn test_analyze_multiline_4space_indent() {
        let tag = "@task(\n    id=\"abc\",\n    status=\"new\"\n)";
        let info = analyze(tag);
        assert!(info.is_multiline);
        assert_eq!(info.attr_indent, "    ");
        assert_eq!(info.close_indent, "");
        assert_eq!(info.attr_separator, ",\n    ");
        assert_eq!(info.attr_order, vec!["id", "status"]);
    }

    #[test]
    fn test_analyze_multiline_tab_indent() {
        let tag = "@task(\n\tid=\"abc\",\n\tstatus=\"new\"\n)";
        let info = analyze(tag);
        assert!(info.is_multiline);
        assert_eq!(info.attr_indent, "\t");
    }

    #[test]
    fn test_analyze_multiline_indented_block() {
        // Both attribute lines and closing paren indented.
        let tag = "@task(\n        id=\"abc\",\n        status=\"new\"\n    )";
        let info = analyze(tag);
        assert_eq!(info.attr_indent, "        ");
        assert_eq!(info.close_indent, "    ");
    }

    // -------------------------------------------------------------------
    // regenerate_tag
    // -------------------------------------------------------------------

    #[test]
    fn test_regenerate_oneline() {
        let info = TagFormatInfo {
            tag_name: "task".into(),
            is_multiline: false,
            attr_indent: String::new(),
            close_indent: String::new(),
            attr_order: vec!["id".into(), "status".into()],
            attr_separator: ", ".into(),
        };
        let attrs = vec![
            ("id".into(), "\"abc\"".into()),
            ("status".into(), "\"new\"".into()),
        ];
        assert_eq!(
            regenerate_tag(&info, &attrs),
            r#"@task(id="abc", status="new")"#
        );
    }

    #[test]
    fn test_regenerate_multiline() {
        let info = TagFormatInfo {
            tag_name: "task".into(),
            is_multiline: true,
            attr_indent: "    ".into(),
            close_indent: String::new(),
            attr_order: vec!["id".into(), "status".into()],
            attr_separator: ",\n    ".into(),
        };
        let attrs = vec![
            ("id".into(), "\"abc\"".into()),
            ("status".into(), "\"new\"".into()),
        ];
        let out = regenerate_tag(&info, &attrs);
        assert_eq!(out, "@task(\n    id=\"abc\",\n    status=\"new\"\n)");
    }

    #[test]
    fn test_regenerate_no_attributes() {
        let info = TagFormatInfo {
            tag_name: "task".into(),
            is_multiline: false,
            attr_indent: String::new(),
            close_indent: String::new(),
            attr_order: vec![],
            attr_separator: ", ".into(),
        };
        assert_eq!(regenerate_tag(&info, &[]), "@task");
    }

    // -------------------------------------------------------------------
    // edit_task_tag — oneline
    // -------------------------------------------------------------------

    #[test]
    fn test_edit_oneline_replace_existing() {
        let tag = r#"@task(id="abc", status="new")"#;
        let out = edit_task_tag(tag, &[("status", "\"active\"")]).unwrap();
        assert_eq!(out, r#"@task(id="abc", status="active")"#);
    }

    #[test]
    fn test_edit_oneline_append_new() {
        let tag = r#"@task(id="abc", status="new")"#;
        let out = edit_task_tag(tag, &[("priority", "5")]).unwrap();
        assert_eq!(out, r#"@task(id="abc", status="new", priority=5)"#);
    }

    #[test]
    fn test_edit_oneline_multiple_changes_mixed() {
        let tag = r#"@task(id="abc", status="new")"#;
        let out = edit_task_tag(
            tag,
            &[
                ("status", "\"active\""),
                ("time_last_updated", "\"2026-06-12T00:00:00Z\""),
            ],
        )
        .unwrap();
        assert_eq!(
            out,
            r#"@task(id="abc", status="active", time_last_updated="2026-06-12T00:00:00Z")"#
        );
    }

    // -------------------------------------------------------------------
    // edit_task_tag — multiline
    // -------------------------------------------------------------------

    #[test]
    fn test_edit_multiline_replace_existing() {
        let tag = "@task(\n    id=\"abc\",\n    status=\"new\"\n)";
        let out = edit_task_tag(tag, &[("status", "\"done\"")]).unwrap();
        assert_eq!(out, "@task(\n    id=\"abc\",\n    status=\"done\"\n)");
    }

    #[test]
    fn test_edit_multiline_append_new_uses_same_indent() {
        let tag = "@task(\n    id=\"abc\",\n    status=\"new\"\n)";
        let out = edit_task_tag(tag, &[("priority", "5")]).unwrap();
        assert_eq!(
            out,
            "@task(\n    id=\"abc\",\n    status=\"new\",\n    priority=5\n)"
        );
    }

    #[test]
    fn test_edit_multiline_tab_indent_preserved() {
        let tag = "@task(\n\tid=\"abc\",\n\tstatus=\"new\"\n)";
        let out = edit_task_tag(tag, &[("priority", "3")]).unwrap();
        assert_eq!(
            out,
            "@task(\n\tid=\"abc\",\n\tstatus=\"new\",\n\tpriority=3\n)"
        );
    }

    #[test]
    fn test_edit_multiline_indented_close_preserved() {
        let tag = "@task(\n        id=\"abc\",\n        status=\"new\"\n    )";
        let out = edit_task_tag(tag, &[("status", "\"done\"")]).unwrap();
        assert_eq!(
            out,
            "@task(\n        id=\"abc\",\n        status=\"done\"\n    )"
        );
    }

    // -------------------------------------------------------------------
    // edit_task_tag — order & value formatting preservation
    // -------------------------------------------------------------------

    #[test]
    fn test_edit_preserves_attribute_order() {
        let tag = r#"@task(c="3", a="1", b="2")"#;
        let out = edit_task_tag(tag, &[("a", "\"X\"")]).unwrap();
        // Order must still be c, a, b.
        assert_eq!(out, r#"@task(c="3", a="X", b="2")"#);
    }

    #[test]
    fn test_edit_preserves_numeric_format() {
        let tag = r#"@task(id="abc", priority=3, worktime=4.5)"#;
        let out = edit_task_tag(tag, &[("id", "\"xyz\"")]).unwrap();
        // Numeric attributes must stay bare.
        assert_eq!(out, r#"@task(id="xyz", priority=3, worktime=4.5)"#);
    }

    #[test]
    fn test_edit_preserves_quotes_on_other_strings() {
        let tag = r#"@task(id="abc", title="Hello World")"#;
        let out = edit_task_tag(tag, &[("id", "\"xyz\"")]).unwrap();
        assert!(out.contains(r#"title="Hello World""#));
    }

    // -------------------------------------------------------------------
    // edit_task_tag — edge cases
    // -------------------------------------------------------------------

    #[test]
    fn test_edit_no_parens() {
        let tag = "@task";
        let out = edit_task_tag(tag, &[("status", "\"new\"")]).unwrap();
        assert_eq!(out, r#"@task(status="new")"#);
    }

    #[test]
    fn test_edit_empty_parens_appends() {
        let tag = "@task()";
        let out = edit_task_tag(tag, &[("status", "\"new\"")]).unwrap();
        assert_eq!(out, r#"@task(status="new")"#);
    }

    #[test]
    fn test_edit_value_with_comma() {
        let tag = r#"@task(id="abc", description="old")"#;
        let out = edit_task_tag(tag, &[("description", "\"First, second\"")]).unwrap();
        assert_eq!(out, r#"@task(id="abc", description="First, second")"#);
    }

    #[test]
    fn test_edit_value_with_parens() {
        let tag = r#"@task(id="abc", title="old")"#;
        let out = edit_task_tag(tag, &[("title", "\"Fix bug (urgent)\"")]).unwrap();
        assert_eq!(out, r#"@task(id="abc", title="Fix bug (urgent)")"#);
    }
}

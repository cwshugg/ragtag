//! Query command implementation.
//!
//! Searches for tags matching a name, applies filters, and prints
//! grep-style output with file paths and line numbers.

use std::io::Write;
use std::path::Path;

use crate::config::{ColorMode, Config};
use crate::discovery;
use crate::error::RagtagError;
use crate::extensions::ExtensionRegistry;
use crate::models::Tag;
use crate::output::format::colorize_path;
use crate::parser;

/// Runs the query command.
pub fn run(
    matches: &clap::ArgMatches,
    config: &Config,
    registry: &ExtensionRegistry,
    color_mode: &ColorMode,
    stdout: &mut dyn Write,
) -> Result<(), RagtagError> {
    let tag_name = matches
        .get_one::<String>("TAG_NAME")
        .ok_or_else(|| RagtagError::UnknownCommand("missing tag name argument".to_string()))?;

    let path_str = matches
        .get_one::<String>("path")
        .map(|s| s.as_str())
        .unwrap_or(".");
    let path = Path::new(path_str);
    let count_only = matches.get_flag("count");

    let filters: Vec<String> = matches
        .get_many::<String>("filter")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();

    let files = discovery::walk_path(path, config)?;
    let mut matching_tags: Vec<Tag> = Vec::new();

    for file_path in &files {
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("skipping unreadable file {}: {}", file_path.display(), e);
                continue;
            }
        };
        let tags = parser::scan_file(&content, file_path);
        for tag in tags {
            if tag.name == *tag_name {
                // Apply filters
                let mut passes = true;
                for f in &filters {
                    passes = apply_filter(&tag, f)?;
                    if !passes {
                        break;
                    }
                }
                if passes {
                    matching_tags.push(tag);
                }
            }
        }
    }

    if count_only {
        writeln!(stdout, "{}", matching_tags.len()).map_err(RagtagError::Io)?;
        return Ok(());
    }

    for tag in &matching_tags {
        // Check if an extension provides custom formatting
        let formatted = registry
            .get_by_tag_name(tag_name)
            .and_then(|ext| ext.format_tag(tag, color_mode));

        if let Some(line) = formatted {
            writeln!(stdout, "{line}").map_err(RagtagError::Io)?;
        } else {
            // Default grep-style output
            let path_display = colorize_path(&tag.location.file_path, color_mode);
            let line_num = tag.location.line;
            writeln!(stdout, "{path_display}:{line_num}: {tag}").map_err(RagtagError::Io)?;
        }
    }

    Ok(())
}

/// Applies a filter expression to a tag.
fn apply_filter(tag: &Tag, filter: &str) -> Result<bool, RagtagError> {
    let filter_expr = parse_filter(filter).ok_or_else(|| {
        RagtagError::InvalidFilter(format!(
            "\"{filter}\" — expected format: field=value, field!=value, field>value, etc."
        ))
    })?;
    Ok(match filter_expr.op {
        FilterOp::Eq => get_tag_attr_str(tag, &filter_expr.field) == filter_expr.value,
        FilterOp::NotEq => get_tag_attr_str(tag, &filter_expr.field) != filter_expr.value,
        FilterOp::Gt => compare_values(
            &get_tag_attr_str(tag, &filter_expr.field),
            &filter_expr.value,
            |a, b| a > b,
        ),
        FilterOp::Lt => compare_values(
            &get_tag_attr_str(tag, &filter_expr.field),
            &filter_expr.value,
            |a, b| a < b,
        ),
        FilterOp::Gte => compare_values(
            &get_tag_attr_str(tag, &filter_expr.field),
            &filter_expr.value,
            |a, b| a >= b,
        ),
        FilterOp::Lte => compare_values(
            &get_tag_attr_str(tag, &filter_expr.field),
            &filter_expr.value,
            |a, b| a <= b,
        ),
    })
}

/// Gets a tag attribute as a string.
fn get_tag_attr_str(tag: &Tag, field: &str) -> String {
    tag.get_named_attribute(field)
        .map(|v| format!("{v}"))
        .unwrap_or_default()
}

/// Compares two values numerically if possible, otherwise lexicographically.
fn compare_values(a: &str, b: &str, cmp: fn(f64, f64) -> bool) -> bool {
    if let (Ok(na), Ok(nb)) = (a.parse::<f64>(), b.parse::<f64>()) {
        cmp(na, nb)
    } else {
        // Fall back to lexicographic string comparison for non-numeric values.
        let ordering = a.cmp(b);
        // Map the string comparison to the same semantics as the numeric comparator:
        // we test the comparator against (0, -1), (0, 0), (0, 1) to determine its behavior.
        match ordering {
            std::cmp::Ordering::Less => cmp(-1.0, 0.0),
            std::cmp::Ordering::Equal => cmp(0.0, 0.0),
            std::cmp::Ordering::Greater => cmp(1.0, 0.0),
        }
    }
}

/// A parsed filter expression.
struct FilterExpr {
    field: String,
    op: FilterOp,
    value: String,
}

/// Filter comparison operators.
enum FilterOp {
    Eq,
    NotEq,
    Gt,
    Lt,
    Gte,
    Lte,
}

/// Parses a filter expression string.
fn parse_filter(expr: &str) -> Option<FilterExpr> {
    // Order matters: check multi-char operators first
    for (op_str, op) in &[
        ("!=", FilterOp::NotEq),
        (">=", FilterOp::Gte),
        ("<=", FilterOp::Lte),
        (">", FilterOp::Gt),
        ("<", FilterOp::Lt),
        ("=", FilterOp::Eq),
    ] {
        if let Some(idx) = expr.find(op_str) {
            return Some(FilterExpr {
                field: expr[..idx].trim().to_string(),
                op: match op {
                    FilterOp::Eq => FilterOp::Eq,
                    FilterOp::NotEq => FilterOp::NotEq,
                    FilterOp::Gt => FilterOp::Gt,
                    FilterOp::Lt => FilterOp::Lt,
                    FilterOp::Gte => FilterOp::Gte,
                    FilterOp::Lte => FilterOp::Lte,
                },
                value: expr[idx + op_str.len()..].trim().to_string(),
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AttributeValue, NumericBase, TagAttribute, TagLocation};
    use std::path::PathBuf;

    fn make_tag(name: &str, attrs: Vec<TagAttribute>) -> Tag {
        Tag {
            name: name.to_string(),
            attributes: attrs,
            location: TagLocation::new(PathBuf::from("test.md"), 1, 1, 0, 20),
            raw_span: 0..20,
        }
    }

    #[test]
    fn test_parse_filter_eq() {
        let f = parse_filter("status=active").unwrap();
        assert_eq!(f.field, "status");
        assert_eq!(f.value, "active");
    }

    #[test]
    fn test_parse_filter_neq() {
        let f = parse_filter("status!=done").unwrap();
        assert_eq!(f.field, "status");
        assert_eq!(f.value, "done");
    }

    #[test]
    fn test_parse_filter_gt() {
        let f = parse_filter("priority>0").unwrap();
        assert_eq!(f.field, "priority");
        assert_eq!(f.value, "0");
    }

    #[test]
    fn test_apply_filter_eq() {
        let tag = make_tag(
            "tag",
            vec![TagAttribute::named(
                "status",
                AttributeValue::Str("active".to_string()),
            )],
        );
        assert!(apply_filter(&tag, "status=active").unwrap());
        assert!(!apply_filter(&tag, "status=done").unwrap());
    }

    #[test]
    fn test_apply_filter_numeric_gt() {
        let tag = make_tag(
            "tag",
            vec![TagAttribute::named(
                "priority",
                AttributeValue::Integer {
                    value: 5,
                    base: NumericBase::Decimal,
                },
            )],
        );
        assert!(apply_filter(&tag, "priority>2").unwrap());
        assert!(!apply_filter(&tag, "priority>10").unwrap());
    }

    #[test]
    fn test_apply_filter_invalid() {
        let tag = make_tag(
            "tag",
            vec![TagAttribute::named(
                "status",
                AttributeValue::Str("active".to_string()),
            )],
        );
        assert!(apply_filter(&tag, "statusinvalid").is_err());
    }

    #[test]
    fn test_apply_filter_string_gt() {
        // Non-numeric values should fall back to lexicographic comparison
        let tag = make_tag(
            "tag",
            vec![TagAttribute::named(
                "status",
                AttributeValue::Str("draft".to_string()),
            )],
        );
        // "draft" > "active" lexicographically
        assert!(apply_filter(&tag, "status>active").unwrap());
        // "draft" < "final" lexicographically
        assert!(!apply_filter(&tag, "status>final").unwrap());
    }

    #[test]
    fn test_apply_filter_string_gte() {
        let tag = make_tag(
            "tag",
            vec![TagAttribute::named(
                "status",
                AttributeValue::Str("draft".to_string()),
            )],
        );
        assert!(apply_filter(&tag, "status>=draft").unwrap());
        assert!(apply_filter(&tag, "status>=active").unwrap());
        assert!(!apply_filter(&tag, "status>=final").unwrap());
    }
}

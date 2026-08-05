//! Query command implementation.
//!
//! Searches for tags matching a name, applies filters, and prints
//! grep-style output with file paths and line numbers.

use std::io::Write;
use std::path::Path;

use crate::cli;
use crate::config::{ColorMode, Config};
use crate::discovery;
use crate::error::RagtagError;
use crate::extensions::ExtensionRegistry;
use crate::filter::{self, FilterExpr};
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
    let tag_name = matches.get_one::<String>("TAG_NAME");

    let path_str = cli::resolve_path(matches);
    let path = Path::new(&path_str);
    let count_only = matches.get_flag("count");

    let filters: Vec<String> = matches
        .get_many::<String>("filter")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default();

    // Parse and validate every filter up front so a malformed expression is
    // reported regardless of how many tags match. Multiple `--filter` flags are
    // AND-combined: a tag must satisfy all of them.
    let parsed_filters: Vec<FilterExpr> = filters
        .iter()
        .map(|f| parse_query_filter(f))
        .collect::<Result<_, _>>()?;

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
            let name_matches = tag_name.is_none_or(|name| tag.name == *name);
            if name_matches {
                let passes = parsed_filters
                    .iter()
                    .all(|expr| eval_query_filter(&tag, expr));
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
        let formatted = tag_name
            .and_then(|name| registry.get_by_tag_name(name))
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

/// Parses and validates a single `--filter` argument into a boolean
/// expression using the shared filter engine.
///
/// The expression may combine conditions with `AND`, `OR`, and parentheses.
/// Each leaf condition must contain a comparison operator; the field may be any
/// tag attribute name.
fn parse_query_filter(filter: &str) -> Result<FilterExpr, RagtagError> {
    let expr = filter::parse_filter_expr(filter)?;
    filter::validate(&expr, &validate_query_condition)?;
    Ok(expr)
}

/// Evaluates a parsed filter expression against a tag.
///
/// Each leaf condition is applied with `apply_query_condition`; the shared
/// engine combines the results with standard boolean logic.
fn eval_query_filter(tag: &Tag, expr: &FilterExpr) -> bool {
    filter::evaluate(expr, &mut |cond| apply_query_condition(tag, cond))
}

/// Validates a single leaf condition for a query filter.
///
/// A condition is valid if it contains one of the comparison operators
/// `!=`, `>=`, `<=`, `>`, `<`, or `=` outside any quoted span.
fn validate_query_condition(cond: &str) -> Result<(), RagtagError> {
    filter::ensure_condition_has_operator(cond)
}

/// Applies a single leaf condition to a tag.
///
/// Supports `=`, `!=`, `>`, `<`, `>=`, and `<=`. Numeric values are compared
/// numerically; non-numeric values fall back to lexicographic comparison.
fn apply_query_condition(tag: &Tag, cond: &str) -> bool {
    let Some((field, op, value)) = filter::split_condition(cond) else {
        // Unreachable: conditions are validated to contain an operator first.
        return false;
    };
    let attr = get_tag_attr_str(tag, field);
    filter::apply_operator(op, &attr, value)
}

/// Gets a tag attribute as a string.
fn get_tag_attr_str(tag: &Tag, field: &str) -> String {
    tag.get_named_attribute(field)
        .map(|v| format!("{v}"))
        .unwrap_or_default()
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

    /// Evaluates a whole filter string against a tag, mirroring the query
    /// command's parse-then-evaluate flow.
    fn apply_filter(tag: &Tag, filter: &str) -> Result<bool, RagtagError> {
        let expr = parse_query_filter(filter)?;
        Ok(eval_query_filter(tag, &expr))
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
        // A condition with no comparison operator is rejected with the
        // "expected format" message.
        let err = apply_filter(&tag, "statusinvalid").unwrap_err();
        assert!(err.to_string().contains("expected format"));
    }

    #[test]
    fn test_apply_filter_boolean_and_or_parens() {
        let tag = make_tag(
            "tag",
            vec![
                TagAttribute::named("status", AttributeValue::Str("active".to_string())),
                TagAttribute::named(
                    "priority",
                    AttributeValue::Integer {
                        value: 0,
                        base: NumericBase::Decimal,
                    },
                ),
            ],
        );
        // (status=active OR priority=9) AND status!=done → true
        assert!(apply_filter(&tag, "(status=active OR priority=9) AND status!=done").unwrap());
        // (status=blocked OR priority=9) AND status!=done → false (neither OR arm holds)
        assert!(!apply_filter(&tag, "(status=blocked OR priority=9) AND status!=done").unwrap());
    }

    #[test]
    fn test_query_and_task_share_parse_results() {
        // The same expression string parses to identical ASTs through the
        // shared engine, whichever command drives it.
        let via_query = parse_query_filter("(a = 1 OR b = 2) AND c != 3").unwrap();
        let via_shared = filter::parse_filter_expr("(a=1 OR b=2) AND c!=3").unwrap();
        assert_eq!(via_query, via_shared);
    }

    #[test]
    fn test_apply_filter_empty_value_matches_absent_attribute() {
        // A tag with a non-matching attribute set, but no `owner`.
        let tag = make_tag(
            "tag",
            vec![TagAttribute::named(
                "status",
                AttributeValue::Str("active".to_string()),
            )],
        );
        // `owner=` matches because the absent attribute reads as empty.
        assert!(apply_filter(&tag, "owner=").unwrap());
        // `owner!=` is the complement and does not match.
        assert!(!apply_filter(&tag, "owner!=").unwrap());
    }
}

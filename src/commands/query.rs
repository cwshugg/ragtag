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
use crate::extensions::task::TASKS_CONFIG_KEY;
use crate::extensions::{ExtensionRegistry, ValidationLevel};
use crate::filter::{self, FilterExpr};
use crate::models::Tag;
use crate::output::format::colorize_path;
use crate::parser;

/// A query candidate with the extension category needed for task semantics.
struct PreparedResult {
    tag: Tag,
    is_task: bool,
}

/// Controls whether and how the final query result list is shuffled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Randomization {
    /// Preserve discovery order.
    Disabled,
    /// Generate a new seed from the operating system.
    Fresh,
    /// Use the caller's reproducible seed.
    Seeded(u64),
}

/// Runs the query command.
pub fn run(
    matches: &clap::ArgMatches,
    config: &Config,
    registry: &ExtensionRegistry,
    color_mode: &ColorMode,
    stdout: &mut dyn Write,
) -> Result<(), RagtagError> {
    run_with_seed_source(
        matches,
        config,
        registry,
        color_mode,
        stdout,
        fresh_random_seed,
    )
}

/// Generates a presentation-quality seed from the operating system.
fn fresh_random_seed() -> Result<u64, RagtagError> {
    let mut bytes = [0_u8; size_of::<u64>()];
    getrandom::fill(&mut bytes).map_err(|error| {
        RagtagError::Io(std::io::Error::other(format!(
            "failed to generate query randomization seed: {error}"
        )))
    })?;
    Ok(u64::from_le_bytes(bytes))
}

/// Resolves the typed randomization mode from parsed clap matches.
fn randomization_from_matches(matches: &clap::ArgMatches) -> Randomization {
    if let Some(seed) = matches.get_one::<u64>("randomize") {
        Randomization::Seeded(*seed)
    } else if matches.contains_id("randomize") {
        Randomization::Fresh
    } else {
        Randomization::Disabled
    }
}

/// Runs the query command with an injectable fresh-seed source.
///
/// Seeded mode never calls `fresh_seed`, keeping production and deterministic
/// tests on the same shuffle implementation.
fn run_with_seed_source(
    matches: &clap::ArgMatches,
    config: &Config,
    registry: &ExtensionRegistry,
    color_mode: &ColorMode,
    stdout: &mut dyn Write,
    fresh_seed: impl FnOnce() -> Result<u64, RagtagError>,
) -> Result<(), RagtagError> {
    let tag_name = matches.get_one::<String>("TAG_NAME");

    let path_str = cli::resolve_path(matches);
    let path = Path::new(&path_str);
    let count_only = matches.get_flag("count");
    let limit = matches.get_one::<usize>("limit").copied();
    let randomization = randomization_from_matches(matches);

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
    let mut matching_results: Vec<PreparedResult> = Vec::new();

    for file_path in &files {
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(error) => {
                log::warn!("skipping an unreadable input file ({:?})", error.kind());
                continue;
            }
        };
        let tags = parser::scan_file(&content, file_path);
        for tag in tags {
            let name_matches = tag_name.is_none_or(|name| tag.name == *name);
            if name_matches {
                let extension = registry.get_by_tag_name(&tag.name);
                let is_task = extension
                    .is_some_and(|extension| extension.config_key() == Some(TASKS_CONFIG_KEY));
                // A syntactically valid tag can still be an invalid task (for
                // example, missing a title). Exclude it before filtering,
                // counting, limiting, or formatting so it can never fall back
                // to generic source-like output. Validation is deliberately
                // independent of presentation, so count-only queries format
                // no extension results.
                if is_task
                    && extension.is_some_and(|extension| {
                        extension
                            .validate_tag(&tag)
                            .iter()
                            .any(|message| message.level == ValidationLevel::Error)
                    })
                {
                    continue;
                }
                let passes = parsed_filters
                    .iter()
                    .all(|expr| eval_query_filter(&tag, expr, is_task));
                if passes {
                    matching_results.push(PreparedResult { tag, is_task });
                }
            }
        }
    }

    finalize_results(&mut matching_results, randomization, limit, fresh_seed)?;

    if count_only {
        writeln!(stdout, "{}", matching_results.len()).map_err(RagtagError::Io)?;
        return Ok(());
    }

    for result in &matching_results {
        let formatted = registry
            .get_by_tag_name(&result.tag.name)
            .and_then(|extension| extension.format_tag(&result.tag, color_mode));
        if let Some(line) = formatted {
            writeln!(stdout, "{line}").map_err(RagtagError::Io)?;
        } else if !result.is_task {
            // Default grep-style output
            let path_display = colorize_path(&result.tag.location.file_path, color_mode);
            let line_num = result.tag.location.line;
            writeln!(stdout, "{path_display}:{line_num}: {}", result.tag)
                .map_err(RagtagError::Io)?;
        }
    }

    Ok(())
}

/// Shuffles filtered results with pinned fastrand 2.4.1 WyRand, then truncates.
fn finalize_results<T>(
    results: &mut Vec<T>,
    randomization: Randomization,
    limit: Option<usize>,
    fresh_seed: impl FnOnce() -> Result<u64, RagtagError>,
) -> Result<(), RagtagError> {
    let seed = match randomization {
        Randomization::Disabled => None,
        Randomization::Fresh => Some(fresh_seed()?),
        Randomization::Seeded(seed) => Some(seed),
    };
    if let Some(seed) = seed {
        fastrand::Rng::with_seed(seed).shuffle(results);
    }
    if let Some(limit) = limit {
        results.truncate(limit);
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
fn eval_query_filter(tag: &Tag, expr: &FilterExpr, normalize_task_type: bool) -> bool {
    filter::evaluate(expr, &mut |cond| {
        apply_query_condition(tag, cond, normalize_task_type)
    })
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
fn apply_query_condition(tag: &Tag, cond: &str, normalize_task_type: bool) -> bool {
    let Some((field, op, value)) = filter::split_condition(cond) else {
        // Unreachable: conditions are validated to contain an operator first.
        return false;
    };
    let attr = get_tag_attr_str(tag, field, normalize_task_type);
    filter::apply_operator(op, &attr, value)
}

/// Gets a tag attribute as a string.
fn get_tag_attr_str(tag: &Tag, field: &str, normalize_task_type: bool) -> String {
    if normalize_task_type && field == "type" {
        return match tag.get_named_attribute(field) {
            Some(crate::models::AttributeValue::Str(value)) => {
                crate::extensions::task::models::TaskType::from_input(value)
                    .as_str()
                    .to_string()
            }
            _ => crate::extensions::task::models::TaskType::Item
                .as_str()
                .to_string(),
        };
    }

    tag.get_named_attribute(field)
        .map(|v| format!("{v}"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::{ExtensionContext, TagExtension, ValidationMessage};
    use crate::models::{AttributeValue, NumericBase, TagAttribute, TagLocation};
    use std::path::PathBuf;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    struct CountingExtension {
        calls: Arc<AtomicUsize>,
    }

    impl TagExtension for CountingExtension {
        fn tag_name(&self) -> &str {
            "counted"
        }

        fn display_name(&self) -> &str {
            "Counting"
        }

        fn description(&self) -> &str {
            "Counts formatting calls"
        }

        fn config_key(&self) -> Option<&str> {
            None
        }

        fn init(&mut self, _: Option<&serde_yml::Value>) -> Result<(), RagtagError> {
            Ok(())
        }

        fn validate_tag(&self, _: &Tag) -> Vec<ValidationMessage> {
            Vec::new()
        }

        fn cli_command(&self) -> clap::Command {
            clap::Command::new("counted")
        }

        fn execute(
            &self,
            _: &clap::ArgMatches,
            _: &mut ExtensionContext,
        ) -> Result<(), RagtagError> {
            Ok(())
        }

        fn format_tag(&self, tag: &Tag, _: &ColorMode) -> Option<String> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Some(format!("formatted:{}", tag.location.line))
        }
    }

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
        Ok(eval_query_filter(tag, &expr, false))
    }

    /// Evaluates a filter with task semantic normalization enabled.
    fn apply_task_filter(tag: &Tag, filter: &str) -> Result<bool, RagtagError> {
        let expr = parse_query_filter(filter)?;
        Ok(eval_query_filter(tag, &expr, true))
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

    #[test]
    fn test_task_type_filter_uses_canonical_semantics() {
        let project = make_tag(
            "task",
            vec![TagAttribute::named(
                "type",
                AttributeValue::Str("PROJECT".to_string()),
            )],
        );
        let missing = make_tag("task", vec![]);
        let numeric = make_tag(
            "task",
            vec![TagAttribute::named(
                "type",
                AttributeValue::Integer {
                    value: 1,
                    base: NumericBase::Decimal,
                },
            )],
        );
        let custom = make_tag(
            "task",
            vec![TagAttribute::named(
                "type",
                AttributeValue::Str(" ProjectX ".to_string()),
            )],
        );

        assert!(apply_task_filter(&project, "type=project").unwrap());
        assert!(apply_task_filter(&missing, "type=item").unwrap());
        assert!(apply_task_filter(&numeric, "type=item").unwrap());
        assert!(apply_task_filter(&custom, "type=' ProjectX '").unwrap());
        assert!(!apply_task_filter(&custom, "type=project").unwrap());
    }

    #[test]
    fn test_finalize_results_limit_preserves_order_and_accepts_zero() {
        let mut limited = vec![1, 2, 3, 4];
        finalize_results(&mut limited, Randomization::Disabled, Some(2), || {
            panic!("seed source must not run when randomization is disabled")
        })
        .unwrap();
        assert_eq!(limited, [1, 2]);

        let mut empty = vec![1, 2, 3, 4];
        finalize_results(&mut empty, Randomization::Disabled, Some(0), || {
            panic!("seed source must not run when randomization is disabled")
        })
        .unwrap();
        assert!(empty.is_empty());

        let mut unchanged = vec![1, 2, 3, 4];
        finalize_results(&mut unchanged, Randomization::Disabled, None, || {
            panic!("seed source must not run when randomization is disabled")
        })
        .unwrap();
        assert_eq!(unchanged, [1, 2, 3, 4]);
    }

    #[test]
    fn test_seeded_shuffle_has_stable_algorithm_contract() {
        let mut results = vec![0, 1, 2, 3, 4, 5, 6, 7];

        finalize_results(&mut results, Randomization::Seeded(42), None, || {
            panic!("seeded mode must not request a fresh seed")
        })
        .unwrap();

        assert_eq!(results, [4, 2, 1, 6, 0, 3, 5, 7]);
    }

    #[test]
    fn test_query_command_uses_fresh_seed_before_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tags.md");
        std::fs::write(&path, "@tag(id=1)\n@tag(id=2)\n@tag(id=3)\n@tag(id=4)\n").unwrap();
        let registry = ExtensionRegistry::new();
        let matches = cli::build_real_cli(&registry)
            .try_get_matches_from([
                "ragtag",
                "query",
                "tag",
                "--path",
                path.to_str().unwrap(),
                "--randomize",
                "--limit",
                "2",
            ])
            .unwrap();
        let query_matches = matches.subcommand_matches("query").unwrap();
        let mut output = Vec::new();

        run_with_seed_source(
            query_matches,
            &Config::default(),
            &registry,
            &ColorMode::Never,
            &mut output,
            || Ok(42),
        )
        .unwrap();

        let output = String::from_utf8(output).unwrap();
        assert_eq!(output.lines().count(), 2);
        assert!(output.contains("@tag(id=1)"));
        assert!(output.contains("@tag(id=3)"));
        assert!(!output.contains("@tag(id=2)"));
        assert!(!output.contains("@tag(id=4)"));
    }

    /// Runs a query using `CountingExtension` and returns its output and call count.
    fn run_counting_query(arguments: &[&str], contents: &str) -> (String, usize) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tags.md");
        std::fs::write(&path, contents).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = ExtensionRegistry::new();
        registry
            .register(Box::new(CountingExtension {
                calls: Arc::clone(&calls),
            }))
            .unwrap();
        let mut argv = vec![
            "ragtag",
            "query",
            "counted",
            "--path",
            path.to_str().unwrap(),
        ];
        argv.extend_from_slice(arguments);
        let matches = cli::build_real_cli(&registry)
            .try_get_matches_from(argv)
            .unwrap();
        let mut output = Vec::new();

        run_with_seed_source(
            matches.subcommand_matches("query").unwrap(),
            &Config::default(),
            &registry,
            &ColorMode::Never,
            &mut output,
            || panic!("unrandomized query must not request a seed"),
        )
        .unwrap();

        (
            String::from_utf8(output).unwrap(),
            calls.load(Ordering::Relaxed),
        )
    }

    #[test]
    fn query_formats_only_final_emitted_results() {
        let cases = [
            (
                Vec::<&str>::new(),
                "@counted(id=1)\n@counted(id=2)\n",
                "formatted:1\nformatted:2\n",
                2,
            ),
            (
                vec!["--count"],
                "@counted(id=1)\n@counted(id=2)\n",
                "2\n",
                0,
            ),
            (
                vec!["--filter", "id=kept"],
                "@counted(id=rejected)\n",
                "",
                0,
            ),
            (
                vec!["--limit", "1"],
                "@counted(id=1)\n@counted(id=2)\n",
                "formatted:1\n",
                1,
            ),
        ];
        for (arguments, contents, expected_output, expected_calls) in cases {
            let (output, calls) = run_counting_query(&arguments, contents);
            assert_eq!(output, expected_output, "arguments={arguments:?}");
            assert_eq!(calls, expected_calls, "arguments={arguments:?}");
        }
    }
}

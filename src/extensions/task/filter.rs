//! Task-specific filter adapters.
//!
//! The generic boolean filter engine lives in [`crate::filter`]; this module
//! supplies the task-specific leaf semantics on top of it. A leaf condition is
//! evaluated by [`apply_task_filter`] and validated by [`validate_task_filter`]
//! (both in the task command module), so the shared engine handles AND/OR,
//! parentheses, and whitespace while task fields give conditions their meaning.

use super::commands::{apply_task_filter, validate_task_filter};
use super::models::TaskTag;
use crate::error::RagtagError;
use crate::filter;

pub use crate::filter::FilterExpr;

/// Parses a filter expression string into a `FilterExpr` AST.
///
/// This is a thin re-export of the shared engine's parser so task commands can
/// depend on a single, task-facing entry point.
pub fn parse_filter_expr(input: &str) -> Result<FilterExpr, RagtagError> {
    filter::parse_filter_expr(input)
}

/// Evaluates a parsed filter expression against a task.
///
/// Each leaf condition is evaluated with `apply_task_filter`; the shared engine
/// combines the results with standard boolean logic.
pub fn evaluate_filter(expr: &FilterExpr, task: &TaskTag) -> bool {
    filter::evaluate(expr, &mut |cond| apply_task_filter(task, cond))
}

/// Validates all leaf conditions in a filter expression.
///
/// Each leaf condition is checked with `validate_task_filter`; returns an error
/// if any condition is missing a comparison operator.
pub fn validate_filter_expr(expr: &FilterExpr) -> Result<(), RagtagError> {
    filter::validate(expr, &validate_task_filter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TagLocation;
    use std::path::PathBuf;

    fn make_task(id: &str, status: &str, owner: &str, priority: Option<u32>) -> TaskTag {
        TaskTag {
            id: id.to_string(),
            pid: None,
            title: format!("Task {id}"),
            description: None,
            owner: owner.to_string(),
            status: status.to_string(),
            priority,
            worktime_spent: None,
            worktime_estimate: Some(4.0),
            time_created: None,
            time_last_updated: None,
            worktime_units: "hours".to_string(),
            location: TagLocation::new(PathBuf::from("test.md"), 1, 1, 0, 50),
            raw_span: 0..50,
        }
    }

    #[test]
    fn test_eval_simple_equality() {
        let task = make_task("a", "active", "alice", Some(1));
        let expr = parse_filter_expr("status=active").unwrap();
        assert!(evaluate_filter(&expr, &task));

        let expr2 = parse_filter_expr("status=done").unwrap();
        assert!(!evaluate_filter(&expr2, &task));
    }

    #[test]
    fn test_eval_complex_expression() {
        let task_a = make_task("a", "active", "alice", Some(0));
        let task_b = make_task("b", "blocked", "bob", Some(3));
        let task_c = make_task("c", "inactive", "alice", Some(1));

        // (status=active OR priority>2) AND owner=alice
        let expr = parse_filter_expr("(status=active OR priority>2) AND owner=alice").unwrap();

        // task_a: status=active (true) OR priority>2 (false) = true; owner=alice = true → true
        assert!(evaluate_filter(&expr, &task_a));
        // task_b: status=active (false) OR priority>2 (true) = true; owner=alice (false) → false
        assert!(!evaluate_filter(&expr, &task_b));
        // task_c: status=active (false) OR priority>2 (false) = false; → false
        assert!(!evaluate_filter(&expr, &task_c));
    }

    #[test]
    fn test_eval_numeric_comparison() {
        let task = make_task("a", "active", "alice", Some(3));
        assert!(evaluate_filter(
            &parse_filter_expr("priority>2").unwrap(),
            &task
        ));
        assert!(!evaluate_filter(
            &parse_filter_expr("priority>3").unwrap(),
            &task
        ));
        assert!(evaluate_filter(
            &parse_filter_expr("priority>=3").unwrap(),
            &task
        ));
        assert!(evaluate_filter(
            &parse_filter_expr("priority<4").unwrap(),
            &task
        ));
        assert!(evaluate_filter(
            &parse_filter_expr("priority<=3").unwrap(),
            &task
        ));
        assert!(evaluate_filter(
            &parse_filter_expr("priority!=0").unwrap(),
            &task
        ));
    }

    #[test]
    fn test_validate_filter_expr_invalid_condition() {
        let expr = parse_filter_expr("nooperator AND status=active").unwrap();
        assert!(validate_filter_expr(&expr).is_err());
    }

    #[test]
    fn test_eval_spaced_matches_spaceless_result() {
        let task = make_task("a", "active", "alice", Some(2));
        let spaced = parse_filter_expr("priority >= 2 AND owner = alice").unwrap();
        let spaceless = parse_filter_expr("priority>=2 AND owner=alice").unwrap();
        assert_eq!(spaced, spaceless);
        assert!(evaluate_filter(&spaced, &task));
    }

    #[test]
    fn test_eval_empty_value_matches_absent_field() {
        // `worktime_spent` and `pid` are `None` on the fixture task, so they
        // read as empty and match an empty-value equality condition.
        let task = make_task("a", "active", "alice", Some(2));
        assert!(evaluate_filter(
            &parse_filter_expr("worktime_spent=").unwrap(),
            &task
        ));
        assert!(evaluate_filter(&parse_filter_expr("pid=").unwrap(), &task));
        // The complement does not match.
        assert!(!evaluate_filter(
            &parse_filter_expr("worktime_spent!=").unwrap(),
            &task
        ));
    }

    #[test]
    fn test_eval_quoted_value_with_operator_char() {
        // A quoted value containing an operator char is compared literally and
        // splits on the real operator, not the one inside the quotes.
        let task = make_task("a", ">2", "alice", Some(2));
        assert!(evaluate_filter(
            &parse_filter_expr("status='>2'").unwrap(),
            &task
        ));
        assert!(!evaluate_filter(
            &parse_filter_expr("status='>3'").unwrap(),
            &task
        ));
    }
}

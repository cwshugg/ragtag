//! Strict task selection for diagram forest pipelines.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use clap::ArgMatches;

use crate::diagram::catalog::ForestSelectionPolicy;
use crate::diagram::diagnostics::Diagnostic;
use crate::diagram::graph::{integer, number, text, GraphNode, NodeKey, ValidatedForest};
use crate::extensions::task::commands::validate_task_filter;
use crate::extensions::task::semantics::TaskSemantics;

/// Selected task keys and whether each was included only for context.
pub(crate) type TaskSelection = BTreeMap<NodeKey, bool>;

/// Applies strict diagram filters and ancestor-context policy.
pub(crate) struct TaskSelectionPolicy {
    semantics: Arc<TaskSemantics>,
}

impl TaskSelectionPolicy {
    pub(crate) fn new(semantics: Arc<TaskSemantics>) -> Self {
        Self { semantics }
    }

    #[cfg(test)]
    pub(crate) fn semantics(&self) -> &Arc<TaskSemantics> {
        &self.semantics
    }
}

impl ForestSelectionPolicy for TaskSelectionPolicy {
    type Selection = TaskSelection;

    fn select(
        &self,
        forest: &ValidatedForest,
        matches: &ArgMatches,
    ) -> Result<Self::Selection, Vec<Diagnostic>> {
        let parsed = match matches.get_one::<String>("filter").map(String::as_str) {
            Some(filter) => {
                let expression = crate::filter::parse_filter_expr(filter).map_err(|_| {
                    vec![Diagnostic::error(
                        "DGM-FILTER-001",
                        "diagram filter expression is invalid",
                    )]
                })?;
                crate::filter::validate(&expression, &validate_strict_condition).map_err(|_| {
                    vec![Diagnostic::error(
                        "DGM-FILTER-002",
                        "diagram filter references an unknown task field or invalid condition",
                    )]
                })?;
                Some(expression)
            }
            None => None,
        };
        let filter_mentions_status = parsed
            .as_ref()
            .is_some_and(|expression| mentions_field(expression, "status"));

        let mut matched = BTreeSet::new();
        for node in forest.nodes() {
            let visible = matches.get_flag("all")
                || filter_mentions_status
                || !self
                    .semantics
                    .is_default_excluded_status(text(node, "status").unwrap_or_default());
            let filter_matches = parsed.as_ref().is_none_or(|expr| {
                crate::filter::evaluate(expr, &mut |condition| evaluate_condition(node, condition))
            });
            if visible && filter_matches {
                matched.insert(node.key.clone());
            }
        }

        let mut selected = matched
            .iter()
            .cloned()
            .map(|key| (key, false))
            .collect::<TaskSelection>();
        for key in matched {
            let mut current = &key;
            while let Some(parent) = forest.parent(current) {
                selected.entry(parent.clone()).or_insert(true);
                current = parent;
            }
        }
        Ok(selected)
    }
}

fn mentions_field(expression: &crate::filter::FilterExpr, expected: &str) -> bool {
    match expression {
        crate::filter::FilterExpr::Condition(condition) => {
            crate::filter::split_condition(condition).is_some_and(|(field, _, _)| field == expected)
        }
        crate::filter::FilterExpr::And(left, right)
        | crate::filter::FilterExpr::Or(left, right) => {
            mentions_field(left, expected) || mentions_field(right, expected)
        }
    }
}

const FIELDS: [&str; 12] = [
    "id",
    "pid",
    "title",
    "description",
    "owner",
    "status",
    "priority",
    "worktime_spent",
    "worktime_estimate",
    "time_created",
    "time_last_updated",
    "worktime_units",
];

fn validate_strict_condition(condition: &str) -> Result<(), crate::error::RagtagError> {
    validate_task_filter(condition)?;
    let (field, _, _) = crate::filter::split_condition(condition)
        .ok_or_else(|| crate::error::RagtagError::Diagram("invalid filter".to_string()))?;
    if !FIELDS.contains(&field) {
        return Err(crate::error::RagtagError::Diagram(
            "unknown diagram filter field".to_string(),
        ));
    }
    Ok(())
}

fn evaluate_condition(node: &GraphNode, condition: &str) -> bool {
    let Some((field, operator, expected)) = crate::filter::split_condition(condition) else {
        return false;
    };
    let actual = field_value(node, field);
    crate::filter::apply_operator(operator, &actual, expected)
}

fn field_value<'a>(node: &'a GraphNode, field: &str) -> Cow<'a, str> {
    match field {
        "priority" => integer(node, field)
            .map(|value| Cow::Owned(value.to_string()))
            .unwrap_or_else(|| Cow::Borrowed("")),
        "worktime_spent" | "worktime_estimate" => number(node, field)
            .map(|value| Cow::Owned(value.to_string()))
            .unwrap_or_else(|| Cow::Borrowed("")),
        _ => text(node, field).map_or_else(|| Cow::Borrowed(""), Cow::Borrowed),
    }
}

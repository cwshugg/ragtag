//! Shared task parsing, filtering, and visibility semantics.

use crate::error::RagtagError;
use crate::filter::FilterExpr;
use crate::models::Tag;

use super::commands::validate_task_filter;
use super::config::{TaskConfig, TaskConfigWarning};
use super::filter::{evaluate_filter, parse_filter_expr, validate_filter_expr};
use super::models::{categorize_status, StatusCategory, TaskTag};

/// Selects compatibility or strict task-filter validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskFilterMode {
    /// Preserve task list/summary behavior for unknown fields.
    CommandCompatibility,
    /// Reject fields that are not part of the task model.
    DiagramStrict,
}

/// A parsed task filter with its visibility-override decision.
#[derive(Debug, Clone)]
pub(crate) struct CompiledTaskFilter {
    expression: FilterExpr,
    mentions_status: bool,
}

impl CompiledTaskFilter {
    /// Evaluates the filter against one task.
    pub(crate) fn matches(&self, task: &TaskTag) -> bool {
        evaluate_filter(&self.expression, task)
    }

    /// Returns whether the filter disables default status exclusions.
    pub(crate) fn mentions_status(&self) -> bool {
        self.mentions_status
    }
}

/// Shared decision for applying configured status exclusions.
#[derive(Debug)]
pub(crate) struct TaskVisibilityRules<'a> {
    excluded: &'a [String],
    show_all: bool,
    filter_mentions_status: bool,
}

impl TaskVisibilityRules<'_> {
    /// Returns whether a task is visible before a user filter is applied.
    pub(crate) fn includes(&self, task: &TaskTag) -> bool {
        self.show_all
            || self.filter_mentions_status
            || !self.excluded.iter().any(|status| status == &task.status)
    }
}

/// Immutable authoritative task behavior shared by every task consumer.
#[derive(Debug)]
pub(crate) struct TaskSemantics {
    config: TaskConfig,
    excluded_statuses: Vec<String>,
}

impl TaskSemantics {
    /// Resolves, validates, and owns the task configuration exactly once.
    pub(crate) fn resolve(
        value: Option<serde_yml::Value>,
    ) -> Result<(Self, Vec<TaskConfigWarning>), RagtagError> {
        let config = value
            .as_ref()
            .map(TaskConfig::from_config_value)
            .transpose()?
            .unwrap_or_default();
        let warnings = config.validate()?;
        let excluded_statuses = config.get_excluded_keywords();
        Ok((
            Self {
                config,
                excluded_statuses,
            },
            warnings,
        ))
    }

    /// Returns the single resolved task configuration.
    pub(crate) fn config(&self) -> &TaskConfig {
        &self.config
    }

    /// Converts a parser tag using authoritative task defaults and validation.
    pub(crate) fn task_from_tag(&self, tag: &Tag) -> Result<TaskTag, RagtagError> {
        TaskTag::from_tag(tag, &self.config)
    }

    /// Returns the configured category for a status.
    pub(crate) fn status_category(&self, status: &str) -> StatusCategory {
        categorize_status(status, &self.config.status_keywords)
    }

    /// Returns whether default task visibility excludes this status.
    pub(crate) fn is_default_excluded_status(&self, status: &str) -> bool {
        self.excluded_statuses
            .iter()
            .any(|excluded| excluded == status)
    }

    /// Compiles a filter in compatibility or strict mode.
    pub(crate) fn compile_filter(
        &self,
        input: &str,
        mode: TaskFilterMode,
    ) -> Result<CompiledTaskFilter, RagtagError> {
        let expression = parse_filter_expr(input)?;
        validate_filter_expr(&expression)?;
        if mode == TaskFilterMode::DiagramStrict {
            crate::filter::validate(&expression, &|condition| {
                let Some((field, _, _)) = crate::filter::split_condition(condition) else {
                    return validate_task_filter(condition);
                };
                if Self::is_known_field(field) {
                    Ok(())
                } else {
                    Err(RagtagError::InvalidFilter(format!(
                        "unknown task field \"{field}\""
                    )))
                }
            })?;
        }
        let mentions_status = match mode {
            TaskFilterMode::CommandCompatibility => input.contains("status"),
            TaskFilterMode::DiagramStrict => expression_mentions_field(&expression, "status"),
        };
        Ok(CompiledTaskFilter {
            expression,
            mentions_status,
        })
    }

    /// Builds status visibility rules for one invocation.
    pub(crate) fn visibility(
        &self,
        show_all: bool,
        filter_mentions_status: bool,
    ) -> TaskVisibilityRules<'_> {
        TaskVisibilityRules {
            excluded: &self.excluded_statuses,
            show_all,
            filter_mentions_status,
        }
    }

    /// Tests whether a field belongs to the existing task model.
    pub(crate) fn is_known_field(field: &str) -> bool {
        matches!(
            field,
            "id" | "pid"
                | "title"
                | "description"
                | "owner"
                | "status"
                | "priority"
                | "worktime_spent"
                | "worktime_estimate"
                | "time_created"
                | "time_last_updated"
                | "worktime_units"
        )
    }
}

/// Detects an exact field reference in a parsed boolean expression.
fn expression_mentions_field(expression: &FilterExpr, expected: &str) -> bool {
    match expression {
        FilterExpr::Condition(condition) => {
            crate::filter::split_condition(condition).is_some_and(|(field, _, _)| field == expected)
        }
        FilterExpr::And(left, right) | FilterExpr::Or(left, right) => {
            expression_mentions_field(left, expected) || expression_mentions_field(right, expected)
        }
    }
}

//! Task extension data models.
//!
//! Defines `TaskTag`, `TaskTagBuilder`, and `StatusCategory`.

use std::ops::Range;

use super::config::{StatusKeywords, TaskConfig, ALLOWED_TIME_UNITS};
use crate::error::RagtagError;
use crate::models::{AttributeValue, Tag, TagLocation};

/// Categories for task status keywords, controlling color output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusCategory {
    Done,
    Active,
    Blocked,
    Abandoned,
    Inactive,
    Unknown,
}

/// Determines the status category for a given status keyword.
pub fn categorize_status(status: &str, keywords: &StatusKeywords) -> StatusCategory {
    let lower = status.to_lowercase();
    if keywords.done.iter().any(|k| k.to_lowercase() == lower) {
        StatusCategory::Done
    } else if keywords.active.iter().any(|k| k.to_lowercase() == lower) {
        StatusCategory::Active
    } else if keywords.blocked.iter().any(|k| k.to_lowercase() == lower) {
        StatusCategory::Blocked
    } else if keywords.abandoned.iter().any(|k| k.to_lowercase() == lower) {
        StatusCategory::Abandoned
    } else if keywords.inactive.iter().any(|k| k.to_lowercase() == lower) {
        StatusCategory::Inactive
    } else {
        StatusCategory::Unknown
    }
}

/// A parsed and validated task tag.
///
/// Created from a generic `Tag` via `TryFrom`, applying validation
/// and config defaults.
#[derive(Debug, Clone)]
pub struct TaskTag {
    pub id: String,
    pub pid: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub owner: String,
    pub status: String,
    pub priority: Option<u32>,
    pub time_spent: Option<f64>,
    pub ttc_estimate: f64,
    pub ttc_actual: Option<f64>,
    pub time_units: String,
    pub location: TagLocation,
    pub raw_span: Range<usize>,
    pub trailing_text: Option<String>,
}

/// Extracts a string value from a named attribute, with optional default.
fn get_str(tag: &Tag, name: &str) -> Option<String> {
    tag.get_named_attribute(name).map(|v| match v {
        AttributeValue::Str(s) => s.clone(),
        AttributeValue::Integer { value, .. } => value.to_string(),
        AttributeValue::Float(f) => f.to_string(),
    })
}

/// Extracts a float value from a named attribute (accepts Integer or Float).
fn get_float(tag: &Tag, name: &str) -> Option<f64> {
    tag.get_named_attribute(name).and_then(|v| v.as_float())
}

/// Extracts a u32 value from a named attribute.
fn get_u32(tag: &Tag, name: &str) -> Option<u32> {
    tag.get_named_attribute(name).and_then(|v| match v {
        AttributeValue::Integer { value, .. } => {
            if *value >= 0 && *value <= u32::MAX as i64 {
                Some(*value as u32)
            } else {
                None
            }
        }
        AttributeValue::Float(f) => {
            if *f >= 0.0 && *f <= u32::MAX as f64 {
                Some(*f as u32)
            } else {
                None
            }
        }
        _ => None,
    })
}

impl TaskTag {
    /// Creates a `TaskTag` from a `Tag` and `TaskConfig`, with access to
    /// the source file content for trailing text extraction.
    pub fn from_tag(
        tag: &Tag,
        config: &TaskConfig,
        file_content: &str,
    ) -> Result<Self, RagtagError> {
        let ext_err = |msg: String| RagtagError::ExtensionError {
            extension_name: "Task Manager".to_string(),
            message: msg,
        };

        // Required: title
        let title = get_str(tag, "title")
            .ok_or_else(|| ext_err("missing required attribute \"title\"".to_string()))?;

        // Required: ttc_estimate
        let ttc_estimate = get_float(tag, "ttc_estimate")
            .ok_or_else(|| ext_err("missing required attribute \"ttc_estimate\"".to_string()))?;

        // time_units with default
        let time_units =
            get_str(tag, "time_units").unwrap_or_else(|| config.default_time_units.clone());
        if !ALLOWED_TIME_UNITS.contains(&time_units.as_str()) {
            return Err(ext_err(format!(
                "invalid time_units \"{}\" — allowed values: {}",
                time_units,
                ALLOWED_TIME_UNITS.join(", ")
            )));
        }

        // status with default
        let status = get_str(tag, "status").unwrap_or_else(|| config.default_status.clone());
        if !config.all_status_keywords().contains(&status.as_str()) {
            return Err(ext_err(format!(
                "invalid status \"{}\" — allowed statuses: {}",
                status,
                config.all_status_keywords().join(", ")
            )));
        }

        // id — required for existing tasks, generated for new ones
        let id = get_str(tag, "id").unwrap_or_default();

        // Extract trailing text
        let trailing_text = extract_trailing_text(file_content, tag.raw_span.end);

        Ok(TaskTag {
            id,
            pid: get_str(tag, "pid"),
            title,
            description: get_str(tag, "description"),
            owner: get_str(tag, "owner").unwrap_or_else(|| config.default_owner.clone()),
            status,
            priority: get_u32(tag, "priority"),
            time_spent: get_float(tag, "time_spent"),
            ttc_estimate,
            ttc_actual: get_float(tag, "ttc_actual"),
            time_units,
            location: tag.location.clone(),
            raw_span: tag.raw_span.clone(),
            trailing_text,
        })
    }
}

/// Extracts trailing text after a tag's closing paren up to the next newline.
fn extract_trailing_text(content: &str, tag_end: usize) -> Option<String> {
    if tag_end >= content.len() {
        return None;
    }
    let rest = &content[tag_end..];
    let line_end = rest.find('\n').unwrap_or(rest.len());
    let text = rest[..line_end].trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

/// Builder for creating tasks via the `create` command.
pub struct TaskTagBuilder {
    pub id: Option<String>,
    pub pid: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub owner: Option<String>,
    pub status: Option<String>,
    pub priority: Option<u32>,
    pub time_spent: Option<f64>,
    pub ttc_estimate: Option<f64>,
    pub ttc_actual: Option<f64>,
    pub time_units: Option<String>,
}

impl TaskTagBuilder {
    /// Creates a new builder with all fields empty.
    pub fn new() -> Self {
        Self {
            id: None,
            pid: None,
            title: None,
            description: None,
            owner: None,
            status: None,
            priority: None,
            time_spent: None,
            ttc_estimate: None,
            ttc_actual: None,
            time_units: None,
        }
    }

    /// Builds a `TaskTag`, applying defaults from config.
    pub fn build(self, config: &TaskConfig) -> Result<TaskTag, RagtagError> {
        let ext_err = |msg: String| RagtagError::ExtensionError {
            extension_name: "Task Manager".to_string(),
            message: msg,
        };

        let title = self
            .title
            .ok_or_else(|| ext_err("missing required field \"title\"".to_string()))?;

        let ttc_estimate = self
            .ttc_estimate
            .ok_or_else(|| ext_err("missing required field \"ttc_estimate\"".to_string()))?;

        let time_units = self
            .time_units
            .unwrap_or_else(|| config.default_time_units.clone());
        if !ALLOWED_TIME_UNITS.contains(&time_units.as_str()) {
            return Err(ext_err(format!(
                "invalid time_units \"{time_units}\" — allowed values: {}",
                ALLOWED_TIME_UNITS.join(", ")
            )));
        }

        let status = self.status.unwrap_or_else(|| config.default_status.clone());
        if !config.all_status_keywords().contains(&status.as_str()) {
            return Err(ext_err(format!(
                "invalid status \"{status}\" — allowed statuses: {}",
                config.all_status_keywords().join(", ")
            )));
        }

        let id = self.id.unwrap_or_default();

        Ok(TaskTag {
            id,
            pid: self.pid,
            title,
            description: self.description,
            owner: self.owner.unwrap_or_else(|| config.default_owner.clone()),
            status,
            priority: self.priority,
            time_spent: self.time_spent,
            ttc_estimate,
            ttc_actual: self.ttc_actual,
            time_units,
            location: TagLocation::new(std::path::PathBuf::new(), 0, 0, 0, 0),
            raw_span: 0..0,
            trailing_text: None,
        })
    }
}

impl Default for TaskTagBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{NumericBase, Tag, TagAttribute};
    use std::path::PathBuf;

    fn make_tag(attrs: Vec<TagAttribute>) -> Tag {
        Tag {
            name: "task".to_string(),
            attributes: attrs,
            location: TagLocation::new(PathBuf::from("test.md"), 1, 1, 0, 50),
            raw_span: 0..50,
        }
    }

    fn default_config() -> TaskConfig {
        TaskConfig::default()
    }

    fn content_with_tag() -> String {
        "@task(id=\"abc123\", title=\"Test\", ttc_estimate=4, time_units=\"hours\") trailing desc\nNext".to_string()
    }

    #[test]
    fn test_from_tag_all_fields() {
        let tag = make_tag(vec![
            TagAttribute::named("id", AttributeValue::Str("abc123".to_string())),
            TagAttribute::named("title", AttributeValue::Str("Test Task".to_string())),
            TagAttribute::named("ttc_estimate", AttributeValue::Float(4.5)),
            TagAttribute::named("time_units", AttributeValue::Str("hours".to_string())),
            TagAttribute::named("status", AttributeValue::Str("active".to_string())),
            TagAttribute::named("owner", AttributeValue::Str("alice".to_string())),
            TagAttribute::named(
                "priority",
                AttributeValue::Integer {
                    value: 1,
                    base: NumericBase::Decimal,
                },
            ),
        ]);
        let task = TaskTag::from_tag(&tag, &default_config(), "").unwrap();
        assert_eq!(task.title, "Test Task");
        assert_eq!(task.ttc_estimate, 4.5);
        assert_eq!(task.status, "active");
        assert_eq!(task.owner, "alice");
    }

    #[test]
    fn test_from_tag_defaults_applied() {
        let tag = make_tag(vec![
            TagAttribute::named("title", AttributeValue::Str("Test".to_string())),
            TagAttribute::named(
                "ttc_estimate",
                AttributeValue::Integer {
                    value: 2,
                    base: NumericBase::Decimal,
                },
            ),
        ]);
        let task = TaskTag::from_tag(&tag, &default_config(), "").unwrap();
        assert_eq!(task.owner, "me");
        assert_eq!(task.status, "new");
        assert_eq!(task.time_units, "hours");
    }

    #[test]
    fn test_from_tag_missing_title() {
        let tag = make_tag(vec![TagAttribute::named(
            "ttc_estimate",
            AttributeValue::Integer {
                value: 1,
                base: NumericBase::Decimal,
            },
        )]);
        assert!(TaskTag::from_tag(&tag, &default_config(), "").is_err());
    }

    #[test]
    fn test_from_tag_missing_ttc_estimate() {
        let tag = make_tag(vec![TagAttribute::named(
            "title",
            AttributeValue::Str("Test".to_string()),
        )]);
        assert!(TaskTag::from_tag(&tag, &default_config(), "").is_err());
    }

    #[test]
    fn test_from_tag_invalid_time_units() {
        let tag = make_tag(vec![
            TagAttribute::named("title", AttributeValue::Str("Test".to_string())),
            TagAttribute::named(
                "ttc_estimate",
                AttributeValue::Integer {
                    value: 1,
                    base: NumericBase::Decimal,
                },
            ),
            TagAttribute::named("time_units", AttributeValue::Str("fortnights".to_string())),
        ]);
        assert!(TaskTag::from_tag(&tag, &default_config(), "").is_err());
    }

    #[test]
    fn test_from_tag_invalid_status() {
        let tag = make_tag(vec![
            TagAttribute::named("title", AttributeValue::Str("Test".to_string())),
            TagAttribute::named(
                "ttc_estimate",
                AttributeValue::Integer {
                    value: 1,
                    base: NumericBase::Decimal,
                },
            ),
            TagAttribute::named("status", AttributeValue::Str("invalid_status".to_string())),
        ]);
        assert!(TaskTag::from_tag(&tag, &default_config(), "").is_err());
    }

    #[test]
    fn test_from_tag_integer_time_field() {
        let tag = make_tag(vec![
            TagAttribute::named("title", AttributeValue::Str("Test".to_string())),
            TagAttribute::named(
                "ttc_estimate",
                AttributeValue::Integer {
                    value: 4,
                    base: NumericBase::Decimal,
                },
            ),
        ]);
        let task = TaskTag::from_tag(&tag, &default_config(), "").unwrap();
        assert_eq!(task.ttc_estimate, 4.0);
    }

    #[test]
    fn test_from_tag_float_time_field() {
        let tag = make_tag(vec![
            TagAttribute::named("title", AttributeValue::Str("Test".to_string())),
            TagAttribute::named("ttc_estimate", AttributeValue::Float(4.5)),
        ]);
        let task = TaskTag::from_tag(&tag, &default_config(), "").unwrap();
        assert_eq!(task.ttc_estimate, 4.5);
    }

    #[test]
    fn test_categorize_status() {
        let kw = StatusKeywords::default();
        assert_eq!(categorize_status("done", &kw), StatusCategory::Done);
        assert_eq!(categorize_status("active", &kw), StatusCategory::Active);
        assert_eq!(categorize_status("blocked", &kw), StatusCategory::Blocked);
        assert_eq!(
            categorize_status("abandoned", &kw),
            StatusCategory::Abandoned
        );
        assert_eq!(categorize_status("new", &kw), StatusCategory::Inactive);
        assert_eq!(categorize_status("xyz", &kw), StatusCategory::Unknown);
    }

    #[test]
    fn test_trailing_text_capture() {
        let content = &content_with_tag();
        let tag = make_tag(vec![
            TagAttribute::named("id", AttributeValue::Str("abc123".to_string())),
            TagAttribute::named("title", AttributeValue::Str("Test".to_string())),
            TagAttribute::named(
                "ttc_estimate",
                AttributeValue::Integer {
                    value: 4,
                    base: NumericBase::Decimal,
                },
            ),
        ]);
        let mut tag = tag;
        tag.raw_span = 0..content.find(')').unwrap() + 1;
        let task = TaskTag::from_tag(&tag, &default_config(), content).unwrap();
        assert!(task.trailing_text.is_some());
        assert_eq!(task.trailing_text.unwrap(), "trailing desc");
    }

    #[test]
    fn test_trailing_text_empty() {
        let tag = make_tag(vec![
            TagAttribute::named("title", AttributeValue::Str("Test".to_string())),
            TagAttribute::named(
                "ttc_estimate",
                AttributeValue::Integer {
                    value: 1,
                    base: NumericBase::Decimal,
                },
            ),
        ]);
        let task = TaskTag::from_tag(&tag, &default_config(), "@task(...)\n").unwrap();
        assert!(task.trailing_text.is_none());
    }

    #[test]
    fn test_builder_ok() {
        let mut builder = TaskTagBuilder::new();
        builder.title = Some("Test".to_string());
        builder.ttc_estimate = Some(4.0);
        builder.id = Some("abc123".to_string());
        let task = builder.build(&default_config()).unwrap();
        assert_eq!(task.title, "Test");
        assert_eq!(task.status, "new");
    }

    #[test]
    fn test_builder_missing_title() {
        let mut builder = TaskTagBuilder::new();
        builder.ttc_estimate = Some(4.0);
        assert!(builder.build(&default_config()).is_err());
    }

    #[test]
    fn test_from_tag_priority_overflow() {
        // An i64 value exceeding u32::MAX should return None for priority
        let tag = make_tag(vec![
            TagAttribute::named("title", AttributeValue::Str("Test".to_string())),
            TagAttribute::named(
                "ttc_estimate",
                AttributeValue::Integer {
                    value: 1,
                    base: NumericBase::Decimal,
                },
            ),
            TagAttribute::named(
                "priority",
                AttributeValue::Integer {
                    value: 5_000_000_000,
                    base: NumericBase::Decimal,
                },
            ),
        ]);
        let task = TaskTag::from_tag(&tag, &default_config(), "").unwrap();
        assert!(task.priority.is_none());
    }

    #[test]
    fn test_from_tag_priority_negative() {
        let tag = make_tag(vec![
            TagAttribute::named("title", AttributeValue::Str("Test".to_string())),
            TagAttribute::named(
                "ttc_estimate",
                AttributeValue::Integer {
                    value: 1,
                    base: NumericBase::Decimal,
                },
            ),
            TagAttribute::named(
                "priority",
                AttributeValue::Integer {
                    value: -1,
                    base: NumericBase::Decimal,
                },
            ),
        ]);
        let task = TaskTag::from_tag(&tag, &default_config(), "").unwrap();
        assert!(task.priority.is_none());
    }
}

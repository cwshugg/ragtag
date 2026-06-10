//! Task tag validation.
//!
//! Validates that tags matching the task extension have the required
//! attributes and valid values.

use super::config::{TaskConfig, ALLOWED_TIME_UNITS};
use crate::extensions::{ValidationLevel, ValidationMessage};
use crate::models::Tag;

/// Validates a task tag against the task configuration.
///
/// Checks for required fields, valid time units, and valid status keywords.
pub fn validate_task_tag(tag: &Tag, config: &TaskConfig) -> Vec<ValidationMessage> {
    let mut messages = Vec::new();

    // Check required: title
    if tag.get_named_attribute("title").is_none() {
        messages.push(ValidationMessage {
            level: ValidationLevel::Error,
            message: "missing required attribute \"title\"".to_string(),
            location: Some(tag.location.clone()),
        });
    }

    // Validate time_units if present
    if let Some(val) = tag.get_named_attribute("time_units") {
        if let Some(s) = val.as_str() {
            if !ALLOWED_TIME_UNITS.contains(&s) {
                messages.push(ValidationMessage {
                    level: ValidationLevel::Error,
                    message: format!(
                        "invalid time_units \"{}\" — allowed values: {}",
                        s,
                        ALLOWED_TIME_UNITS.join(", ")
                    ),
                    location: Some(tag.location.clone()),
                });
            }
        } else {
            messages.push(ValidationMessage {
                level: ValidationLevel::Error,
                message: "time_units must be a string value".to_string(),
                location: Some(tag.location.clone()),
            });
        }
    }

    // Validate status if present
    if let Some(val) = tag.get_named_attribute("status") {
        if let Some(s) = val.as_str() {
            if !config.all_status_keywords().contains(&s) {
                messages.push(ValidationMessage {
                    level: ValidationLevel::Error,
                    message: format!(
                        "invalid status \"{}\" — allowed statuses: {}",
                        s,
                        config.all_status_keywords().join(", ")
                    ),
                    location: Some(tag.location.clone()),
                });
            }
        } else {
            messages.push(ValidationMessage {
                level: ValidationLevel::Error,
                message: "status must be a string value".to_string(),
                location: Some(tag.location.clone()),
            });
        }
    }

    // Validate time fields are numeric
    for field in &["time_spent", "ttc_estimate", "ttc_actual"] {
        if let Some(val) = tag.get_named_attribute(field) {
            if val.as_float().is_none() {
                messages.push(ValidationMessage {
                    level: ValidationLevel::Warning,
                    message: format!("\"{field}\" should be a numeric value"),
                    location: Some(tag.location.clone()),
                });
            }
        }
    }

    messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AttributeValue, NumericBase, TagAttribute, TagLocation};
    use std::path::PathBuf;

    fn make_valid_tag() -> Tag {
        Tag {
            name: "task".to_string(),
            attributes: vec![
                TagAttribute::named("title", AttributeValue::Str("Test".to_string())),
                TagAttribute::named(
                    "ttc_estimate",
                    AttributeValue::Integer {
                        value: 4,
                        base: NumericBase::Decimal,
                    },
                ),
                TagAttribute::named("status", AttributeValue::Str("new".to_string())),
            ],
            location: TagLocation::new(PathBuf::from("test.md"), 1, 1, 0, 50),
            raw_span: 0..50,
        }
    }

    #[test]
    fn test_valid_tag_no_errors() {
        let tag = make_valid_tag();
        let msgs = validate_task_tag(&tag, &TaskConfig::default());
        let errors: Vec<_> = msgs
            .iter()
            .filter(|m| m.level == ValidationLevel::Error)
            .collect();
        assert!(errors.is_empty());
    }

    #[test]
    fn test_missing_title() {
        let tag = Tag {
            name: "task".to_string(),
            attributes: vec![TagAttribute::named(
                "ttc_estimate",
                AttributeValue::Integer {
                    value: 4,
                    base: NumericBase::Decimal,
                },
            )],
            location: TagLocation::new(PathBuf::from("test.md"), 1, 1, 0, 50),
            raw_span: 0..50,
        };
        let msgs = validate_task_tag(&tag, &TaskConfig::default());
        assert!(msgs.iter().any(|m| m.message.contains("title")));
    }

    #[test]
    fn test_invalid_status() {
        let tag = Tag {
            name: "task".to_string(),
            attributes: vec![
                TagAttribute::named("title", AttributeValue::Str("Test".to_string())),
                TagAttribute::named(
                    "ttc_estimate",
                    AttributeValue::Integer {
                        value: 4,
                        base: NumericBase::Decimal,
                    },
                ),
                TagAttribute::named("status", AttributeValue::Str("invalid".to_string())),
            ],
            location: TagLocation::new(PathBuf::from("test.md"), 1, 1, 0, 50),
            raw_span: 0..50,
        };
        let msgs = validate_task_tag(&tag, &TaskConfig::default());
        assert!(msgs.iter().any(|m| m.message.contains("status")));
    }

    #[test]
    fn test_invalid_time_units() {
        let tag = Tag {
            name: "task".to_string(),
            attributes: vec![
                TagAttribute::named("title", AttributeValue::Str("Test".to_string())),
                TagAttribute::named(
                    "ttc_estimate",
                    AttributeValue::Integer {
                        value: 4,
                        base: NumericBase::Decimal,
                    },
                ),
                TagAttribute::named("time_units", AttributeValue::Str("fortnights".to_string())),
            ],
            location: TagLocation::new(PathBuf::from("test.md"), 1, 1, 0, 50),
            raw_span: 0..50,
        };
        let msgs = validate_task_tag(&tag, &TaskConfig::default());
        assert!(msgs.iter().any(|m| m.message.contains("time_units")));
    }

    #[test]
    fn test_non_string_status_rejected() {
        let tag = Tag {
            name: "task".to_string(),
            attributes: vec![
                TagAttribute::named("title", AttributeValue::Str("Test".to_string())),
                TagAttribute::named(
                    "ttc_estimate",
                    AttributeValue::Integer {
                        value: 4,
                        base: NumericBase::Decimal,
                    },
                ),
                TagAttribute::named(
                    "status",
                    AttributeValue::Integer {
                        value: 42,
                        base: NumericBase::Decimal,
                    },
                ),
            ],
            location: TagLocation::new(PathBuf::from("test.md"), 1, 1, 0, 50),
            raw_span: 0..50,
        };
        let msgs = validate_task_tag(&tag, &TaskConfig::default());
        assert!(msgs
            .iter()
            .any(|m| m.message.contains("status must be a string")));
    }

    #[test]
    fn test_non_string_time_units_rejected() {
        let tag = Tag {
            name: "task".to_string(),
            attributes: vec![
                TagAttribute::named("title", AttributeValue::Str("Test".to_string())),
                TagAttribute::named(
                    "ttc_estimate",
                    AttributeValue::Integer {
                        value: 4,
                        base: NumericBase::Decimal,
                    },
                ),
                TagAttribute::named("time_units", AttributeValue::Float(3.5)),
            ],
            location: TagLocation::new(PathBuf::from("test.md"), 1, 1, 0, 50),
            raw_span: 0..50,
        };
        let msgs = validate_task_tag(&tag, &TaskConfig::default());
        assert!(msgs
            .iter()
            .any(|m| m.message.contains("time_units must be a string")));
    }
}

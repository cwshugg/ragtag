//! Bounded backend-neutral diagram documents and task projections.

use std::collections::BTreeSet;

use super::diagnostics::Diagnostic;
use super::graph::{integer, text, NodeKey, ValidatedForest};
use super::task::selection::TaskSelection;
use super::DiagramLimits;

/// Diagram layout direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Down,
    Right,
    Up,
    Left,
}

impl Direction {
    pub(crate) fn d2(self) -> &'static str {
        match self {
            Self::Down => "down",
            Self::Right => "right",
            Self::Up => "up",
            Self::Left => "left",
        }
    }
}

/// A sanitized label with one trusted semantic newline.
#[derive(Debug, Clone)]
pub(crate) struct Label {
    pub(crate) first: String,
    pub(crate) second: String,
}

/// Neutral element role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ElementRole {
    Task,
    Bucket,
}

/// Trusted task status style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatusRole {
    Done,
    Active,
    Blocked,
    Abandoned,
    Inactive,
}

/// One neutral diagram element.
#[derive(Debug, Clone)]
pub(crate) struct Element {
    pub(crate) id: String,
    pub(crate) parent: Option<String>,
    pub(crate) label: Label,
    pub(crate) role: ElementRole,
    pub(crate) status: StatusRole,
    pub(crate) context: bool,
    pub(crate) priority: Option<u32>,
}

/// One neutral document edge.
#[derive(Debug, Clone)]
pub(crate) struct Edge {
    pub(crate) from: String,
    pub(crate) to: String,
}

/// Validated backend-neutral document.
#[derive(Debug)]
pub(crate) struct Document {
    pub(crate) direction: Direction,
    pub(crate) elements: Vec<Element>,
    pub(crate) edges: Vec<Edge>,
}

/// Projects selected tasks as a flat node-and-edge tree document.
pub(crate) fn project_tree(
    forest: &ValidatedForest,
    selection: &TaskSelection,
    direction: Direction,
    limits: &DiagramLimits,
) -> Result<Document, Vec<Diagnostic>> {
    let mut keys = selection.keys().cloned().collect::<Vec<_>>();
    keys.sort_by(|left, right| stable_task_order(forest, left, right));
    let elements = build_elements(forest, selection, &keys, limits, |_| {
        (None, ElementRole::Task)
    })?;
    let edges = forest
        .edges()
        .iter()
        .filter(|edge| selection.contains_key(&edge.from) && selection.contains_key(&edge.to))
        .map(|edge| Edge {
            from: edge.from.0.clone(),
            to: edge.to.0.clone(),
        })
        .collect();
    validate_document(Document {
        direction,
        elements,
        edges,
    })
}

/// Projects selected tasks as recursive containment buckets.
pub(crate) fn project_buckets(
    forest: &ValidatedForest,
    selection: &TaskSelection,
    direction: Direction,
    limits: &DiagramLimits,
) -> Result<Document, Vec<Diagnostic>> {
    let keys = bucket_postorder(forest, selection);
    let elements = build_elements(forest, selection, &keys, limits, |key| {
        let selected_children = forest
            .children(key)
            .iter()
            .filter(|child| selection.contains_key(*child))
            .count();
        (
            forest
                .parent(key)
                .filter(|parent| selection.contains_key(*parent))
                .map(|parent| parent.0.clone()),
            if selected_children == 0 {
                ElementRole::Task
            } else {
                ElementRole::Bucket
            },
        )
    })?;
    validate_document(Document {
        direction,
        elements,
        edges: Vec::new(),
    })
}

fn build_elements(
    forest: &ValidatedForest,
    selection: &TaskSelection,
    keys: &[NodeKey],
    limits: &DiagramLimits,
    mut hierarchy: impl FnMut(&NodeKey) -> (Option<String>, ElementRole),
) -> Result<Vec<Element>, Vec<Diagnostic>> {
    let mut elements = Vec::with_capacity(keys.len());
    let mut text_bytes = 0usize;

    for key in keys {
        let node = forest.node(key).expect("selection keys are validated");
        let title = sanitize(
            text(node, "title")
                .filter(|value| !value.is_empty())
                .unwrap_or("(untitled)"),
            limits.maximum_document_field_bytes,
        )?;
        let status = sanitize(
            text(node, "status").unwrap_or_default(),
            limits.maximum_document_field_bytes,
        )?;
        let owner = sanitize(
            text(node, "owner").unwrap_or_default(),
            limits.maximum_document_field_bytes,
        )?;
        let priority = integer(node, "priority").and_then(|value| u32::try_from(value).ok());
        let mut second = format!("[{status}]");
        if let Some(priority) = priority {
            second.push_str(&format!(" · P{priority}"));
        }
        if !owner.is_empty() {
            second.push_str(" · ");
            second.push_str(&owner);
        }
        if title.len().saturating_add(second.len()).saturating_add(1) > limits.maximum_label_bytes {
            return Err(one("DIA-DOC-006", "document label limit exceeded"));
        }
        text_bytes = text_bytes
            .checked_add(title.len() + second.len())
            .ok_or_else(|| one("DIA-DOC-002", "document text limit exceeded"))?;
        if text_bytes > limits.maximum_document_text {
            return Err(one("DIA-DOC-002", "document text limit exceeded"));
        }

        let (parent, role) = hierarchy(key);
        elements.push(Element {
            id: key.0.clone(),
            parent,
            label: Label {
                first: title,
                second,
            },
            role,
            status: status_role(text(node, "status_category")),
            context: selection[key],
            priority,
        });
    }
    Ok(elements)
}

fn bucket_postorder(forest: &ValidatedForest, selection: &TaskSelection) -> Vec<NodeKey> {
    let mut roots = selection
        .keys()
        .filter(|key| {
            forest
                .parent(key)
                .is_none_or(|parent| !selection.contains_key(parent))
        })
        .cloned()
        .collect::<Vec<_>>();
    roots.sort_by(|left, right| stable_task_order(forest, left, right));
    let mut postorder = Vec::with_capacity(selection.len());
    for root in roots {
        let mut stack = vec![(root, false)];
        while let Some((key, expanded)) = stack.pop() {
            if expanded {
                postorder.push(key);
                continue;
            }
            stack.push((key.clone(), true));
            let mut children = forest
                .children(&key)
                .iter()
                .filter(|child| selection.contains_key(*child))
                .cloned()
                .collect::<Vec<_>>();
            children.sort_by(|left, right| stable_task_order(forest, left, right));
            stack.extend(children.into_iter().rev().map(|child| (child, false)));
        }
    }
    postorder
}

fn stable_task_order(
    forest: &ValidatedForest,
    left: &NodeKey,
    right: &NodeKey,
) -> std::cmp::Ordering {
    let left = forest.node(left).expect("validated key");
    let right = forest.node(right).expect("validated key");
    integer(left, "priority")
        .unwrap_or(i64::MAX)
        .cmp(&integer(right, "priority").unwrap_or(i64::MAX))
        .then_with(|| text(left, "title").cmp(&text(right, "title")))
        .then_with(|| text(left, "id").cmp(&text(right, "id")))
        .then_with(|| text(left, "source_path").cmp(&text(right, "source_path")))
        .then_with(|| integer(left, "source_offset").cmp(&integer(right, "source_offset")))
        .then_with(|| left.key.cmp(&right.key))
}

fn status_role(value: Option<&str>) -> StatusRole {
    match value {
        Some("done") => StatusRole::Done,
        Some("active") => StatusRole::Active,
        Some("blocked") => StatusRole::Blocked,
        Some("abandoned") => StatusRole::Abandoned,
        _ => StatusRole::Inactive,
    }
}

pub(crate) fn validate_document(document: Document) -> Result<Document, Vec<Diagnostic>> {
    let identifiers = document
        .elements
        .iter()
        .map(|element| element.id.as_str())
        .collect::<BTreeSet<_>>();
    if identifiers.len() != document.elements.len() {
        return Err(one(
            "DIA-DOC-003",
            "document contains duplicate element identifiers",
        ));
    }
    if document.elements.iter().any(|element| {
        element
            .parent
            .as_deref()
            .is_some_and(|parent| !identifiers.contains(parent))
    }) || document.edges.iter().any(|edge| {
        !identifiers.contains(edge.from.as_str()) || !identifiers.contains(edge.to.as_str())
    }) {
        return Err(one("DIA-DOC-004", "document references an unknown element"));
    }
    Ok(document)
}

fn sanitize(value: &str, maximum: usize) -> Result<String, Vec<Diagnostic>> {
    let mut output = String::new();
    for character in value.chars() {
        if character.is_control()
            || matches!(
                character,
                '\u{2028}' | '\u{2029}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
            )
        {
            output.extend(character.escape_unicode());
        } else {
            output.push(character);
        }
        if output.len() > maximum {
            return Err(one("DIA-DOC-005", "document field limit exceeded"));
        }
    }
    Ok(output)
}

fn one(code: &'static str, message: &'static str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(code, message)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizer_removes_controls_and_semantic_newlines() {
        let sanitized = sanitize("hello\n\u{1b}[31m\u{202e}", 64 * 1024).unwrap();
        assert!(!sanitized.contains('\n'));
        assert!(!sanitized.contains('\u{1b}'));
        assert!(!sanitized.contains('\u{202e}'));
        assert!(sanitized.contains("\\u{a}"));
    }

    #[test]
    fn sanitizer_uses_injected_field_limit_at_unicode_scalar_boundary() {
        assert!(sanitize("é", 1).is_err());
        assert_eq!(sanitize("é", 2).unwrap(), "é");
    }
}

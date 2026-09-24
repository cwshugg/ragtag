//! Task-tag provider for diagram forest pipelines.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use crate::diagram::catalog::ForestProvider;
use crate::diagram::diagnostics::{Diagnostic, DiagnosticBag};
use crate::diagram::graph::{
    GraphEdge, GraphNode, GraphValue, NodeKey, ProviderId, ProviderResult, UnvalidatedGraph,
};
use crate::diagram::source::SourceIndex;
use crate::diagram::DiagramLimits;
use crate::extensions::task::models::{StatusCategory, TaskTag};
use crate::extensions::task::semantics::TaskSemantics;
use crate::models::AttributeKind;

const RECOGNIZED_FIELDS: [&str; 12] = [
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

/// Converts parsed task occurrences into the authoritative task forest.
pub(crate) struct TaskGraphProvider {
    id: ProviderId,
    semantics: Arc<TaskSemantics>,
}

impl TaskGraphProvider {
    pub(crate) fn new(id: ProviderId, semantics: Arc<TaskSemantics>) -> Self {
        Self { id, semantics }
    }

    #[cfg(test)]
    pub(crate) fn semantics(&self) -> &Arc<TaskSemantics> {
        &self.semantics
    }
}

impl ForestProvider for TaskGraphProvider {
    fn provider_id(&self) -> ProviderId {
        self.id
    }

    fn provide(&self, source: &SourceIndex, limits: &DiagramLimits) -> ProviderResult {
        let mut diagnostics = DiagnosticBag::new(2_048);
        let mut nodes = Vec::new();
        let mut ids = BTreeMap::<String, NodeKey>::new();
        let mut pending_parents = Vec::<(NodeKey, String)>::new();
        let mut retained_property_count = 0usize;
        let mut retained_property_bytes = 0usize;

        for (index, occurrence) in source
            .named(self.semantics.config().tag_name.as_str())
            .enumerate()
        {
            if nodes.len() >= limits.maximum_nodes {
                diagnostics.push(Diagnostic::error(
                    "DGM-LIMIT-002",
                    "task node limit exceeded",
                ));
                break;
            }
            let key = NodeKey(format!(
                "{}:{}:{}",
                source.normalized_path(occurrence.file).to_string_lossy(),
                occurrence.tag.location.byte_offset,
                index
            ));
            let repeated = repeated_fields(&occurrence.tag);
            if !repeated.is_empty() {
                diagnostics.push(Diagnostic::warning(
                    "DGM-TASK-004",
                    "task contains repeated recognized attributes; the first value is used",
                ));
            }
            let task = match TaskTag::from_tag(&occurrence.tag, self.semantics.config()) {
                Ok(task) => task,
                Err(_) => {
                    diagnostics.push(Diagnostic::error("DGM-TASK-009", "task record is invalid"));
                    continue;
                }
            };
            if task.title.is_empty() {
                diagnostics.push(Diagnostic::warning(
                    "DGM-TASK-008",
                    "task title is empty; the diagram uses an untitled label",
                ));
            }
            if task.id.is_empty() {
                diagnostics.push(Diagnostic::warning(
                    "DGM-TASK-001",
                    "task has no explicit ID and is shown as a standalone task",
                ));
                if task.pid.is_some() {
                    diagnostics.push(Diagnostic::warning(
                        "DGM-TASK-002",
                        "parent reference on a task without an ID is ignored",
                    ));
                }
            } else if ids.insert(task.id.clone(), key.clone()).is_some() {
                diagnostics.push(Diagnostic::error("DGM-TASK-003", "duplicate task ID"));
            }

            let source_path = source.normalized_path(occurrence.file).to_string_lossy();
            let source_offset =
                i64::try_from(occurrence.tag.location.byte_offset).unwrap_or(i64::MAX);
            let Some((property_count, property_bytes)) = property_metrics(
                &task,
                &self.semantics,
                &source_path,
                source_offset,
                limits,
                &mut diagnostics,
            ) else {
                break;
            };
            let Some(next_property_count) = retained_property_count.checked_add(property_count)
            else {
                diagnostics.push(Diagnostic::error(
                    "DGM-LIMIT-006",
                    "aggregate task property limit exceeded",
                ));
                break;
            };
            let Some(next_property_bytes) = retained_property_bytes.checked_add(property_bytes)
            else {
                diagnostics.push(Diagnostic::error(
                    "DGM-LIMIT-007",
                    "aggregate task property byte limit exceeded",
                ));
                break;
            };
            if next_property_count > limits.maximum_properties {
                diagnostics.push(Diagnostic::error(
                    "DGM-LIMIT-006",
                    "aggregate task property limit exceeded",
                ));
                break;
            }
            if next_property_bytes > limits.maximum_graph_property_bytes {
                diagnostics.push(Diagnostic::error(
                    "DGM-LIMIT-007",
                    "aggregate task property byte limit exceeded",
                ));
                break;
            }
            retained_property_count = next_property_count;
            retained_property_bytes = next_property_bytes;
            let properties = task_properties(&task, &self.semantics, &source_path, source_offset);
            if !task.id.is_empty() {
                if let Some(parent) = task.pid.as_deref() {
                    if parent == task.id {
                        diagnostics.push(Diagnostic::error(
                            "DGM-TASK-006",
                            "task cannot be its own parent",
                        ));
                    }
                    pending_parents.push((key.clone(), parent.to_string()));
                }
            }
            nodes.push(GraphNode {
                provider: self.id,
                key,
                properties,
            });
        }

        let mut edges = Vec::new();
        for (child, parent_id) in pending_parents {
            if let Some(parent) = ids.get(&parent_id) {
                if edges.len() >= limits.maximum_edges {
                    diagnostics.push(Diagnostic::error(
                        "DGM-LIMIT-003",
                        "task edge limit exceeded",
                    ));
                    break;
                }
                edges.push(GraphEdge {
                    provider: self.id,
                    from: parent.clone(),
                    to: child,
                });
            } else {
                diagnostics.push(Diagnostic::warning(
                    "DGM-TASK-005",
                    "task references an unknown parent and is shown as a root",
                ));
            }
        }
        if contains_cycle(&nodes, &edges) {
            diagnostics.push(Diagnostic::error(
                "DGM-TASK-007",
                "task parent hierarchy contains a cycle",
            ));
        }

        ProviderResult {
            graph: Some(UnvalidatedGraph {
                provider: self.id,
                nodes,
                edges,
            }),
            diagnostics: diagnostics.into_vec(),
        }
    }
}

fn contains_cycle(nodes: &[GraphNode], edges: &[GraphEdge]) -> bool {
    let parents = edges
        .iter()
        .map(|edge| (&edge.to, &edge.from))
        .collect::<HashMap<_, _>>();
    let mut completed = HashSet::new();
    for node in nodes {
        if completed.contains(&node.key) {
            continue;
        }
        let mut positions = HashSet::new();
        let mut path = Vec::new();
        let mut cursor = &node.key;
        loop {
            if completed.contains(cursor) {
                break;
            }
            if !positions.insert(cursor.clone()) {
                return true;
            }
            path.push(cursor.clone());
            let Some(parent) = parents.get(cursor) else {
                break;
            };
            cursor = parent;
        }
        completed.extend(path);
    }
    false
}

fn repeated_fields(tag: &crate::models::Tag) -> BTreeSet<&str> {
    let mut seen = BTreeSet::new();
    let mut repeated = BTreeSet::new();
    for attribute in &tag.attributes {
        if let AttributeKind::Named { name, .. } = &attribute.kind {
            if RECOGNIZED_FIELDS.contains(&name.as_str()) && !seen.insert(name.as_str()) {
                repeated.insert(name.as_str());
            }
        }
    }
    repeated
}

fn task_properties(
    task: &TaskTag,
    semantics: &TaskSemantics,
    source_path: &str,
    source_offset: i64,
) -> BTreeMap<&'static str, GraphValue> {
    let mut properties = BTreeMap::new();
    visit_task_properties(
        task,
        semantics,
        source_path,
        source_offset,
        |name, value| {
            properties.insert(name, value.to_owned());
        },
    );
    properties
}

#[derive(Clone, Copy)]
enum PropertyValue<'a> {
    Text(&'a str),
    Integer(i64),
    Number(f64),
}

impl PropertyValue<'_> {
    fn byte_len(self) -> usize {
        match self {
            Self::Text(value) => value.len(),
            Self::Integer(_) | Self::Number(_) => std::mem::size_of::<u64>(),
        }
    }

    fn to_owned(self) -> GraphValue {
        match self {
            Self::Text(value) => GraphValue::Text(value.to_string()),
            Self::Integer(value) => GraphValue::Integer(value),
            Self::Number(value) => GraphValue::Number(value),
        }
    }
}

fn property_metrics(
    task: &TaskTag,
    semantics: &TaskSemantics,
    source_path: &str,
    source_offset: i64,
    limits: &DiagramLimits,
    diagnostics: &mut DiagnosticBag,
) -> Option<(usize, usize)> {
    let mut count = 0usize;
    let mut bytes = 0usize;
    let mut value_too_large = false;
    visit_task_properties(
        task,
        semantics,
        source_path,
        source_offset,
        |name, value| {
            count = count.saturating_add(1);
            let value_bytes = value.byte_len();
            value_too_large |= value_bytes > limits.maximum_graph_value_bytes;
            bytes = bytes.saturating_add(name.len()).saturating_add(value_bytes);
        },
    );
    if count > limits.maximum_properties_per_node {
        diagnostics.push(Diagnostic::error(
            "DGM-LIMIT-004",
            "task property limit exceeded",
        ));
        return None;
    }
    if value_too_large {
        diagnostics.push(Diagnostic::error(
            "DGM-LIMIT-005",
            "task property value limit exceeded",
        ));
        return None;
    }
    Some((count, bytes))
}

fn visit_task_properties<'a>(
    task: &'a TaskTag,
    semantics: &TaskSemantics,
    source_path: &'a str,
    source_offset: i64,
    mut visit: impl FnMut(&'static str, PropertyValue<'a>),
) {
    if !task.id.is_empty() {
        visit("id", PropertyValue::Text(&task.id));
    }
    if let Some(value) = task.pid.as_deref() {
        visit("pid", PropertyValue::Text(value));
    }
    visit("title", PropertyValue::Text(&task.title));
    if let Some(value) = task.description.as_deref() {
        visit("description", PropertyValue::Text(value));
    }
    visit("owner", PropertyValue::Text(&task.owner));
    visit("status", PropertyValue::Text(&task.status));
    visit(
        "status_category",
        PropertyValue::Text(match semantics.status_category(&task.status) {
            StatusCategory::Done => "done",
            StatusCategory::Active => "active",
            StatusCategory::Blocked => "blocked",
            StatusCategory::Abandoned => "abandoned",
            StatusCategory::Inactive | StatusCategory::Unknown => "inactive",
        }),
    );
    if let Some(value) = task.priority {
        visit("priority", PropertyValue::Integer(i64::from(value)));
    }
    if let Some(value) = task.worktime_spent {
        visit("worktime_spent", PropertyValue::Number(value));
    }
    if let Some(value) = task.worktime_estimate {
        visit("worktime_estimate", PropertyValue::Number(value));
    }
    if let Some(value) = task.time_created.as_deref() {
        visit("time_created", PropertyValue::Text(value));
    }
    if let Some(value) = task.time_last_updated.as_deref() {
        visit("time_last_updated", PropertyValue::Text(value));
    }
    visit("worktime_units", PropertyValue::Text(&task.worktime_units));
    visit("source_path", PropertyValue::Text(source_path));
    visit("source_offset", PropertyValue::Integer(source_offset));
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::models::TagLocation;

    fn task(title: String) -> TaskTag {
        TaskTag {
            id: "id".to_string(),
            pid: None,
            title,
            description: None,
            owner: String::new(),
            status: "active".to_string(),
            priority: None,
            worktime_spent: None,
            worktime_estimate: None,
            time_created: None,
            time_last_updated: None,
            worktime_units: "hours".to_string(),
            location: TagLocation::new(PathBuf::from("test.md"), 1, 1, 0, 1),
            raw_span: 0..1,
        }
    }

    #[test]
    fn provider_rejects_value_limit_before_building_owned_properties() {
        let (semantics, warnings) = TaskSemantics::resolve(None).unwrap();
        assert!(warnings.is_empty());
        let limits = DiagramLimits {
            maximum_graph_value_bytes: 3,
            ..DiagramLimits::default()
        };
        let mut diagnostics = DiagnosticBag::new(limits.maximum_diagnostics);

        assert!(property_metrics(
            &task("four".to_string()),
            &semantics,
            "test.md",
            0,
            &limits,
            &mut diagnostics,
        )
        .is_none());
        assert_eq!(diagnostics.into_vec()[0].code, "DGM-LIMIT-005");
    }

    #[test]
    fn provider_counts_properties_before_building_owned_map() {
        let (semantics, warnings) = TaskSemantics::resolve(None).unwrap();
        assert!(warnings.is_empty());
        let limits = DiagramLimits {
            maximum_properties_per_node: 1,
            ..DiagramLimits::default()
        };
        let mut diagnostics = DiagnosticBag::new(limits.maximum_diagnostics);

        assert!(property_metrics(
            &task("title".to_string()),
            &semantics,
            "test.md",
            0,
            &limits,
            &mut diagnostics,
        )
        .is_none());
        assert_eq!(diagnostics.into_vec()[0].code, "DGM-LIMIT-004");
    }
}

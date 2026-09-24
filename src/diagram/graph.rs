//! Typed unvalidated and validated diagram graph contracts.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::diagnostics::{Diagnostic, DiagnosticBag, Severity};
use super::DiagramLimits;

/// Stable provider identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ProviderId(pub(crate) &'static str);

/// Stable graph node identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct NodeKey(pub(crate) String);

/// A bounded graph property value.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum GraphValue {
    Text(String),
    Integer(i64),
    Number(f64),
}

/// One unvalidated graph node.
#[derive(Debug, Clone)]
pub(crate) struct GraphNode {
    pub(crate) provider: ProviderId,
    pub(crate) key: NodeKey,
    pub(crate) properties: BTreeMap<&'static str, GraphValue>,
}

/// One directed graph edge.
#[derive(Debug, Clone)]
pub(crate) struct GraphEdge {
    pub(crate) provider: ProviderId,
    pub(crate) from: NodeKey,
    pub(crate) to: NodeKey,
}

/// Partial graph output from a provider.
#[derive(Debug)]
pub(crate) struct UnvalidatedGraph {
    pub(crate) provider: ProviderId,
    pub(crate) nodes: Vec<GraphNode>,
    pub(crate) edges: Vec<GraphEdge>,
}

/// Complete provider outcome, including partial data and provider diagnostics.
#[derive(Debug)]
pub(crate) struct ProviderResult {
    pub(crate) graph: Option<UnvalidatedGraph>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

/// Validated general graph capability.
#[derive(Debug)]
pub(crate) struct ValidatedGraph {
    nodes: BTreeMap<NodeKey, GraphNode>,
    edges: Vec<GraphEdge>,
}

impl ValidatedGraph {
    pub(crate) fn nodes(&self) -> impl Iterator<Item = &GraphNode> {
        self.nodes.values()
    }

    pub(crate) fn node(&self, key: &NodeKey) -> Option<&GraphNode> {
        self.nodes.get(key)
    }

    pub(crate) fn edges(&self) -> &[GraphEdge] {
        &self.edges
    }
}

/// Validated single-parent acyclic forest capability.
#[derive(Debug)]
pub(crate) struct ValidatedForest {
    graph: ValidatedGraph,
    parents: BTreeMap<NodeKey, NodeKey>,
    children: BTreeMap<NodeKey, Vec<NodeKey>>,
}

impl ValidatedForest {
    pub(crate) fn nodes(&self) -> impl Iterator<Item = &GraphNode> {
        self.graph.nodes()
    }

    pub(crate) fn node(&self, key: &NodeKey) -> Option<&GraphNode> {
        self.graph.node(key)
    }

    pub(crate) fn edges(&self) -> &[GraphEdge] {
        self.graph.edges()
    }

    pub(crate) fn parent(&self, key: &NodeKey) -> Option<&NodeKey> {
        self.parents.get(key)
    }

    pub(crate) fn children(&self, key: &NodeKey) -> &[NodeKey] {
        self.children
            .get(key)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
}

/// Capability/no-error or no-capability/error validator outcome.
pub(crate) struct ValidationResult<T> {
    capability: Option<T>,
    diagnostics: Vec<Diagnostic>,
}

impl<T> ValidationResult<T> {
    pub(crate) fn into_parts(self) -> (Option<T>, Vec<Diagnostic>) {
        (self.capability, self.diagnostics)
    }
}

struct GraphParts {
    nodes: BTreeMap<NodeKey, GraphNode>,
    edges: Vec<GraphEdge>,
}

/// The sole constructor for validated graph and forest capabilities.
pub(crate) struct GraphValidator;

impl GraphValidator {
    #[allow(dead_code)]
    pub(crate) fn validate_graph(
        provider: ProviderResult,
        expected: ProviderId,
        limits: &DiagramLimits,
    ) -> ValidationResult<ValidatedGraph> {
        let (parts, diagnostics) = inspect_graph(provider, expected, limits);
        let capability = if has_errors(&diagnostics) {
            None
        } else {
            parts.map(|parts| ValidatedGraph {
                nodes: parts.nodes,
                edges: parts.edges,
            })
        };
        ValidationResult {
            capability,
            diagnostics,
        }
    }

    pub(crate) fn validate_forest(
        provider: ProviderResult,
        expected: ProviderId,
        limits: &DiagramLimits,
    ) -> ValidationResult<ValidatedForest> {
        let (parts, mut diagnostics) = inspect_graph(provider, expected, limits);
        let mut parents = BTreeMap::new();
        let mut children = BTreeMap::<NodeKey, Vec<NodeKey>>::new();
        if let Some(parts) = parts.as_ref() {
            let mut integrity = DiagnosticBag::new(2_048);
            for edge in &parts.edges {
                if edge.from == edge.to {
                    integrity.push(Diagnostic::error(
                        "DIA-FOREST-001",
                        "forest contains a self-parent edge",
                    ));
                }
                if parents.insert(edge.to.clone(), edge.from.clone()).is_some() {
                    integrity.push(Diagnostic::error(
                        "DIA-FOREST-002",
                        "forest node has more than one parent",
                    ));
                }
                children
                    .entry(edge.from.clone())
                    .or_default()
                    .push(edge.to.clone());
            }
            for values in children.values_mut() {
                values.sort();
            }
            let hierarchy = inspect_hierarchy(
                &parts.nodes,
                &parents,
                limits.maximum_nesting,
                &mut integrity,
            );
            debug_assert!(hierarchy.visited_nodes <= parts.nodes.len());
            if hierarchy.nesting_exceeded {
                integrity.push(Diagnostic::error(
                    "DIA-FOREST-004",
                    "forest nesting limit exceeded",
                ));
            }
            diagnostics.extend(integrity.into_vec());
        }
        let capability = if has_errors(&diagnostics) {
            None
        } else {
            parts.map(|parts| ValidatedForest {
                graph: ValidatedGraph {
                    nodes: parts.nodes,
                    edges: parts.edges,
                },
                parents,
                children,
            })
        };
        ValidationResult {
            capability,
            diagnostics,
        }
    }
}

fn inspect_graph(
    provider: ProviderResult,
    expected: ProviderId,
    limits: &DiagramLimits,
) -> (Option<GraphParts>, Vec<Diagnostic>) {
    let mut provider_diagnostics = DiagnosticBag::new(2_048);
    for diagnostic in provider.diagnostics {
        provider_diagnostics.push(diagnostic);
    }
    let mut diagnostics = provider_diagnostics.into_vec();
    let Some(graph) = provider.graph else {
        if !has_errors(&diagnostics) {
            diagnostics.push(Diagnostic::error(
                "DIA-GRAPH-001",
                "provider returned no graph and no error",
            ));
        }
        return (None, diagnostics);
    };
    let mut integrity = DiagnosticBag::new(2_048);
    if graph.provider != expected {
        integrity.push(Diagnostic::error(
            "DIA-GRAPH-002",
            "graph provider identity does not match registration",
        ));
    }
    if graph.nodes.len() > limits.maximum_nodes {
        integrity.push(Diagnostic::error(
            "DIA-GRAPH-003",
            "graph node limit exceeded",
        ));
    }
    if graph.edges.len() > limits.maximum_edges {
        integrity.push(Diagnostic::error(
            "DIA-GRAPH-004",
            "graph edge limit exceeded",
        ));
    }

    let mut nodes = BTreeMap::new();
    let mut property_count = 0usize;
    let mut property_bytes = 0usize;
    for node in graph.nodes {
        if node.provider != expected {
            integrity.push(Diagnostic::error(
                "DIA-GRAPH-005",
                "node provider identity is invalid",
            ));
        }
        if node.properties.len() > limits.maximum_properties_per_node {
            integrity.push(Diagnostic::error(
                "DIA-GRAPH-010",
                "graph node property limit exceeded",
            ));
        }
        property_count = property_count.saturating_add(node.properties.len());
        for (key, value) in &node.properties {
            let value_bytes = match value {
                GraphValue::Text(value) => value.len(),
                GraphValue::Integer(_) | GraphValue::Number(_) => std::mem::size_of::<u64>(),
            };
            if value_bytes > limits.maximum_graph_value_bytes {
                integrity.push(Diagnostic::error(
                    "DIA-GRAPH-011",
                    "graph property value limit exceeded",
                ));
            }
            property_bytes = property_bytes
                .saturating_add(key.len())
                .saturating_add(value_bytes);
        }
        if node
            .properties
            .values()
            .any(|value| matches!(value, GraphValue::Number(number) if !number.is_finite()))
        {
            integrity.push(Diagnostic::error(
                "DIA-GRAPH-006",
                "graph contains a non-finite number",
            ));
        }
        if nodes.insert(node.key.clone(), node).is_some() {
            integrity.push(Diagnostic::error(
                "DIA-GRAPH-007",
                "graph contains a duplicate node key",
            ));
        }
    }
    if property_count > limits.maximum_properties {
        integrity.push(Diagnostic::error(
            "DIA-GRAPH-012",
            "aggregate graph property limit exceeded",
        ));
    }
    if property_bytes > limits.maximum_graph_property_bytes {
        integrity.push(Diagnostic::error(
            "DIA-GRAPH-013",
            "aggregate graph property byte limit exceeded",
        ));
    }
    for edge in &graph.edges {
        if edge.provider != expected {
            integrity.push(Diagnostic::error(
                "DIA-GRAPH-008",
                "edge provider identity is invalid",
            ));
        }

        if !nodes.contains_key(&edge.from) || !nodes.contains_key(&edge.to) {
            integrity.push(Diagnostic::error(
                "DIA-GRAPH-009",
                "graph edge references an unknown node",
            ));
        }
    }
    diagnostics.extend(integrity.into_vec());
    (
        Some(GraphParts {
            nodes,
            edges: graph.edges,
        }),
        diagnostics,
    )
}

struct HierarchyInspection {
    nesting_exceeded: bool,
    visited_nodes: usize,
}

fn inspect_hierarchy(
    nodes: &BTreeMap<NodeKey, GraphNode>,
    parents: &BTreeMap<NodeKey, NodeKey>,
    maximum: usize,
    diagnostics: &mut DiagnosticBag,
) -> HierarchyInspection {
    let parent_lookup = parents.iter().collect::<HashMap<_, _>>();
    let mut depths = HashMap::<&NodeKey, usize>::new();
    let mut nesting_exceeded = false;
    let mut visited_nodes = 0usize;
    for start in nodes.keys() {
        if depths.contains_key(start) {
            continue;
        }
        let mut positions = HashSet::new();
        let mut path = Vec::new();
        let mut cursor = start;
        let mut base_depth = None;
        let mut cycle = false;
        loop {
            if let Some(depth) = depths.get(cursor) {
                base_depth = Some(*depth);
                break;
            }
            if !positions.insert(cursor) {
                diagnostics.push(Diagnostic::error(
                    "DIA-FOREST-003",
                    "forest hierarchy contains a cycle",
                ));
                cycle = true;
                break;
            }
            path.push(cursor);
            visited_nodes = visited_nodes.saturating_add(1);
            let Some(parent) = parent_lookup.get(cursor) else {
                let root = path.pop().expect("hierarchy path contains its root");
                depths.insert(root, 0);
                base_depth = Some(0);
                break;
            };
            cursor = parent;
        }
        if cycle {
            for key in path {
                depths.insert(key, usize::MAX);
            }
            continue;
        }
        let mut depth = base_depth.expect("acyclic hierarchy has a base depth");
        for key in path.into_iter().rev() {
            depth = depth.saturating_add(1);
            nesting_exceeded |= depth > maximum;
            depths.insert(key, depth);
        }
    }
    debug_assert!(visited_nodes <= nodes.len());
    HierarchyInspection {
        nesting_exceeded,
        visited_nodes,
    }
}

fn has_errors(diagnostics: &[Diagnostic]) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
}

pub(crate) fn text<'a>(node: &'a GraphNode, key: &str) -> Option<&'a str> {
    match node.properties.get(key) {
        Some(GraphValue::Text(value)) => Some(value),
        _ => None,
    }
}

pub(crate) fn integer(node: &GraphNode, key: &str) -> Option<i64> {
    match node.properties.get(key) {
        Some(GraphValue::Integer(value)) => Some(*value),
        _ => None,
    }
}

pub(crate) fn number(node: &GraphNode, key: &str) -> Option<f64> {
    match node.properties.get(key) {
        Some(GraphValue::Number(value)) => Some(*value),
        Some(GraphValue::Integer(value)) => Some(*value as f64),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(key: &str) -> GraphNode {
        GraphNode {
            provider: ProviderId("test"),
            key: NodeKey(key.to_string()),
            properties: BTreeMap::new(),
        }
    }

    #[test]
    fn graph_and_forest_capabilities_are_distinct_and_error_gated() {
        let limits = DiagramLimits::default();
        let graph = GraphValidator::validate_graph(
            ProviderResult {
                graph: Some(UnvalidatedGraph {
                    provider: ProviderId("test"),
                    nodes: vec![node("a")],
                    edges: Vec::new(),
                }),
                diagnostics: Vec::new(),
            },
            ProviderId("test"),
            &limits,
        );
        assert!(graph.into_parts().0.is_some());

        let forest = GraphValidator::validate_forest(
            ProviderResult {
                graph: Some(UnvalidatedGraph {
                    provider: ProviderId("test"),
                    nodes: vec![node("a")],
                    edges: vec![GraphEdge {
                        provider: ProviderId("test"),
                        from: NodeKey("a".to_string()),
                        to: NodeKey("a".to_string()),
                    }],
                }),
                diagnostics: vec![Diagnostic::error("PROVIDER", "provider failed")],
            },
            ProviderId("test"),
            &limits,
        );
        let (capability, diagnostics) = forest.into_parts();
        assert!(capability.is_none());
        assert!(diagnostics.iter().any(|item| item.code == "PROVIDER"));
        assert!(diagnostics.iter().any(|item| item.code == "DIA-FOREST-001"));
    }

    #[test]
    fn graph_property_and_forest_depth_limits_are_error_gated() {
        let limits = DiagramLimits {
            maximum_properties_per_node: 0,
            maximum_graph_value_bytes: 1,
            maximum_nesting: 0,
            ..DiagramLimits::default()
        };
        let mut properties = BTreeMap::new();
        properties.insert("title", GraphValue::Text("long".to_string()));
        let result = GraphValidator::validate_forest(
            ProviderResult {
                graph: Some(UnvalidatedGraph {
                    provider: ProviderId("test"),
                    nodes: vec![
                        GraphNode {
                            provider: ProviderId("test"),
                            key: NodeKey("a".to_string()),
                            properties,
                        },
                        node("b"),
                    ],
                    edges: vec![GraphEdge {
                        provider: ProviderId("test"),
                        from: NodeKey("a".to_string()),
                        to: NodeKey("b".to_string()),
                    }],
                }),
                diagnostics: Vec::new(),
            },
            ProviderId("test"),
            &limits,
        );
        let (capability, diagnostics) = result.into_parts();
        assert!(capability.is_none());
        assert!(diagnostics.iter().any(|item| item.code == "DIA-GRAPH-010"));
        assert!(diagnostics.iter().any(|item| item.code == "DIA-GRAPH-011"));
        assert!(diagnostics.iter().any(|item| item.code == "DIA-FOREST-004"));
    }

    #[test]
    fn deep_valid_forest_is_validated_with_one_iterative_hierarchy_pass() {
        const NODE_COUNT: usize = 20_000;
        let provider = ProviderId("test");
        let nodes = (0..NODE_COUNT)
            .map(|index| node(&index.to_string()))
            .collect::<Vec<_>>();
        let edges = (1..NODE_COUNT)
            .map(|index| GraphEdge {
                provider,
                from: NodeKey((index - 1).to_string()),
                to: NodeKey(index.to_string()),
            })
            .collect();
        let limits = DiagramLimits {
            maximum_nodes: NODE_COUNT,
            maximum_edges: NODE_COUNT,
            maximum_nesting: NODE_COUNT,
            ..DiagramLimits::default()
        };

        let result = GraphValidator::validate_forest(
            ProviderResult {
                graph: Some(UnvalidatedGraph {
                    provider,
                    nodes,
                    edges,
                }),
                diagnostics: Vec::new(),
            },
            provider,
            &limits,
        );
        let (capability, diagnostics) = result.into_parts();
        let capability = capability.expect("deep valid chain should produce a forest");
        assert!(diagnostics.is_empty());
        let mut hierarchy_diagnostics = DiagnosticBag::new(10);
        let inspection = inspect_hierarchy(
            &capability.graph.nodes,
            &capability.parents,
            NODE_COUNT,
            &mut hierarchy_diagnostics,
        );
        assert_eq!(inspection.visited_nodes, NODE_COUNT);
        assert!(!inspection.nesting_exceeded);
        assert!(hierarchy_diagnostics.into_vec().is_empty());
    }
}

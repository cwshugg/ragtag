//! Crate-private typed provider/policy/capability pipelines.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use clap::ArgMatches;

use crate::error::RagtagError;

use super::diagnostics::{render, Diagnostic, Severity};
use super::document::{Direction, Document};
use super::graph::{GraphValidator, ProviderId, ProviderResult, ValidatedForest, ValidatedGraph};
use super::source::SourceIndex;
use super::DiagramLimits;

#[allow(dead_code)]
pub(crate) trait GraphProvider: Send + Sync {
    fn provider_id(&self) -> ProviderId;
    fn provide(&self, source: &SourceIndex, limits: &DiagramLimits) -> ProviderResult;
}

pub(crate) trait ForestProvider: Send + Sync {
    fn provider_id(&self) -> ProviderId;
    fn provide(&self, source: &SourceIndex, limits: &DiagramLimits) -> ProviderResult;
}

#[allow(dead_code)]
pub(crate) trait GraphSelectionPolicy: Send + Sync {
    type Selection;
    fn select(
        &self,
        graph: &ValidatedGraph,
        matches: &ArgMatches,
    ) -> Result<Self::Selection, Vec<Diagnostic>>;
}

pub(crate) trait ForestSelectionPolicy: Send + Sync {
    type Selection;
    fn select(
        &self,
        forest: &ValidatedForest,
        matches: &ArgMatches,
    ) -> Result<Self::Selection, Vec<Diagnostic>>;
}

#[allow(dead_code)]
pub(crate) trait GraphDiagramKind<S>: Send + Sync {
    fn kind_id(&self) -> &'static str;
    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }
    fn project(
        &self,
        graph: &ValidatedGraph,
        selection: &S,
        direction: Direction,
        limits: &DiagramLimits,
    ) -> Result<Document, Vec<Diagnostic>>;
}

pub(crate) trait ForestDiagramKind<S>: Send + Sync {
    fn kind_id(&self) -> &'static str;
    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }
    fn project(
        &self,
        forest: &ValidatedForest,
        selection: &S,
        direction: Direction,
        limits: &DiagramLimits,
    ) -> Result<Document, Vec<Diagnostic>>;
}

pub(crate) trait GraphExecutable: Send + Sync {
    fn execute(
        &self,
        source: &SourceIndex,
        matches: &ArgMatches,
        direction: Direction,
        limits: &DiagramLimits,
        stderr: &mut dyn Write,
    ) -> Result<Document, RagtagError>;
}

pub(crate) trait ForestExecutable: Send + Sync {
    fn execute(
        &self,
        source: &SourceIndex,
        matches: &ArgMatches,
        direction: Direction,
        limits: &DiagramLimits,
        stderr: &mut dyn Write,
    ) -> Result<Document, RagtagError>;
}

#[allow(dead_code)]
struct GraphPipeline<P, S, K> {
    provider: P,
    policy: S,
    kind: K,
}

impl<P, S, K> GraphExecutable for GraphPipeline<P, S, K>
where
    P: GraphProvider,
    S: GraphSelectionPolicy,
    K: GraphDiagramKind<S::Selection>,
{
    fn execute(
        &self,
        source: &SourceIndex,
        matches: &ArgMatches,
        direction: Direction,
        limits: &DiagramLimits,
        stderr: &mut dyn Write,
    ) -> Result<Document, RagtagError> {
        let validation = GraphValidator::validate_graph(
            self.provider.provide(source, limits),
            self.provider.provider_id(),
            limits,
        );
        let graph = gate_validation(validation.into_parts(), stderr)?;
        let selection = gate_stage(self.policy.select(&graph, matches), stderr, "selection")?;
        gate_stage(
            self.kind.project(&graph, &selection, direction, limits),
            stderr,
            "projection",
        )
    }
}

struct ForestPipeline<P, S, K> {
    provider: P,
    policy: S,
    kind: K,
}

impl<P, S, K> ForestExecutable for ForestPipeline<P, S, K>
where
    P: ForestProvider,
    S: ForestSelectionPolicy,
    K: ForestDiagramKind<S::Selection>,
{
    fn execute(
        &self,
        source: &SourceIndex,
        matches: &ArgMatches,
        direction: Direction,
        limits: &DiagramLimits,
        stderr: &mut dyn Write,
    ) -> Result<Document, RagtagError> {
        let validation = GraphValidator::validate_forest(
            self.provider.provide(source, limits),
            self.provider.provider_id(),
            limits,
        );
        let forest = gate_validation(validation.into_parts(), stderr)?;
        let selection = gate_stage(self.policy.select(&forest, matches), stderr, "selection")?;
        gate_stage(
            self.kind.project(&forest, &selection, direction, limits),
            stderr,
            "projection",
        )
    }
}

pub(crate) enum ExecutableKind {
    #[allow(dead_code)]
    Graph(Box<dyn GraphExecutable>),
    Forest(Box<dyn ForestExecutable>),
}

impl ExecutableKind {
    pub(crate) fn execute(
        &self,
        source: &SourceIndex,
        matches: &ArgMatches,
        direction: Direction,
        limits: &DiagramLimits,
        stderr: &mut dyn Write,
    ) -> Result<Document, RagtagError> {
        match self {
            Self::Graph(pipeline) => pipeline.execute(source, matches, direction, limits, stderr),
            Self::Forest(pipeline) => pipeline.execute(source, matches, direction, limits, stderr),
        }
    }
}

pub(crate) struct Catalog {
    registrations: BTreeMap<&'static str, ExecutableKind>,
    aliases: BTreeMap<&'static str, &'static str>,
}

impl Catalog {
    pub(crate) fn get(&self, name: &str) -> Option<&ExecutableKind> {
        let canonical = self.aliases.get(name).copied().unwrap_or(name);
        self.registrations.get(canonical)
    }
}

pub(crate) struct CatalogBuilder {
    registrations: BTreeMap<&'static str, ExecutableKind>,
    aliases: BTreeMap<&'static str, &'static str>,
    providers: BTreeSet<ProviderId>,
}

impl CatalogBuilder {
    pub(crate) fn new() -> Self {
        Self {
            registrations: BTreeMap::new(),
            aliases: BTreeMap::new(),
            providers: BTreeSet::new(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn register_graph<P, S, K>(
        mut self,
        provider: P,
        policy: S,
        kind: K,
    ) -> Result<Self, RagtagError>
    where
        P: GraphProvider + 'static,
        S: GraphSelectionPolicy + 'static,
        K: GraphDiagramKind<S::Selection> + 'static,
    {
        let kind_id = kind.kind_id();
        let aliases = kind.aliases();
        self.reserve(provider.provider_id(), kind_id, aliases)?;
        self.registrations.insert(
            kind_id,
            ExecutableKind::Graph(Box::new(GraphPipeline {
                provider,
                policy,
                kind,
            })),
        );
        Ok(self)
    }

    pub(crate) fn register_forest<P, S, K>(
        mut self,
        provider: P,
        policy: S,
        kind: K,
    ) -> Result<Self, RagtagError>
    where
        P: ForestProvider + 'static,
        S: ForestSelectionPolicy + 'static,
        K: ForestDiagramKind<S::Selection> + 'static,
    {
        let kind_id = kind.kind_id();
        let aliases = kind.aliases();
        self.reserve(provider.provider_id(), kind_id, aliases)?;
        self.registrations.insert(
            kind_id,
            ExecutableKind::Forest(Box::new(ForestPipeline {
                provider,
                policy,
                kind,
            })),
        );
        Ok(self)
    }

    fn reserve(
        &mut self,
        provider: ProviderId,
        kind: &'static str,
        aliases: &'static [&'static str],
    ) -> Result<(), RagtagError> {
        if !self.providers.insert(provider) {
            return Err(RagtagError::InvalidConfig(format!(
                "duplicate diagram provider \"{}\"",
                provider.0
            )));
        }
        if self.registrations.contains_key(kind) || self.aliases.contains_key(kind) {
            return Err(RagtagError::InvalidConfig(format!(
                "duplicate diagram kind \"{kind}\""
            )));
        }
        for alias in aliases {
            if *alias == kind
                || self.registrations.contains_key(alias)
                || self.aliases.contains_key(alias)
            {
                return Err(RagtagError::InvalidConfig(format!(
                    "duplicate diagram alias \"{alias}\""
                )));
            }
            self.aliases.insert(alias, kind);
        }
        Ok(())
    }

    pub(crate) fn build(self) -> Catalog {
        Catalog {
            registrations: self.registrations,
            aliases: self.aliases,
        }
    }
}

fn gate_validation<T>(
    (capability, diagnostics): (Option<T>, Vec<Diagnostic>),
    stderr: &mut dyn Write,
) -> Result<T, RagtagError> {
    render(&diagnostics, stderr)?;
    capability.ok_or_else(|| RagtagError::Diagram("graph validation failed".to_string()))
}

fn gate_stage<T>(
    result: Result<T, Vec<Diagnostic>>,
    stderr: &mut dyn Write,
    stage: &'static str,
) -> Result<T, RagtagError> {
    match result {
        Ok(value) => Ok(value),
        Err(diagnostics) => {
            render(&diagnostics, stderr)?;
            let has_error = diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == Severity::Error);
            if has_error {
                Err(RagtagError::Diagram(format!("{stage} failed")))
            } else {
                Err(RagtagError::Diagram(format!("{stage} returned no value")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use super::*;
    use crate::diagram::graph::{GraphEdge, GraphNode, NodeKey, UnvalidatedGraph};

    struct InventoryProvider(ProviderId);

    impl GraphProvider for InventoryProvider {
        fn provider_id(&self) -> ProviderId {
            self.0
        }

        fn provide(&self, _source: &SourceIndex, _limits: &DiagramLimits) -> ProviderResult {
            ProviderResult {
                graph: Some(UnvalidatedGraph {
                    provider: self.0,
                    nodes: vec![GraphNode {
                        provider: self.0,
                        key: NodeKey("file".to_string()),
                        properties: BTreeMap::new(),
                    }],
                    edges: Vec::new(),
                }),
                diagnostics: Vec::new(),
            }
        }
    }

    struct SelectAll;

    impl GraphSelectionPolicy for SelectAll {
        type Selection = usize;

        fn select(
            &self,
            graph: &ValidatedGraph,
            _matches: &ArgMatches,
        ) -> Result<Self::Selection, Vec<Diagnostic>> {
            Ok(graph.nodes().count())
        }
    }

    struct InventoryKind;

    impl GraphDiagramKind<usize> for InventoryKind {
        fn kind_id(&self) -> &'static str {
            "inventory"
        }

        fn project(
            &self,
            _graph: &ValidatedGraph,
            selected: &usize,
            direction: Direction,
            _limits: &DiagramLimits,
        ) -> Result<Document, Vec<Diagnostic>> {
            assert_eq!(*selected, 1);
            Ok(Document {
                direction,
                elements: Vec::new(),
                edges: Vec::new(),
            })
        }
    }

    struct ForestInventoryProvider;

    impl ForestProvider for ForestInventoryProvider {
        fn provider_id(&self) -> ProviderId {
            ProviderId("forest-inventory-provider")
        }

        fn provide(&self, source: &SourceIndex, limits: &DiagramLimits) -> ProviderResult {
            InventoryProvider(self.provider_id()).provide(source, limits)
        }
    }

    struct SelectForest;

    impl ForestSelectionPolicy for SelectForest {
        type Selection = usize;

        fn select(
            &self,
            forest: &ValidatedForest,
            _matches: &ArgMatches,
        ) -> Result<Self::Selection, Vec<Diagnostic>> {
            Ok(forest.nodes().count())
        }
    }

    struct ForestInventoryKind;

    impl ForestDiagramKind<usize> for ForestInventoryKind {
        fn kind_id(&self) -> &'static str {
            "forest-inventory"
        }

        fn project(
            &self,
            _forest: &ValidatedForest,
            selected: &usize,
            direction: Direction,
            _limits: &DiagramLimits,
        ) -> Result<Document, Vec<Diagnostic>> {
            assert_eq!(*selected, 1);
            Ok(Document {
                direction,
                elements: Vec::new(),
                edges: Vec::new(),
            })
        }
    }

    #[test]
    fn non_task_graph_executes_through_registered_typed_pipeline() {
        let catalog = CatalogBuilder::new()
            .register_graph(
                InventoryProvider(ProviderId("inventory-provider")),
                SelectAll,
                InventoryKind,
            )
            .unwrap()
            .build();
        let matches = clap::Command::new("test").get_matches_from(["test"]);
        let mut diagnostics = Vec::new();
        let document = catalog
            .get("inventory")
            .unwrap()
            .execute(
                &SourceIndex::empty(),
                &matches,
                Direction::Down,
                &DiagramLimits::default(),
                &mut diagnostics,
            )
            .unwrap();
        assert!(document.elements.is_empty());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn duplicate_provider_registration_is_rejected() {
        let builder = CatalogBuilder::new()
            .register_graph(
                InventoryProvider(ProviderId("same")),
                SelectAll,
                InventoryKind,
            )
            .unwrap();
        assert!(builder
            .register_graph(
                InventoryProvider(ProviderId("same")),
                SelectAll,
                InventoryKind,
            )
            .is_err());
    }

    #[test]
    fn forest_registration_releases_only_a_forest_capability() {
        let catalog = CatalogBuilder::new()
            .register_forest(ForestInventoryProvider, SelectForest, ForestInventoryKind)
            .unwrap()
            .build();
        let matches = clap::Command::new("test").get_matches_from(["test"]);
        let mut diagnostics = Vec::new();
        assert!(catalog
            .get("forest-inventory")
            .unwrap()
            .execute(
                &SourceIndex::empty(),
                &matches,
                Direction::Down,
                &DiagramLimits::default(),
                &mut diagnostics,
            )
            .is_ok());
    }

    #[test]
    fn canonical_kind_collision_is_rejected() {
        assert!(CatalogBuilder::new()
            .register_graph(
                InventoryProvider(ProviderId("one")),
                SelectAll,
                InventoryKind,
            )
            .unwrap()
            .register_graph(
                InventoryProvider(ProviderId("two")),
                SelectAll,
                InventoryKind,
            )
            .is_err());
    }

    struct InvalidForestProvider;

    impl ForestProvider for InvalidForestProvider {
        fn provider_id(&self) -> ProviderId {
            ProviderId("invalid-forest")
        }

        fn provide(&self, _source: &SourceIndex, _limits: &DiagramLimits) -> ProviderResult {
            let provider = self.provider_id();
            ProviderResult {
                graph: Some(UnvalidatedGraph {
                    provider,
                    nodes: vec![node_for(provider, "a"), node_for(provider, "b")],
                    edges: vec![
                        edge_for(provider, "a", "a"),
                        edge_for(provider, "b", "a"),
                        edge_for(provider, "a", "b"),
                    ],
                }),
                diagnostics: vec![Diagnostic::error(
                    "PROVIDER-FAIL",
                    "provider found an invalid record",
                )],
            }
        }
    }

    struct CountingForestSelection(Arc<AtomicUsize>);

    impl ForestSelectionPolicy for CountingForestSelection {
        type Selection = ();

        fn select(
            &self,
            _forest: &ValidatedForest,
            _matches: &ArgMatches,
        ) -> Result<Self::Selection, Vec<Diagnostic>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct CountingForestKind(Arc<AtomicUsize>);

    impl ForestDiagramKind<()> for CountingForestKind {
        fn kind_id(&self) -> &'static str {
            "invalid-forest-kind"
        }

        fn project(
            &self,
            _forest: &ValidatedForest,
            _selection: &(),
            direction: Direction,
            _limits: &DiagramLimits,
        ) -> Result<Document, Vec<Diagnostic>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(Document {
                direction,
                elements: Vec::new(),
                edges: Vec::new(),
            })
        }
    }

    fn node_for(provider: ProviderId, key: &str) -> GraphNode {
        GraphNode {
            provider,
            key: NodeKey(key.to_string()),
            properties: BTreeMap::new(),
        }
    }

    fn edge_for(provider: ProviderId, from: &str, to: &str) -> GraphEdge {
        GraphEdge {
            provider,
            from: NodeKey(from.to_string()),
            to: NodeKey(to.to_string()),
        }
    }

    #[test]
    fn gate_b_accumulates_provider_and_hierarchy_errors_before_stopping_pipeline() {
        let selection_calls = Arc::new(AtomicUsize::new(0));
        let projection_calls = Arc::new(AtomicUsize::new(0));
        let sink_calls = AtomicUsize::new(0);
        let catalog = CatalogBuilder::new()
            .register_forest(
                InvalidForestProvider,
                CountingForestSelection(Arc::clone(&selection_calls)),
                CountingForestKind(Arc::clone(&projection_calls)),
            )
            .unwrap()
            .build();
        let matches = clap::Command::new("test").get_matches_from(["test"]);
        let mut stderr = Vec::new();
        let result = catalog.get("invalid-forest-kind").unwrap().execute(
            &SourceIndex::empty(),
            &matches,
            Direction::Down,
            &DiagramLimits::default(),
            &mut stderr,
        );
        if result.is_ok() {
            sink_calls.fetch_add(1, Ordering::SeqCst);
        }

        assert!(result.is_err());
        let rendered = String::from_utf8(stderr).unwrap();
        for code in [
            "PROVIDER-FAIL",
            "DIA-FOREST-001",
            "DIA-FOREST-002",
            "DIA-FOREST-003",
        ] {
            assert!(rendered.contains(code), "missing {code}: {rendered}");
        }
        assert_eq!(selection_calls.load(Ordering::SeqCst), 0);
        assert_eq!(projection_calls.load(Ordering::SeqCst), 0);
        assert_eq!(sink_calls.load(Ordering::SeqCst), 0);
    }
}

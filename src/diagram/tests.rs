//! Internal extensibility proof for a non-task provider.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use clap::ArgMatches;

use super::backend;
use super::catalog::{CatalogBuilder, GraphDiagramKind, GraphProvider, GraphSelectionPolicy};
use super::diagnostics::Diagnostic;
use super::document::{
    validate_document, Direction, Document, Element, ElementRole, Label, StatusRole,
};
use super::graph::{
    text, GraphNode, GraphValue, NodeKey, ProviderId, ProviderResult, UnvalidatedGraph,
    ValidatedGraph,
};
use super::source::{build_source_index, FilePayload, SourceIndex};
use super::DiagramLimits;
use crate::discovery::{BoundedFileWalker, BoundedWalk};

struct FixedWalker(Vec<PathBuf>);

impl BoundedFileWalker for FixedWalker {
    fn walk_bounded_complete(
        &self,
        _path: &Path,
        _maximum_files: usize,
        _maximum_path_bytes: usize,
        _maximum_path_length: usize,
    ) -> Result<BoundedWalk, crate::error::RagtagError> {
        Ok(BoundedWalk::Complete(self.0.clone()))
    }
}

struct FileInventoryProvider {
    calls: Arc<AtomicUsize>,
}

impl GraphProvider for FileInventoryProvider {
    fn provider_id(&self) -> ProviderId {
        ProviderId("file-inventory-provider")
    }

    fn provide(&self, source: &SourceIndex, _limits: &DiagramLimits) -> ProviderResult {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let tagged = source
            .named("task")
            .map(|occurrence| occurrence.file)
            .collect::<BTreeSet<_>>();
        let nodes = source
            .files()
            .iter()
            .map(|file| {
                let path = source
                    .normalized_path(file.id)
                    .to_string_lossy()
                    .into_owned();
                let state = match &file.payload {
                    FilePayload::Unavailable(_) => "unavailable",
                    FilePayload::Text(_) if tagged.contains(&file.id) => "tagged",
                    FilePayload::Text(_) => "untagged",
                };
                let mut properties = BTreeMap::new();
                properties.insert("path", GraphValue::Text(path.clone()));
                properties.insert("state", GraphValue::Text(state.to_string()));
                GraphNode {
                    provider: self.provider_id(),
                    key: NodeKey(path),
                    properties,
                }
            })
            .collect();
        ProviderResult {
            graph: Some(UnvalidatedGraph {
                provider: self.provider_id(),
                nodes,
                edges: Vec::new(),
            }),
            diagnostics: Vec::new(),
        }
    }
}

struct FileInventorySelection {
    calls: Arc<AtomicUsize>,
}

impl GraphSelectionPolicy for FileInventorySelection {
    type Selection = Vec<NodeKey>;

    fn select(
        &self,
        graph: &ValidatedGraph,
        _matches: &ArgMatches,
    ) -> Result<Self::Selection, Vec<Diagnostic>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(graph.nodes().map(|node| node.key.clone()).collect())
    }
}

struct FileInventoryKind {
    calls: Arc<AtomicUsize>,
}

impl GraphDiagramKind<Vec<NodeKey>> for FileInventoryKind {
    fn kind_id(&self) -> &'static str {
        "file-inventory"
    }

    fn project(
        &self,
        graph: &ValidatedGraph,
        selection: &Vec<NodeKey>,
        direction: Direction,
        _limits: &DiagramLimits,
    ) -> Result<Document, Vec<Diagnostic>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let elements = selection
            .iter()
            .map(|key| {
                let node = graph.node(key).expect("selection contains validated keys");
                Element {
                    id: key.0.clone(),
                    parent: None,
                    label: Label {
                        first: text(node, "path").unwrap().to_string(),
                        second: text(node, "state").unwrap().to_string(),
                    },
                    role: ElementRole::Task,
                    status: StatusRole::Inactive,
                    context: false,
                    priority: None,
                }
            })
            .collect();
        validate_document(Document {
            direction,
            elements,
            edges: Vec::new(),
        })
    }
}

#[test]
fn file_inventory_provider_executes_every_stage_through_catalog() {
    let directory = tempfile::tempdir().unwrap();
    let tagged = directory.path().join("tagged.md");
    let untagged = directory.path().join("untagged.md");
    let unavailable = directory.path().join("unavailable.bin");
    std::fs::write(&tagged, "@task(id=one, title=One, status=active)\n").unwrap();
    std::fs::write(&untagged, "plain text\n").unwrap();
    std::fs::write(&unavailable, [0xff, 0xfe, 0x80]).unwrap();
    let (source, diagnostics) = build_source_index(
        &FixedWalker(vec![tagged, untagged, unavailable]),
        directory.path(),
        &DiagramLimits::default(),
    );
    assert!(diagnostics.is_empty());

    let provider_calls = Arc::new(AtomicUsize::new(0));
    let selection_calls = Arc::new(AtomicUsize::new(0));
    let projection_calls = Arc::new(AtomicUsize::new(0));
    let catalog = CatalogBuilder::new()
        .register_graph(
            FileInventoryProvider {
                calls: Arc::clone(&provider_calls),
            },
            FileInventorySelection {
                calls: Arc::clone(&selection_calls),
            },
            FileInventoryKind {
                calls: Arc::clone(&projection_calls),
            },
        )
        .unwrap()
        .build();
    let matches = clap::Command::new("test").get_matches_from(["test"]);
    let mut stderr = Vec::new();
    let document = catalog
        .get("file-inventory")
        .unwrap()
        .execute(
            source.as_ref().unwrap(),
            &matches,
            Direction::Down,
            &DiagramLimits::default(),
            &mut stderr,
        )
        .unwrap();
    let output = String::from_utf8(backend::serialize(&document, 16 * 1024).unwrap()).unwrap();

    assert!(stderr.is_empty());
    assert!(output.contains("tagged.md\\ntagged"));
    assert!(output.contains("untagged.md\\nuntagged"));
    assert!(output.contains("unavailable.bin\\nunavailable"));
    assert_eq!(provider_calls.load(Ordering::SeqCst), 1);
    assert_eq!(selection_calls.load(Ordering::SeqCst), 1);
    assert_eq!(projection_calls.load(Ordering::SeqCst), 1);
}

//! Source-only diagram generation pipeline.

pub(crate) mod backend;
pub(crate) mod catalog;
pub(crate) mod cli;
pub(crate) mod command;
pub(crate) mod diagnostics;
pub(crate) mod document;
pub(crate) mod graph;
pub(crate) mod sink;
pub(crate) mod source;
pub(crate) mod task;

#[cfg(test)]
mod tests;

/// Injectable resource limits for deterministic pre-allocation checks.
#[derive(Debug, Clone)]
pub(crate) struct DiagramLimits {
    pub(crate) maximum_files: usize,
    pub(crate) maximum_path_bytes: usize,
    pub(crate) maximum_path_length: usize,
    pub(crate) maximum_file_bytes: usize,
    pub(crate) maximum_source_bytes: usize,
    pub(crate) maximum_occurrences: usize,
    pub(crate) maximum_diagnostics: usize,
    pub(crate) maximum_nodes: usize,
    pub(crate) maximum_edges: usize,
    pub(crate) maximum_properties_per_node: usize,
    pub(crate) maximum_properties: usize,
    pub(crate) maximum_graph_value_bytes: usize,
    pub(crate) maximum_graph_property_bytes: usize,
    pub(crate) maximum_document_text: usize,
    pub(crate) maximum_document_field_bytes: usize,
    pub(crate) maximum_label_bytes: usize,
    pub(crate) maximum_nesting: usize,
    pub(crate) maximum_serialized_bytes: usize,
}

impl Default for DiagramLimits {
    fn default() -> Self {
        Self {
            maximum_files: 100_000,
            maximum_path_bytes: 64 * 1024 * 1024,
            maximum_path_length: 32 * 1024,
            maximum_file_bytes: 10 * 1024 * 1024,
            maximum_source_bytes: 256 * 1024 * 1024,
            maximum_occurrences: 1_000_000,
            maximum_diagnostics: 4_096,
            maximum_nodes: 100_000,
            maximum_edges: 100_000,
            maximum_properties_per_node: 32,
            maximum_properties: 1_600_000,
            maximum_graph_value_bytes: 64 * 1024,
            maximum_graph_property_bytes: 64 * 1024 * 1024,
            maximum_document_text: 64 * 1024 * 1024,
            maximum_document_field_bytes: 64 * 1024,
            maximum_label_bytes: 128 * 1024,
            maximum_nesting: 1_024,
            maximum_serialized_bytes: 256 * 1024 * 1024,
        }
    }
}

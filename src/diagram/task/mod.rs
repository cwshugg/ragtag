//! Built-in task diagram registrations.

pub(crate) mod provider;
pub(crate) mod selection;

use crate::diagram::catalog::ForestDiagramKind;
use crate::diagram::diagnostics::Diagnostic;
use crate::diagram::document::{self, Document};
use crate::diagram::graph::ValidatedForest;
use crate::diagram::DiagramLimits;

use self::selection::TaskSelection;

/// Flat task-tree projector.
pub(crate) struct TaskTree {
    limits: DiagramLimits,
}

impl TaskTree {
    pub(crate) fn new(limits: DiagramLimits) -> Self {
        Self { limits }
    }
}

impl ForestDiagramKind<TaskSelection> for TaskTree {
    fn kind_id(&self) -> &'static str {
        "task-tree"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }

    fn project(
        &self,
        forest: &ValidatedForest,
        selection: &TaskSelection,
        direction: document::Direction,
        limits: &DiagramLimits,
    ) -> Result<Document, Vec<Diagnostic>> {
        let _ = limits;
        document::project_tree(forest, selection, direction, &self.limits)
    }
}

/// Recursive task-bucket projector.
pub(crate) struct TaskBuckets {
    limits: DiagramLimits,
}

impl TaskBuckets {
    pub(crate) fn new(limits: DiagramLimits) -> Self {
        Self { limits }
    }
}

impl ForestDiagramKind<TaskSelection> for TaskBuckets {
    fn kind_id(&self) -> &'static str {
        "task-buckets"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }

    fn project(
        &self,
        forest: &ValidatedForest,
        selection: &TaskSelection,
        direction: document::Direction,
        limits: &DiagramLimits,
    ) -> Result<Document, Vec<Diagnostic>> {
        let _ = limits;
        document::project_buckets(forest, selection, direction, &self.limits)
    }
}

#[cfg(test)]
mod tests {
    use super::provider::TaskGraphProvider;
    use super::selection::TaskSelectionPolicy;
    use crate::diagram::graph::ProviderId;
    use crate::extensions::task::semantics::TaskSemantics;
    use crate::extensions::task::TaskExtension;
    use std::sync::Arc;

    #[test]
    fn extension_provider_and_selection_share_authoritative_semantics() {
        let (semantics, warnings) = TaskSemantics::resolve(None).unwrap();
        assert!(warnings.is_empty());
        let semantics = Arc::new(semantics);
        let provider = TaskGraphProvider::new(ProviderId("task-test"), Arc::clone(&semantics));
        let selection = TaskSelectionPolicy::new(Arc::clone(&semantics));
        let extension = TaskExtension::new(Arc::clone(&semantics));
        assert!(Arc::ptr_eq(provider.semantics(), selection.semantics()));
        assert!(Arc::ptr_eq(provider.semantics(), extension.semantics()));
        assert!(Arc::ptr_eq(provider.semantics(), &semantics));
    }
}

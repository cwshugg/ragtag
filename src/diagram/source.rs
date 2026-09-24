//! Bounded immutable source corpus for diagram providers.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::discovery::{BoundedFileWalker, BoundedWalk, DiscoveryLimitKind};
use crate::models::Tag;
use crate::parser::scan_file;

use super::diagnostics::{Diagnostic, DiagnosticBag};
use super::DiagramLimits;

/// Stable index into `SourceIndex::files`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct FileId(pub(crate) usize);

/// Successfully retained source or an explicit unavailable state.
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum FilePayload {
    Text(String),
    Unavailable(UnavailableReason),
}

/// Why source text could not be retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnavailableReason {
    RejectedByLimit,
    ReadFailed,
    InvalidUtf8,
}

/// One deterministic source record.
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct FileRecord {
    pub(crate) id: FileId,
    pub(crate) path: PathBuf,
    pub(crate) payload: FilePayload,
}

/// One parsed tag occurrence tied to its source.
#[derive(Debug, Clone)]
pub(crate) struct TagOccurrence {
    #[allow(dead_code)]
    pub(crate) file: FileId,
    pub(crate) tag: Tag,
}

/// Complete immutable source corpus consumed by every provider.
#[derive(Debug)]
pub(crate) struct SourceIndex {
    root: PathBuf,
    #[allow(dead_code)]
    files: Vec<FileRecord>,
    occurrences: Vec<TagOccurrence>,
}

impl SourceIndex {
    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        Self {
            root: PathBuf::new(),
            files: Vec::new(),
            occurrences: Vec::new(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn files(&self) -> &[FileRecord] {
        &self.files
    }

    pub(crate) fn normalized_path(&self, file: FileId) -> &Path {
        let path = &self.files[file.0].path;
        path.strip_prefix(&self.root).unwrap_or(path)
    }

    pub(crate) fn named<'a>(
        &'a self,
        name: &'a str,
    ) -> impl Iterator<Item = &'a TagOccurrence> + 'a {
        self.occurrences
            .iter()
            .filter(move |occurrence| occurrence.tag.name == name)
    }
}

/// Scans a complete path corpus under diagram-specific limits.
pub(crate) fn build_source_index(
    walker: &dyn BoundedFileWalker,
    path: &Path,
    limits: &DiagramLimits,
) -> (Option<SourceIndex>, Vec<Diagnostic>) {
    let mut diagnostics = DiagnosticBag::new(limits.maximum_diagnostics);
    let paths = match walker.walk_bounded_complete(
        path,
        limits.maximum_files,
        limits.maximum_path_bytes,
        limits.maximum_path_length,
    ) {
        Ok(BoundedWalk::Complete(paths)) => paths,
        Ok(BoundedWalk::LimitExceeded { kind }) => {
            let description = match kind {
                DiscoveryLimitKind::FileCount => "file count",
                DiscoveryLimitKind::PathBytes => "aggregate path bytes",
                DiscoveryLimitKind::PathLength => "path length",
            };
            diagnostics.push(
                Diagnostic::error("DIA-SCAN-001", "source discovery limit exceeded")
                    .parameter(description),
            );
            return (None, diagnostics.into_vec());
        }
        Err(error) => {
            diagnostics.push(
                Diagnostic::error("DIA-SCAN-002", "source discovery failed")
                    .parameter(error.to_string()),
            );
            return (None, diagnostics.into_vec());
        }
    };

    let mut files = Vec::with_capacity(paths.len());
    let mut occurrences = Vec::new();
    let mut aggregate_source_bytes = 0usize;
    for (index, path) in paths.into_iter().enumerate() {
        let id = FileId(index);
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => {
                diagnostics.push(
                    Diagnostic::error("DIA-SCAN-003", "source file metadata is unavailable")
                        .parameter(path.display().to_string()),
                );
                files.push(FileRecord {
                    id,
                    path,
                    payload: FilePayload::Unavailable(UnavailableReason::ReadFailed),
                });
                continue;
            }
        };
        if metadata.len() > limits.maximum_file_bytes as u64 {
            diagnostics.push(
                Diagnostic::error("DIA-SCAN-004", "source file exceeds the configured limit")
                    .parameter(path.display().to_string()),
            );
            files.push(FileRecord {
                id,
                path,
                payload: FilePayload::Unavailable(UnavailableReason::RejectedByLimit),
            });
            continue;
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        let read = File::open(&path).and_then(|file| {
            file.take(limits.maximum_file_bytes as u64 + 1)
                .read_to_end(&mut bytes)
        });
        if read.is_err() || bytes.len() > limits.maximum_file_bytes {
            diagnostics.push(
                Diagnostic::error("DIA-SCAN-005", "source file could not be read completely")
                    .parameter(path.display().to_string()),
            );
            files.push(FileRecord {
                id,
                path,
                payload: FilePayload::Unavailable(UnavailableReason::ReadFailed),
            });
            continue;
        }
        let text = match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(_) => {
                files.push(FileRecord {
                    id,
                    path,
                    payload: FilePayload::Unavailable(UnavailableReason::InvalidUtf8),
                });
                continue;
            }
        };
        let Some(next_source_bytes) = aggregate_source_bytes.checked_add(text.len()) else {
            diagnostics.push(Diagnostic::error(
                "DIA-SCAN-006",
                "aggregate source byte limit exceeded",
            ));
            break;
        };
        if next_source_bytes > limits.maximum_source_bytes {
            diagnostics.push(Diagnostic::error(
                "DIA-SCAN-006",
                "aggregate source byte limit exceeded",
            ));
            break;
        }
        aggregate_source_bytes = next_source_bytes;
        let tags = scan_file(&text, &path);
        if occurrences.len().saturating_add(tags.len()) > limits.maximum_occurrences {
            diagnostics.push(Diagnostic::error(
                "DIA-SCAN-008",
                "tag occurrence limit exceeded",
            ));
            break;
        }
        occurrences.extend(tags.into_iter().map(|tag| TagOccurrence { file: id, tag }));
        files.push(FileRecord {
            id,
            path,
            payload: FilePayload::Text(text),
        });
    }
    if diagnostics.has_errors() {
        (None, diagnostics.into_vec())
    } else {
        (
            Some(SourceIndex {
                root: path.to_path_buf(),
                files,
                occurrences,
            }),
            diagnostics.into_vec(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn invalid_utf8_files_are_retained_as_skipped_without_diagnostics() {
        let directory = tempfile::tempdir().unwrap();
        let text = directory.path().join("tasks.md");
        let png = directory.path().join("image.png");
        let binary = directory.path().join("binary");
        std::fs::write(&text, "@task(id=valid, title=\"Valid\", status=active)\n").unwrap();
        std::fs::write(&png, [0x89, b'P', b'N', b'G', 0xff, 0x00]).unwrap();
        std::fs::write(&binary, [0x00, 0x80, 0xfe, 0xff]).unwrap();

        let walker = FixedWalker(vec![text, png, binary]);
        let (source, diagnostics) =
            build_source_index(&walker, directory.path(), &DiagramLimits::default());
        assert!(diagnostics.is_empty());
        let source = source.expect("binary inputs do not invalidate the source index");
        assert_eq!(source.occurrences.len(), 1);
        assert_eq!(
            source
                .files()
                .iter()
                .filter(|file| matches!(
                    file.payload,
                    FilePayload::Unavailable(UnavailableReason::InvalidUtf8)
                ))
                .count(),
            2
        );
    }
}

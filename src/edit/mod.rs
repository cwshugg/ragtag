//! In-place file editing module.
//!
//! Provides atomic file writes via tempfile + rename for safe
//! in-place tag attribute updates.

mod scan;
pub mod tag_format;
pub mod writer;

pub use tag_format::{edit_task_tag, regenerate_tag, upsert_unique_named_attribute, TagFormatInfo};
pub use writer::{modify_tag_attribute, write_file_atomically, AtomicFileEditor, FileEditor};
pub(crate) use writer::{
    read_file_snapshot, verify_file_snapshot, write_file_atomically_if_unchanged_with_hook,
    FileSnapshot,
};

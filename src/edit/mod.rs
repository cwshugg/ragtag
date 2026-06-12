//! In-place file editing module.
//!
//! Provides atomic file writes via tempfile + rename for safe
//! in-place tag attribute updates.

pub mod tag_format;
pub mod writer;

pub use tag_format::{edit_task_tag, regenerate_tag, TagFormatInfo};
pub use writer::{modify_tag_attribute, write_file_atomically, AtomicFileEditor, FileEditor};

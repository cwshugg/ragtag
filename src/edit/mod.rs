//! In-place file editing module.
//!
//! Provides atomic file writes via tempfile + rename for safe
//! in-place tag attribute updates.

pub mod writer;

pub use writer::{modify_tag_attribute, AtomicFileEditor, FileEditor};

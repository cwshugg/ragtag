//! In-place file editing module.
//!
//! Provides atomic file writes via tempfile + rename for safe
//! in-place tag attribute updates.

pub mod writer;

pub use writer::{AtomicFileEditor, FileEditor};

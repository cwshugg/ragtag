//! File discovery module.
//!
//! Walks directories to find files for tag scanning, respecting
//! .gitignore, hidden file settings, and user-configured ignore patterns.

pub mod walker;

pub use walker::{walk_path, FileWalker, IgnoreWalker};

//! Core data models for ragtag.
//!
//! This module defines the foundational types produced by the parser:
//! `Tag`, `TagAttribute`, `AttributeValue`, `NumericBase`, and `TagLocation`.

pub mod location;
pub mod tag;

pub use location::TagLocation;
pub use tag::{AttributeKind, AttributeValue, NumericBase, Tag, TagAttribute};

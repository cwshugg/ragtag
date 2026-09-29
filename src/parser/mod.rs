//! Hand-rolled tag parser.
//!
//! This module contains a custom parser built entirely with Rust code —
//! no third-party parser libraries are used. It operates on `&str` input
//! using a cursor/scanner approach.

pub mod cursor;
pub mod scanner;
pub mod strict;
pub mod tag;
pub mod value;

pub use scanner::scan_file;
pub use strict::{
    contains_forbidden_created_text, parse_complete_attribute_value,
    parse_complete_named_attribute, parse_complete_tag, validate_complete_tag_name,
    validate_creatable_tag, validate_creatable_value, MAX_ATTRIBUTES_PER_TAG,
};

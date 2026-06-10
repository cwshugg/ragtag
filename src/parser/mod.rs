//! Hand-rolled tag parser.
//!
//! This module contains a custom parser built entirely with Rust code —
//! no third-party parser libraries are used. It operates on `&str` input
//! using a cursor/scanner approach.

pub mod cursor;
pub mod scanner;
pub mod tag;
pub mod value;

pub use scanner::scan_file;

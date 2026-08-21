//! Configuration system for ragtag.
//!
//! Handles config schema definitions, YAML deserialization, and
//! walk-up file discovery with `.git` boundary detection.

pub mod loader;
pub mod schema;

pub use loader::{discover_config_file, load_config, LoadedConfig};
pub use schema::{Alias, ColorMode, Config, FileConfig, OutputConfig};

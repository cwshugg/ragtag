//! ragtag — A CLI tool for parsing `@tag(attr=value)` from plain text files.
//!
//! This library provides the core functionality for tag discovery,
//! parsing, and management. The `@task` extension demonstrates the
//! extensibility of the system.

pub mod cli;
pub mod commands;
pub mod config;
pub mod discovery;
pub mod edit;
pub mod error;
pub mod extensions;
pub mod models;
pub mod output;
pub mod parser;

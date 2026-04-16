pub mod commands;
pub mod config;
pub mod conflicts;
pub mod datastore;
pub mod error;
pub mod execution;
pub mod fs;
pub mod handlers;
pub mod operations;
pub mod packs;
pub mod paths;
pub mod preprocessing;
pub mod render;
pub mod rules;
pub mod shell;

// The testing module is available:
// - Always during `cargo test` (dev-dependencies provide tempfile)
// - When the `test-utils` feature is enabled (for external consumers)
#[cfg(any(test, feature = "test-utils"))]
pub mod testing;

pub use error::{DodotError, Result};

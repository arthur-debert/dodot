pub mod commands;
pub mod config;
pub mod conflicts;
pub mod datastore;
pub mod equivalence;
pub mod error;
pub mod execution;
pub mod external;
pub mod fs;
pub mod gates;
pub mod handlers;
pub mod operations;
pub mod packs;
pub mod paths;
pub mod plists;
pub mod preprocessing;
pub mod probe;
pub mod prompts;
pub mod render;
pub mod rules;
pub mod safety_lock;
pub mod secret;
pub mod shell;

// `test-utils` exposes the testing module to external consumers.
#[cfg(any(test, feature = "test-utils"))]
pub mod testing;

pub use error::{DodotError, Result};

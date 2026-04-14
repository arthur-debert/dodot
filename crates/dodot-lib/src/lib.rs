pub mod error;
pub mod fs;
pub mod paths;

// The testing module is available:
// - Always during `cargo test` (dev-dependencies provide tempfile)
// - When the `test-utils` feature is enabled (for external consumers)
#[cfg(any(test, feature = "test-utils"))]
pub mod testing;

pub use error::{DodotError, Result};

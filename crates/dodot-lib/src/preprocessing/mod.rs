//! Preprocessing pipeline — transforms source files before handler dispatch.
//!
//! Preprocessors expand files whose version-controlled source differs from
//! the deployed artifact (templates, plists, encrypted secrets). The
//! preprocessing phase runs before handler dispatch, producing virtual
//! entries that downstream handlers (symlink, shell, path, install,
//! homebrew) consume transparently.
//!
//! See `docs/proposals/preprocessing-pipeline.lex` for the full design.

pub mod identity;
pub mod pipeline;
pub mod unarchive;

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::fs::Fs;
use crate::Result;

/// The safety model for a preprocessor's transformation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TransformType {
    /// Source generates destination; reversal is heuristic (templates).
    Generative,
    /// Source and destination are lossless representations (plists).
    Representational,
    /// Source is decoded on deploy; no reverse path (GPG).
    Opaque,
}

/// A single file produced by a preprocessor's expansion.
#[derive(Debug, Clone)]
pub struct ExpandedFile {
    /// Path relative to the expansion output (usually just the filename).
    pub relative_path: PathBuf,
    /// The file content.
    pub content: Vec<u8>,
    /// Whether this entry is a directory marker.
    pub is_dir: bool,
}

/// The core preprocessor abstraction.
///
/// Each preprocessor is a small struct that implements this trait.
/// Preprocessors are stored in a [`PreprocessorRegistry`] and dispatched
/// by file extension at preprocessing time.
///
/// Preprocessors are pure transformers — they read source files and
/// produce expanded content. Writing to the datastore is handled by the
/// pipeline, not by individual preprocessors.
pub trait Preprocessor: Send + Sync {
    /// Unique name for this preprocessor (e.g. `"template"`, `"plist"`).
    fn name(&self) -> &str;

    /// The safety model for this transformation.
    fn transform_type(&self) -> TransformType;

    /// Whether this preprocessor handles a file with the given name.
    fn matches_extension(&self, filename: &str) -> bool;

    /// Strip the preprocessor extension to get the logical filename.
    /// e.g. `"config.toml.tmpl"` → `"config.toml"`.
    fn stripped_name(&self, filename: &str) -> String;

    /// Expand the source file into one or more output files.
    ///
    /// For single-file preprocessors (templates): returns one entry.
    /// For multi-file preprocessors (archives): returns many entries.
    ///
    /// The `source` path points to the original file in the pack directory.
    ///
    /// # Memory
    ///
    /// Expanded content is held fully in memory via [`Vec<u8>`]. This is
    /// appropriate for dotfile-sized payloads (configs, small scripts,
    /// small archives). Preprocessors that may handle very large inputs
    /// (e.g. multi-hundred-MB archives of pre-built toolchains) should
    /// consider adding a streaming path rather than materialising the
    /// entire decoded stream at once.
    fn expand(&self, source: &Path, fs: &dyn Fs) -> Result<Vec<ExpandedFile>>;
}

/// Registry of available preprocessors.
///
/// Preprocessors are checked in registration order. The first preprocessor
/// whose `matches_extension` returns true for a filename wins.
pub struct PreprocessorRegistry {
    preprocessors: Vec<Box<dyn Preprocessor>>,
}

impl PreprocessorRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            preprocessors: Vec::new(),
        }
    }

    /// Register a preprocessor.
    pub fn register(&mut self, preprocessor: Box<dyn Preprocessor>) {
        self.preprocessors.push(preprocessor);
    }

    /// Find the preprocessor that handles a given filename, if any.
    pub fn find_for_file(&self, filename: &str) -> Option<&dyn Preprocessor> {
        self.preprocessors
            .iter()
            .find(|p| p.matches_extension(filename))
            .map(|p| p.as_ref())
    }

    /// Whether any registered preprocessor handles this filename.
    pub fn is_preprocessor_file(&self, filename: &str) -> bool {
        self.find_for_file(filename).is_some()
    }

    /// Whether the registry has any preprocessors registered.
    pub fn is_empty(&self) -> bool {
        self.preprocessors.is_empty()
    }

    /// Number of registered preprocessors.
    pub fn len(&self) -> usize {
        self.preprocessors.len()
    }
}

impl Default for PreprocessorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time check: Preprocessor must be object-safe
    #[allow(dead_code)]
    fn assert_object_safe(_: &dyn Preprocessor) {}

    #[allow(dead_code)]
    fn assert_boxable(_: Box<dyn Preprocessor>) {}

    #[test]
    fn transform_type_eq() {
        assert_eq!(TransformType::Generative, TransformType::Generative);
        assert_ne!(TransformType::Generative, TransformType::Opaque);
    }

    #[test]
    fn empty_registry() {
        let registry = PreprocessorRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        assert!(!registry.is_preprocessor_file("anything.txt"));
        assert!(registry.find_for_file("anything.txt").is_none());
    }

    #[test]
    fn registry_finds_preprocessor() {
        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(
            crate::preprocessing::identity::IdentityPreprocessor::new(),
        ));

        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);
        assert!(registry.is_preprocessor_file("config.toml.identity"));
        assert!(!registry.is_preprocessor_file("config.toml"));

        let found = registry.find_for_file("config.toml.identity").unwrap();
        assert_eq!(found.name(), "identity");
    }

    #[test]
    fn registry_first_match_wins() {
        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(
            crate::preprocessing::identity::IdentityPreprocessor::new(),
        ));
        // Registering a second one that matches the same extension
        registry.register(Box::new(
            crate::preprocessing::identity::IdentityPreprocessor::with_extension("identity"),
        ));

        let found = registry.find_for_file("test.identity").unwrap();
        assert_eq!(found.name(), "identity");
    }

    #[test]
    fn registry_multiple_different_preprocessors() {
        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(
            crate::preprocessing::identity::IdentityPreprocessor::new(),
        ));
        registry.register(Box::new(
            crate::preprocessing::unarchive::UnarchivePreprocessor::new(),
        ));

        assert_eq!(registry.len(), 2);

        // Each matches its own extension
        assert!(registry.is_preprocessor_file("config.toml.identity"));
        assert!(registry.is_preprocessor_file("bin.tar.gz"));

        // Neither matches the other
        let identity = registry.find_for_file("config.toml.identity").unwrap();
        assert_eq!(identity.name(), "identity");

        let unarchive = registry.find_for_file("bin.tar.gz").unwrap();
        assert_eq!(unarchive.name(), "unarchive");

        // Non-preprocessor files still return None
        assert!(registry.find_for_file("regular.txt").is_none());
    }

    #[test]
    fn registry_does_not_match_partial_extension() {
        let mut registry = PreprocessorRegistry::new();
        registry.register(Box::new(
            crate::preprocessing::identity::IdentityPreprocessor::new(),
        ));

        // "identity" alone is not ".identity"
        assert!(!registry.is_preprocessor_file("identity"));
        // File without the dot prefix shouldn't match
        assert!(!registry.is_preprocessor_file("fileidentity"));
    }
}

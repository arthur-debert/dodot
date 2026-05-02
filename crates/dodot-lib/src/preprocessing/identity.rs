//! Identity preprocessor — passes content through unchanged.
//!
//! This preprocessor exists for testing the preprocessing pipeline.
//! It matches files with the `.identity` extension and returns their
//! content unchanged.

use std::path::{Path, PathBuf};

use crate::fs::Fs;
use crate::preprocessing::{ExpandedFile, Preprocessor, TransformType};
use crate::Result;

/// A preprocessor that passes content through unchanged.
///
/// Useful for testing the preprocessing pipeline without depending
/// on any transformation engine.
pub struct IdentityPreprocessor {
    extension: String,
}

impl IdentityPreprocessor {
    /// Create a new identity preprocessor with the default extension `.identity`.
    pub fn new() -> Self {
        Self {
            extension: "identity".to_string(),
        }
    }

    /// Create an identity preprocessor with a custom extension.
    pub fn with_extension(ext: &str) -> Self {
        Self {
            extension: ext.to_string(),
        }
    }
}

impl Default for IdentityPreprocessor {
    fn default() -> Self {
        Self::new()
    }
}

impl Preprocessor for IdentityPreprocessor {
    fn name(&self) -> &str {
        "identity"
    }

    fn transform_type(&self) -> TransformType {
        TransformType::Generative
    }

    fn matches_extension(&self, filename: &str) -> bool {
        let suffix = format!(".{}", self.extension);
        filename.ends_with(&suffix)
    }

    fn stripped_name(&self, filename: &str) -> String {
        let suffix = format!(".{}", self.extension);
        filename
            .strip_suffix(&suffix)
            .unwrap_or(filename)
            .to_string()
    }

    fn expand(&self, source: &Path, fs: &dyn Fs) -> Result<Vec<ExpandedFile>> {
        let content = fs.read_file(source)?;
        let stripped =
            self.stripped_name(&source.file_name().unwrap_or_default().to_string_lossy());

        Ok(vec![ExpandedFile {
            relative_path: PathBuf::from(stripped),
            content,
            is_dir: false,
            tracked_render: None,
            context_hash: None,
            secret_line_ranges: Vec::new(),
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_identity_extension() {
        let pp = IdentityPreprocessor::new();
        assert!(pp.matches_extension("config.toml.identity"));
        assert!(pp.matches_extension("aliases.sh.identity"));
        assert!(!pp.matches_extension("config.toml"));
        assert!(!pp.matches_extension("identity"));
        assert!(!pp.matches_extension("config.identity.bak"));
    }

    #[test]
    fn stripped_name_removes_extension() {
        let pp = IdentityPreprocessor::new();
        assert_eq!(pp.stripped_name("config.toml.identity"), "config.toml");
        assert_eq!(pp.stripped_name("aliases.sh.identity"), "aliases.sh");
        assert_eq!(pp.stripped_name("simple.identity"), "simple");
    }

    #[test]
    fn custom_extension() {
        let pp = IdentityPreprocessor::with_extension("test");
        assert!(pp.matches_extension("config.toml.test"));
        assert!(!pp.matches_extension("config.toml.identity"));
        assert_eq!(pp.stripped_name("config.toml.test"), "config.toml");
    }

    #[test]
    fn expand_returns_content_unchanged() {
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("config.toml.identity", "host = localhost\nport = 5432")
            .done()
            .build();

        let pp = IdentityPreprocessor::new();
        let source = env.dotfiles_root.join("app/config.toml.identity");
        let result = pp.expand(&source, env.fs.as_ref()).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].relative_path, PathBuf::from("config.toml"));
        assert_eq!(
            String::from_utf8_lossy(&result[0].content),
            "host = localhost\nport = 5432"
        );
        assert!(!result[0].is_dir);
    }

    #[test]
    fn trait_properties() {
        let pp = IdentityPreprocessor::new();
        assert_eq!(pp.name(), "identity");
        assert_eq!(pp.transform_type(), TransformType::Generative);
    }

    #[test]
    fn expand_empty_file() {
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("empty.conf.identity", "")
            .done()
            .build();

        let pp = IdentityPreprocessor::new();
        let source = env.dotfiles_root.join("app/empty.conf.identity");
        let result = pp.expand(&source, env.fs.as_ref()).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].relative_path, PathBuf::from("empty.conf"));
        assert!(result[0].content.is_empty());
    }

    #[test]
    fn expand_binary_content() {
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("data.bin.identity", "")
            .done()
            .build();

        // Write binary content directly
        let source = env.dotfiles_root.join("app/data.bin.identity");
        let binary = vec![0u8, 1, 2, 255, 128, 64];
        env.fs.write_file(&source, &binary).unwrap();

        let pp = IdentityPreprocessor::new();
        let result = pp.expand(&source, env.fs.as_ref()).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content, binary);
    }

    #[test]
    fn expand_missing_file_returns_error() {
        let env = crate::testing::TempEnvironment::builder().build();

        let pp = IdentityPreprocessor::new();
        let source = env.dotfiles_root.join("nonexistent.identity");
        let err = pp.expand(&source, env.fs.as_ref());

        assert!(err.is_err(), "expanding a missing file should fail");
    }

    #[test]
    fn double_extension_only_strips_last() {
        let pp = IdentityPreprocessor::new();
        // Only the outermost .identity is stripped
        assert_eq!(pp.stripped_name("file.identity.identity"), "file.identity");
        assert!(pp.matches_extension("file.identity.identity"));
    }

    #[test]
    fn expand_is_idempotent() {
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("config.toml.identity", "data")
            .done()
            .build();

        let pp = IdentityPreprocessor::new();
        let source = env.dotfiles_root.join("app/config.toml.identity");

        let result1 = pp.expand(&source, env.fs.as_ref()).unwrap();
        let result2 = pp.expand(&source, env.fs.as_ref()).unwrap();

        assert_eq!(result1.len(), result2.len());
        assert_eq!(result1[0].relative_path, result2[0].relative_path);
        assert_eq!(result1[0].content, result2[0].content);
    }
}

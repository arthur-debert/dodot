//! Preprocessing pipeline — transforms source files before handler dispatch.
//!
//! Preprocessors expand files whose version-controlled source differs from
//! the deployed artifact (templates, plists, encrypted secrets). The
//! preprocessing phase runs before handler dispatch, producing virtual
//! entries that downstream handlers (symlink, shell, path, install,
//! homebrew) consume transparently.
//!
//! See `docs/proposals/preprocessing-pipeline.lex` for the full design.

pub mod baseline;
pub mod conflict;
pub mod divergence;
pub mod identity;
pub mod no_reverse;
pub mod pipeline;
pub mod reverse_merge;
pub mod template;
pub mod unarchive;

pub use pipeline::PreprocessMode;

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

/// One entry in a per-render secrets sidecar — a span of lines whose
/// content was produced by a `secret(...)` call, paired with the
/// reference that produced it.
///
/// Lines are 0-indexed and `start..end` is half-open. A single-line
/// secret occupies line `start` and is encoded as `end == start + 1`
/// (`start == end` would be an empty range and is never produced).
/// For Phase S1 every entry is single-line: multi-line secrets are
/// refused at resolution time per `secrets.lex` §3.4. The `end` field
/// is preserved in the schema for forward-compatibility but the
/// renderer never produces `end > start + 1`.
///
/// Persisted to disk under `<baseline>.secret.json` (see
/// `secrets.lex` §3.3); consumed by the dry-run preview rendering
/// (§7.4) to mask resolved values, and by the burgertocow mask
/// integration (issue arthur-debert/burgertocow#13) to skip those
/// lines from the reverse diff.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SecretLineRange {
    /// First line, 0-indexed, inclusive.
    pub start: usize,
    /// One past the last line, 0-indexed, exclusive. `start + 1` for
    /// a single-line value.
    pub end: usize,
    /// The original `secret(...)` argument string, e.g.
    /// `"op://Personal/DB/password"`. Surfaces in the dry-run
    /// `[SECRET: <reference>]` placeholder.
    pub reference: String,
}

/// A single file produced by a preprocessor's expansion.
///
/// Construct ad-hoc via the struct literal; tests commonly use
/// `ExpandedFile { relative_path, content, ..Default::default() }` to
/// fill in the optional cache-related fields.
#[derive(Debug, Clone, Default)]
pub struct ExpandedFile {
    /// Path relative to the expansion output (usually just the filename).
    pub relative_path: PathBuf,
    /// The file content.
    pub content: Vec<u8>,
    /// Whether this entry is a directory marker.
    pub is_dir: bool,
    /// Marker-annotated rendered output, populated by Generative
    /// preprocessors that support cache-backed reverse-diff (templates).
    /// `None` for Representational, Opaque, or generative preprocessors
    /// that don't track variable boundaries (e.g. unarchive).
    ///
    /// When present, the pipeline persists this string in the baseline
    /// cache so the clean filter and `dodot transform check` can compute
    /// reverse-diffs without re-rendering — the latter being important
    /// because re-rendering can re-trigger secret-provider auth prompts.
    pub tracked_render: Option<String>,
    /// SHA-256 of the rendering context (variables, env values resolved
    /// at render time). `None` for preprocessors that don't have a
    /// meaningful context concept.
    ///
    /// The pipeline pairs this with the source-file hash and rendered
    /// content hash in the baseline cache. `dodot up` re-rendering and
    /// install/homebrew sentinels both use the context hash to decide
    /// when work is stale.
    pub context_hash: Option<[u8; 32]>,
    /// Per-render secret-line tracking. Empty when no `secret(...)`
    /// calls fired (the common case today; will be the common case
    /// forever for templates that don't use secrets). Populated by
    /// `TemplatePreprocessor` when a [`crate::secret::SecretRegistry`]
    /// is wired in. The pipeline persists this as a sidecar JSON
    /// alongside the baseline.
    pub secret_line_ranges: Vec<SecretLineRange>,
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

    /// Whether this preprocessor participates in the reverse-merge
    /// pipeline. Reverse-merge is the cache-backed flow that lets
    /// `dodot transform check` propagate edits from the deployed file
    /// back into the source by writing a unified diff (and, for
    /// ambiguous edits, dodot-conflict marker blocks).
    ///
    /// Default `false`. Generative preprocessors that emit a
    /// [`tracked_render`](ExpandedFile::tracked_render) and want their
    /// sources scanned for unresolved markers before expansion override
    /// this to `true`. The pipeline uses the flag to:
    ///
    /// - Decide whether to run [`crate::preprocessing::conflict::
    ///   ensure_no_unresolved_markers`] on the source bytes before
    ///   calling `expand` — refusing to render a template that already
    ///   carries an unresolved conflict block (otherwise the markers
    ///   would deploy as garbage).
    /// - Filter the set of files visited by `dodot transform check` to
    ///   those whose preprocessor knows how to write reverse-diffs.
    ///
    /// A preprocessor that returns `true` here MUST also populate
    /// `tracked_render` on its `ExpandedFile`s; otherwise the cache
    /// layer has no marker stream to feed into burgertocow.
    fn supports_reverse_merge(&self) -> bool {
        false
    }
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

/// The default registry used on the normal execution path.
///
/// Contains all user-facing preprocessors:
/// - [`unarchive::UnarchivePreprocessor`] for `.tar.gz` extraction
/// - [`template::TemplatePreprocessor`] for Jinja2-style templates
///
/// The [`identity`] preprocessor is test-only and is intentionally *not*
/// registered here (it would match innocuous-looking `.identity` files in
/// user dotfiles).
///
/// `secret_config` controls whether the template preprocessor gets a
/// [`SecretRegistry`] wired in. When `[secret] enabled = true` and at
/// least one provider is enabled, this function builds the registry,
/// wires it onto the template preprocessor, and returns it via
/// `out_secret_registry` so the caller can run preflight checks
/// (`crate::secret::preflight`) before any rendering begins. When
/// secrets are disabled, the template preprocessor is built without a
/// registry and `secret(...)` calls in templates surface a config-
/// pointing render error.
pub fn default_registry(
    template_config: &crate::config::PreprocessorTemplateSection,
    secret_config: &crate::config::SecretSection,
    pather: &dyn crate::paths::Pather,
    command_runner: std::sync::Arc<dyn crate::datastore::CommandRunner>,
) -> Result<(
    PreprocessorRegistry,
    Option<std::sync::Arc<crate::secret::SecretRegistry>>,
)> {
    use std::sync::Arc;

    let mut registry = PreprocessorRegistry::new();
    registry.register(Box::new(unarchive::UnarchivePreprocessor::new()));

    let mut tpl = template::TemplatePreprocessor::new(
        template_config.extensions.clone(),
        template_config.vars.clone(),
        pather,
    )?;

    let secret_registry = if secret_config.enabled {
        build_secret_registry(secret_config, command_runner, pather.dotfiles_root())
    } else {
        None
    };

    if let Some(sr) = &secret_registry {
        tpl = tpl.with_secret_registry(Arc::clone(sr));
    }

    registry.register(Box::new(tpl));
    Ok((registry, secret_registry))
}

/// Construct a [`crate::secret::SecretRegistry`] from the per-provider
/// `[secret.providers.*]` config blocks. Each enabled provider is
/// constructed with the shared `CommandRunner` (so tests can inject a
/// mock runner) and registered. Returns `None` if no provider is
/// enabled — the secrets layer treats that case as "secrets feature
/// fully off" and templates with `secret(...)` calls fail loudly.
///
/// `dotfiles_root` is the anchor for relative paths in
/// provider-specific references — currently used by the `sops`
/// provider, whose `sops:secrets.yaml#k.p` references resolve
/// `secrets.yaml` relative to this directory.
///
/// Public so `commands::up` can build a single registry from the root
/// config to run [`crate::secret::preflight`] once per run, before any
/// per-pack template rendering begins (`secrets.lex` §5.4).
pub fn build_secret_registry(
    config: &crate::config::SecretSection,
    runner: std::sync::Arc<dyn crate::datastore::CommandRunner>,
    dotfiles_root: &std::path::Path,
) -> Option<std::sync::Arc<crate::secret::SecretRegistry>> {
    use std::path::PathBuf;
    use std::sync::Arc;

    let mut reg = crate::secret::SecretRegistry::new();
    let mut any_enabled = false;

    if config.providers.pass.enabled {
        let store_dir = if config.providers.pass.store_dir.is_empty() {
            // Defer to env / default: PassProvider::from_env reads
            // $PASSWORD_STORE_DIR or falls back to ~/.password-store.
            None
        } else {
            Some(PathBuf::from(&config.providers.pass.store_dir))
        };
        let provider = match store_dir {
            Some(dir) => crate::secret::PassProvider::new(Arc::clone(&runner), dir),
            None => crate::secret::PassProvider::from_env(Arc::clone(&runner)),
        };
        reg.register(Arc::new(provider));
        any_enabled = true;
    }

    if config.providers.op.enabled {
        let provider = crate::secret::OpProvider::from_env(Arc::clone(&runner));
        reg.register(Arc::new(provider));
        any_enabled = true;
    }

    if config.providers.bw.enabled {
        let provider = crate::secret::BwProvider::from_env(Arc::clone(&runner));
        reg.register(Arc::new(provider));
        any_enabled = true;
    }

    if config.providers.sops.enabled {
        // sops anchors relative file paths (`sops:secrets.yaml#k`)
        // at the dotfiles root, so `.sops.yaml` configuration in the
        // repo root applies. Absolute paths in references bypass
        // this anchor.
        let provider =
            crate::secret::SopsProvider::new(Arc::clone(&runner), dotfiles_root.to_path_buf());
        reg.register(Arc::new(provider));
        any_enabled = true;
    }

    if any_enabled {
        Some(Arc::new(reg))
    } else {
        None
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

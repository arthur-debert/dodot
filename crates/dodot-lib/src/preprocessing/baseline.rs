//! Per-file baseline cache for the preprocessing pipeline.
//!
//! Every successful expansion writes a JSON record at
//! `<cache_dir>/preprocessor/<pack>/<handler>/<filename>.json` capturing
//! enough state to (a) detect drift on the deployed file, (b) decide
//! whether the source has changed, and (c) drive cache-backed
//! reverse-merge without re-rendering the template.
//!
//! See `docs/proposals/preprocessing-pipeline.lex` §5.2 for the
//! field-level contract and `docs/proposals/magic.lex` §"Cache That
//! Makes It Cheap" for why the `tracked_render` field exists.
//!
//! # Lifecycle
//!
//! - **Write**: `preprocess_pack` calls [`Baseline::write`] after every
//!   successful expansion. Re-running `dodot up` overwrites the file in
//!   place.
//! - **Read**: `dodot transform check` and the clean filter call
//!   [`Baseline::load`] to drive divergence detection.
//! - **Cleanup**: `dodot down` deletes the per-pack subdirectory; the
//!   cache survives `dodot up` failures so partial deployments don't
//!   strand baseline data for files that did succeed.
//!
//! # Schema versioning
//!
//! Records carry a `version` field. The current schema is `1`. Future
//! changes that add fields can stay at `v1` (serde-default fills in the
//! missing value); breaking changes bump the version, and load returns
//! a clean error so the user can clear the cache and re-baseline.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::fs::Fs;
use crate::paths::Pather;
use crate::{DodotError, Result};

/// Current baseline-cache schema version. Bump on incompatible changes.
pub const SCHEMA_VERSION: u32 = 1;

/// One baseline record — the cached state of a single processed file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Baseline {
    /// Schema version — see [`SCHEMA_VERSION`].
    pub version: u32,
    /// Absolute path of the source file at expansion time. Captured so
    /// `dodot transform check` can re-find the template to patch
    /// without re-walking the pack tree, and so cache-only diagnostics
    /// can name the source even after pack reorganisation.
    ///
    /// `#[serde(default)]` for forward compatibility with any v1
    /// baseline written before this field existed (treated as empty;
    /// transform check will skip such entries until they're rewritten
    /// by the next `dodot up`).
    #[serde(default)]
    pub source_path: PathBuf,
    /// SHA-256 of the rendered (visible, marker-free) output, hex-encoded.
    pub rendered_hash: String,
    /// The full rendered output verbatim. Stored so reverse-merge can
    /// diff the deployed file against the baseline byte-for-byte
    /// without re-rendering the template.
    pub rendered_content: String,
    /// SHA-256 of the source file's bytes at the moment of expansion,
    /// hex-encoded. Used to distinguish "user edited the source" from
    /// "user edited the deployed file" (the 4-state matrix in the
    /// pipeline spec §6.1).
    pub source_hash: String,
    /// SHA-256 of the rendering context (variables, dodot.* values),
    /// hex-encoded. Provided by the preprocessor; for templates this is
    /// the deterministic projection computed by
    /// [`compute_context_hash`](crate::preprocessing::template). May be
    /// empty if the preprocessor has no meaningful context concept.
    #[serde(default)]
    pub context_hash: String,
    /// Marker-annotated rendered output (burgertocow's "tracked"
    /// stream). Empty when the preprocessor doesn't produce one.
    /// Persisted so the clean filter can rehydrate a `TrackedRender`
    /// via [`burgertocow::TrackedRender::from_tracked_string`] and
    /// drive the reverse-diff without re-rendering — re-rendering at
    /// clean-filter time would re-trigger any secret-provider auth
    /// prompts on every `git status`.
    #[serde(default)]
    pub tracked_render: String,
    /// Wall-clock unix timestamp (seconds) of when the baseline was
    /// written. Used by `dodot transform status` to show "deployed
    /// since …". Not load-bearing for divergence detection.
    pub timestamp: u64,
}

impl Baseline {
    /// Build a baseline from raw inputs. Hashes are computed here so
    /// callers don't repeat the SHA setup; the optional `tracked_render`
    /// and `context_hash` come straight off the preprocessor's
    /// `ExpandedFile`.
    ///
    /// `source_path` is the absolute path of the source file inside
    /// the pack — recorded so reverse-merge knows where to write the
    /// patched template back to.
    pub fn build(
        source_path: &Path,
        rendered_content: &[u8],
        source_bytes: &[u8],
        tracked_render: Option<&str>,
        context_hash: Option<&[u8; 32]>,
    ) -> Self {
        Self {
            version: SCHEMA_VERSION,
            source_path: source_path.to_path_buf(),
            rendered_hash: hex_sha256(rendered_content),
            rendered_content: String::from_utf8_lossy(rendered_content).into_owned(),
            source_hash: hex_sha256(source_bytes),
            context_hash: context_hash.map(hex_encode_32).unwrap_or_default(),
            tracked_render: tracked_render.unwrap_or("").to_string(),
            timestamp: now_secs_unix(),
        }
    }

    /// Persist this baseline to its JSON path under the cache dir.
    /// Creates parent directories as needed. Overwrites any existing
    /// file at the target path.
    pub fn write(
        &self,
        fs: &dyn Fs,
        paths: &dyn Pather,
        pack: &str,
        handler: &str,
        filename: &str,
    ) -> Result<PathBuf> {
        let path = paths.preprocessor_baseline_path(pack, handler, filename);
        if let Some(parent) = path.parent() {
            fs.mkdir_all(parent)?;
        }
        let body = serde_json::to_string_pretty(self).map_err(|e| {
            DodotError::Other(format!(
                "failed to serialise baseline for {pack}/{handler}/{filename}: {e}"
            ))
        })?;
        fs.write_file(&path, body.as_bytes())?;
        Ok(path)
    }

    /// Load a baseline from its JSON path. Returns `Ok(None)` if the
    /// file does not exist (a file with no baseline is a normal state
    /// for a brand-new pack); returns an error for parse failures or
    /// unsupported schema versions so the caller can suggest a manual
    /// clear.
    pub fn load(
        fs: &dyn Fs,
        paths: &dyn Pather,
        pack: &str,
        handler: &str,
        filename: &str,
    ) -> Result<Option<Self>> {
        let path = paths.preprocessor_baseline_path(pack, handler, filename);
        if !fs.exists(&path) {
            return Ok(None);
        }
        let raw = fs.read_to_string(&path)?;
        let baseline: Self = serde_json::from_str(&raw).map_err(|e| {
            DodotError::Other(format!(
                "failed to parse baseline at {}: {e}\n  \
                 Try `dodot up --force` to re-baseline.",
                path.display()
            ))
        })?;
        if baseline.version != SCHEMA_VERSION {
            return Err(DodotError::Other(format!(
                "baseline at {} has unsupported schema version {} (expected {}). \
                 Clear the file and run `dodot up` to rebuild.",
                path.display(),
                baseline.version,
                SCHEMA_VERSION
            )));
        }
        Ok(Some(baseline))
    }
}

/// SHA-256 → 64-char lowercase hex. Used by the baseline cache for
/// rendered/source content hashing and by the divergence walker for
/// the same purpose against current on-disk state. `pub(crate)` so
/// the divergence module reuses it instead of cloning a parallel
/// implementation.
pub(crate) fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_encode_32(&hasher.finalize().into())
}

pub(crate) fn hex_encode_32(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for b in bytes {
        out.push(hex_nibble(b >> 4));
        out.push(hex_nibble(b & 0x0f));
    }
    out
}

fn hex_nibble(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        10..=15 => (b'a' + n - 10) as char,
        _ => unreachable!(),
    }
}

fn now_secs_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Canonical cache key for a baseline given a logical (stripped)
/// pack-relative path. Preserves the full relative path so two
/// tracked files with the same basename in different subdirectories
/// (`a/config.toml` vs `b/config.toml`) get distinct cache slots.
///
/// Returns a slash-separated string. `Baseline::write` joins it onto
/// the cache root via `preprocessor_baseline_path`, which produces
/// `<cache>/preprocessor/<pack>/<handler>/<relative>.json` — and
/// `mkdir_all`s any required parent directories. The cache layout
/// thus mirrors the datastore layout under
/// `<data>/packs/<pack>/<handler>/<relative>`.
///
/// `.` segments are dropped (the same normalisation the pipeline
/// applies to virtual entries). An empty / pure-`.` input falls back
/// to the lossy string form to avoid panicking, but the pipeline's
/// `validate_safe_relative_path` rejects such inputs upstream.
pub fn cache_filename_for(virtual_relative: &Path) -> String {
    use std::path::Component;
    let mut parts: Vec<String> = Vec::new();
    for component in virtual_relative.components() {
        if let Component::Normal(n) = component {
            parts.push(n.to_string_lossy().into_owned());
        }
    }
    if parts.is_empty() {
        return virtual_relative.to_string_lossy().into_owned();
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempEnvironment;

    #[test]
    fn build_then_write_then_load_round_trips() {
        let env = TempEnvironment::builder().build();
        let baseline = Baseline::build(
            Path::new("/tmp/config.toml.tmpl"),
            b"name = Alice\n",
            b"name = {{ name }}\n",
            Some("name = \u{1e}Alice\u{1f}\n"),
            Some(&[0x42; 32]),
        );
        let path = baseline
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "config.toml",
            )
            .unwrap();
        assert!(env.fs.exists(&path));

        let loaded = Baseline::load(
            env.fs.as_ref(),
            env.paths.as_ref(),
            "app",
            "preprocessed",
            "config.toml",
        )
        .unwrap()
        .expect("baseline must exist after write");
        assert_eq!(loaded, baseline);
    }

    #[test]
    fn load_returns_none_for_missing_file() {
        let env = TempEnvironment::builder().build();
        let result = Baseline::load(
            env.fs.as_ref(),
            env.paths.as_ref(),
            "app",
            "preprocessed",
            "nope.toml",
        )
        .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn load_rejects_unsupported_schema_version() {
        let env = TempEnvironment::builder().build();
        let path = env
            .paths
            .preprocessor_baseline_path("app", "preprocessed", "config.toml");
        env.fs.mkdir_all(path.parent().unwrap()).unwrap();
        env.fs
            .write_file(
                &path,
                br#"{"version": 999, "rendered_hash": "x", "rendered_content": "x", "source_hash": "x", "timestamp": 0}"#,
            )
            .unwrap();

        let err = Baseline::load(
            env.fs.as_ref(),
            env.paths.as_ref(),
            "app",
            "preprocessed",
            "config.toml",
        )
        .unwrap_err();
        assert!(
            format!("{err}").contains("unsupported schema version"),
            "got: {err}"
        );
    }

    #[test]
    fn load_rejects_corrupted_json() {
        let env = TempEnvironment::builder().build();
        let path = env
            .paths
            .preprocessor_baseline_path("app", "preprocessed", "config.toml");
        env.fs.mkdir_all(path.parent().unwrap()).unwrap();
        env.fs.write_file(&path, b"{not json").unwrap();

        let err = Baseline::load(
            env.fs.as_ref(),
            env.paths.as_ref(),
            "app",
            "preprocessed",
            "config.toml",
        )
        .unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("failed to parse"), "got: {msg}");
        // Hint to clear the cache should be in the error so users have
        // a recovery path.
        assert!(
            msg.contains("--force"),
            "expected recovery hint, got: {msg}"
        );
    }

    #[test]
    fn build_records_hashes_and_optional_fields() {
        // Empty optionals → empty strings (serde default), not Null.
        let p = Path::new("/dummy/source");
        let b = Baseline::build(p, b"hello", b"hello", None, None);
        assert_eq!(b.version, SCHEMA_VERSION);
        assert_eq!(b.source_path, p);
        assert_eq!(b.rendered_hash.len(), 64); // SHA-256 hex
        assert_eq!(b.source_hash, b.rendered_hash); // same bytes
        assert!(b.context_hash.is_empty());
        assert!(b.tracked_render.is_empty());

        // Provided optionals → encoded.
        let b2 = Baseline::build(p, b"x", b"y", Some("tracked"), Some(&[0xff; 32]));
        assert_eq!(b2.context_hash.len(), 64);
        assert!(b2.context_hash.chars().all(|c| c == 'f'));
        assert_eq!(b2.tracked_render, "tracked");
    }

    #[test]
    fn rendered_content_preserves_lossy_utf8() {
        // The cache holds rendered_content as UTF-8 (templates are
        // text); this test pins the loss behaviour for non-UTF-8 bytes
        // so a future change is a deliberate decision.
        let b = Baseline::build(
            Path::new("/dummy"),
            &[0x66, 0x6f, 0xff, 0x6f],
            b"src",
            None,
            None,
        );
        // Replacement character for the invalid 0xff.
        assert_eq!(b.rendered_content, "fo\u{fffd}o");
    }

    #[test]
    fn write_creates_nested_directories() {
        // Pack-and-handler directories may not exist on first write;
        // confirm we mkdir_all rather than expecting them to be there.
        let env = TempEnvironment::builder().build();
        let baseline = Baseline::build(Path::new("/dummy"), b"x", b"y", None, None);
        let path = baseline
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "deep",
                "preprocessed",
                "x",
            )
            .unwrap();
        assert!(env.fs.exists(&path));
        assert!(env.fs.is_dir(path.parent().unwrap()));
    }

    #[test]
    fn write_overwrites_existing_baseline() {
        // A second write at the same logical path replaces the first.
        let env = TempEnvironment::builder().build();
        let first = Baseline::build(Path::new("/dummy"), b"first", b"src", None, None);
        first
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "f",
            )
            .unwrap();
        let second = Baseline::build(Path::new("/dummy"), b"second", b"src", None, None);
        second
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "f",
            )
            .unwrap();

        let loaded = Baseline::load(
            env.fs.as_ref(),
            env.paths.as_ref(),
            "app",
            "preprocessed",
            "f",
        )
        .unwrap()
        .unwrap();
        assert_eq!(loaded.rendered_content, "second");
    }

    #[test]
    fn cache_filename_for_preserves_relative_path() {
        // Top-level files: bare name, no separators introduced.
        assert_eq!(cache_filename_for(Path::new("config.toml")), "config.toml");
        // Nested files: full relative path preserved so two files
        // with the same basename in different subdirectories get
        // distinct cache slots.
        assert_eq!(
            cache_filename_for(Path::new("subdir/config.toml")),
            "subdir/config.toml"
        );
        assert_eq!(
            cache_filename_for(Path::new("a/b/c/leaf.txt")),
            "a/b/c/leaf.txt"
        );
        // `.` segments are dropped (matches the pipeline's
        // virtual-path normalisation).
        assert_eq!(
            cache_filename_for(Path::new("./config.toml")),
            "config.toml"
        );
    }

    #[test]
    fn hex_encoding_is_lowercase_and_padded() {
        assert_eq!(hex_encode_32(&[0; 32]).len(), 64);
        assert!(hex_encode_32(&[0; 32]).chars().all(|c| c == '0'));
        assert_eq!(hex_encode_32(&[0xab; 32]).len(), 64);
        // Lowercase by convention.
        assert!(hex_encode_32(&[0xab; 32])
            .chars()
            .all(|c| c == 'a' || c == 'b'));
    }
}

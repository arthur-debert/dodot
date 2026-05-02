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
    ///
    /// Lazy migration: when `filename` is a nested path (contains a
    /// `/`), the legacy cache file written under the basename-only
    /// layout (pre-PR-#118-cache-fix) may also exist. Delete it
    /// **only if its `source_path` matches `self.source_path`** —
    /// otherwise it could be a legitimate top-level (or
    /// different-nested) baseline that shares the same basename and
    /// must NOT be touched. Migration is triggered the first time
    /// `up` re-baselines a nested template after the upgrade.
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

        // Migration cleanup: drop the legacy basename-only cache
        // file if one exists AND it represents the same source as
        // what we're writing now.
        //
        // Disambiguation rules:
        // - Strict match: legacy `source_path` equals `self.source_path`
        //   → definitely ours, delete.
        // - Best-effort match: legacy entry has empty `source_path`
        //   (very old v1 baseline that predates the field). We can't
        //   confirm it's ours, but leaving it behind produces a
        //   permanent stale `MissingSource` row in `transform status`
        //   and the orphan would persist forever. Treat as ours so
        //   the migration completes.
        //
        // The trade-off mirrors the load-side decision: on the
        // narrow false-positive path (empty-source_path legacy entry
        // that actually belongs to a different same-basename file),
        // we may delete an entry that should have stayed. The next
        // `up` for that file will rebaseline it from scratch, so the
        // cost is a one-time loss of divergence-detection state for
        // a file that already had ambiguous cache data — not data
        // loss in the source or deployed file.
        if let Some(basename) = legacy_basename_for(filename) {
            let legacy_path = paths.preprocessor_baseline_path(pack, handler, &basename);
            if legacy_path != path && fs.exists(&legacy_path) {
                let should_delete = match read_baseline_at(fs, &legacy_path) {
                    Ok(Some(legacy)) => {
                        legacy.source_path.as_os_str().is_empty()
                            || legacy.source_path == self.source_path
                    }
                    Ok(None) => {
                        // File vanished between the `exists` check
                        // and the read (race with another process).
                        // Nothing to do.
                        false
                    }
                    Err(_) => {
                        // Legacy file is unreadable — corrupt JSON
                        // or schema mismatch. We've already written
                        // a valid replacement at the new nested
                        // path, so leaving the corrupt file behind
                        // would only break later
                        // `collect_baselines` walks (every
                        // `transform check` / `status` / `refresh`
                        // would error on parse). Delete it.
                        true
                    }
                };
                if should_delete {
                    // Best-effort: a remove failure is non-fatal —
                    // the new baseline is already written, so the
                    // only cost is a stale entry until the user
                    // clears it manually.
                    let _ = fs.remove_file(&legacy_path);
                }
            }
        }

        Ok(path)
    }

    /// Load a baseline from its JSON path. Returns `Ok(None)` if the
    /// file does not exist (a file with no baseline is a normal state
    /// for a brand-new pack); returns an error for parse failures or
    /// unsupported schema versions so the caller can suggest a manual
    /// clear.
    ///
    /// Legacy-layout fallback: if `filename` is a nested path
    /// (`subdir/config.toml`) and no baseline lives at the new
    /// nested cache path, retry with the basename-only path
    /// (`config.toml`) — that's where the PR-#118 cache-fix moved
    /// the layout away from. The fallback **only returns the legacy
    /// baseline if its `source_path` matches the file we're loading**
    /// (path-tail check on the source_path's components, accounting
    /// for the trailing preprocessor extension). This disambiguates
    /// the case where a pack legitimately has both a top-level and a
    /// nested file with the same basename — the basename-only path
    /// could host either, and returning the wrong one would compare
    /// the divergence guard against the wrong cached render.
    pub fn load(
        fs: &dyn Fs,
        paths: &dyn Pather,
        pack: &str,
        handler: &str,
        filename: &str,
    ) -> Result<Option<Self>> {
        let path = paths.preprocessor_baseline_path(pack, handler, filename);
        if let Some(b) = read_baseline_at(fs, &path)? {
            return Ok(Some(b));
        }
        // Fallback to the legacy basename-only layout for upgraders.
        if let Some(basename) = legacy_basename_for(filename) {
            let legacy_path = paths.preprocessor_baseline_path(pack, handler, &basename);
            if legacy_path != path {
                if let Some(b) = read_baseline_at(fs, &legacy_path)? {
                    if legacy_baseline_belongs_to_filename(&b.source_path, filename) {
                        return Ok(Some(b));
                    }
                }
            }
        }
        Ok(None)
    }
}

/// Return `true` when the legacy baseline at the basename-only cache
/// path actually belongs to the nested `filename` we're loading
/// (rather than to a different file that happens to share the same
/// basename).
///
/// Strategy:
/// - When `legacy_source_path` is **populated**: take its components,
///   strip a single trailing preprocessor extension from the leaf
///   (e.g. `.tmpl`, `.identity`), and check that the resulting path
///   **ends** with `filename` at a `/` boundary. The boundary check
///   rules out the false positive where a path component happens to
///   contain `filename` as a substring.
/// - When `legacy_source_path` is **empty** (very old v1 baseline
///   that predates the `source_path` field; serde-default fills
///   empty): we can't disambiguate, so we accept the entry as
///   "best-effort ours." The trade-off:
///   * Reject (older behavior): risk of data loss for upgraded
///     users with a nested-file baseline from before the field
///     existed — the guard would treat the next `up` as a fresh
///     deploy and could overwrite a user-edited deployed file.
///   * Accept (current behavior): risk of false-positive preservation
///     when the legacy entry actually belongs to a different
///     same-basename file. In that case the divergence guard
///     compares against the wrong `rendered_hash`, which all but
///     guarantees a hash mismatch → preserve. The user sees a
///     warning and can resolve via `--force`. **Preserve > overwrite.**
fn legacy_baseline_belongs_to_filename(legacy_source_path: &Path, filename: &str) -> bool {
    if legacy_source_path.as_os_str().is_empty() {
        // Empty source_path → very old v1 baseline. Accept as
        // best-effort ours so the guard's fail-safe (preserve)
        // covers upgraded users who would otherwise lose edits.
        return true;
    }
    let mut components: Vec<String> = legacy_source_path
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let last = match components.last_mut() {
        Some(l) => l,
        None => return false,
    };
    if let Some(dot_idx) = last.rfind('.') {
        last.truncate(dot_idx);
    }
    let rebuilt = components.join("/");
    if !rebuilt.ends_with(filename) {
        return false;
    }
    // Boundary check: the character immediately before `filename` in
    // `rebuilt` must be `/`, or `filename` must be the entire string.
    let prefix_len = rebuilt.len() - filename.len();
    prefix_len == 0 || rebuilt.as_bytes().get(prefix_len - 1).copied() == Some(b'/')
}

/// Read and validate a baseline JSON at the given on-disk path. Returns
/// `Ok(None)` for a missing file; `Ok(Some)` on success;
/// `Err` on parse failure or schema-version mismatch (the caller
/// surfaces the recovery hint).
fn read_baseline_at(fs: &dyn Fs, path: &Path) -> Result<Option<Baseline>> {
    if !fs.exists(path) {
        return Ok(None);
    }
    let raw = fs.read_to_string(path)?;
    let baseline: Baseline = serde_json::from_str(&raw).map_err(|e| {
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

/// Return the basename of a slash-separated cache filename, **only
/// when it differs from the input**. Used for the legacy-layout
/// fallback and migration: a top-level file (`config.toml`) returns
/// `None` because there's no separate legacy path; a nested file
/// (`subdir/config.toml`) returns `Some("config.toml")`, the path the
/// pre-PR-#118 layout would have used.
fn legacy_basename_for(filename: &str) -> Option<String> {
    let basename = std::path::Path::new(filename)
        .file_name()?
        .to_string_lossy()
        .into_owned();
    if basename == filename {
        None
    } else {
        Some(basename)
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

    // ── Cache-layout migration (PR #118 6th-pass H) ─────────────────

    #[test]
    fn load_falls_back_to_legacy_basename_layout_for_nested_files() {
        // Pre-PR-#118 layout: a nested template (`subdir/config.toml`)
        // had its baseline cached at the basename-only path
        // (`<cache>/.../preprocessed/config.toml.json`). After the
        // cache-layout fix, lookups use the full nested path. Ensure
        // upgraders don't lose their existing baselines: load tries
        // the new path, falls back to the legacy path.
        let env = TempEnvironment::builder().build();
        // Write only at the legacy basename path.
        let legacy = Baseline::build(
            Path::new("/dotfiles/app/subdir/config.toml.tmpl"),
            b"rendered",
            b"src",
            Some(""),
            None,
        );
        legacy
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "config.toml", // legacy: basename only
            )
            .unwrap();

        // Load via the new nested key. Must find the legacy entry.
        let found = Baseline::load(
            env.fs.as_ref(),
            env.paths.as_ref(),
            "app",
            "preprocessed",
            "subdir/config.toml",
        )
        .unwrap();
        assert!(
            found.is_some(),
            "load must fall back to legacy basename-only layout"
        );
        assert_eq!(found.unwrap().rendered_content, "rendered");
    }

    #[test]
    fn write_at_nested_path_removes_legacy_basename_file() {
        // Migration cleanup: when a nested baseline gets written under
        // the new layout, the legacy basename-only file must be
        // deleted so transform status doesn't carry an orphan entry
        // alongside the migrated baseline.
        let env = TempEnvironment::builder().build();

        // Stage a legacy entry.
        let legacy = Baseline::build(
            Path::new("/dotfiles/app/subdir/config.toml.tmpl"),
            b"old",
            b"old-src",
            Some(""),
            None,
        );
        let legacy_path = legacy
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "config.toml",
            )
            .unwrap();
        assert!(env.fs.exists(&legacy_path));

        // Write at the new nested path. Legacy file must vanish.
        let new = Baseline::build(
            Path::new("/dotfiles/app/subdir/config.toml.tmpl"),
            b"new",
            b"new-src",
            Some(""),
            None,
        );
        let new_path = new
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "subdir/config.toml",
            )
            .unwrap();

        assert!(env.fs.exists(&new_path), "new baseline must exist");
        assert!(
            !env.fs.exists(&legacy_path),
            "legacy basename-only file must be removed during migration"
        );
        assert_ne!(legacy_path, new_path);
    }

    #[test]
    fn write_at_top_level_does_not_touch_legacy_path() {
        // Top-level files (`config.toml`) have no separate legacy
        // path — the new and legacy paths coincide. Ensure write
        // doesn't mistakenly try to delete itself.
        let env = TempEnvironment::builder().build();
        let baseline = Baseline::build(Path::new("/dummy"), b"x", b"y", Some(""), None);
        let path = baseline
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "config.toml",
            )
            .unwrap();
        // The file we just wrote should still exist (no
        // self-deletion via the legacy-removal codepath).
        assert!(env.fs.exists(&path));
    }

    #[test]
    fn legacy_basename_for_returns_none_for_top_level() {
        assert_eq!(legacy_basename_for("config.toml"), None);
        assert_eq!(legacy_basename_for("a"), None);
        assert_eq!(legacy_basename_for(""), None);
    }

    #[test]
    fn legacy_basename_for_returns_basename_for_nested() {
        assert_eq!(
            legacy_basename_for("subdir/config.toml"),
            Some("config.toml".to_string())
        );
        assert_eq!(
            legacy_basename_for("a/b/c/leaf.txt"),
            Some("leaf.txt".to_string())
        );
    }

    // ── Migration disambiguation (PR #118 7th-pass) ─────────────────

    #[test]
    fn legacy_baseline_belongs_to_filename_matches_nested_source() {
        // Source `/dotfiles/app/subdir/config.toml.tmpl` belongs to
        // virtual filename `subdir/config.toml`.
        assert!(legacy_baseline_belongs_to_filename(
            Path::new("/dotfiles/app/subdir/config.toml.tmpl"),
            "subdir/config.toml"
        ));
        // Deeper nesting also matches.
        assert!(legacy_baseline_belongs_to_filename(
            Path::new("/home/user/dotfiles/pkg/a/b/leaf.txt.identity"),
            "a/b/leaf.txt"
        ));
    }

    #[test]
    fn legacy_baseline_belongs_to_filename_rejects_top_level_for_nested() {
        // Source `/dotfiles/app/config.toml.tmpl` is the top-level
        // file. When loading `subdir/config.toml`, the legacy path
        // `<cache>/.../config.toml.json` could host either the
        // top-level baseline (which we DON'T want) or the nested
        // baseline (which we do). The disambiguation must reject
        // the top-level case.
        assert!(!legacy_baseline_belongs_to_filename(
            Path::new("/dotfiles/app/config.toml.tmpl"),
            "subdir/config.toml"
        ));
    }

    #[test]
    fn legacy_baseline_belongs_to_filename_rejects_substring_match() {
        // Source path contains `subdir/config.toml` as a substring
        // but not at a `/` boundary — e.g. a file under
        // `xsubdir/config.toml.tmpl`. Must NOT match.
        assert!(!legacy_baseline_belongs_to_filename(
            Path::new("/dotfiles/app/xsubdir/config.toml.tmpl"),
            "subdir/config.toml"
        ));
    }

    #[test]
    fn legacy_baseline_belongs_to_filename_accepts_empty_source_as_best_effort() {
        // PR #118 9th-pass (Comment O): v1 baselines that predate the
        // `source_path` field serde-default to empty PathBuf. We
        // accept them as best-effort ours so the divergence guard
        // protects upgraded users with old nested-file caches —
        // rejecting would leave the next `up` to overwrite a
        // user-edited deployed file. The trade-off (false-positive
        // preservation if the legacy entry actually belongs to a
        // different same-basename file) is documented at the helper.
        assert!(legacy_baseline_belongs_to_filename(
            Path::new(""),
            "subdir/config.toml"
        ));
    }

    #[test]
    fn load_does_not_return_top_level_baseline_for_nested_lookup() {
        // Pack has a top-level `config.toml` baseline at the legacy
        // basename-only path. A separate nested `subdir/config.toml`
        // does NOT have a baseline yet. Loading "subdir/config.toml"
        // must return None — NOT the top-level file's baseline.
        let env = TempEnvironment::builder().build();
        let top = Baseline::build(
            Path::new("/dotfiles/app/config.toml.tmpl"),
            b"top-rendered",
            b"top-src",
            Some(""),
            None,
        );
        top.write(
            env.fs.as_ref(),
            env.paths.as_ref(),
            "app",
            "preprocessed",
            "config.toml",
        )
        .unwrap();

        // Load by the nested key: must NOT inherit top-level's baseline.
        let result = Baseline::load(
            env.fs.as_ref(),
            env.paths.as_ref(),
            "app",
            "preprocessed",
            "subdir/config.toml",
        )
        .unwrap();
        assert!(
            result.is_none(),
            "load(subdir/config.toml) must NOT return the top-level baseline"
        );
        // And the top-level lookup still works.
        let top_again = Baseline::load(
            env.fs.as_ref(),
            env.paths.as_ref(),
            "app",
            "preprocessed",
            "config.toml",
        )
        .unwrap();
        assert!(top_again.is_some());
    }

    #[test]
    fn write_does_not_delete_top_level_baseline_when_writing_nested() {
        // Pack has a legitimate top-level `config.toml` baseline at
        // `<cache>/.../config.toml.json`. We write a NEW baseline
        // for the nested `subdir/config.toml`. The top-level entry
        // must NOT be deleted by the migration cleanup, because its
        // source_path doesn't match what we're writing.
        let env = TempEnvironment::builder().build();
        let top_source = Path::new("/dotfiles/app/config.toml.tmpl");
        let top = Baseline::build(top_source, b"top-rendered", b"top-src", Some(""), None);
        let top_path = top
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "config.toml",
            )
            .unwrap();
        assert!(env.fs.exists(&top_path));

        // Write the NESTED baseline — different source.
        let nested_source = Path::new("/dotfiles/app/subdir/config.toml.tmpl");
        let nested = Baseline::build(
            nested_source,
            b"nested-rendered",
            b"nested-src",
            Some(""),
            None,
        );
        let nested_path = nested
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "subdir/config.toml",
            )
            .unwrap();
        assert!(env.fs.exists(&nested_path));

        // Top-level baseline must STILL be there — the migration
        // cleanup must have noticed the source_path mismatch and
        // left it alone.
        assert!(
            env.fs.exists(&top_path),
            "top-level baseline must be preserved when writing a nested baseline with the same basename"
        );
        // Sanity: top-level baseline is still loadable by its key.
        let top_check = Baseline::load(
            env.fs.as_ref(),
            env.paths.as_ref(),
            "app",
            "preprocessed",
            "config.toml",
        )
        .unwrap()
        .unwrap();
        assert_eq!(top_check.rendered_content, "top-rendered");
    }

    #[test]
    fn write_still_migrates_when_legacy_belongs_to_us() {
        // Counterpart to the previous test: write cleanup MUST
        // delete the legacy file when its source_path matches
        // (i.e., it was the same nested file's pre-migration
        // baseline). This pins that the disambiguation didn't
        // accidentally disable the legitimate migration path.
        let env = TempEnvironment::builder().build();
        let nested_source = Path::new("/dotfiles/app/subdir/config.toml.tmpl");

        // Pre-migration legacy entry for the nested file (cached
        // under basename-only).
        let legacy = Baseline::build(nested_source, b"old-rendered", b"old-src", Some(""), None);
        let legacy_path = legacy
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "config.toml",
            )
            .unwrap();
        assert!(env.fs.exists(&legacy_path));

        // Now re-baseline at the new nested path with the same source.
        let new = Baseline::build(nested_source, b"new-rendered", b"new-src", Some(""), None);
        let new_path = new
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "subdir/config.toml",
            )
            .unwrap();

        assert!(env.fs.exists(&new_path));
        assert!(
            !env.fs.exists(&legacy_path),
            "legacy entry whose source_path matches must be removed during migration"
        );
    }

    // ── Empty source_path migration (PR #118 9th-pass O+P) ──────────

    #[test]
    fn load_falls_back_to_legacy_with_empty_source_path() {
        // PR #118 9th-pass Comment O: a v1 baseline written before
        // the `source_path` field existed deserializes to empty
        // PathBuf. For an upgraded user with a previously-deployed
        // nested template, the baseline lives at the legacy
        // basename-only cache path and has empty `source_path`.
        // `Baseline::load` for the nested key MUST return it (rather
        // than None) so the divergence guard can preserve any
        // user-edited deployed file. Rejecting would let the next
        // `up` overwrite the user's edit.
        let env = TempEnvironment::builder().build();
        let legacy = Baseline::build(
            // Empty source_path simulating pre-`source_path`-field v1.
            Path::new(""),
            b"rendered",
            b"src",
            Some(""),
            None,
        );
        legacy
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "config.toml",
            )
            .unwrap();

        // Load by the nested key: must return the legacy entry as
        // best-effort match.
        let result = Baseline::load(
            env.fs.as_ref(),
            env.paths.as_ref(),
            "app",
            "preprocessed",
            "subdir/config.toml",
        )
        .unwrap();
        assert!(
            result.is_some(),
            "load(subdir/config.toml) must accept empty-source_path legacy entry as best-effort"
        );
        assert_eq!(result.unwrap().rendered_content, "rendered");
    }

    #[test]
    fn write_at_nested_path_removes_legacy_with_empty_source_path() {
        // PR #118 9th-pass Comment P: a legacy v1 entry with empty
        // source_path must be cleaned up when we write a nested
        // baseline. Otherwise it lingers as a stale orphan and
        // produces a permanent bogus `MissingSource` row in
        // `transform status`.
        let env = TempEnvironment::builder().build();
        // Legacy v1 entry: empty source_path.
        let legacy = Baseline::build(Path::new(""), b"old", b"src", Some(""), None);
        let legacy_path = legacy
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "config.toml",
            )
            .unwrap();
        assert!(env.fs.exists(&legacy_path));

        // Migrate by writing at the new nested path with a populated source_path.
        let new = Baseline::build(
            Path::new("/dotfiles/app/subdir/config.toml.tmpl"),
            b"new",
            b"new-src",
            Some(""),
            None,
        );
        let new_path = new
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "subdir/config.toml",
            )
            .unwrap();

        assert!(env.fs.exists(&new_path), "new baseline written");
        assert!(
            !env.fs.exists(&legacy_path),
            "legacy entry with empty source_path must be cleaned up to avoid permanent orphan"
        );
    }

    #[test]
    fn write_at_nested_path_removes_corrupt_legacy_basename_file() {
        // PR #118 10th-pass Comment R: when the legacy basename-only
        // file is corrupt (truncated JSON, schema mismatch), the
        // write cleanup must DELETE it. Otherwise the corrupt file
        // would later poison `collect_baselines`, breaking every
        // `transform check` / `status` / `refresh` run with a
        // parse error.
        let env = TempEnvironment::builder().build();

        // Stage a corrupt legacy entry directly on disk.
        let legacy_path =
            env.paths
                .preprocessor_baseline_path("app", "preprocessed", "config.toml");
        env.fs.mkdir_all(legacy_path.parent().unwrap()).unwrap();
        env.fs
            .write_file(&legacy_path, b"{this is not valid json")
            .unwrap();
        assert!(env.fs.exists(&legacy_path));

        // Write a valid baseline at the new nested path.
        let new = Baseline::build(
            Path::new("/dotfiles/app/subdir/config.toml.tmpl"),
            b"new",
            b"new-src",
            Some(""),
            None,
        );
        let new_path = new
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "subdir/config.toml",
            )
            .unwrap();

        assert!(env.fs.exists(&new_path), "new baseline written");
        assert!(
            !env.fs.exists(&legacy_path),
            "corrupt legacy file must be removed during migration to avoid breaking collect_baselines later"
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

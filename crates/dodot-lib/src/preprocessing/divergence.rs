//! Drift detection for preprocessor outputs (the 4-state matrix).
//!
//! Walks the per-pack baseline cache and compares each cached record
//! against the current source file (in the pack) and the current
//! deployed file (in the datastore). Classifies each pair into one of
//! the four states defined in `docs/proposals/preprocessing-pipeline.lex`
//! §6.1:
//!
//! | source | deployed | state           |
//! |--------|----------|-----------------|
//! | same   | same     | `Synced`        |
//! | new    | same     | `InputChanged`  |
//! | same   | edited   | `OutputChanged` |
//! | new    | edited   | `BothChanged`   |
//!
//! Plus two special states for missing files: a baseline whose source
//! has been deleted (`MissingSource`) or whose deployed artifact is
//! gone (`MissingDeployed`).
//!
//! This module is **read-only**. It produces a [`DivergenceReport`] per
//! cached baseline; the action layer (`commands::transform::check`)
//! decides what to do with each report (apply a reverse-merge diff,
//! emit a conflict block, etc).

use std::path::PathBuf;

use serde::Serialize;

use crate::fs::Fs;
use crate::paths::Pather;
use crate::preprocessing::baseline::{hex_sha256, Baseline};
use crate::Result;

/// Where a single processed file sits in the 4-state matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DivergenceState {
    /// Source unchanged, deployed file matches the cached render.
    /// Nothing to do.
    Synced,
    /// Source has changed since the cached render, but the deployed
    /// file is still the cached render. The next `dodot up` will
    /// re-render — no action from `transform check`.
    InputChanged,
    /// Source unchanged, deployed file edited by the user. The
    /// reverse-merge engine should propagate the edit back to the
    /// source.
    OutputChanged,
    /// Both the source and the deployed file have changed since the
    /// last `dodot up`. The reverse-merge engine still tries to
    /// produce a diff, but the result is more likely to require a
    /// conflict block.
    BothChanged,
    /// The cached source path no longer exists on disk. The pack file
    /// was renamed or removed; the baseline is stale and should be
    /// dropped on the next `up`.
    MissingSource,
    /// The cached deployed path is gone. The user (or some external
    /// tool) deleted the rendered file. Unusual but worth surfacing.
    MissingDeployed,
}

/// One row in `dodot transform check`'s report.
#[derive(Debug, Clone, Serialize)]
pub struct DivergenceReport {
    pub pack: String,
    pub handler: String,
    /// Filename inside the cache (e.g. `"config.toml"`). Same as the
    /// stripped virtual entry the preprocessor produced.
    pub filename: String,
    /// Absolute path of the source file in the pack.
    pub source_path: PathBuf,
    /// Absolute path of the deployed (rendered) file in the datastore.
    pub deployed_path: PathBuf,
    /// The classified state.
    pub state: DivergenceState,
}

/// One unreadable cache entry surfaced by the walker. A corrupt or
/// version-mismatched baseline can't be loaded into a `Baseline`,
/// but silently dropping it from the report would let `dodot
/// transform check` succeed with an incomplete scan and miss real
/// divergence. We surface these alongside the successful entries so
/// callers can decide how to react: `transform check` flags them
/// as findings (non-zero exit), `transform status` displays them as
/// "cache_error" rows, `refresh` ignores them.
#[derive(Debug, Clone, Serialize)]
pub struct UnreadableBaseline {
    pub pack: String,
    pub handler: String,
    pub filename: String,
    /// Absolute path of the corrupt cache file on disk.
    pub cache_path: PathBuf,
    /// Human-readable description of what went wrong (parse error
    /// message, schema-version mismatch, etc.).
    pub error: String,
}

/// Walk the per-pack baseline cache directory and load every record.
///
/// Returns `(pack, handler, filename, baseline)` tuples where
/// `filename` is the slash-separated relative path under the
/// pack-and-handler directory (matching what
/// [`cache_filename_for`](crate::preprocessing::baseline::cache_filename_for)
/// produces). The cache layout is
/// `<cache_dir>/preprocessor/<pack>/<handler>/<relative>.json`, with
/// `<relative>` mirroring the datastore layout — so we descend
/// recursively below the handler level. Missing or unreadable
/// subdirectories are skipped silently — the cache is rederivable,
/// and we never want a transient permission glitch to crash a check
/// run.
///
/// Unreadable baseline JSON files (corrupt content, schema mismatch)
/// are silently skipped here. Callers that need to surface those
/// errors to the user (e.g. `dodot transform check`) should use
/// [`collect_baselines_and_errors`] instead.
pub fn collect_baselines(
    fs: &dyn Fs,
    paths: &dyn Pather,
) -> Result<Vec<(String, String, String, Baseline)>> {
    collect_baselines_and_errors(fs, paths).map(|(entries, _)| entries)
}

/// Like [`collect_baselines`], but also returns any unreadable cache
/// entries encountered during the walk (corrupt JSON, schema-
/// version mismatch). Callers that participate in pre-commit /
/// pre-deployment correctness (`dodot transform check`,
/// `transform status`) use this so they can surface the broken
/// entries to the user instead of silently dropping them — a
/// silently-dropped corrupt entry would let `transform check`
/// "succeed" with an incomplete scan and let the pre-commit hook
/// miss real divergence.
#[allow(clippy::type_complexity)] // tuple return mirrors collect_baselines + adds error list
pub fn collect_baselines_and_errors(
    fs: &dyn Fs,
    paths: &dyn Pather,
) -> Result<(
    Vec<(String, String, String, Baseline)>,
    Vec<UnreadableBaseline>,
)> {
    let root = paths.cache_dir().join("preprocessor");
    if !fs.is_dir(&root) {
        return Ok((Vec::new(), Vec::new()));
    }

    let mut out = Vec::new();
    let mut errors: Vec<UnreadableBaseline> = Vec::new();
    let mut packs = match fs.read_dir(&root) {
        Ok(v) => v,
        Err(_) => return Ok((Vec::new(), Vec::new())),
    };
    packs.sort_by(|a, b| a.name.cmp(&b.name));

    for pack in packs {
        if !pack.is_dir {
            continue;
        }
        let mut handlers = match fs.read_dir(&pack.path) {
            Ok(v) => v,
            Err(_) => continue,
        };
        handlers.sort_by(|a, b| a.name.cmp(&b.name));

        for handler in handlers {
            if !handler.is_dir {
                continue;
            }
            // Recursively collect every `*.json` file under this
            // handler dir, recording its relative path (slash-joined)
            // so the cache key matches what `cache_filename_for`
            // produces.
            let mut filenames: Vec<String> = Vec::new();
            walk_baseline_dir(fs, &handler.path, "", &mut filenames);
            filenames.sort();
            for filename in filenames {
                match Baseline::load(fs, paths, &pack.name, &handler.name, &filename) {
                    Ok(Some(baseline)) => {
                        // Legacy-layout reconciliation: a flat
                        // (basename-only) cache entry from before
                        // PR-#118 can hold the baseline for a nested
                        // template (e.g. `subdir/config.toml.tmpl`)
                        // under just `config.toml.json`. If we
                        // surface that under its cache-key basename,
                        // every downstream consumer (`transform
                        // check`, `transform status`, `refresh`, the
                        // clean filter) derives the wrong deployed
                        // path and can't reconcile the file. Use the
                        // baseline's `source_path` to recover the
                        // correct virtual_relative — the source path
                        // is authoritative.
                        // Compute the resolved filename. For a flat
                        // cache entry whose source_path indicates a
                        // nested file, we'd normally override the
                        // key to the nested virtual_relative. But
                        // during the transient migration window
                        // BOTH the new nested file and the legacy
                        // basename file exist on disk — emitting
                        // both under the same logical key would
                        // produce duplicate rows in
                        // `transform status` / `transform check`.
                        // Detect that case and skip the legacy
                        // entry entirely; the canonical (new)
                        // entry will be emitted on its own pass.
                        let mut skip_emission = false;
                        let resolved_filename = if filename.contains('/') {
                            // Already nested; trust the cache layout.
                            filename
                        } else {
                            match derive_filename_from_source_path(
                                &baseline.source_path,
                                &pack.name,
                                paths.dotfiles_root(),
                            )
                            .filter(|derived| derived.contains('/'))
                            {
                                Some(derived) => {
                                    let canonical_path = paths.preprocessor_baseline_path(
                                        &pack.name,
                                        &handler.name,
                                        &derived,
                                    );
                                    // Drop the legacy duplicate
                                    // ONLY when the canonical
                                    // entry is actually loadable.
                                    // If the canonical exists but
                                    // its JSON is corrupt, keeping
                                    // the legacy gives downstream
                                    // tooling a usable fallback —
                                    // the canonical's parse error
                                    // is already surfaced to the
                                    // user as a `CacheErrorEntry`
                                    // by `collect_baselines_and_errors`,
                                    // so dropping the legacy here
                                    // would leave them with no
                                    // working baseline at all.
                                    let canonical_loadable = matches!(
                                        crate::preprocessing::baseline::read_baseline_at(
                                            fs,
                                            &canonical_path,
                                        ),
                                        Ok(Some(_)),
                                    );
                                    if canonical_loadable {
                                        skip_emission = true;
                                        filename
                                    } else {
                                        derived
                                    }
                                }
                                None => filename,
                            }
                        };
                        if !skip_emission {
                            out.push((
                                pack.name.clone(),
                                handler.name.clone(),
                                resolved_filename,
                                baseline,
                            ));
                        }
                    }
                    Ok(None) => {} // race with cache eviction; tolerate
                    Err(err) => {
                        // Record the unreadable entry so callers
                        // (specifically `transform check`) can
                        // surface it as a finding rather than
                        // silently completing with an incomplete
                        // scan. The walk continues — a single
                        // damaged entry must not break operations
                        // for unrelated entries — but the error is
                        // preserved in the parallel return list
                        // for the caller to react to.
                        let cache_path =
                            paths.preprocessor_baseline_path(&pack.name, &handler.name, &filename);
                        tracing::warn!(
                            pack = %pack.name,
                            handler = %handler.name,
                            file = %filename,
                            error = %err,
                            "unreadable baseline cache entry"
                        );
                        errors.push(UnreadableBaseline {
                            pack: pack.name.clone(),
                            handler: handler.name.clone(),
                            filename: filename.clone(),
                            cache_path,
                            error: err.to_string(),
                        });
                    }
                }
            }
        }
    }
    // The walker's per-handler sort produces a per-pack-and-handler
    // ordering, but resolved_filename overrides may have changed the
    // logical key of some entries (legacy basename → nested path).
    // Re-sort the full output so the final list is stable across
    // legacy / migrated state.
    out.sort_by(|a, b| (&a.0, &a.1, &a.2).cmp(&(&b.0, &b.1, &b.2)));
    errors.sort_by(|a, b| {
        (&a.pack, &a.handler, &a.filename).cmp(&(&b.pack, &b.handler, &b.filename))
    });

    Ok((out, errors))
}

/// Derive a virtual_relative cache key from a baseline's
/// `source_path` plus the pack name and dotfiles root. Used by
/// [`collect_baselines`] to reconcile legacy basename-only cache
/// entries with their true logical key (the nested virtual path).
///
/// Algorithm:
/// 1. Strip the `dotfiles_root` prefix from `source_path` to get a
///    pack-rooted relative path.
/// 2. Verify the first component matches `pack_name` (otherwise the
///    baseline doesn't belong to this pack — bail out).
/// 3. Drop the leading pack-name component.
/// 4. Strip a single trailing extension from the leaf (the
///    preprocessor extension: `.tmpl`, `.identity`, etc.).
/// 5. Return the remaining components joined with `/`.
///
/// Stripping by the explicit `dotfiles_root` + `pack_name` pair
/// (rather than searching for `pack_name` in the components) is
/// essential when a path component **inside** the pack happens to
/// share its name with the pack itself — e.g.
/// `/dotfiles/app/app/config.toml.tmpl` with pack `app` must yield
/// `app/config.toml`, not `config.toml`. A `rposition` /
/// first-match search would conflate the two.
///
/// Returns `None` for empty `source_path`, source path outside the
/// dotfiles root, mismatched pack name, or empty post-pack tail.
/// The walker treats `None` as "keep using the cache-derived
/// filename."
fn derive_filename_from_source_path(
    source_path: &std::path::Path,
    pack_name: &str,
    dotfiles_root: &std::path::Path,
) -> Option<String> {
    if source_path.as_os_str().is_empty() {
        return None;
    }
    let rel = source_path.strip_prefix(dotfiles_root).ok()?;
    let mut components: Vec<String> = rel
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    // First component must be the pack root.
    if components.first().map(String::as_str) != Some(pack_name) {
        return None;
    }
    components.remove(0);
    if components.is_empty() {
        return None;
    }
    if let Some(last) = components.last_mut() {
        if let Some(dot_idx) = last.rfind('.') {
            last.truncate(dot_idx);
        }
    }
    Some(components.join("/"))
}

/// Recursively walk a baseline-cache subtree, collecting the
/// slash-separated relative path of every `<name>.json` file (with
/// the `.json` suffix stripped). `relative_prefix` accumulates the
/// directory components leading to the current node; the empty
/// string represents the handler-dir root.
///
/// Matches the cache layout produced by
/// [`cache_filename_for`](crate::preprocessing::baseline::cache_filename_for):
/// the cache mirrors the datastore tree, so the walker recurses
/// rather than scanning a single flat layer.
fn walk_baseline_dir(
    fs: &dyn Fs,
    dir: &std::path::Path,
    relative_prefix: &str,
    out: &mut Vec<String>,
) {
    let entries = match fs.read_dir(dir) {
        Ok(v) => v,
        Err(_) => return,
    };
    for entry in entries {
        if entry.is_dir {
            let new_prefix = if relative_prefix.is_empty() {
                entry.name.clone()
            } else {
                format!("{}/{}", relative_prefix, entry.name)
            };
            walk_baseline_dir(fs, &entry.path, &new_prefix, out);
        } else if entry.is_file {
            let Some(stem) = entry.name.strip_suffix(".json") else {
                continue;
            };
            let full = if relative_prefix.is_empty() {
                stem.to_string()
            } else {
                format!("{}/{}", relative_prefix, stem)
            };
            out.push(full);
        }
    }
}

/// Classify a single baseline against the current state on disk.
///
/// The deployed-file path is derived from the datastore layout: a
/// preprocessor-expanded file lives at
/// `<data_dir>/packs/<pack>/<handler>/<filename>`. The user's
/// home-side symlink dereferences to this path, so reading the bytes
/// here is the same as reading what the user sees — the double-link
/// model means the deployed file *is* the file in the datastore.
pub fn classify_one(
    fs: &dyn Fs,
    paths: &dyn Pather,
    pack: &str,
    handler: &str,
    filename: &str,
    baseline: &Baseline,
) -> DivergenceReport {
    let source_path = baseline.source_path.clone();
    let deployed_path = paths
        .data_dir()
        .join("packs")
        .join(pack)
        .join(handler)
        .join(filename);

    let source_exists = !source_path.as_os_str().is_empty() && fs.exists(&source_path);
    let deployed_exists = fs.exists(&deployed_path);

    let state = if !source_exists {
        DivergenceState::MissingSource
    } else if !deployed_exists {
        DivergenceState::MissingDeployed
    } else {
        // Best-effort reads: if either side is unreadable mid-walk
        // (rare; e.g. a permissions hiccup), we fall back to "Synced"
        // rather than crashing the report. The caller can re-run.
        let source_changed = match fs.read_file(&source_path) {
            Ok(bytes) => hex_sha256(&bytes) != baseline.source_hash,
            Err(_) => false,
        };
        let deployed_changed = match fs.read_file(&deployed_path) {
            Ok(bytes) => hex_sha256(&bytes) != baseline.rendered_hash,
            Err(_) => false,
        };
        match (source_changed, deployed_changed) {
            (false, false) => DivergenceState::Synced,
            (true, false) => DivergenceState::InputChanged,
            (false, true) => DivergenceState::OutputChanged,
            (true, true) => DivergenceState::BothChanged,
        }
    };

    DivergenceReport {
        pack: pack.to_string(),
        handler: handler.to_string(),
        filename: filename.to_string(),
        source_path,
        deployed_path,
        state,
    }
}

/// Walk every cached baseline and produce a divergence report.
///
/// The report is sorted by `(pack, handler, filename)` so consumers can
/// rely on a stable display order without a second sort.
pub fn collect_divergences(fs: &dyn Fs, paths: &dyn Pather) -> Result<Vec<DivergenceReport>> {
    let baselines = collect_baselines(fs, paths)?;
    let reports: Vec<DivergenceReport> = baselines
        .iter()
        .map(|(p, h, f, b)| classify_one(fs, paths, p, h, f, b))
        .collect();
    Ok(reports)
}

/// Look up the baseline whose `source_path` matches `target`, plus
/// the `(pack, handler, filename)` triple that identifies it in the
/// cache layout.
///
/// Used by the clean filter (R6): git invokes the filter with the
/// source path of the file being processed, and the filter needs the
/// matching baseline to find the deployed bytes and the cached
/// tracked render. The lookup is a linear scan of the cache — fast
/// enough for the realistic per-repo template count (tens to low
/// hundreds), and avoids the on-disk index file the cache layout
/// would otherwise need.
///
/// Returns `Ok(None)` when no baseline matches; the clean filter
/// treats that as "echo stdin unchanged" rather than an error.
pub fn find_baseline_for_source(
    fs: &dyn Fs,
    paths: &dyn Pather,
    target: &std::path::Path,
) -> Result<Option<(String, String, String, Baseline)>> {
    for (pack, handler, filename, baseline) in collect_baselines(fs, paths)? {
        if baseline.source_path == target {
            return Ok(Some((pack, handler, filename, baseline)));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempEnvironment;

    fn write_pack_template(env: &TempEnvironment, pack: &str, name: &str, body: &str) {
        let path = env.dotfiles_root.join(pack).join(name);
        env.fs.mkdir_all(path.parent().unwrap()).unwrap();
        env.fs.write_file(&path, body.as_bytes()).unwrap();
    }

    fn write_deployed(env: &TempEnvironment, pack: &str, handler: &str, name: &str, body: &str) {
        let path = env
            .paths
            .data_dir()
            .join("packs")
            .join(pack)
            .join(handler)
            .join(name);
        env.fs.mkdir_all(path.parent().unwrap()).unwrap();
        env.fs.write_file(&path, body.as_bytes()).unwrap();
    }

    fn baseline_for(source_path: &std::path::Path, rendered: &[u8], source: &[u8]) -> Baseline {
        Baseline::build(source_path, rendered, source, Some(""), None)
    }

    #[test]
    fn empty_cache_yields_empty_report() {
        let env = TempEnvironment::builder().build();
        let reports = collect_divergences(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert!(reports.is_empty());
    }

    #[test]
    fn synced_state_when_nothing_changed() {
        // Baseline + source bytes + deployed bytes all match.
        let env = TempEnvironment::builder().build();
        write_pack_template(&env, "app", "config.toml.tmpl", "src");
        write_deployed(&env, "app", "preprocessed", "config.toml", "rendered");
        let src_path = env.dotfiles_root.join("app/config.toml.tmpl");
        let baseline = baseline_for(&src_path, b"rendered", b"src");
        baseline
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "config.toml",
            )
            .unwrap();

        let reports = collect_divergences(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].state, DivergenceState::Synced);
    }

    #[test]
    fn input_changed_when_source_edited() {
        // Source bytes diverge from baseline; deployed bytes still
        // match. The next `up` will re-render — `transform check`
        // takes no action here.
        let env = TempEnvironment::builder().build();
        write_pack_template(&env, "app", "config.toml.tmpl", "src EDITED");
        write_deployed(&env, "app", "preprocessed", "config.toml", "rendered");
        let src_path = env.dotfiles_root.join("app/config.toml.tmpl");
        let baseline = baseline_for(&src_path, b"rendered", b"src");
        baseline
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "config.toml",
            )
            .unwrap();

        let reports = collect_divergences(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert_eq!(reports[0].state, DivergenceState::InputChanged);
    }

    #[test]
    fn output_changed_when_deployed_edited() {
        // The auto-merge happy path: only the deployed file moved.
        let env = TempEnvironment::builder().build();
        write_pack_template(&env, "app", "config.toml.tmpl", "src");
        write_deployed(&env, "app", "preprocessed", "config.toml", "rendered EDIT");
        let src_path = env.dotfiles_root.join("app/config.toml.tmpl");
        let baseline = baseline_for(&src_path, b"rendered", b"src");
        baseline
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "config.toml",
            )
            .unwrap();

        let reports = collect_divergences(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert_eq!(reports[0].state, DivergenceState::OutputChanged);
    }

    #[test]
    fn both_changed_when_both_edited() {
        let env = TempEnvironment::builder().build();
        write_pack_template(&env, "app", "config.toml.tmpl", "src EDIT");
        write_deployed(&env, "app", "preprocessed", "config.toml", "rendered EDIT");
        let src_path = env.dotfiles_root.join("app/config.toml.tmpl");
        let baseline = baseline_for(&src_path, b"rendered", b"src");
        baseline
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "config.toml",
            )
            .unwrap();

        let reports = collect_divergences(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert_eq!(reports[0].state, DivergenceState::BothChanged);
    }

    #[test]
    fn missing_source_when_pack_file_deleted() {
        // Baseline points at a source path that's been removed (e.g.
        // the user renamed or deleted the template). Surfaced as a
        // distinct state so callers can offer to drop the stale
        // baseline.
        let env = TempEnvironment::builder().build();
        write_deployed(&env, "app", "preprocessed", "config.toml", "rendered");
        let baseline = baseline_for(
            &env.dotfiles_root.join("app/config.toml.tmpl"),
            b"rendered",
            b"src",
        );
        baseline
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "config.toml",
            )
            .unwrap();

        let reports = collect_divergences(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert_eq!(reports[0].state, DivergenceState::MissingSource);
    }

    #[test]
    fn missing_deployed_when_datastore_file_gone() {
        let env = TempEnvironment::builder().build();
        write_pack_template(&env, "app", "config.toml.tmpl", "src");
        let src_path = env.dotfiles_root.join("app/config.toml.tmpl");
        let baseline = baseline_for(&src_path, b"rendered", b"src");
        baseline
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "config.toml",
            )
            .unwrap();
        // Deliberately do NOT write the deployed file.

        let reports = collect_divergences(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert_eq!(reports[0].state, DivergenceState::MissingDeployed);
    }

    #[test]
    fn report_is_sorted_by_pack_handler_filename() {
        // Two packs with two files each, registered in non-sorted
        // order. The walker must surface them in (pack, handler,
        // filename) order so display layers don't need a second sort.
        let env = TempEnvironment::builder().build();
        for (pack, name, body) in [
            ("zebra", "z.toml.tmpl", "z-src"),
            ("alpha", "b.toml.tmpl", "b-src"),
            ("alpha", "a.toml.tmpl", "a-src"),
        ] {
            write_pack_template(&env, pack, name, body);
            let cache_name = name.strip_suffix(".tmpl").unwrap();
            write_deployed(&env, pack, "preprocessed", cache_name, "rendered");
            let src_path = env.dotfiles_root.join(pack).join(name);
            let baseline = baseline_for(&src_path, b"rendered", body.as_bytes());
            baseline
                .write(
                    env.fs.as_ref(),
                    env.paths.as_ref(),
                    pack,
                    "preprocessed",
                    cache_name,
                )
                .unwrap();
        }

        let reports = collect_divergences(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        let order: Vec<_> = reports
            .iter()
            .map(|r| (r.pack.clone(), r.filename.clone()))
            .collect();
        assert_eq!(
            order,
            vec![
                ("alpha".into(), "a.toml".into()),
                ("alpha".into(), "b.toml".into()),
                ("zebra".into(), "z.toml".into()),
            ]
        );
    }

    #[test]
    fn baseline_with_empty_source_path_is_classified_missing_source() {
        // Forward-compat with v1 baselines written before source_path
        // existed: serde-default fills in an empty PathBuf, and the
        // classifier reports MissingSource so the user sees the issue
        // and re-runs `dodot up` to rebuild the cache.
        let env = TempEnvironment::builder().build();
        write_deployed(&env, "app", "preprocessed", "config.toml", "rendered");
        let baseline = baseline_for(std::path::Path::new(""), b"rendered", b"src");
        baseline
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "config.toml",
            )
            .unwrap();

        let reports = collect_divergences(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert_eq!(reports[0].state, DivergenceState::MissingSource);
    }

    // ── Walker legacy-layout reconciliation (PR #118 8th-pass) ──────

    #[test]
    fn collect_baselines_recovers_nested_key_for_legacy_basename_entry() {
        // PR #118 8th-pass: an upgraded user has a legacy
        // basename-only cache entry whose source_path indicates a
        // nested template. The walker must surface it under the
        // *nested* key, not under its flat cache-file basename, so
        // `transform check` / `status` / `refresh` derive the right
        // deployed path.
        let env = TempEnvironment::builder().build();

        // Stage the deployed file at the NESTED path (mirroring
        // the datastore layout).
        write_deployed(
            &env,
            "app",
            "preprocessed",
            "subdir/config.toml",
            "rendered",
        );
        // Create the source template at the NESTED location too.
        write_pack_template(&env, "app", "subdir/config.toml.tmpl", "src");

        // Write the baseline at the LEGACY basename-only cache path.
        let src_path = env.dotfiles_root.join("app/subdir/config.toml.tmpl");
        let baseline = baseline_for(&src_path, b"rendered", b"src");
        baseline
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "config.toml", // legacy: basename only
            )
            .unwrap();

        let baselines = collect_baselines(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert_eq!(baselines.len(), 1);
        let (pack, handler, filename, _) = &baselines[0];
        assert_eq!(pack, "app");
        assert_eq!(handler, "preprocessed");
        assert_eq!(
            filename, "subdir/config.toml",
            "walker must recover the nested virtual_relative key from baseline.source_path"
        );

        // And the resulting divergence report points at the
        // correct deployed path (which exists in the datastore).
        let reports = collect_divergences(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].state, DivergenceState::Synced);
    }

    #[test]
    fn collect_baselines_keeps_basename_key_for_genuine_top_level_entry() {
        // Symmetric to the legacy-recovery case: a top-level cache
        // entry whose source_path is also top-level must NOT be
        // re-keyed. Only basename-only entries pointing at a *nested*
        // source get the override.
        let env = TempEnvironment::builder().build();
        write_pack_template(&env, "app", "config.toml.tmpl", "src");
        write_deployed(&env, "app", "preprocessed", "config.toml", "rendered");
        let src_path = env.dotfiles_root.join("app/config.toml.tmpl");
        let baseline = baseline_for(&src_path, b"rendered", b"src");
        baseline
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "config.toml",
            )
            .unwrap();

        let baselines = collect_baselines(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert_eq!(baselines.len(), 1);
        assert_eq!(baselines[0].2, "config.toml");
    }

    #[test]
    fn collect_baselines_does_not_override_when_source_path_is_empty() {
        // v1 baselines written before `source_path` existed
        // serde-default to empty PathBuf. The walker can't recover
        // anything from an empty path, so it should keep the
        // cache-derived filename (basename) and let the existing
        // MissingSource handling kick in downstream.
        let env = TempEnvironment::builder().build();
        write_deployed(&env, "app", "preprocessed", "config.toml", "rendered");
        let baseline = baseline_for(std::path::Path::new(""), b"rendered", b"src");
        baseline
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "config.toml",
            )
            .unwrap();

        let baselines = collect_baselines(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert_eq!(baselines.len(), 1);
        assert_eq!(baselines[0].2, "config.toml");
    }

    #[test]
    fn collect_baselines_skips_canonical_when_legacy_duplicate_exists() {
        // PR #118 11th-pass Comment S: during the transient
        // migration window where BOTH the new nested baseline and
        // the legacy basename file exist on disk, the walker must
        // not emit both as separate entries with the same logical
        // filename — that produces duplicate rows in
        // `transform status`. The legacy entry gets dropped; only
        // the canonical (new) entry surfaces.
        let env = TempEnvironment::builder().build();
        write_pack_template(&env, "app", "subdir/config.toml.tmpl", "src");
        write_deployed(
            &env,
            "app",
            "preprocessed",
            "subdir/config.toml",
            "rendered",
        );
        let src_path = env.dotfiles_root.join("app/subdir/config.toml.tmpl");

        // Stage BOTH cache files: new nested layout + legacy basename.
        let canonical = baseline_for(&src_path, b"rendered", b"src");
        canonical
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "subdir/config.toml",
            )
            .unwrap();
        // The above write would normally migrate; force the legacy
        // file back into existence to simulate the transient window
        // where both coexist (e.g. interrupted migration).
        let legacy = baseline_for(&src_path, b"rendered", b"src");
        legacy
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "config.toml",
            )
            .unwrap();

        let baselines = collect_baselines(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        // Only the canonical entry surfaces; the legacy duplicate
        // is suppressed.
        assert_eq!(
            baselines.len(),
            1,
            "transient duplicate must be deduped — got: {:?}",
            baselines.iter().map(|b| &b.2).collect::<Vec<_>>()
        );
        assert_eq!(baselines[0].2, "subdir/config.toml");
    }

    #[test]
    fn collect_baselines_keeps_legacy_when_canonical_is_corrupt() {
        // PR #118 13th-pass Comment AA: dedup must verify the
        // canonical entry can actually be LOADED, not just that
        // it exists on disk. If the canonical is present but
        // corrupt while the legacy basename file is valid, the
        // legacy entry must surface so downstream tooling has a
        // usable fallback during the migration window.
        let env = TempEnvironment::builder().build();
        write_pack_template(&env, "app", "subdir/config.toml.tmpl", "src");
        write_deployed(
            &env,
            "app",
            "preprocessed",
            "subdir/config.toml",
            "rendered",
        );
        let src_path = env.dotfiles_root.join("app/subdir/config.toml.tmpl");

        // Stage the legacy basename entry as a VALID baseline.
        let legacy = baseline_for(&src_path, b"rendered", b"src");
        legacy
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "config.toml",
            )
            .unwrap();

        // Stage the canonical (new-layout) entry as CORRUPT JSON.
        let canonical_path =
            env.paths
                .preprocessor_baseline_path("app", "preprocessed", "subdir/config.toml");
        env.fs.mkdir_all(canonical_path.parent().unwrap()).unwrap();
        env.fs
            .write_file(&canonical_path, b"{not valid json")
            .unwrap();

        // The walker must surface the legacy entry under its
        // resolved nested key, NOT skip it just because a corrupt
        // canonical file exists. The corrupt canonical is recorded
        // as a cache error.
        let (baselines, errors) =
            collect_baselines_and_errors(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert_eq!(
            baselines.len(),
            1,
            "legacy entry must be preserved when canonical is corrupt"
        );
        assert_eq!(baselines[0].2, "subdir/config.toml");
        assert_eq!(baselines[0].3.rendered_content, "rendered");
        // Corrupt canonical surfaces as a cache error.
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].filename, "subdir/config.toml");
    }

    #[test]
    fn collect_baselines_soft_fails_on_corrupt_entry() {
        // PR #118 11th-pass: a corrupt cache entry must NOT abort
        // the entire walk. Earlier behavior propagated the parse
        // error from `Baseline::load`, breaking every
        // `transform check` / `status` / `refresh` until the user
        // manually cleared the cache. The walker now skips and
        // logs.
        let env = TempEnvironment::builder().build();
        // One healthy entry.
        write_pack_template(&env, "app", "good.toml.tmpl", "src");
        write_deployed(&env, "app", "preprocessed", "good.toml", "rendered");
        let src_path = env.dotfiles_root.join("app/good.toml.tmpl");
        let healthy = baseline_for(&src_path, b"rendered", b"src");
        healthy
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "good.toml",
            )
            .unwrap();

        // One corrupt entry alongside.
        let bad_path = env
            .paths
            .preprocessor_baseline_path("app", "preprocessed", "bad.toml");
        env.fs.write_file(&bad_path, b"{not json").unwrap();

        // Walker must succeed with the healthy entry; corrupt entry
        // is skipped from the success list. The legacy soft-fail
        // (collect_baselines) returns just the healthy entry.
        let baselines = collect_baselines(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert_eq!(baselines.len(), 1);
        assert_eq!(baselines[0].2, "good.toml");
    }

    #[test]
    fn collect_baselines_and_errors_surfaces_corrupt_entries() {
        // PR #118 12th-pass Comment W: callers that participate in
        // pre-commit / pre-deployment correctness need to see the
        // corrupt entries — silently dropping them lets
        // `transform check` succeed with an incomplete scan.
        // `collect_baselines_and_errors` returns both lists so
        // callers can react.
        let env = TempEnvironment::builder().build();
        write_pack_template(&env, "app", "good.toml.tmpl", "src");
        write_deployed(&env, "app", "preprocessed", "good.toml", "rendered");
        let src_path = env.dotfiles_root.join("app/good.toml.tmpl");
        let healthy = baseline_for(&src_path, b"rendered", b"src");
        healthy
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "good.toml",
            )
            .unwrap();

        let bad_path = env
            .paths
            .preprocessor_baseline_path("app", "preprocessed", "bad.toml");
        env.fs.write_file(&bad_path, b"{not json").unwrap();

        let (baselines, errors) =
            collect_baselines_and_errors(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert_eq!(baselines.len(), 1);
        assert_eq!(baselines[0].2, "good.toml");
        assert_eq!(
            errors.len(),
            1,
            "corrupt entry must be surfaced via the errors list"
        );
        assert_eq!(errors[0].pack, "app");
        assert_eq!(errors[0].handler, "preprocessed");
        assert_eq!(errors[0].filename, "bad.toml");
        assert_eq!(errors[0].cache_path, bad_path);
        assert!(
            errors[0].error.contains("failed to parse"),
            "error string should describe the parse failure, got: {:?}",
            errors[0].error
        );
    }

    #[test]
    fn derive_filename_from_source_path_extracts_nested_path() {
        assert_eq!(
            derive_filename_from_source_path(
                std::path::Path::new("/dotfiles/app/subdir/config.toml.tmpl"),
                "app",
                std::path::Path::new("/dotfiles"),
            ),
            Some("subdir/config.toml".to_string())
        );
        assert_eq!(
            derive_filename_from_source_path(
                std::path::Path::new("/home/user/dotfiles/pkg/a/b/leaf.txt.identity"),
                "pkg",
                std::path::Path::new("/home/user/dotfiles"),
            ),
            Some("a/b/leaf.txt".to_string())
        );
    }

    #[test]
    fn derive_filename_from_source_path_handles_pack_named_subdir_collision() {
        // Comment Q: when a nested directory has the same name as
        // the pack, an `rposition` search would pick the WRONG
        // pack root. The fix relies on `dotfiles_root` to strip
        // the prefix unambiguously: `/dotfiles/app/app/config.toml`
        // with pack `app` and dotfiles_root `/dotfiles` gives the
        // post-strip path `app/app/config.toml.tmpl`, peel the
        // leading `app` (pack root) → `app/config.toml`. NOT
        // `config.toml` (which a rposition search would produce).
        assert_eq!(
            derive_filename_from_source_path(
                std::path::Path::new("/dotfiles/app/app/config.toml.tmpl"),
                "app",
                std::path::Path::new("/dotfiles"),
            ),
            Some("app/config.toml".to_string())
        );
    }

    #[test]
    fn derive_filename_from_source_path_returns_top_level_for_pack_root_file() {
        // Source is the pack's top-level file: post-pack tail is
        // a single component. The helper still returns it; the
        // walker filter (`derived.contains('/')`) decides whether
        // an override applies.
        assert_eq!(
            derive_filename_from_source_path(
                std::path::Path::new("/dotfiles/app/config.toml.tmpl"),
                "app",
                std::path::Path::new("/dotfiles"),
            ),
            Some("config.toml".to_string())
        );
    }

    #[test]
    fn derive_filename_from_source_path_returns_none_for_path_outside_dotfiles_root() {
        // Source path doesn't live under dotfiles_root (unusual /
        // moved repo). Returns None — walker keeps cache-derived
        // filename.
        assert_eq!(
            derive_filename_from_source_path(
                std::path::Path::new("/elsewhere/pkg/config.toml.tmpl"),
                "pkg",
                std::path::Path::new("/dotfiles"),
            ),
            None
        );
    }

    #[test]
    fn derive_filename_from_source_path_returns_none_for_missing_pack() {
        // Pack name doesn't appear at the right position (unusual /
        // moved pack). Helper returns None — walker keeps cache-
        // derived filename.
        assert_eq!(
            derive_filename_from_source_path(
                std::path::Path::new("/dotfiles/other-pack/config.toml.tmpl"),
                "app",
                std::path::Path::new("/dotfiles"),
            ),
            None
        );
    }

    // ── find_baseline_for_source ────────────────────────────────

    #[test]
    fn find_baseline_for_source_returns_match() {
        // Stage two baselines with distinct source paths; the lookup
        // must return only the one whose `source_path` matches.
        let env = TempEnvironment::builder().build();
        let src_a = env.dotfiles_root.join("app/a.toml.tmpl");
        write_pack_template(&env, "app", "a.toml.tmpl", "src-a");
        write_deployed(&env, "app", "preprocessed", "a.toml", "rendered-a");
        baseline_for(&src_a, b"rendered-a", b"src-a")
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "a.toml",
            )
            .unwrap();

        let src_b = env.dotfiles_root.join("app/b.toml.tmpl");
        write_pack_template(&env, "app", "b.toml.tmpl", "src-b");
        write_deployed(&env, "app", "preprocessed", "b.toml", "rendered-b");
        baseline_for(&src_b, b"rendered-b", b"src-b")
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "b.toml",
            )
            .unwrap();

        let hit = find_baseline_for_source(env.fs.as_ref(), env.paths.as_ref(), &src_a).unwrap();
        let (pack, handler, filename, baseline) = hit.expect("baseline must be found");
        assert_eq!(pack, "app");
        assert_eq!(handler, "preprocessed");
        assert_eq!(filename, "a.toml");
        assert_eq!(baseline.source_path, src_a);
        assert_eq!(baseline.rendered_content, "rendered-a");
    }

    #[test]
    fn find_baseline_for_source_returns_none_when_unknown() {
        // Path the cache has never seen → Ok(None). The clean
        // filter treats this as "echo stdin unchanged", so the
        // None case is part of the normal contract, not an error.
        let env = TempEnvironment::builder().build();
        let unknown = env.dotfiles_root.join("never-cached.tmpl");
        let result =
            find_baseline_for_source(env.fs.as_ref(), env.paths.as_ref(), &unknown).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn find_baseline_for_source_on_empty_cache_returns_none() {
        // No baselines on disk at all (e.g. user has never run
        // `dodot up`) → Ok(None), not an error.
        let env = TempEnvironment::builder().build();
        let any = env.dotfiles_root.join("anything.tmpl");
        let result = find_baseline_for_source(env.fs.as_ref(), env.paths.as_ref(), &any).unwrap();
        assert!(result.is_none());
    }
}

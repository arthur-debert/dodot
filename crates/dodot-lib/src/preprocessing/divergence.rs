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

/// Walk the per-pack baseline cache directory and load every record.
///
/// Returns `(pack, handler, filename, baseline)` tuples. The cache
/// layout is `<cache_dir>/preprocessor/<pack>/<handler>/<filename>.json`,
/// so this function is a 3-level read_dir walk. Missing or unreadable
/// subdirectories are skipped silently — the cache is rederivable, and
/// we never want a transient permission glitch to crash a check run.
pub fn collect_baselines(
    fs: &dyn Fs,
    paths: &dyn Pather,
) -> Result<Vec<(String, String, String, Baseline)>> {
    let root = paths.cache_dir().join("preprocessor");
    if !fs.is_dir(&root) {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    let mut packs = match fs.read_dir(&root) {
        Ok(v) => v,
        Err(_) => return Ok(Vec::new()),
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
            let mut files = match fs.read_dir(&handler.path) {
                Ok(v) => v,
                Err(_) => continue,
            };
            files.sort_by(|a, b| a.name.cmp(&b.name));

            for file in files {
                if !file.is_file {
                    continue;
                }
                // Filenames in the cache are `<logical>.json` for
                // baselines and `<logical>.secret.json` for sidecars
                // (`secrets.lex` §3.3). Skip sidecars so they don't
                // get fed into Baseline::load — they have a separate
                // schema. Their content is loaded on demand by
                // SecretsSidecar::load(...) keyed off the baseline's
                // logical name.
                if file.name.ends_with(".secret.json") {
                    continue;
                }
                let Some(filename) = file.name.strip_suffix(".json").map(str::to_string) else {
                    continue;
                };
                match Baseline::load(fs, paths, &pack.name, &handler.name, &filename) {
                    Ok(Some(baseline)) => {
                        out.push((pack.name.clone(), handler.name.clone(), filename, baseline));
                    }
                    // A corrupt baseline gets surfaced as an error
                    // here so the user knows to clear it; better than
                    // silently dropping it from the report.
                    Ok(None) => {} // unreachable when fs.is_file is true, but tolerate
                    Err(e) => return Err(e),
                }
            }
        }
    }

    Ok(out)
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
    fn collect_baselines_skips_secret_sidecars() {
        // The Phase S2 sidecar (`<filename>.secret.json`) lives in
        // the same handler dir as baselines and shares the `.json`
        // suffix. Pin that the collector skips it instead of trying
        // to parse it as a baseline (which fails with a confusing
        // "missing field rendered_hash" error).
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
        // Drop a sidecar next to it.
        let sidecar = crate::preprocessing::baseline::SecretsSidecar::new(vec![
            crate::preprocessing::SecretLineRange {
                start: 0,
                end: 1,
                reference: "pass:k".into(),
            },
        ]);
        sidecar
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "config.toml",
            )
            .unwrap();

        let baselines = collect_baselines(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        // Exactly one entry — the baseline. The sidecar is skipped.
        assert_eq!(baselines.len(), 1);
        assert_eq!(baselines[0].2, "config.toml");
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

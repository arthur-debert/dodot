//! `dodot transform check` — propagate deployed-file edits back to
//! template sources via the cached baseline + reverse-merge pipeline.
//!
//! Reads every per-file baseline under `<cache_dir>/preprocessor/`,
//! classifies each entry against the 4-state matrix from
//! `docs/proposals/preprocessing-pipeline.lex` §6.1, and acts on each
//! state:
//!
//! | state            | action                                              |
//! |------------------|-----------------------------------------------------|
//! | `Synced`         | nothing (no divergence)                             |
//! | `InputChanged`   | nothing (next `dodot up` re-renders)                |
//! | `OutputChanged`  | reverse-merge into source; clean diff → write back  |
//! | `BothChanged`    | reverse-merge into source; conflict → report       |
//! | `MissingSource`  | report only (cache stale; next `up` will refresh)   |
//! | `MissingDeployed`| report only (deployed file gone; manual recovery)   |
//!
//! A baseline whose source does not lie inside the authorized root never
//! reaches that matrix — see "One cache, one root" below.
//!
//! For `OutputChanged` and `BothChanged`, the call into burgertocow
//! returns either a clean unified diff (which is applied to the source
//! file via `diffy`) or a conflict block (which is *not* written —
//! instead surfaced in the report so the user resolves it manually).
//! The intent: `transform check` only mutates source files when the
//! reverse-merge is unambiguous, and surfaces every other case for
//! human review.
//!
//! # Strict mode
//!
//! `check(ctx, strict=true)` is the form used by the pre-commit hook.
//! On top of the matrix work above, it scans every authorized source
//! file for unresolved [`crate::preprocessing::conflict`] markers — if
//! any are found, the result reports them and the command exits non-zero
//! so a commit is blocked until the user resolves them.
//!
//! # One cache, one root
//!
//! The baseline cache is shared across every dotfiles root on the
//! machine, and reverse-merge writes each baseline's stored source path.
//! Approval for one root must not authorize a patch into another root's
//! user-authored source, so the mutation set is scoped to the root this
//! invocation is authorized for: only sources that canonically lie
//! inside it are classified, patched, or scanned, and every other
//! baseline is reported
//! (`docs/adr/0002-guard-root-derived-mutations.md`). Strict mode reuses
//! that same authorized set rather than re-walking the cache, so the
//! marker scan cannot reach a file the patching pass was not allowed to
//! touch.

use serde::Serialize;

use crate::packs::orchestration::ExecutionContext;
use crate::preprocessing::conflict::find_unresolved_marker_lines;
use crate::preprocessing::divergence::{
    classify_one, collect_baselines, DivergenceReport, DivergenceState,
};
use crate::preprocessing::no_reverse::is_no_reverse;
use crate::preprocessing::reverse_merge::{reverse_merge, ReverseMergeOutcome};
use crate::safety_lock::{scope_to_root, OsPathProbe, OutOfRootReason, RootIdentity, ScopeOutcome};
use crate::Result;

/// What `transform check` did to a single processed file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransformAction {
    /// Source and deployed match the baseline — no action.
    Synced,
    /// Source has been edited; next `dodot up` will re-render.
    InputChanged,
    /// The reverse-merge produced a clean unified diff and the source
    /// file was patched in place.
    Patched,
    /// The reverse-merge surfaced a conflict block; the source file is
    /// left untouched. The user resolves it manually.
    Conflict,
    /// Reverse-merge declined to act (e.g. cached `tracked_render` was
    /// empty — typically a v1 baseline written before this field
    /// existed). Re-run `dodot up` to refresh the baseline.
    NeedsRebaseline,
    /// The cached source path no longer exists on disk.
    MissingSource,
    /// The deployed file is gone from the datastore.
    MissingDeployed,
    /// The cached source belongs to a different dotfiles root. Reported;
    /// this invocation is not authorized to patch it.
    OutOfRoot,
    /// The baseline records no absolute source path at all — the stored
    /// path is empty (an entry written before the cache tracked source
    /// paths) or relative (meaningless without the process working
    /// directory, which Safety Lock does not read). Reported; the next
    /// `dodot up` rewrites it.
    StaleSource,
    /// The cached source exists but could not be resolved, so it cannot
    /// be placed inside the root. Reported, never patched.
    UnresolvableSource,
}

impl TransformAction {
    /// Name the outcome for a baseline that stayed out of the mutation
    /// set, and say whether it is a finding (i.e. a non-zero exit).
    ///
    /// One action per reason rather than a single "not mine" bucket: a
    /// source under another root, a deleted source, a source path the
    /// cache never recorded, and one Dodot cannot resolve are four
    /// different states of the user's machine with four different fixes
    /// — and only three of them are this repository's problem.
    fn out_of_root(reason: OutOfRootReason) -> (Self, bool) {
        match reason {
            // A baseline belonging to *another* root is deliberately not
            // a finding: two roots sharing one cache is a supported
            // arrangement, and the pre-commit hook installed in one
            // repository must not start refusing commits because a
            // sibling repository also uses Dodot.
            OutOfRootReason::OutsideRoot => (TransformAction::OutOfRoot, false),
            // The rest are cache health, and stay findings exactly as a
            // missing source always has.
            OutOfRootReason::Missing => (TransformAction::MissingSource, true),
            OutOfRootReason::Stale => (TransformAction::StaleSource, true),
            OutOfRootReason::Uncanonicalizable => (TransformAction::UnresolvableSource, true),
        }
    }
}

/// One row in the transform-check report.
#[derive(Debug, Clone, Serialize)]
pub struct TransformCheckEntry {
    pub pack: String,
    pub handler: String,
    pub filename: String,
    pub source_path: String,
    pub deployed_path: String,
    pub action: TransformAction,
    /// For `Conflict`: the burgertocow-emitted block, ready for the
    /// CLI layer to print. Empty for other actions.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub conflict_block: String,
}

/// One unresolved-marker hit found in `--strict` mode. Path-and-line
/// granularity, identical in shape to what the pipeline gate reports.
#[derive(Debug, Clone, Serialize)]
pub struct UnresolvedMarkerEntry {
    pub source_path: String,
    pub line_numbers: Vec<usize>,
}

/// Aggregate outcome of a `transform check` invocation.
#[derive(Debug, Clone, Serialize)]
pub struct TransformCheckResult {
    pub entries: Vec<TransformCheckEntry>,
    /// Populated only when `strict = true` and at least one source
    /// carries unresolved dodot-conflict markers.
    pub unresolved_markers: Vec<UnresolvedMarkerEntry>,
    /// True iff at least one entry has a non-clean state that should
    /// make the command exit non-zero (Conflict, NeedsRebaseline,
    /// MissingSource, MissingDeployed, StaleSource, UnresolvableSource)
    /// or `--strict` found unresolved markers. CLI uses this to decide
    /// the process exit code.
    ///
    /// `OutOfRoot` does *not* set this, and is the one out-of-mutation-set
    /// action that does not: a baseline belonging to another root is a
    /// supported arrangement of two roots sharing one cache, so it is
    /// reported and nothing more. The other three — `MissingSource`,
    /// `StaleSource`, `UnresolvableSource` — are this cache's health and
    /// stay findings. [`TransformAction::out_of_root`] is where that split
    /// is decided.
    ///
    /// `Patched` does *not* set this — an unambiguous reverse-merge is
    /// the auto-merge happy path: burgertocow + diffy produced a clean
    /// unified patch with no markers, the source has been rewritten
    /// to match, and there's nothing for the user to review. The
    /// pre-commit hook lets the original `git commit` proceed; the
    /// patched source surfaces as modified on the next `git status`,
    /// at which point the user `git add`s and commits a follow-up
    /// (or amends) if they want a clean history. Issue #113 walks
    /// through the rationale.
    pub has_findings: bool,
    pub strict: bool,
}

impl TransformCheckResult {
    /// Process exit code per the spec: 0 if everything is clean, 1
    /// otherwise. Strict-mode unresolved markers also flip this to 1.
    pub fn exit_code(&self) -> i32 {
        if self.has_findings {
            1
        } else {
            0
        }
    }
}

/// One row in `dodot transform status`'s passive report.
///
/// Mirrors `TransformCheckEntry` but without any of the action /
/// conflict-block fields — `status` is a read-only inspection;
/// `check` is the action layer.
#[derive(Debug, Clone, Serialize)]
pub struct TransformStatusEntry {
    pub pack: String,
    pub handler: String,
    pub filename: String,
    pub source_path: String,
    pub deployed_path: String,
    /// Mirror of `DivergenceState`, serialised as snake_case so the
    /// template branches and JSON consumers see the same shape they
    /// see in `transform check`.
    #[serde(rename = "state")]
    pub state: String,
    /// References this file resolved through `secret(...)` on its
    /// last successful render. Populated from
    /// `<baseline>.secret.json` (per `secrets.lex` §3.3); empty
    /// when the file has no sidecar (the common case for templates
    /// that don't use secrets, and for baselines written before
    /// sidecar tracking existed). Surfaced in the rendered status so
    /// users can see *which* secret references each baseline depends
    /// on without re-rendering. JSON consumers see the same field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secret_references: Vec<String>,
}

/// Aggregate result of `dodot transform status` — one row per
/// cached baseline, plus a few rollup counters for the renderer.
#[derive(Debug, Clone, Serialize)]
pub struct TransformStatusResult {
    pub entries: Vec<TransformStatusEntry>,
    pub synced_count: usize,
    pub diverged_count: usize,
    pub missing_count: usize,
}

/// Run `dodot transform status` — read-only view of the baseline
/// cache. Walks every cached entry and reports its state without
/// running the reverse-merge engine, writing source files, or doing
/// anything else that mutates state. Useful as a "what's currently
/// out of sync?" check before deciding whether to run `dodot transform
/// check`. Always exits 0 — even a fully-diverged repo isn't a
/// failure here, just information.
pub fn status(ctx: &ExecutionContext) -> Result<TransformStatusResult> {
    use crate::preprocessing::divergence::{collect_divergences, DivergenceState};
    let reports = collect_divergences(ctx.fs.as_ref(), ctx.paths.as_ref())?;
    let mut synced_count = 0usize;
    let mut diverged_count = 0usize;
    let mut missing_count = 0usize;
    let entries: Vec<TransformStatusEntry> = reports
        .into_iter()
        .map(|r| {
            let state_str = match r.state {
                DivergenceState::Synced => {
                    synced_count += 1;
                    "synced"
                }
                DivergenceState::InputChanged => {
                    diverged_count += 1;
                    "input_changed"
                }
                DivergenceState::OutputChanged => {
                    diverged_count += 1;
                    "output_changed"
                }
                DivergenceState::BothChanged => {
                    diverged_count += 1;
                    "both_changed"
                }
                DivergenceState::MissingSource => {
                    missing_count += 1;
                    "missing_source"
                }
                DivergenceState::MissingDeployed => {
                    missing_count += 1;
                    "missing_deployed"
                }
            };
            // Sidecar reads are best-effort: a parse error
            // shouldn't fail the whole status report, just leave
            // this row's secret_references empty. The user can
            // re-render to fix the sidecar via `dodot up
            // --force` separately.
            let secret_references = crate::preprocessing::baseline::SecretsSidecar::load(
                ctx.fs.as_ref(),
                ctx.paths.as_ref(),
                &r.pack,
                &r.handler,
                &r.filename,
            )
            .ok()
            .flatten()
            .map(|s| {
                s.secret_line_ranges
                    .into_iter()
                    .map(|range| range.reference)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
            TransformStatusEntry {
                pack: r.pack,
                handler: r.handler,
                filename: r.filename,
                source_path: render_path(&r.source_path, ctx.paths.home_dir()),
                deployed_path: render_path(&r.deployed_path, ctx.paths.home_dir()),
                state: state_str.to_string(),
                secret_references,
            }
        })
        .collect();
    Ok(TransformStatusResult {
        entries,
        synced_count,
        diverged_count,
        missing_count,
    })
}

/// Run `dodot transform check`, patching only sources under
/// `authorized_root`. See module docs for the matrix and for the
/// scoping.
///
/// `authorized_root` is the canonical root this invocation may write
/// under: the root the CLI gate resolved once at the process boundary and
/// either found already approved or had the user approve. It is not
/// re-derived here: the root that was authorized and
/// the root that is mutated must be the same one (ADR-0001).
pub fn check(
    ctx: &ExecutionContext,
    strict: bool,
    authorized_root: &RootIdentity,
) -> Result<TransformCheckResult> {
    let baselines = collect_baselines(ctx.fs.as_ref(), ctx.paths.as_ref())?;
    let sources: Vec<std::path::PathBuf> = baselines
        .iter()
        .map(|(_, _, _, baseline)| baseline.source_path.clone())
        .collect();
    // The command layer's filesystem is the real one (`OsFs`), so
    // canonicalization asks the matching OS probe. Scoping happens once
    // for the whole invocation; the strict pass below reuses this same
    // authorized set.
    let scope = scope_to_root(authorized_root, &sources, &OsPathProbe);

    let mut entries: Vec<TransformCheckEntry> = Vec::with_capacity(baselines.len());
    let mut has_findings = false;
    // Memoise no_reverse patterns by pack within this check
    // invocation. ConfigManager already caches resolved configs by
    // absolute path, but each lookup still allocates and clones the
    // Vec — for repos with many baselines per pack, that's wasted
    // work. The map keeps the inner work to a single lookup per pack.
    let mut no_reverse_cache: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    for ((pack, handler, filename, baseline), outcome) in baselines.iter().zip(scope.outcomes()) {
        // Only the authorized canonical path is ever written; everything
        // else is a row in the report and nothing more.
        let authorized = match outcome {
            ScopeOutcome::InRoot(canonical) => canonical,
            ScopeOutcome::OutOfRoot(target) => {
                let (action, is_finding) = TransformAction::out_of_root(target.reason);
                has_findings |= is_finding;
                entries.push(TransformCheckEntry {
                    pack: pack.clone(),
                    handler: handler.clone(),
                    filename: filename.clone(),
                    source_path: render_path(&baseline.source_path, ctx.paths.home_dir()),
                    deployed_path: render_path(
                        &ctx.paths.handler_data_dir(pack, handler).join(filename),
                        ctx.paths.home_dir(),
                    ),
                    action,
                    conflict_block: String::new(),
                });
                continue;
            }
        };

        let report = classify_one(
            ctx.fs.as_ref(),
            ctx.paths.as_ref(),
            pack,
            handler,
            filename,
            baseline,
        );
        // Per-pack [preprocessor.template] no_reverse opt-out: when a
        // file matches, we treat it as Synced regardless of which
        // divergence state the matrix reports. This keeps the file
        // out of the reverse-merge engine (which can produce more
        // conflict markers than usable diffs on mostly-dynamic
        // templates) while leaving `dodot transform status` alone —
        // status still surfaces the underlying state for visibility.
        let no_reverse_patterns = no_reverse_cache
            .entry(pack.clone())
            .or_insert_with(|| pack_no_reverse_patterns(ctx, pack));
        let no_reverse = is_no_reverse(&report.source_path, no_reverse_patterns);
        let action = match report.state {
            DivergenceState::Synced => TransformAction::Synced,
            DivergenceState::InputChanged => TransformAction::InputChanged,
            DivergenceState::MissingSource => {
                has_findings = true;
                TransformAction::MissingSource
            }
            DivergenceState::MissingDeployed => {
                has_findings = true;
                TransformAction::MissingDeployed
            }
            DivergenceState::OutputChanged | DivergenceState::BothChanged if no_reverse => {
                // Opted out — surface as Synced without touching the
                // source.
                TransformAction::Synced
            }
            DivergenceState::OutputChanged | DivergenceState::BothChanged => {
                // Forward-compat short-circuit: a baseline written
                // before the tracked-render field existed (or by a
                // future preprocessor that opts into reverse-merge
                // without producing a marker stream) has nothing for
                // burgertocow to chew on. Surface as NeedsRebaseline
                // — a finding in its own right — rather than masking
                // it as Synced via reverse_merge's Unchanged fallback.
                // Without this branch, an OutputChanged file with an
                // empty tracked_render would silently report "no
                // divergence" and the user would never know.
                if baseline.tracked_render.is_empty() {
                    has_findings = true;
                    TransformAction::NeedsRebaseline
                } else {
                    // Run the reverse-merge engine. `Unchanged` means the
                    // deployed edit touched only variable values, so the
                    // template source needs no change. Both the read and
                    // the write below go through the authorized canonical
                    // path — the one containment was decided on — so a
                    // source reached through an in-root symlink is merged
                    // at its resolved location.
                    let template_src = ctx.fs.read_to_string(authorized)?;
                    let deployed = ctx.fs.read_to_string(&report.deployed_path)?;
                    // Load the per-render secrets sidecar so the
                    // reverse-merge masks lines whose source-of-truth
                    // is a vault, not the deployed bytes. Absence of
                    // the sidecar = empty mask = unmasked merge. See
                    // secrets.lex §3.3 and burgertocow#13.
                    let secret_ranges = crate::preprocessing::baseline::SecretsSidecar::load(
                        ctx.fs.as_ref(),
                        ctx.paths.as_ref(),
                        pack,
                        handler,
                        filename,
                    )?
                    .map(|s| s.secret_line_ranges)
                    .unwrap_or_default();
                    match reverse_merge(
                        &template_src,
                        &baseline.tracked_render,
                        &deployed,
                        &secret_ranges,
                    )? {
                        ReverseMergeOutcome::Unchanged => TransformAction::Synced,
                        ReverseMergeOutcome::Patched(patched) => {
                            if !ctx.dry_run {
                                ctx.fs.write_file(authorized, patched.as_bytes())?;
                            }
                            // The auto-merge happy path: `has_findings`
                            // deliberately stays false (see
                            // `TransformCheckResult::has_findings`).
                            TransformAction::Patched
                        }
                        ReverseMergeOutcome::Conflict(block) => {
                            has_findings = true;
                            return_conflict_entry(
                                &mut entries,
                                report,
                                block,
                                ctx.paths.home_dir(),
                            );
                            continue;
                        }
                    }
                }
            }
        };

        entries.push(make_entry(report, action, ctx.paths.home_dir()));
    }

    let mut unresolved_markers = Vec::new();
    if strict {
        // Scan each source for dodot-conflict markers. Any hit blocks a
        // commit (when this is run from the pre-commit hook).
        //
        // This walks the *same* authorized set as the loop above rather
        // than re-collecting the cache: a second unscoped walk would
        // read — and fail a commit over — sources under a root this
        // invocation was never authorized for, and the caches are
        // shared. Entries the loop above skipped via `continue` are
        // still covered, because the set comes from the scoping pass and
        // not from how far that loop got.
        for ((_pack, _handler, _filename, baseline), outcome) in
            baselines.iter().zip(scope.outcomes())
        {
            let ScopeOutcome::InRoot(authorized) = outcome else {
                continue;
            };
            let bytes = ctx.fs.read_file(authorized)?;
            let content = String::from_utf8_lossy(&bytes);
            let lines = find_unresolved_marker_lines(&content);
            if !lines.is_empty() {
                has_findings = true;
                unresolved_markers.push(UnresolvedMarkerEntry {
                    source_path: render_path(&baseline.source_path, ctx.paths.home_dir()),
                    line_numbers: lines.iter().map(|(n, _)| *n).collect(),
                });
            }
        }
    }

    Ok(TransformCheckResult {
        entries,
        unresolved_markers,
        has_findings,
        strict,
    })
}

fn make_entry(
    report: DivergenceReport,
    action: TransformAction,
    home: &std::path::Path,
) -> TransformCheckEntry {
    TransformCheckEntry {
        pack: report.pack,
        handler: report.handler,
        filename: report.filename,
        source_path: render_path(&report.source_path, home),
        deployed_path: render_path(&report.deployed_path, home),
        action,
        conflict_block: String::new(),
    }
}

fn return_conflict_entry(
    entries: &mut Vec<TransformCheckEntry>,
    report: DivergenceReport,
    block: String,
    home: &std::path::Path,
) {
    entries.push(TransformCheckEntry {
        pack: report.pack,
        handler: report.handler,
        filename: report.filename,
        source_path: render_path(&report.source_path, home),
        deployed_path: render_path(&report.deployed_path, home),
        action: TransformAction::Conflict,
        conflict_block: block,
    });
}

pub(super) fn render_path(p: &std::path::Path, home: &std::path::Path) -> String {
    if let Ok(rel) = p.strip_prefix(home) {
        format!("~/{}", rel.display())
    } else {
        p.display().to_string()
    }
}

/// Resolve `[preprocessor.template] no_reverse` for the given pack.
/// Honours the root → pack config inheritance. Returns an empty list
/// on any config-loading hiccup (the user shouldn't lose `transform
/// check` over a malformed pack `.dodot.toml` — the next `dodot up`
/// will surface the actual config error).
fn pack_no_reverse_patterns(ctx: &ExecutionContext, pack: &str) -> Vec<String> {
    let pack_path = ctx.paths.dotfiles_root().join(pack);
    match ctx.config_manager.config_for_pack(&pack_path) {
        Ok(cfg) => cfg.preprocessor.template.no_reverse.clone(),
        Err(_) => Vec::new(),
    }
}

mod install_hook;

#[cfg(test)]
mod test_support;

pub use install_hook::{
    hook_is_installed, install_hook, managed_block, InstallHookOutcome, InstallHookResult,
};

#[cfg(test)]
mod tests {
    #![allow(unused_imports)]

    use super::test_support::make_ctx;
    use super::*;
    use crate::fs::Fs;
    use crate::paths::Pather;
    use crate::testing::TempEnvironment;

    /// Run a real `dodot up` against a single-template pack so the
    /// baseline cache + datastore are populated the same way they
    /// would be in production. Returns the source path in the pack.
    fn deploy_template(
        env: &TempEnvironment,
        pack: &str,
        template_name: &str,
        template_body: &str,
        config_toml: &str,
    ) -> std::path::PathBuf {
        let src_path = env.dotfiles_root.join(pack).join(template_name);
        env.fs.mkdir_all(src_path.parent().unwrap()).unwrap();
        env.fs
            .write_file(&src_path, template_body.as_bytes())
            .unwrap();

        if !config_toml.is_empty() {
            env.fs
                .write_file(
                    &env.dotfiles_root.join(".dodot.toml"),
                    config_toml.as_bytes(),
                )
                .unwrap();
        }

        let ctx = make_ctx(env);
        let _ = crate::commands::up::up(None, &ctx).unwrap();

        src_path
    }

    fn deployed_path(env: &TempEnvironment, pack: &str, filename: &str) -> std::path::PathBuf {
        env.paths
            .data_dir()
            .join("packs")
            .join(pack)
            .join("preprocessed")
            .join(filename)
    }

    /// The root this invocation is authorized to patch: the
    /// environment's dotfiles root, canonicalized.
    ///
    /// Canonicalized because the command canonicalizes every source
    /// before placing it, and a root that is not itself canonical
    /// contains none of them — `TempDir` hands back `/var/…` on macOS
    /// while every resolved source under it comes back as `/private/var/…`.
    fn root(env: &TempEnvironment) -> RootIdentity {
        canonical_root(&env.dotfiles_root)
    }

    fn canonical_root(path: &std::path::Path) -> RootIdentity {
        RootIdentity::new(std::fs::canonicalize(path).unwrap()).unwrap()
    }

    /// A second dotfiles root beside the first, sharing this
    /// environment's one data and cache directory — the arrangement the
    /// scoping exists for.
    fn second_root_dir(env: &TempEnvironment) -> std::path::PathBuf {
        let dir = env.home.join("other-dotfiles");
        env.fs.mkdir_all(&dir).unwrap();
        dir
    }

    /// Stage a baseline whose source lives at `source` and whose
    /// deployed file was edited afterwards, i.e. an `OutputChanged`
    /// entry the reverse-merge would patch if it were allowed to.
    ///
    /// Written by hand rather than through `dodot up` because a second
    /// root's baselines cannot be produced by an `up` run against the
    /// first one.
    fn stage_edited(
        env: &TempEnvironment,
        pack: &str,
        filename: &str,
        source: &std::path::Path,
        template_body: &str,
        rendered: &str,
        deployed_body: &str,
    ) {
        env.fs.mkdir_all(source.parent().unwrap()).unwrap();
        env.fs.write_file(source, template_body.as_bytes()).unwrap();

        let deployed = deployed_path(env, pack, filename);
        env.fs.mkdir_all(deployed.parent().unwrap()).unwrap();
        env.fs
            .write_file(&deployed, deployed_body.as_bytes())
            .unwrap();

        crate::preprocessing::baseline::Baseline::build(
            source,
            rendered.as_bytes(),
            template_body.as_bytes(),
            Some(rendered),
            None,
        )
        .write(
            env.fs.as_ref(),
            env.paths.as_ref(),
            pack,
            "preprocessed",
            filename,
        )
        .unwrap();
    }

    #[test]
    fn empty_cache_yields_clean_no_findings() {
        let env = TempEnvironment::builder().build();
        let ctx = make_ctx(&env);
        let result = check(&ctx, false, &root(&env)).unwrap();
        assert!(result.entries.is_empty());
        assert!(!result.has_findings);
        assert_eq!(result.exit_code(), 0);
    }

    #[test]
    fn synced_files_report_synced_and_no_findings() {
        let env = TempEnvironment::builder().build();
        deploy_template(
            &env,
            "app",
            "config.toml.tmpl",
            "name = {{ name }}\n",
            "[preprocessor.template.vars]\nname = \"Alice\"\n",
        );
        let ctx = make_ctx(&env);
        let result = check(&ctx, false, &root(&env)).unwrap();
        assert_eq!(result.entries.len(), 1);
        assert!(matches!(result.entries[0].action, TransformAction::Synced));
        assert!(!result.has_findings);
    }

    #[test]
    fn output_changed_static_edit_patches_source() {
        let env = TempEnvironment::builder().build();
        let src_path = deploy_template(
            &env,
            "app",
            "config.toml.tmpl",
            "name = {{ name }}\nport = 5432\n",
            "[preprocessor.template.vars]\nname = \"Alice\"\n",
        );
        let deployed = deployed_path(&env, "app", "config.toml");
        env.fs
            .write_file(&deployed, b"name = Alice\nport = 9999\n")
            .unwrap();

        let ctx = make_ctx(&env);
        let result = check(&ctx, false, &root(&env)).unwrap();
        assert_eq!(result.entries.len(), 1);
        assert!(
            matches!(result.entries[0].action, TransformAction::Patched),
            "got: {:?}",
            result.entries[0].action
        );
        assert!(!result.has_findings);
        assert_eq!(result.exit_code(), 0);

        let new_src = env.fs.read_to_string(&src_path).unwrap();
        assert!(new_src.contains("port = 9999"), "src: {new_src:?}");
        assert!(new_src.contains("name = {{ name }}"), "src: {new_src:?}");
    }

    #[test]
    fn output_changed_pure_data_edit_yields_synced() {
        // The user changed only the variable's *value* in the
        // deployed file. burgertocow flags it as a pure-data edit;
        // the source needs no change. Action: Synced (no findings,
        // no source mutation).
        let env = TempEnvironment::builder().build();
        let src_path = deploy_template(
            &env,
            "app",
            "config.toml.tmpl",
            "name = {{ name }}\n",
            "[preprocessor.template.vars]\nname = \"Alice\"\n",
        );
        let original_src = env.fs.read_to_string(&src_path).unwrap();
        let deployed = deployed_path(&env, "app", "config.toml");
        env.fs.write_file(&deployed, b"name = Bob\n").unwrap();

        let ctx = make_ctx(&env);
        let result = check(&ctx, false, &root(&env)).unwrap();
        assert_eq!(result.entries.len(), 1);
        assert!(matches!(result.entries[0].action, TransformAction::Synced));
        assert_eq!(env.fs.read_to_string(&src_path).unwrap(), original_src);
    }

    #[test]
    fn no_reverse_pattern_skips_reverse_merge() {
        // Same scenario as output_changed_static_edit_patches_source,
        // but with `no_reverse = ["config.toml.tmpl"]` in the root
        // config. The user opted out of reverse-merge for this file
        // — `transform check` must report Synced, leave the source
        // untouched, and have no findings (so the pre-commit hook
        // would let the commit through).
        let env = TempEnvironment::builder().build();
        let src_path = deploy_template(
            &env,
            "app",
            "config.toml.tmpl",
            "name = {{ name }}\nport = 5432\n",
            "[preprocessor.template.vars]\n\
             name = \"Alice\"\n\
             [preprocessor.template]\n\
             no_reverse = [\"config.toml.tmpl\"]\n",
        );
        let original_src = env.fs.read_to_string(&src_path).unwrap();

        let deployed = deployed_path(&env, "app", "config.toml");
        env.fs
            .write_file(&deployed, b"name = Alice\nport = 9999\n")
            .unwrap();

        let ctx = make_ctx(&env);
        let result = check(&ctx, false, &root(&env)).unwrap();
        assert_eq!(result.entries.len(), 1);
        assert!(
            matches!(result.entries[0].action, TransformAction::Synced),
            "no_reverse must short-circuit to Synced; got: {:?}",
            result.entries[0].action
        );
        assert!(!result.has_findings);
        assert_eq!(result.exit_code(), 0);
        assert_eq!(env.fs.read_to_string(&src_path).unwrap(), original_src);
    }

    #[test]
    fn no_reverse_glob_pattern_skips_reverse_merge() {
        // Glob form of the opt-out — `*.gen.tmpl` matches the
        // generated template's filename and skips reverse-merge.
        let env = TempEnvironment::builder().build();
        let src_path = deploy_template(
            &env,
            "app",
            "foo.gen.tmpl",
            "name = {{ name }}\nport = 5432\n",
            "[preprocessor.template.vars]\n\
             name = \"Alice\"\n\
             [preprocessor.template]\n\
             no_reverse = [\"*.gen.tmpl\"]\n",
        );
        let original_src = env.fs.read_to_string(&src_path).unwrap();
        let deployed = deployed_path(&env, "app", "foo.gen");
        env.fs
            .write_file(&deployed, b"name = Alice\nport = 9999\n")
            .unwrap();

        let ctx = make_ctx(&env);
        let result = check(&ctx, false, &root(&env)).unwrap();
        assert_eq!(result.entries.len(), 1);
        assert!(matches!(result.entries[0].action, TransformAction::Synced));
        assert!(!result.has_findings);
        assert_eq!(env.fs.read_to_string(&src_path).unwrap(), original_src);
    }

    #[test]
    fn dry_run_does_not_write_to_source() {
        // Same scenario as the static-edit patch test, but with
        // dry_run=true. The action is still reported as Patched (so
        // the user sees what *would* happen), but the source is left
        // alone on disk.
        let env = TempEnvironment::builder().build();
        let src_path = deploy_template(
            &env,
            "app",
            "config.toml.tmpl",
            "name = {{ name }}\nport = 5432\n",
            "[preprocessor.template.vars]\nname = \"Alice\"\n",
        );
        let original_src = env.fs.read_to_string(&src_path).unwrap();
        let deployed = deployed_path(&env, "app", "config.toml");
        env.fs
            .write_file(&deployed, b"name = Alice\nport = 9999\n")
            .unwrap();

        let mut ctx = make_ctx(&env);
        ctx.dry_run = true;
        let result = check(&ctx, false, &root(&env)).unwrap();
        assert!(matches!(result.entries[0].action, TransformAction::Patched));
        assert_eq!(env.fs.read_to_string(&src_path).unwrap(), original_src);
    }

    #[test]
    fn needs_rebaseline_when_tracked_render_is_empty_and_deployed_edited() {
        // A baseline without a marker stream cannot drive burgertocow.
        // Edited deployed content must be reported as NeedsRebaseline,
        // never silently as Synced.
        let env = TempEnvironment::builder().build();
        let src_path = env.dotfiles_root.join("app/config.toml.tmpl");
        env.fs.mkdir_all(src_path.parent().unwrap()).unwrap();
        env.fs.write_file(&src_path, b"name = {{ name }}").unwrap();
        let baseline = crate::preprocessing::baseline::Baseline::build(
            &src_path,
            b"name = Alice",
            b"name = {{ name }}",
            None, // <-- the load-bearing detail: no tracked render
            None,
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
        let deployed = deployed_path(&env, "app", "config.toml");
        env.fs.mkdir_all(deployed.parent().unwrap()).unwrap();
        env.fs
            .write_file(&deployed, b"name = Edited\nport = 9999")
            .unwrap();

        let ctx = make_ctx(&env);
        let result = check(&ctx, false, &root(&env)).unwrap();
        assert_eq!(result.entries.len(), 1);
        assert!(
            matches!(result.entries[0].action, TransformAction::NeedsRebaseline),
            "got: {:?}",
            result.entries[0].action
        );
        assert!(
            result.has_findings,
            "NeedsRebaseline must count as a finding"
        );
        assert_eq!(result.exit_code(), 1);

        let src_after = env.fs.read_to_string(&src_path).unwrap();
        assert_eq!(src_after, "name = {{ name }}");
    }

    #[test]
    fn missing_source_is_reported_with_finding() {
        // Stage a baseline with a source path that doesn't exist.
        // (Easier than going through `dodot up` and then deleting
        // the file.)
        let env = TempEnvironment::builder().build();
        let baseline = crate::preprocessing::baseline::Baseline::build(
            &env.dotfiles_root.join("app/missing.toml.tmpl"),
            b"rendered",
            b"src",
            Some(""),
            None,
        );
        baseline
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "missing.toml",
            )
            .unwrap();
        let deployed = deployed_path(&env, "app", "missing.toml");
        env.fs.mkdir_all(deployed.parent().unwrap()).unwrap();
        env.fs.write_file(&deployed, b"rendered").unwrap();

        let ctx = make_ctx(&env);
        let result = check(&ctx, false, &root(&env)).unwrap();
        assert!(matches!(
            result.entries[0].action,
            TransformAction::MissingSource
        ));
        assert!(result.has_findings);
    }

    #[test]
    fn strict_mode_flags_unresolved_marker_in_source() {
        // Strict mode catches dodot-conflict markers left in the source.
        let env = TempEnvironment::builder().build();
        let src_path = deploy_template(
            &env,
            "app",
            "config.toml.tmpl",
            "name = {{ name }}\n",
            "[preprocessor.template.vars]\nname = \"Alice\"\n",
        );
        let dirty = format!(
            "first\n{}\nbody\n{}\n",
            crate::preprocessing::conflict::MARKER_START,
            crate::preprocessing::conflict::MARKER_END,
        );
        env.fs.write_file(&src_path, dirty.as_bytes()).unwrap();

        let ctx = make_ctx(&env);
        let lax = check(&ctx, false, &root(&env)).unwrap();
        assert!(lax.unresolved_markers.is_empty());

        let strict = check(&ctx, true, &root(&env)).unwrap();
        assert_eq!(strict.unresolved_markers.len(), 1);
        assert_eq!(strict.unresolved_markers[0].line_numbers, vec![2, 4]);
        assert!(strict.has_findings);
        assert_eq!(strict.exit_code(), 1);
    }

    #[test]
    fn strict_mode_clean_repo_is_zero_findings() {
        let env = TempEnvironment::builder().build();
        deploy_template(
            &env,
            "app",
            "config.toml.tmpl",
            "name = {{ name }}\n",
            "[preprocessor.template.vars]\nname = \"Alice\"\n",
        );
        let ctx = make_ctx(&env);
        let result = check(&ctx, true, &root(&env)).unwrap();
        assert!(result.unresolved_markers.is_empty());
        assert!(!result.has_findings);
        assert_eq!(result.exit_code(), 0);
    }

    #[test]
    fn paths_are_rendered_relative_to_home_for_display() {
        // Deployed paths under `data_dir` (which lives under the
        // sandbox $HOME) should render with `~/` prefix in the
        // report. Pure cosmetic — `dodot transform check`'s output
        // is meant to be readable in a terminal.
        let env = TempEnvironment::builder().build();
        deploy_template(
            &env,
            "app",
            "config.toml.tmpl",
            "name = {{ name }}\n",
            "[preprocessor.template.vars]\nname = \"Alice\"\n",
        );
        let ctx = make_ctx(&env);
        let result = check(&ctx, false, &root(&env)).unwrap();
        let entry = &result.entries[0];
        assert!(
            entry.source_path.starts_with("~/") || entry.deployed_path.starts_with("~/"),
            "expected ~/-relative paths in report, got source={} deployed={}",
            entry.source_path,
            entry.deployed_path
        );
    }

    // ── mutation scoping (ACC01-WS06) ────────────────────────────

    /// Two roots, one shared cache: whichever root the invocation is
    /// authorized for, only that root's sources are patched. This is the
    /// property the scoping exists for — approval for one root cannot
    /// rewrite a second repository's user-authored template (ADR-0002).
    #[test]
    fn authorizing_one_root_never_patches_the_other_roots_sources() {
        let env = TempEnvironment::builder().build();
        let here = deploy_template(
            &env,
            "app",
            "config.toml.tmpl",
            "name = {{ name }}\nport = 5432\n",
            "[preprocessor.template.vars]\nname = \"Alice\"\n",
        );
        env.fs
            .write_file(
                &deployed_path(&env, "app", "config.toml"),
                b"name = Alice\nport = 9999\n",
            )
            .unwrap();

        // A second root's baseline in the same cache, equally ready to
        // be reverse-merged.
        let other_root = second_root_dir(&env);
        let there = other_root.join("other/config.toml.tmpl");
        stage_edited(
            &env,
            "other",
            "config.toml",
            &there,
            "port = 5432\n",
            "port = 5432\n",
            "port = 9999\n",
        );

        let ctx = make_ctx(&env);
        let result = check(&ctx, false, &root(&env)).unwrap();

        let actions: Vec<(&str, &TransformAction)> = result
            .entries
            .iter()
            .map(|e| (e.pack.as_str(), &e.action))
            .collect();
        assert!(
            matches!(
                actions.as_slice(),
                [
                    ("app", TransformAction::Patched),
                    ("other", TransformAction::OutOfRoot)
                ]
            ),
            "unexpected actions: {actions:?}"
        );
        assert!(env.fs.read_to_string(&here).unwrap().contains("9999"));
        assert_eq!(
            env.fs.read_to_string(&there).unwrap(),
            "port = 5432\n",
            "another root's source was patched"
        );
        // A sibling root's baseline is not a finding: it must not make
        // this repository's pre-commit hook refuse the commit.
        assert!(!result.has_findings);
        assert_eq!(result.exit_code(), 0);

        // Authorize the other root instead: the mirror image, from the
        // same cache.
        let mirrored = check(&ctx, false, &canonical_root(&other_root)).unwrap();
        let actions: Vec<(&str, &TransformAction)> = mirrored
            .entries
            .iter()
            .map(|e| (e.pack.as_str(), &e.action))
            .collect();
        assert!(
            matches!(
                actions.as_slice(),
                [
                    ("app", TransformAction::OutOfRoot),
                    ("other", TransformAction::Patched)
                ]
            ),
            "unexpected actions: {actions:?}"
        );
        assert!(env.fs.read_to_string(&there).unwrap().contains("9999"));
    }

    /// Strict mode walks the authorized set, not the cache: an
    /// unresolved conflict marker in *another* root's source must not
    /// block a commit here, and must still block one there.
    #[test]
    fn strict_mode_scans_only_the_authorized_roots_sources() {
        let env = TempEnvironment::builder().build();
        deploy_template(
            &env,
            "app",
            "config.toml.tmpl",
            "name = {{ name }}\n",
            "[preprocessor.template.vars]\nname = \"Alice\"\n",
        );

        let other_root = second_root_dir(&env);
        let there = other_root.join("other/config.toml.tmpl");
        let dirty = format!(
            "first\n{}\nbody\n{}\n",
            crate::preprocessing::conflict::MARKER_START,
            crate::preprocessing::conflict::MARKER_END,
        );
        stage_edited(
            &env,
            "other",
            "config.toml",
            &there,
            &dirty,
            "rendered\n",
            "rendered\n",
        );

        let ctx = make_ctx(&env);
        let result = check(&ctx, true, &root(&env)).unwrap();
        assert!(
            result.unresolved_markers.is_empty(),
            "scanned a source under another root: {:?}",
            result.unresolved_markers
        );
        assert!(!result.has_findings);
        assert_eq!(result.exit_code(), 0);

        let mirrored = check(&ctx, true, &canonical_root(&other_root)).unwrap();
        assert_eq!(mirrored.unresolved_markers.len(), 1);
        assert_eq!(mirrored.unresolved_markers[0].line_numbers, vec![2, 4]);
        assert_eq!(mirrored.exit_code(), 1);
    }

    /// A source that *lives* under the authorized root but *resolves*
    /// outside it: patching the stored spelling would write through the
    /// link into another tree.
    #[test]
    fn a_source_symlinked_out_of_the_root_is_reported_not_patched() {
        let env = TempEnvironment::builder().build();
        let other_root = second_root_dir(&env);
        let outside = other_root.join("real.toml.tmpl");
        env.fs.mkdir_all(outside.parent().unwrap()).unwrap();
        env.fs.write_file(&outside, b"port = 5432\n").unwrap();

        let link = env.dotfiles_root.join("app/config.toml.tmpl");
        env.fs.mkdir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let deployed = deployed_path(&env, "app", "config.toml");
        env.fs.mkdir_all(deployed.parent().unwrap()).unwrap();
        env.fs.write_file(&deployed, b"port = 9999\n").unwrap();
        crate::preprocessing::baseline::Baseline::build(
            &link,
            b"port = 5432\n",
            b"port = 5432\n",
            Some("port = 5432\n"),
            None,
        )
        .write(
            env.fs.as_ref(),
            env.paths.as_ref(),
            "app",
            "preprocessed",
            "config.toml",
        )
        .unwrap();

        let ctx = make_ctx(&env);
        let result = check(&ctx, false, &root(&env)).unwrap();

        assert!(
            matches!(result.entries[0].action, TransformAction::OutOfRoot),
            "unexpected action: {:?}",
            result.entries[0].action
        );
        assert_eq!(env.fs.read_to_string(&outside).unwrap(), "port = 5432\n");
    }

    /// A baseline written before the cache recorded source paths stores
    /// an empty one. Nothing was looked for and nothing is missing — it
    /// is a stale cache entry, reported apart from a deleted source, and
    /// still a finding because the cache needs rebuilding.
    #[test]
    fn a_baseline_without_a_source_path_is_reported_as_stale() {
        let env = TempEnvironment::builder().build();
        crate::preprocessing::baseline::Baseline::build(
            std::path::Path::new(""),
            b"rendered",
            b"src",
            Some(""),
            None,
        )
        .write(
            env.fs.as_ref(),
            env.paths.as_ref(),
            "app",
            "preprocessed",
            "config.toml",
        )
        .unwrap();
        let deployed = deployed_path(&env, "app", "config.toml");
        env.fs.mkdir_all(deployed.parent().unwrap()).unwrap();
        env.fs.write_file(&deployed, b"rendered").unwrap();

        let ctx = make_ctx(&env);
        let result = check(&ctx, true, &root(&env)).unwrap();

        assert!(
            matches!(result.entries[0].action, TransformAction::StaleSource),
            "unexpected action: {:?}",
            result.entries[0].action
        );
        assert!(result.has_findings);
        assert!(result.unresolved_markers.is_empty());
    }

    // ── status ──────────────────────────────────────────────────

    #[test]
    fn status_on_clean_repo_reports_one_synced_row() {
        let env = TempEnvironment::builder().build();
        deploy_template(
            &env,
            "app",
            "config.toml.tmpl",
            "name = {{ name }}\n",
            "[preprocessor.template.vars]\nname = \"Alice\"\n",
        );
        let ctx = make_ctx(&env);
        let result = status(&ctx).unwrap();
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].state, "synced");
        assert_eq!(result.synced_count, 1);
        assert_eq!(result.diverged_count, 0);
        assert_eq!(result.missing_count, 0);
    }

    #[test]
    fn status_surfaces_secret_references_from_sidecar() {
        // Phase S5: a baseline with a sidecar exposes the
        // resolved references in `transform status`. The
        // user can see WHICH secrets each baseline depends on
        // without re-rendering.
        let env = TempEnvironment::builder().build();
        deploy_template(
            &env,
            "app",
            "config.toml.tmpl",
            "name = {{ name }}\n",
            "[preprocessor.template.vars]\nname = \"Alice\"\n",
        );
        let sidecar = crate::preprocessing::baseline::SecretsSidecar::new(vec![
            crate::preprocessing::SecretLineRange {
                start: 0,
                end: 1,
                reference: "pass:test/db_password".into(),
            },
            crate::preprocessing::SecretLineRange {
                start: 2,
                end: 3,
                reference: "op://Personal/api/token".into(),
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

        let ctx = make_ctx(&env);
        let result = status(&ctx).unwrap();
        assert_eq!(result.entries.len(), 1);
        assert_eq!(
            result.entries[0].secret_references,
            vec![
                "pass:test/db_password".to_string(),
                "op://Personal/api/token".to_string(),
            ]
        );
    }

    #[test]
    fn status_returns_empty_secret_references_when_no_sidecar() {
        // Default state: a template that doesn't use secrets
        // has no sidecar, so `secret_references` is the empty
        // vec. The serde `skip_serializing_if = "Vec::is_empty"`
        // attribute means JSON consumers don't see the field at
        // all in this case — pin the rust-side state too.
        let env = TempEnvironment::builder().build();
        deploy_template(
            &env,
            "app",
            "config.toml.tmpl",
            "name = {{ name }}\n",
            "[preprocessor.template.vars]\nname = \"Alice\"\n",
        );
        let ctx = make_ctx(&env);
        let result = status(&ctx).unwrap();
        assert!(result.entries[0].secret_references.is_empty());
    }

    #[test]
    fn status_classifies_output_change() {
        let env = TempEnvironment::builder().build();
        deploy_template(
            &env,
            "app",
            "config.toml.tmpl",
            "name = {{ name }}\nport = 5432\n",
            "[preprocessor.template.vars]\nname = \"Alice\"\n",
        );
        let deployed = deployed_path(&env, "app", "config.toml");
        env.fs
            .write_file(&deployed, b"name = Alice\nport = 9999\n")
            .unwrap();

        let ctx = make_ctx(&env);
        let result = status(&ctx).unwrap();
        assert_eq!(result.entries[0].state, "output_changed");
        assert_eq!(result.diverged_count, 1);
        assert_eq!(result.synced_count, 0);
    }

    #[test]
    fn status_does_not_mutate_anything() {
        // The entire point of `status` (vs `check`) is that it's
        // read-only. Run it on a divergent repo and confirm the
        // source file is byte-identical afterwards.
        let env = TempEnvironment::builder().build();
        let src = deploy_template(
            &env,
            "app",
            "config.toml.tmpl",
            "name = {{ name }}\nport = 5432\n",
            "[preprocessor.template.vars]\nname = \"Alice\"\n",
        );
        let original_src = env.fs.read_to_string(&src).unwrap();
        let deployed = deployed_path(&env, "app", "config.toml");
        env.fs
            .write_file(&deployed, b"name = Alice\nport = 9999\n")
            .unwrap();

        let ctx = make_ctx(&env);
        let _ = status(&ctx).unwrap();
        assert_eq!(env.fs.read_to_string(&src).unwrap(), original_src);
    }

    #[test]
    fn status_empty_cache_yields_zero_counts() {
        let env = TempEnvironment::builder().build();
        let ctx = make_ctx(&env);
        let result = status(&ctx).unwrap();
        assert!(result.entries.is_empty());
        assert_eq!(result.synced_count, 0);
        assert_eq!(result.diverged_count, 0);
        assert_eq!(result.missing_count, 0);
    }
}

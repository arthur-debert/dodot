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
//! `check(ctx, strict=true)` is the form used by the pre-commit hook
//! (R4). On top of the matrix work above, it scans every source file
//! for unresolved [`crate::preprocessing::conflict`] markers — if any
//! are found, the result reports them and the command exits non-zero
//! so a commit is blocked until the user resolves them.

use serde::Serialize;

use crate::packs::orchestration::ExecutionContext;
use crate::preprocessing::conflict::find_unresolved_marker_lines;
use crate::preprocessing::divergence::{
    classify_one, collect_baselines, DivergenceReport, DivergenceState,
};
use crate::preprocessing::no_reverse::is_no_reverse;
use crate::preprocessing::reverse_merge::{reverse_merge, ReverseMergeOutcome};
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
    /// MissingSource, MissingDeployed) or `--strict` found unresolved
    /// markers. CLI uses this to decide the process exit code.
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
    /// when the file has no sidecar (which is also the common
    /// case for templates that don't use secrets, and for
    /// pre-Phase-S1 baselines that pre-date sidecar tracking).
    /// Phase S5 surfaces this in the rendered status so users can
    /// see *which* secret references each baseline depends on
    /// without re-rendering. JSON consumers see the same field.
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

/// Run `dodot transform check`. See module docs for the matrix.
pub fn check(ctx: &ExecutionContext, strict: bool) -> Result<TransformCheckResult> {
    let baselines = collect_baselines(ctx.fs.as_ref(), ctx.paths.as_ref())?;
    let mut entries: Vec<TransformCheckEntry> = Vec::with_capacity(baselines.len());
    let mut has_findings = false;
    // Memoise no_reverse patterns by pack within this check
    // invocation. ConfigManager already caches resolved configs by
    // absolute path, but each lookup still allocates and clones the
    // Vec — for repos with many baselines per pack, that's wasted
    // work. The map keeps the inner work to a single lookup per pack.
    let mut no_reverse_cache: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    for (pack, handler, filename, baseline) in baselines {
        let report = classify_one(
            ctx.fs.as_ref(),
            ctx.paths.as_ref(),
            &pack,
            &handler,
            &filename,
            &baseline,
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
            .or_insert_with(|| pack_no_reverse_patterns(ctx, &pack));
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
                // Opted out — leave source untouched, surface as
                // Synced. The user has explicitly chosen "detect
                // divergence but don't auto-merge"; `transform
                // status` still shows the real state.
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
                    // Run the reverse-merge engine. Unchanged → variable-
                    // only edit, no action. Patched → write back to source.
                    // Conflict → report the block, leave source alone.
                    let template_src = ctx.fs.read_to_string(&report.source_path)?;
                    let deployed = ctx.fs.read_to_string(&report.deployed_path)?;
                    // Load the per-render secrets sidecar so the
                    // reverse-merge masks lines whose source-of-truth
                    // is a vault, not the deployed bytes. Absence of
                    // the sidecar = empty mask = byte-identical to
                    // pre-Phase-S2 behavior. See secrets.lex §3.3 and
                    // burgertocow#13.
                    let secret_ranges = crate::preprocessing::baseline::SecretsSidecar::load(
                        ctx.fs.as_ref(),
                        ctx.paths.as_ref(),
                        &pack,
                        &handler,
                        &filename,
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
                                ctx.fs.write_file(&report.source_path, patched.as_bytes())?;
                            }
                            // `Patched` is the auto-merge happy path:
                            // burgertocow + diffy produced an
                            // unambiguous unified patch, the source
                            // is now in sync with the user's edit.
                            // Nothing for the user to review →
                            // `has_findings` stays false. The patched
                            // source surfaces as modified on the next
                            // `git status` for a follow-up commit.
                            // See #113.
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
        // Re-walk the cache, scanning each source for dodot-conflict
        // markers. Any hit blocks a commit (when this is run from the
        // pre-commit hook). We re-walk rather than reusing the loop
        // above because the loop may have skipped entries via
        // MissingSource / continue paths.
        let baselines = collect_baselines(ctx.fs.as_ref(), ctx.paths.as_ref())?;
        for (_pack, _handler, _filename, baseline) in baselines {
            if baseline.source_path.as_os_str().is_empty() || !ctx.fs.exists(&baseline.source_path)
            {
                continue;
            }
            let bytes = ctx.fs.read_file(&baseline.source_path)?;
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
        // Write the template source.
        let src_path = env.dotfiles_root.join(pack).join(template_name);
        env.fs.mkdir_all(src_path.parent().unwrap()).unwrap();
        env.fs
            .write_file(&src_path, template_body.as_bytes())
            .unwrap();

        // Write a root .dodot.toml carrying the desired vars.
        if !config_toml.is_empty() {
            env.fs
                .write_file(
                    &env.dotfiles_root.join(".dodot.toml"),
                    config_toml.as_bytes(),
                )
                .unwrap();
        }

        // Deploy via `dodot up`.
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

    #[test]
    fn empty_cache_yields_clean_no_findings() {
        let env = TempEnvironment::builder().build();
        let ctx = make_ctx(&env);
        let result = check(&ctx, false).unwrap();
        assert!(result.entries.is_empty());
        assert!(!result.has_findings);
        assert_eq!(result.exit_code(), 0);
    }

    #[test]
    fn synced_files_report_synced_and_no_findings() {
        // Run `dodot up` on a template, immediately run `transform
        // check`. Nothing edited → all entries are Synced, no findings.
        let env = TempEnvironment::builder().build();
        deploy_template(
            &env,
            "app",
            "config.toml.tmpl",
            "name = {{ name }}\n",
            "[preprocessor.template.vars]\nname = \"Alice\"\n",
        );
        let ctx = make_ctx(&env);
        let result = check(&ctx, false).unwrap();
        assert_eq!(result.entries.len(), 1);
        assert!(matches!(result.entries[0].action, TransformAction::Synced));
        assert!(!result.has_findings);
    }

    #[test]
    fn output_changed_static_edit_patches_source() {
        // Edit the deployed file's static content. The source file's
        // template variable should be preserved; the static edit
        // should land in the template via diffy.
        let env = TempEnvironment::builder().build();
        let src_path = deploy_template(
            &env,
            "app",
            "config.toml.tmpl",
            "name = {{ name }}\nport = 5432\n",
            "[preprocessor.template.vars]\nname = \"Alice\"\n",
        );
        // Edit the deployed file (the rendered content in the
        // datastore — that's what the user-side symlink dereferences
        // to). Change the static `port` line.
        let deployed = deployed_path(&env, "app", "config.toml");
        env.fs
            .write_file(&deployed, b"name = Alice\nport = 9999\n")
            .unwrap();

        let ctx = make_ctx(&env);
        let result = check(&ctx, false).unwrap();
        assert_eq!(result.entries.len(), 1);
        assert!(
            matches!(result.entries[0].action, TransformAction::Patched),
            "got: {:?}",
            result.entries[0].action
        );
        // Patched is the auto-merge happy path: clean unified diff,
        // source rewritten, nothing for the user to review. The
        // pre-commit hook lets the commit proceed; the user does a
        // follow-up `git add` + commit on the patched source. See #113.
        assert!(!result.has_findings);
        assert_eq!(result.exit_code(), 0);

        // Source was rewritten: the static line is updated, the
        // variable-bearing line is preserved verbatim.
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
        let result = check(&ctx, false).unwrap();
        assert_eq!(result.entries.len(), 1);
        assert!(matches!(result.entries[0].action, TransformAction::Synced));
        // Source must be byte-identical to the original.
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

        // Edit the deployed file the same way the patching test does.
        let deployed = deployed_path(&env, "app", "config.toml");
        env.fs
            .write_file(&deployed, b"name = Alice\nport = 9999\n")
            .unwrap();

        let ctx = make_ctx(&env);
        let result = check(&ctx, false).unwrap();
        assert_eq!(result.entries.len(), 1);
        assert!(
            matches!(result.entries[0].action, TransformAction::Synced),
            "no_reverse must short-circuit to Synced; got: {:?}",
            result.entries[0].action
        );
        assert!(!result.has_findings);
        assert_eq!(result.exit_code(), 0);
        // Source untouched on disk.
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
        let result = check(&ctx, false).unwrap();
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
        let result = check(&ctx, false).unwrap();
        assert!(matches!(result.entries[0].action, TransformAction::Patched));
        // Source unchanged on disk despite the action label.
        assert_eq!(env.fs.read_to_string(&src_path).unwrap(), original_src);
    }

    #[test]
    fn needs_rebaseline_when_tracked_render_is_empty_and_deployed_edited() {
        // Forward-compat surface: a baseline written before
        // tracked_render existed (or by a future preprocessor that
        // opts in without producing a marker stream) is unable to
        // drive burgertocow. If the deployed file has been edited,
        // the action MUST be NeedsRebaseline — never silently
        // reported as Synced. This test pins that contract because
        // the bug existed in the first cut: empty tracked_render
        // produced reverse_merge → Unchanged → mapped to Synced,
        // hiding real divergence from the user.
        let env = TempEnvironment::builder().build();
        // Stage a baseline by hand with an empty tracked_render.
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
        // Lay down a deployed file that DIVERGES from the baseline.
        let deployed = deployed_path(&env, "app", "config.toml");
        env.fs.mkdir_all(deployed.parent().unwrap()).unwrap();
        env.fs
            .write_file(&deployed, b"name = Edited\nport = 9999")
            .unwrap();

        let ctx = make_ctx(&env);
        let result = check(&ctx, false).unwrap();
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

        // Source must NOT have been mutated (we couldn't compute a
        // safe diff without the marker stream).
        let src_after = env.fs.read_to_string(&src_path).unwrap();
        assert_eq!(src_after, "name = {{ name }}");
    }

    #[test]
    fn missing_source_is_reported_with_finding() {
        // Stage a baseline with a source path that doesn't exist.
        // (Easier than going through `dodot up` and then deleting
        // the file.)
        let env = TempEnvironment::builder().build();
        // Build a minimal baseline by hand at the cache path.
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
        // Also lay down a deployed file so we don't conflate
        // MissingSource with MissingDeployed.
        let deployed = deployed_path(&env, "app", "missing.toml");
        env.fs.mkdir_all(deployed.parent().unwrap()).unwrap();
        env.fs.write_file(&deployed, b"rendered").unwrap();

        let ctx = make_ctx(&env);
        let result = check(&ctx, false).unwrap();
        assert!(matches!(
            result.entries[0].action,
            TransformAction::MissingSource
        ));
        assert!(result.has_findings);
    }

    #[test]
    fn strict_mode_flags_unresolved_marker_in_source() {
        // Deploy a template, then write dodot-conflict markers into
        // the source file (simulating a previous `transform check`
        // run that emitted them). Strict mode catches it.
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
        // Non-strict: no marker scan, so no findings (the source
        // change makes it InputChanged, which is fine).
        let lax = check(&ctx, false).unwrap();
        assert!(lax.unresolved_markers.is_empty());

        // Strict: scan picks up the markers, has_findings=true.
        let strict = check(&ctx, true).unwrap();
        assert_eq!(strict.unresolved_markers.len(), 1);
        assert_eq!(strict.unresolved_markers[0].line_numbers, vec![2, 4]);
        assert!(strict.has_findings);
        assert_eq!(strict.exit_code(), 1);
    }

    #[test]
    fn strict_mode_clean_repo_is_zero_findings() {
        // No source has markers → strict mode reports zero unresolved
        // markers and (assuming no divergence either) no findings.
        let env = TempEnvironment::builder().build();
        deploy_template(
            &env,
            "app",
            "config.toml.tmpl",
            "name = {{ name }}\n",
            "[preprocessor.template.vars]\nname = \"Alice\"\n",
        );
        let ctx = make_ctx(&env);
        let result = check(&ctx, true).unwrap();
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
        let result = check(&ctx, false).unwrap();
        // At least one of source/deployed should start with `~/`.
        let entry = &result.entries[0];
        assert!(
            entry.source_path.starts_with("~/") || entry.deployed_path.starts_with("~/"),
            "expected ~/-relative paths in report, got source={} deployed={}",
            entry.source_path,
            entry.deployed_path
        );
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
        // Drop a sidecar next to the baseline. (In production
        // the renderer writes this; tests can build it
        // directly since the file shape is stable.)
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

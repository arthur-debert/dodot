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
    /// make the command exit non-zero (Patched, Conflict,
    /// NeedsRebaseline, MissingSource, MissingDeployed) or `--strict`
    /// found unresolved markers. CLI uses this to decide the process
    /// exit code.
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

/// Run `dodot transform check`. See module docs for the matrix.
pub fn check(ctx: &ExecutionContext, strict: bool) -> Result<TransformCheckResult> {
    let baselines = collect_baselines(ctx.fs.as_ref(), ctx.paths.as_ref())?;
    let mut entries: Vec<TransformCheckEntry> = Vec::with_capacity(baselines.len());
    let mut has_findings = false;

    for (pack, handler, filename, baseline) in baselines {
        let report = classify_one(
            ctx.fs.as_ref(),
            ctx.paths.as_ref(),
            &pack,
            &handler,
            &filename,
            &baseline,
        );
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
                    match reverse_merge(&template_src, &baseline.tracked_render, &deployed)? {
                        ReverseMergeOutcome::Unchanged => TransformAction::Synced,
                        ReverseMergeOutcome::Patched(patched) => {
                            if !ctx.dry_run {
                                ctx.fs.write_file(&report.source_path, patched.as_bytes())?;
                            }
                            has_findings = true;
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

fn render_path(p: &std::path::Path, home: &std::path::Path) -> String {
    if let Ok(rel) = p.strip_prefix(home) {
        format!("~/{}", rel.display())
    } else {
        p.display().to_string()
    }
}

// ── install-hook ────────────────────────────────────────────────

/// The guard line that opens our managed block in `.git/hooks/pre-commit`.
/// Detection of this string is what makes [`install_hook`] idempotent.
pub(crate) const HOOK_GUARD_START: &str =
    "# >>> dodot transform check --strict (managed by `dodot transform install-hook`) >>>";

/// The guard line that closes our managed block. Paired with
/// [`HOOK_GUARD_START`].
pub(crate) const HOOK_GUARD_END: &str = "# <<< dodot transform check --strict <<<";

/// Outcome of `dodot transform install-hook`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallHookOutcome {
    /// Hook file did not exist; we created it with shebang + our block.
    Created,
    /// Hook file existed; we appended our block to it. Existing content
    /// is preserved.
    Appended,
    /// Hook was already installed (guard line found) — no change.
    AlreadyInstalled,
}

/// Result returned by [`install_hook`]. Renders through the
/// `transform-install-hook.jinja` template; CLI exits 0 in all three
/// outcomes (every state is a success).
#[derive(Debug, Clone, Serialize)]
pub struct InstallHookResult {
    pub outcome: InstallHookOutcome,
    /// Absolute path of the hook file that was written or inspected.
    pub hook_path: String,
    /// Path of the hook rendered relative to `$HOME` for display.
    pub hook_display_path: String,
    /// The exact line the hook will execute on each commit. Surfaced
    /// so the user can see what `--strict` looks like in their hook.
    pub command_line: String,
}

/// Install (or detect-already-installed) the dodot pre-commit hook
/// that runs `dodot transform check --strict`.
///
/// # Behavior
///
/// - If `<dotfiles_root>/.git/hooks/pre-commit` does not exist:
///   create it with `#!/bin/sh` + our guarded block, mode `0o755`.
/// - If it exists and already contains [`HOOK_GUARD_START`]:
///   no-op, return [`InstallHookOutcome::AlreadyInstalled`].
/// - If it exists without our guard: append our block (preserving
///   existing content), ensure executable bit is set.
///
/// # Errors
///
/// Returns an error if `<dotfiles_root>/.git` doesn't exist (the
/// dotfiles repo isn't a git working tree). The hook only makes
/// sense in a git context.
pub fn install_hook(ctx: &ExecutionContext) -> Result<InstallHookResult> {
    let dotfiles_root = ctx.paths.dotfiles_root();
    let git_dir = dotfiles_root.join(".git");
    if !ctx.fs.is_dir(&git_dir) {
        return Err(crate::DodotError::Other(format!(
            "no .git directory at {}; pre-commit hooks only apply to git working \
             trees. Run `git init` in your dotfiles repo first.",
            dotfiles_root.display()
        )));
    }

    let hooks_dir = git_dir.join("hooks");
    let hook_path = hooks_dir.join("pre-commit");

    let block = managed_block();

    let outcome = if ctx.fs.exists(&hook_path) {
        let existing = ctx.fs.read_to_string(&hook_path)?;
        if existing.contains(HOOK_GUARD_START) {
            InstallHookOutcome::AlreadyInstalled
        } else {
            // Append the block, preserving the existing hook. Add a
            // leading blank line if the existing content doesn't end
            // in one — readability when the hook is opened by hand.
            let mut new_content = existing.clone();
            if !new_content.ends_with('\n') {
                new_content.push('\n');
            }
            if !new_content.ends_with("\n\n") {
                new_content.push('\n');
            }
            new_content.push_str(&block);
            ctx.fs.write_file(&hook_path, new_content.as_bytes())?;
            ctx.fs.set_permissions(&hook_path, 0o755)?;
            InstallHookOutcome::Appended
        }
    } else {
        ctx.fs.mkdir_all(&hooks_dir)?;
        let mut new_content = String::from("#!/bin/sh\n\n");
        new_content.push_str(&block);
        ctx.fs.write_file(&hook_path, new_content.as_bytes())?;
        ctx.fs.set_permissions(&hook_path, 0o755)?;
        InstallHookOutcome::Created
    };

    Ok(InstallHookResult {
        outcome,
        hook_path: hook_path.display().to_string(),
        hook_display_path: render_path(&hook_path, ctx.paths.home_dir()),
        command_line: HOOK_COMMAND.to_string(),
    })
}

/// Detect whether the hook is currently installed in the dotfiles
/// repo. Used by the `dodot up` first-template-deploy prompt to
/// decide whether to offer installation. Cheap (single read of the
/// hook file).
pub fn hook_is_installed(ctx: &ExecutionContext) -> Result<bool> {
    let hook_path = ctx.paths.dotfiles_root().join(".git/hooks/pre-commit");
    if !ctx.fs.exists(&hook_path) {
        return Ok(false);
    }
    let existing = ctx.fs.read_to_string(&hook_path)?;
    Ok(existing.contains(HOOK_GUARD_START))
}

/// Public for `dodot transform show-hook` (future) and for the
/// onboarding prompt in `commands::up` to surface what would be
/// installed. Includes the guard lines so callers can grep-detect
/// the block in arbitrary contexts.
pub fn managed_block() -> String {
    format!(
        "{guard_start}\n\
         # Aborts the commit if `dodot transform check --strict` reports findings —\n\
         # template files whose deployed sides drifted, or unresolved dodot-conflict\n\
         # markers. Remove this block to opt out.\n\
         {command}\n\
         {guard_end}\n",
        guard_start = HOOK_GUARD_START,
        guard_end = HOOK_GUARD_END,
        command = HOOK_COMMAND,
    )
}

/// The exact shell line our hook block runs. Pulled out as a
/// constant so the install path and the renderer surface the same
/// string verbatim.
pub(crate) const HOOK_COMMAND: &str = "dodot transform check --strict || exit 1";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::Fs;
    use crate::paths::Pather;
    use crate::testing::TempEnvironment;

    fn make_ctx(env: &TempEnvironment) -> ExecutionContext {
        use crate::config::ConfigManager;
        use crate::datastore::{CommandOutput, CommandRunner, FilesystemDataStore};
        use crate::fs::Fs;
        use crate::paths::Pather;
        use std::sync::Arc;

        struct NoopRunner;
        impl CommandRunner for NoopRunner {
            fn run(&self, _e: &str, _a: &[String]) -> Result<CommandOutput> {
                Ok(CommandOutput {
                    exit_code: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                })
            }
        }
        let runner: Arc<dyn CommandRunner> = Arc::new(NoopRunner);
        let datastore = Arc::new(FilesystemDataStore::new(
            env.fs.clone(),
            env.paths.clone(),
            runner.clone(),
        ));
        let config_manager = Arc::new(ConfigManager::new(&env.dotfiles_root).unwrap());
        ExecutionContext {
            fs: env.fs.clone() as Arc<dyn Fs>,
            datastore,
            paths: env.paths.clone() as Arc<dyn Pather>,
            config_manager,
            syntax_checker: Arc::new(crate::shell::NoopSyntaxChecker),
            command_runner: runner,
            dry_run: false,
            no_provision: true,
            provision_rerun: false,
            force: false,
            view_mode: crate::commands::ViewMode::Full,
            group_mode: crate::commands::GroupMode::Name,
            verbose: false,
        }
    }

    /// Run a real `dodot up` against a single-template pack so the
    /// baseline cache + datastore are populated the same way they
    /// would be in production. Returns the (pack_name,
    /// source_path_in_pack) pair for the test to drive.
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
        assert!(result.has_findings);
        assert_eq!(result.exit_code(), 1);

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

    // ── install_hook ────────────────────────────────────────────

    /// Stand up a fake `.git` directory inside the dotfiles_root so
    /// `install_hook` recognises the dotfiles repo as a git working
    /// tree. We don't `git init` for real because every test would
    /// pay the subprocess cost; the installer only checks for
    /// `.git` as a dir, so a bare `mkdir` suffices.
    fn fake_git_dir(env: &TempEnvironment) {
        env.fs
            .mkdir_all(&env.dotfiles_root.join(".git/hooks"))
            .unwrap();
    }

    #[test]
    fn install_hook_creates_new_pre_commit_when_absent() {
        let env = TempEnvironment::builder().build();
        fake_git_dir(&env);
        // Make sure the hooks dir exists but the hook file does not.
        let hook_path = env.dotfiles_root.join(".git/hooks/pre-commit");
        assert!(!env.fs.exists(&hook_path));

        let ctx = make_ctx(&env);
        let result = install_hook(&ctx).unwrap();
        assert!(matches!(result.outcome, InstallHookOutcome::Created));
        assert!(env.fs.exists(&hook_path));

        let body = env.fs.read_to_string(&hook_path).unwrap();
        assert!(body.starts_with("#!/bin/sh\n"), "body: {body:?}");
        assert!(body.contains(HOOK_GUARD_START), "body: {body:?}");
        assert!(body.contains(HOOK_COMMAND), "body: {body:?}");
        assert!(body.contains(HOOK_GUARD_END), "body: {body:?}");
    }

    #[test]
    fn install_hook_appends_to_existing_pre_commit() {
        // The user already has a hook (e.g. installed by another tool
        // or a personal script). install_hook must preserve that
        // content and append our block, not clobber it.
        let env = TempEnvironment::builder().build();
        fake_git_dir(&env);
        let hook_path = env.dotfiles_root.join(".git/hooks/pre-commit");
        let existing = "#!/bin/sh\necho 'my pre-commit'\nexit 0\n";
        env.fs.write_file(&hook_path, existing.as_bytes()).unwrap();

        let ctx = make_ctx(&env);
        let result = install_hook(&ctx).unwrap();
        assert!(matches!(result.outcome, InstallHookOutcome::Appended));

        let body = env.fs.read_to_string(&hook_path).unwrap();
        assert!(body.starts_with(existing), "user content lost: {body:?}");
        assert!(body.contains(HOOK_GUARD_START));
        assert!(body.contains(HOOK_COMMAND));
    }

    #[test]
    fn install_hook_is_idempotent_on_second_call() {
        // Running `dodot transform install-hook` twice in a row must
        // not double-append the block. The guard line is what makes
        // this safe.
        let env = TempEnvironment::builder().build();
        fake_git_dir(&env);
        let ctx = make_ctx(&env);

        let r1 = install_hook(&ctx).unwrap();
        assert!(matches!(r1.outcome, InstallHookOutcome::Created));

        let body_after_first = env
            .fs
            .read_to_string(&env.dotfiles_root.join(".git/hooks/pre-commit"))
            .unwrap();

        let r2 = install_hook(&ctx).unwrap();
        assert!(matches!(r2.outcome, InstallHookOutcome::AlreadyInstalled));

        let body_after_second = env
            .fs
            .read_to_string(&env.dotfiles_root.join(".git/hooks/pre-commit"))
            .unwrap();
        assert_eq!(
            body_after_first, body_after_second,
            "body changed on second call"
        );
        // Exactly one occurrence of the guard line.
        assert_eq!(body_after_second.matches(HOOK_GUARD_START).count(), 1);
    }

    #[test]
    fn install_hook_errors_if_no_git_dir() {
        // If the dotfiles root isn't a git working tree, refuse
        // with a clear error rather than silently writing a hook
        // that nothing will ever invoke.
        let env = TempEnvironment::builder().build();
        let ctx = make_ctx(&env);
        let err = install_hook(&ctx).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("no .git directory"), "msg: {msg}");
        assert!(msg.contains("git init"), "msg: {msg}");
    }

    #[test]
    fn hook_is_installed_reports_correctly() {
        let env = TempEnvironment::builder().build();
        fake_git_dir(&env);
        let ctx = make_ctx(&env);

        // No hook yet → not installed.
        assert!(!hook_is_installed(&ctx).unwrap());

        // Install it → reported as installed.
        install_hook(&ctx).unwrap();
        assert!(hook_is_installed(&ctx).unwrap());

        // A user-written hook without our guard → not installed
        // (from our perspective).
        let hook_path = env.dotfiles_root.join(".git/hooks/pre-commit");
        env.fs
            .write_file(&hook_path, b"#!/bin/sh\necho hello\n")
            .unwrap();
        assert!(!hook_is_installed(&ctx).unwrap());
    }

    #[test]
    fn install_hook_sets_executable_bit() {
        // The hook needs +x to be invoked by git. Confirm we set
        // the bit on both the create and the append paths.
        use std::os::unix::fs::PermissionsExt;

        let env = TempEnvironment::builder().build();
        fake_git_dir(&env);
        let ctx = make_ctx(&env);
        install_hook(&ctx).unwrap();

        let hook_path = env.dotfiles_root.join(".git/hooks/pre-commit");
        let mode = std::fs::metadata(&hook_path).unwrap().permissions().mode();
        // owner-execute bit must be set; we test for any execute
        // rather than exact 0o755 because the OS may apply umask.
        assert!(
            mode & 0o100 != 0,
            "hook is not executable, mode = {:o}",
            mode
        );
    }

    #[test]
    fn managed_block_is_self_contained_and_grep_detectable() {
        // The block alone should be enough to detect the install:
        // its first line is exactly HOOK_GUARD_START. This pins the
        // contract that downstream tools (or future `transform
        // uninstall-hook`) can grep for the guard line.
        let block = managed_block();
        assert!(block.starts_with(HOOK_GUARD_START));
        assert!(block.trim_end().ends_with(HOOK_GUARD_END));
        assert!(block.contains(HOOK_COMMAND));
    }
}

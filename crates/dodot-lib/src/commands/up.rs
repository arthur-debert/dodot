//! `up` command — deploy packs (create symlinks, run provisioning).
//!
//! Uses a two-phase execution model:
//! 1. **Collect** intents from all packs (no mutations).
//! 2. **Detect** cross-pack conflicts across all collected intents.
//! 3. **Execute** only if no conflicts are found.
//!
//! This prevents partial deployments where one pack silently overwrites
//! another pack's symlinks.
//!
//! ## Output rendering
//!
//! For non-dry-run executions, `up` renders by calling `status::status()`
//! on the affected packs after execution and overlaying any operation
//! errors. This guarantees that the per-file labels you see after `up`
//! match exactly what you'd see if you ran `status` immediately
//! afterward — there's a single rendering path, not two.
//!
//! Dry-run keeps the per-intent rendering since there's no
//! post-execution state to verify.

use tracing::{debug, info};

use crate::commands::{
    handler_description, handler_symbol, status, status_style, DisplayConflict, DisplayFile,
    DisplayNote, DisplayPack, PackStatusResult,
};
use crate::conflicts;
use crate::datastore::format_command_for_display;
use crate::operations::HandlerIntent;
use crate::packs::orchestration::{self, ExecutionContext, PackResult};
use crate::probe;
use crate::shell;
use crate::Result;

/// Run the `up` command: deploy packs and regenerate shell init.
///
/// Collects all intents across all packs first, checks for cross-pack
/// conflicts, then executes. If conflicts are found, **no** pack is
/// deployed and a `CrossPackConflict` error is returned — even if
/// `--force` is set, because cross-pack conflicts are a configuration
/// problem, not a deployment problem.
pub fn up(pack_filter: Option<&[String]>, ctx: &ExecutionContext) -> Result<PackStatusResult> {
    info!(
        dry_run = ctx.dry_run,
        force = ctx.force,
        no_provision = ctx.no_provision,
        "starting up command"
    );

    // Phase 1: Discover packs and collect intents
    let packs = orchestration::prepare_packs(pack_filter, ctx)?;

    let mut pack_intents: Vec<(String, Vec<HandlerIntent>)> = Vec::with_capacity(packs.len());
    let mut intent_errors: Vec<PackResult> = Vec::new();

    for pack in &packs {
        match orchestration::collect_pack_intents(pack, ctx) {
            Ok(intents) => {
                pack_intents.push((pack.display_name.clone(), intents));
            }
            Err(e) => {
                info!(pack = %pack.display_name, error = %e, "intent collection failed");
                intent_errors.push(PackResult {
                    pack_name: pack.display_name.clone(),
                    success: false,
                    operations: Vec::new(),
                    error: Some(format!("intent collection error: {e}")),
                });
            }
        }
    }

    // Phase 2: Detect cross-pack conflicts
    info!("checking for cross-pack conflicts");
    let conflicts = conflicts::detect_cross_pack_conflicts(&pack_intents, ctx.fs.as_ref());
    if !conflicts.is_empty() {
        info!(count = conflicts.len(), "cross-pack conflicts detected");
        return Err(crate::DodotError::CrossPackConflict { conflicts });
    }
    debug!("no cross-pack conflicts");

    // Phase 3: Execute intents for each pack
    let mut pack_results: Vec<PackResult> = intent_errors;

    for (pack_name, intents) in pack_intents {
        info!(pack = %pack_name, intents = intents.len(), "executing pack");
        match orchestration::execute_intents(intents, ctx) {
            Ok(operations) => {
                let success = operations.iter().all(|r| r.success);
                let succeeded = operations.iter().filter(|o| o.success).count();
                let failed = operations.iter().filter(|o| !o.success).count();
                debug!(pack = %pack_name, succeeded, failed, "pack execution complete");
                pack_results.push(PackResult {
                    pack_name,
                    success,
                    operations,
                    error: None,
                });
            }
            Err(e) => {
                info!(pack = %pack_name, error = %e, "pack execution failed");
                pack_results.push(PackResult {
                    pack_name,
                    success: false,
                    operations: Vec::new(),
                    error: Some(format!("execution error: {e}")),
                });
            }
        }
    }

    // Regenerate shell init script and deployment map
    if !ctx.dry_run {
        info!("regenerating shell init script");
        let root_config = ctx.config_manager.root_config()?;
        shell::write_init_script(
            ctx.fs.as_ref(),
            ctx.paths.as_ref(),
            root_config.profiling.enabled,
        )?;
        info!("writing deployment map");
        probe::write_deployment_map(ctx.fs.as_ref(), ctx.paths.as_ref())?;
        // Record the unix timestamp of this up so `dodot probe shell-init`
        // can flag profiles captured before it as stale. Best effort —
        // a clock skip would only affect the staleness banner, never the
        // deployment itself, so we don't fail the run on a write error.
        if let Err(e) = probe::write_last_up_marker(ctx.fs.as_ref(), ctx.paths.as_ref()) {
            debug!(error = %e, "failed to write last-up marker");
        }
        // Prune old shell-init profile reports. Cheap (one read_dir +
        // a few unlinks at most) and runs in dodot's process, not the
        // user's shell.
        let removed = probe::rotate_profiles(
            ctx.fs.as_ref(),
            ctx.paths.as_ref(),
            root_config.profiling.keep_last_runs,
        )?;
        if removed > 0 {
            debug!(removed, "pruned old shell-init profiles");
        }

        // Pre-flight syntax check: parse-only run of bash/zsh against
        // each deployed shell source so a typo in `aliases.sh` shows up
        // here instead of silently breaking next shell startup. The
        // sidecar files this writes are read back by `dodot status`.
        // The checker is injected via context so tests can stub it out.
        let report = shell::validate_shell_sources(
            ctx.fs.as_ref(),
            ctx.paths.as_ref(),
            ctx.syntax_checker.as_ref(),
        )?;
        if !report.failures.is_empty() {
            info!(
                count = report.failures.len(),
                "shell syntax check found failures"
            );
            eprintln!(
                "dodot: {} shell file{} failed pre-flight syntax check (see `dodot status`)",
                report.failures.len(),
                if report.failures.len() == 1 { "" } else { "s" }
            );
        }
        for interp in &report.missing_interpreters {
            // One-line skip notice per missing interpreter, not per
            // file. Doesn't fail the run — the file is still deployed.
            eprintln!(
                "dodot: `{interp}` not on PATH, skipped syntax check for matching shell files"
            );
        }
    }

    let has_failures = pack_results
        .iter()
        .any(|pr| !pr.success || pr.operations.iter().any(|op| !op.success));

    // Build display packs.
    //
    // For real executions, render through status::status() so the user sees
    // the same labels they'd see by running `dodot status` immediately
    // afterward. Operation failures flip their matching row's status to
    // "error" and attach a command-wide note; pack-level errors synthesize
    // an error row at the end of the pack.
    //
    // For dry-run, render the simulated operations directly — there's no
    // post-execution state to verify, and the user wants to see the planned
    // changes, not the unchanged current state.
    let (display_packs, notes) = if ctx.dry_run {
        render_intents(&pack_results, ctx.paths.home_dir())
    } else {
        let pack_names: Vec<String> = packs.iter().map(|p| p.display_name.clone()).collect();
        let status_result = status::status(Some(&pack_names), ctx)?;
        // status::status() may have populated notes (PendingConflict etc.);
        // preserve them and continue numbering from there.
        let mut notes = status_result.notes;
        let display_packs = overlay_errors(
            status_result.packs,
            &pack_results,
            ctx.paths.home_dir(),
            &mut notes,
        );
        (display_packs, notes)
    };

    let message = if has_failures {
        "Packs deployed with errors.".into()
    } else {
        "Packs deployed.".into()
    };

    Ok(PackStatusResult {
        message: Some(message),
        dry_run: ctx.dry_run,
        packs: display_packs,
        warnings: Vec::new(),
        notes,
        conflicts: Vec::new(),
        ignored_packs: Vec::new(),
        view_mode: ctx.view_mode.as_str().into(),
        group_mode: ctx.group_mode.as_str().into(),
    })
}

/// Run `up`, falling back to a status render when a cross-pack conflict
/// blocks deployment.
///
/// On a plain success, this returns `up()`'s result unchanged. On a
/// cross-pack conflict it re-runs the status scan and folds the
/// conflicts in, so the caller gets the full per-pack file listing plus
/// the conflicts section — the same view `dodot status` produces — with
/// a top-level message explaining that nothing was deployed. Other
/// errors propagate unchanged.
///
/// The CLI uses this so `dodot up` and `dodot status` look identical
/// when a cross-pack conflict is present, rather than stripping the
/// per-pack rows down to a bare conflict dump.
pub fn up_or_status_for_conflict(
    pack_filter: Option<&[String]>,
    ctx: &ExecutionContext,
) -> Result<PackStatusResult> {
    match up(pack_filter, ctx) {
        Ok(r) => Ok(r),
        Err(crate::DodotError::CrossPackConflict { conflicts: raw }) => {
            let home = ctx.paths.home_dir();
            let display_conflicts: Vec<DisplayConflict> = raw
                .iter()
                .map(|c| DisplayConflict::from_conflict(c, home))
                .collect();
            let mut base = status::status(pack_filter, ctx)?;
            base.message = Some("Cross-pack conflicts prevent deployment.".into());
            base.dry_run = ctx.dry_run;
            base.conflicts = display_conflicts;
            Ok(base)
        }
        Err(e) => Err(e),
    }
}

/// Render operations directly from pack_results — used for dry-run, where
/// there's no executed state to verify and the user wants to see the
/// planned changes rather than the unchanged status quo.
///
/// Returns (packs, notes). Failed operations keep their row but receive a
/// `note_ref` into the command-wide notes list, keeping the column layout
/// intact.
fn render_intents(
    pack_results: &[PackResult],
    home: &std::path::Path,
) -> (Vec<DisplayPack>, Vec<DisplayNote>) {
    let mut notes: Vec<DisplayNote> = Vec::new();
    let packs = pack_results
        .iter()
        .map(|pr| {
            let mut files: Vec<DisplayFile> = pr
                .operations
                .iter()
                .map(|op| {
                    let (handler, name, user_target) = extract_op_info(&op.operation, home);
                    let (status, status_label, note_ref) = if op.success {
                        (status_style(true).to_string(), op.message.clone(), None)
                    } else {
                        notes.push(DisplayNote {
                            body: op.message.clone(),
                            hint: None,
                        });
                        (
                            "error".to_string(),
                            "error".to_string(),
                            Some(notes.len() as u32),
                        )
                    };
                    DisplayFile {
                        name: name.clone(),
                        symbol: handler_symbol(&handler).into(),
                        description: handler_description(&handler, &name, user_target.as_deref()),
                        status,
                        status_label,
                        handler,
                        note_ref,
                    }
                })
                .collect();

            if let Some(err) = &pr.error {
                notes.push(DisplayNote {
                    body: err.clone(),
                    hint: None,
                });
                files.push(DisplayFile {
                    name: String::new(),
                    symbol: "×".into(),
                    description: String::new(),
                    status: "error".into(),
                    status_label: "error".into(),
                    handler: String::new(),
                    note_ref: Some(notes.len() as u32),
                });
            }

            DisplayPack::new(pr.pack_name.clone(), files)
        })
        .collect();
    (packs, notes)
}

/// Take the steady-state DisplayPacks produced by `status::status()` and
/// flip the matching row's status to "error" for any failed operation,
/// attaching a note with the full error body. Pack-level errors (intent
/// collection, execution) synthesize a dedicated error row at the end of
/// the pack. All notes share a single 1-based command-wide index.
pub(crate) fn overlay_errors(
    mut packs: Vec<DisplayPack>,
    pack_results: &[PackResult],
    home: &std::path::Path,
    notes: &mut Vec<DisplayNote>,
) -> Vec<DisplayPack> {
    for pr in pack_results {
        let display_pack = match packs.iter_mut().find(|p| p.name == pr.pack_name) {
            Some(p) => p,
            None => continue,
        };

        for op_result in &pr.operations {
            if op_result.success {
                continue;
            }
            let (handler, name, user_target) = extract_op_info(&op_result.operation, home);
            let body = op_result.message.clone();

            // Prefer to flip the existing status row so the file listing
            // stays one line per item. Match order:
            //   1. (handler, name) — exact match by pack-relative basename.
            //      Correct when the operation's source basename equals the
            //      file row's pack-relative path (flat layout, common case).
            //   2. (handler, user_target) — covers the pre-link CreateUserLink
            //      conflict case where datastore_path is defaulted (empty)
            //      and the op's "name" comes from user_path.file_name(),
            //      which won't match a `home.X` or subdir pack row. The
            //      row's description is the shortened user_target, so that
            //      matches what the op tried to write.
            //   3. Fallback: match by name only (any handler).
            //   4. Synthesize a new error row if nothing matched.
            let pos = display_pack
                .files
                .iter()
                .position(|f| f.handler == handler && f.name == name)
                .or_else(|| {
                    user_target.as_ref().and_then(|ut| {
                        display_pack
                            .files
                            .iter()
                            .position(|f| f.handler == handler && &f.description == ut)
                    })
                })
                .or_else(|| display_pack.files.iter().position(|f| f.name == name));

            match pos {
                Some(idx) => {
                    // If the row already carries a note (e.g. status flagged
                    // it as PendingConflict), replace that note in place so
                    // numbering stays contiguous and we don't leave a stale
                    // "would conflict" note alongside the actual failure.
                    let file = &mut display_pack.files[idx];
                    if let Some(existing) = file.note_ref {
                        notes[(existing - 1) as usize] = DisplayNote { body, hint: None };
                    } else {
                        notes.push(DisplayNote { body, hint: None });
                        file.note_ref = Some(notes.len() as u32);
                    }
                    file.status = "error".into();
                    file.status_label = "error".into();
                }
                None => {
                    notes.push(DisplayNote { body, hint: None });
                    display_pack.files.push(DisplayFile {
                        name: name.clone(),
                        symbol: handler_symbol(&handler).into(),
                        description: handler_description(&handler, &name, user_target.as_deref()),
                        status: "error".into(),
                        status_label: "error".into(),
                        handler,
                        note_ref: Some(notes.len() as u32),
                    });
                }
            }
        }

        if let Some(err) = &pr.error {
            // Pack-level errors (intent collection failure, orchestration
            // failure bubbled up from execute_intents) don't name a
            // specific file. Attach the note to an existing row so the
            // user can tell which item the failure relates to. Prefer a
            // row that isn't already flipped to error (otherwise we'd
            // clobber a more specific per-op note); if no row qualifies,
            // synthesize a pack-level error row as a last resort.
            let fallback_idx = if display_pack.files.is_empty() {
                None
            } else {
                Some(0)
            };
            let target_idx = display_pack
                .files
                .iter()
                .position(|f| f.status != "error")
                .or(fallback_idx);
            let body = err.clone();
            match target_idx {
                Some(idx) => {
                    let file = &mut display_pack.files[idx];
                    if let Some(existing) = file.note_ref {
                        notes[(existing - 1) as usize] = DisplayNote { body, hint: None };
                    } else {
                        notes.push(DisplayNote { body, hint: None });
                        file.note_ref = Some(notes.len() as u32);
                    }
                    file.status = "error".into();
                    file.status_label = "error".into();
                }
                None => {
                    notes.push(DisplayNote { body, hint: None });
                    display_pack.files.push(DisplayFile {
                        name: String::new(),
                        symbol: "×".into(),
                        description: String::new(),
                        status: "error".into(),
                        status_label: "error".into(),
                        handler: String::new(),
                        note_ref: Some(notes.len() as u32),
                    });
                }
            }
        }
    }
    for pack in &mut packs {
        pack.recompute_summary();
    }
    packs
}

/// Extract handler name, display name, and optional user target from an operation.
fn extract_op_info(
    op: &crate::operations::Operation,
    home: &std::path::Path,
) -> (String, String, Option<String>) {
    match op {
        crate::operations::Operation::CreateDataLink {
            handler, source, ..
        } => (
            handler.clone(),
            source
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            None,
        ),
        crate::operations::Operation::CreateUserLink {
            handler,
            datastore_path,
            user_path,
            ..
        } => {
            // Name: filename from the datastore path (pack-relative name)
            let name = datastore_path
                .file_name()
                .unwrap_or_else(|| user_path.file_name().unwrap_or_default())
                .to_string_lossy()
                .into_owned();
            // Target: user_path displayed relative to ~ for readability
            let target = if let Ok(rel) = user_path.strip_prefix(home) {
                format!("~/{}", rel.display())
            } else {
                user_path.display().to_string()
            };
            (handler.clone(), name, Some(target))
        }
        crate::operations::Operation::RunCommand {
            handler,
            executable,
            arguments,
            ..
        } => (
            handler.clone(),
            format_command_for_display(executable, arguments),
            None,
        ),
        crate::operations::Operation::CheckSentinel {
            handler, sentinel, ..
        } => (handler.clone(), sentinel.clone(), None),
    }
}

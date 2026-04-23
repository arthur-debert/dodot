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
    handler_description, handler_symbol, status, status_style, DisplayFile, DisplayPack,
    PackStatusResult,
};
use crate::conflicts;
use crate::datastore::format_command_for_display;
use crate::operations::HandlerIntent;
use crate::packs::orchestration::{self, ExecutionContext, PackResult};
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
                pack_intents.push((pack.name.clone(), intents));
            }
            Err(e) => {
                info!(pack = %pack.name, error = %e, "intent collection failed");
                intent_errors.push(PackResult {
                    pack_name: pack.name.clone(),
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

    // Regenerate shell init script
    if !ctx.dry_run {
        info!("regenerating shell init script");
        shell::write_init_script(ctx.fs.as_ref(), ctx.paths.as_ref())?;
    }

    let has_failures = pack_results
        .iter()
        .any(|pr| !pr.success || pr.operations.iter().any(|op| !op.success));

    // Build display packs.
    //
    // For real executions, render through status::status() so the user sees
    // the same labels they'd see by running `dodot status` immediately
    // afterward. Operation failures are overlaid as additional rows.
    //
    // For dry-run, render the simulated operations directly — there's no
    // post-execution state to verify, and the user wants to see the planned
    // changes, not the unchanged current state.
    let display_packs = if ctx.dry_run {
        render_intents(&pack_results, ctx.paths.home_dir())
    } else {
        let pack_names: Vec<String> = packs.iter().map(|p| p.name.clone()).collect();
        let status_result = status::status(Some(&pack_names), ctx)?;
        overlay_errors(status_result.packs, &pack_results, ctx.paths.home_dir())
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
        conflicts: Vec::new(),
        ignored_packs: Vec::new(),
    })
}

/// Render operations directly from pack_results — used for dry-run, where
/// there's no executed state to verify and the user wants to see the
/// planned changes rather than the unchanged status quo.
fn render_intents(pack_results: &[PackResult], home: &std::path::Path) -> Vec<DisplayPack> {
    pack_results
        .iter()
        .map(|pr| {
            let mut files: Vec<DisplayFile> = pr
                .operations
                .iter()
                .map(|op| {
                    let (handler, name, user_target) = extract_op_info(&op.operation, home);
                    let status = if op.success {
                        status_style(true).into()
                    } else {
                        "error".into()
                    };
                    DisplayFile {
                        name: name.clone(),
                        symbol: handler_symbol(&handler).into(),
                        description: handler_description(&handler, &name, user_target.as_deref()),
                        status,
                        status_label: op.message.clone(),
                        handler,
                    }
                })
                .collect();

            if let Some(err) = &pr.error {
                files.push(DisplayFile {
                    name: String::new(),
                    symbol: "×".into(),
                    description: String::new(),
                    status: "error".into(),
                    status_label: err.clone(),
                    handler: String::new(),
                });
            }

            DisplayPack {
                name: pr.pack_name.clone(),
                files,
                footnotes: Vec::new(),
            }
        })
        .collect()
}

/// Take the steady-state DisplayPacks produced by `status::status()` and
/// append error rows for any failed operations or pack-level errors. The
/// status rows still describe what *is* on disk; the appended rows
/// describe what *failed* during the just-completed execution.
pub(crate) fn overlay_errors(
    mut packs: Vec<DisplayPack>,
    pack_results: &[PackResult],
    home: &std::path::Path,
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
            display_pack.files.push(DisplayFile {
                name: name.clone(),
                symbol: handler_symbol(&handler).into(),
                description: handler_description(&handler, &name, user_target.as_deref()),
                status: "error".into(),
                status_label: op_result.message.clone(),
                handler,
            });
        }

        if let Some(err) = &pr.error {
            display_pack.files.push(DisplayFile {
                name: String::new(),
                symbol: "×".into(),
                description: String::new(),
                status: "error".into(),
                status_label: err.clone(),
                handler: String::new(),
            });
        }
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

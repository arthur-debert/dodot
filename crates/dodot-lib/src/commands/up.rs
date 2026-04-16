//! `up` command — deploy packs (create symlinks, run provisioning).
//!
//! Uses a two-phase execution model:
//! 1. **Collect** intents from all packs (no mutations).
//! 2. **Detect** cross-pack conflicts across all collected intents.
//! 3. **Execute** only if no conflicts are found.
//!
//! This prevents partial deployments where one pack silently overwrites
//! another pack's symlinks.

use crate::commands::{
    handler_description, handler_symbol, status_style, DisplayFile, DisplayPack, PackStatusResult,
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
    let conflicts = conflicts::detect_cross_pack_conflicts(&pack_intents);
    if !conflicts.is_empty() {
        return Err(crate::DodotError::CrossPackConflict { conflicts });
    }

    // Phase 3: Execute intents for each pack
    let mut pack_results: Vec<PackResult> = intent_errors;

    for (pack_name, intents) in pack_intents {
        match orchestration::execute_intents(intents, ctx) {
            Ok(operations) => {
                let success = operations.iter().all(|r| r.success);
                pack_results.push(PackResult {
                    pack_name,
                    success,
                    operations,
                    error: None,
                });
            }
            Err(e) => {
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
        shell::write_init_script(ctx.fs.as_ref(), ctx.paths.as_ref())?;
    }

    // Convert to display format
    let home = ctx.paths.home_dir();
    let display_packs: Vec<DisplayPack> = pack_results
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

            // Include error from orchestration (e.g. pack-level config error)
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
            }
        })
        .collect();

    // Message reflects actual results
    let has_failures = pack_results
        .iter()
        .any(|pr| !pr.success || pr.operations.iter().any(|op| !op.success));
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
    })
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

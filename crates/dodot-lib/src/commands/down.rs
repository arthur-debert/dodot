//! `down` command — remove all deployed state for packs.
//!
//! Output rendering: same principle as `up` — for real removals, render
//! through `status::status()` so the per-file labels match what `dodot
//! status` would show. After `down`, files appear in their `Pending`
//! handler-specific form (`not in PATH`, `not sourced`, `pending`,
//! `never run`). The action itself is communicated via the message
//! line.
//!
//! Dry-run keeps the per-handler "would remove" rendering.

use tracing::{debug, info};

use crate::commands::{handler_symbol, status, DisplayFile, DisplayPack, PackStatusResult};
use crate::handlers::HANDLER_SYMLINK;
use crate::packs;
use crate::packs::orchestration::{self, ExecutionContext};
use crate::probe;
use crate::shell;
use crate::Result;

/// Run the `down` command: remove all state for specified (or all) packs.
pub fn down(pack_filter: Option<&[String]>, ctx: &ExecutionContext) -> Result<PackStatusResult> {
    info!(dry_run = ctx.dry_run, "starting down command");

    // Validate pack names before doing anything
    let mut warnings = Vec::new();
    if let Some(names) = pack_filter {
        warnings = orchestration::validate_pack_names(names, ctx)?;
    }

    let root_config = ctx.config_manager.root_config()?;
    let mut all_packs = packs::discover_packs(
        ctx.fs.as_ref(),
        ctx.paths.dotfiles_root(),
        &root_config.pack.ignore,
    )?;
    info!(count = all_packs.len(), "discovered packs");

    if let Some(names) = pack_filter {
        all_packs.retain(|p| names.iter().any(|n| n == &p.display_name || n == &p.name));
    }

    let mut affected_packs = Vec::new();
    let mut dry_run_display: Vec<DisplayPack> = Vec::new();
    let mut any_removed = false;

    for pack in &all_packs {
        // Datastore is keyed by the on-disk directory name, not the
        // display name — the directory `010-nvim` keeps its `010-nvim/`
        // subtree in the datastore.
        let handlers = ctx.datastore.list_pack_handlers(&pack.name)?;

        if handlers.is_empty() {
            debug!(pack = %pack.display_name, "already down, skipping");
            continue;
        }

        info!(pack = %pack.display_name, handlers = ?handlers, "removing pack state");
        any_removed = true;
        affected_packs.push(pack.display_name.clone());

        if ctx.dry_run {
            dry_run_display.push(build_dry_run_display(pack, &handlers, ctx)?);
        } else {
            for handler in &handlers {
                ctx.datastore.remove_state(&pack.name, handler)?;
            }
        }
    }

    // Regenerate shell init script and deployment map (now reflecting
    // the removed state).
    if !ctx.dry_run {
        info!("regenerating shell init script");
        shell::write_init_script(
            ctx.fs.as_ref(),
            ctx.paths.as_ref(),
            root_config.profiling.enabled,
        )?;
        info!("writing deployment map");
        probe::write_deployment_map(ctx.fs.as_ref(), ctx.paths.as_ref())?;
    }

    let display_packs = if ctx.dry_run {
        dry_run_display
    } else {
        // Render through status — files for removed packs will now show as
        // pending in their handler-specific vocabulary.
        status::status(Some(&affected_packs), ctx)?.packs
    };

    let message = if any_removed {
        "Packs deactivated."
    } else {
        "Nothing to deactivate."
    };

    Ok(PackStatusResult {
        message: Some(message.into()),
        dry_run: ctx.dry_run,
        packs: display_packs,
        warnings,
        notes: Vec::new(),
        conflicts: Vec::new(),
        ignored_packs: Vec::new(),
        inactive_packs: Vec::new(),
        view_mode: ctx.view_mode.as_str().into(),
        group_mode: ctx.group_mode.as_str().into(),
        diffs: Vec::new(),
    })
}

/// Build the per-pack dry-run display: lists what would be removed,
/// per-handler. For symlink handlers we list individual data-link entries
/// since the user usually wants to know which files would be affected.
fn build_dry_run_display(
    pack: &packs::Pack,
    handlers: &[String],
    ctx: &ExecutionContext,
) -> Result<DisplayPack> {
    let mut files = Vec::new();
    for handler in handlers {
        if handler == HANDLER_SYMLINK {
            let handler_dir = ctx.paths.handler_data_dir(&pack.name, handler);
            let entries = ctx.fs.read_dir(&handler_dir)?;
            for entry in entries {
                files.push(DisplayFile {
                    name: entry.name.clone(),
                    symbol: handler_symbol(handler).into(),
                    description: "state would be removed".into(),
                    status: "pending".into(),
                    status_label: "[dry-run] would remove".into(),
                    handler: handler.clone(),
                    note_ref: None,
                });
            }
        } else {
            files.push(DisplayFile {
                name: handler.clone(),
                symbol: handler_symbol(handler).into(),
                description: "state would be removed".into(),
                status: "pending".into(),
                status_label: "[dry-run] would remove".into(),
                handler: handler.clone(),
                note_ref: None,
            });
        }
    }
    Ok(DisplayPack::new(pack.display_name.clone(), files))
}

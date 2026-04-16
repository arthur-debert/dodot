//! `down` command — remove all deployed state for packs.

use tracing::{debug, info};

use crate::commands::{handler_symbol, DisplayFile, DisplayPack, PackStatusResult};
use crate::handlers::HANDLER_SYMLINK;
use crate::packs;
use crate::packs::orchestration::{self, ExecutionContext};
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
        all_packs.retain(|p| names.iter().any(|n| n == &p.name));
    }

    let mut display_packs = Vec::new();
    let mut any_removed = false;

    for pack in &all_packs {
        let handlers = ctx.datastore.list_pack_handlers(&pack.name)?;

        if handlers.is_empty() {
            debug!(pack = %pack.name, "already down, skipping");
            continue;
        }

        info!(pack = %pack.name, handlers = ?handlers, "removing pack state");
        any_removed = true;
        let mut files = Vec::new();

        for handler in &handlers {
            // For symlink handler, list individual files (#14)
            if handler == HANDLER_SYMLINK {
                let handler_dir = ctx.paths.handler_data_dir(&pack.name, handler);
                let entries = ctx.fs.read_dir(&handler_dir)?;
                for entry in entries {
                    let label = if ctx.dry_run {
                        "[dry-run] would remove"
                    } else {
                        "removed"
                    };
                    let status = if ctx.dry_run { "pending" } else { "deployed" };
                    files.push(DisplayFile {
                        name: entry.name.clone(),
                        symbol: handler_symbol(handler).into(),
                        description: "state removed".into(),
                        status: status.into(),
                        status_label: label.into(),
                        handler: handler.clone(),
                    });
                }
            } else {
                // Non-symlink handlers: show handler name
                if ctx.dry_run {
                    files.push(DisplayFile {
                        name: handler.clone(),
                        symbol: handler_symbol(handler).into(),
                        description: "state will be removed".into(),
                        status: "pending".into(),
                        status_label: "[dry-run] would remove".into(),
                        handler: handler.clone(),
                    });
                } else {
                    files.push(DisplayFile {
                        name: handler.clone(),
                        symbol: handler_symbol(handler).into(),
                        description: "state removed".into(),
                        status: "deployed".into(),
                        status_label: "removed".into(),
                        handler: handler.clone(),
                    });
                }
            }

            if !ctx.dry_run {
                ctx.datastore.remove_state(&pack.name, handler)?;
            }
        }

        display_packs.push(DisplayPack {
            name: pack.name.clone(),
            files,
        });
    }

    // Regenerate shell init script (now empty for removed packs)
    if !ctx.dry_run {
        info!("regenerating shell init script");
        shell::write_init_script(ctx.fs.as_ref(), ctx.paths.as_ref())?;
    }

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
    })
}

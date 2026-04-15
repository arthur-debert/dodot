//! `down` command — remove all deployed state for packs.

use crate::commands::{handler_symbol, DisplayFile, DisplayPack, PackStatusResult};
use crate::packs;
use crate::packs::orchestration::ExecutionContext;
use crate::shell;
use crate::Result;

/// Run the `down` command: remove all state for specified (or all) packs.
pub fn down(pack_filter: Option<&[String]>, ctx: &ExecutionContext) -> Result<PackStatusResult> {
    let root_config = ctx.config_manager.root_config()?;
    let mut all_packs = packs::discover_packs(
        ctx.fs.as_ref(),
        ctx.paths.dotfiles_root(),
        &root_config.pack.ignore,
    )?;

    if let Some(names) = pack_filter {
        all_packs.retain(|p| names.iter().any(|n| n == &p.name));
    }

    let mut display_packs = Vec::new();

    for pack in &all_packs {
        let handlers = ctx.datastore.list_pack_handlers(&pack.name)?;

        let mut files = Vec::new();
        for handler in &handlers {
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
                ctx.datastore.remove_state(&pack.name, handler)?;
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

        display_packs.push(DisplayPack {
            name: pack.name.clone(),
            files,
            error: None,
        });
    }

    // Regenerate shell init script (now empty for removed packs)
    if !ctx.dry_run {
        shell::write_init_script(ctx.fs.as_ref(), ctx.paths.as_ref())?;
    }

    Ok(PackStatusResult {
        message: Some("Packs deactivated.".into()),
        dry_run: ctx.dry_run,
        packs: display_packs,
    })
}

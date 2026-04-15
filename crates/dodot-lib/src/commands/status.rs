//! `status` command — shows current deployment state without making changes.

use crate::commands::{
    handler_description, handler_symbol, status_label, status_style, DisplayFile, DisplayPack,
    PackStatusResult,
};
use crate::config::mappings_to_rules;
use crate::handlers::{self};
use crate::packs::orchestration::ExecutionContext;
use crate::packs::{self};
use crate::rules::Scanner;
use crate::Result;

/// Run the `status` command: scan packs and check handler deployment state.
pub fn status(pack_filter: Option<&[String]>, ctx: &ExecutionContext) -> Result<PackStatusResult> {
    let root_config = ctx.config_manager.root_config()?;
    let mut all_packs = packs::discover_packs(
        ctx.fs.as_ref(),
        ctx.paths.dotfiles_root(),
        &root_config.pack.ignore,
    )?;

    if let Some(names) = pack_filter {
        all_packs.retain(|p| names.iter().any(|n| n == &p.name));
    }

    let registry = handlers::create_registry(ctx.fs.as_ref());
    let mut display_packs = Vec::new();

    for mut pack in all_packs {
        let pack_config = ctx.config_manager.config_for_pack(&pack.path)?;
        pack.config = pack_config.to_handler_config();
        let rules = mappings_to_rules(&pack_config.mappings);

        let scanner = Scanner::new(ctx.fs.as_ref());
        let matches = scanner.scan_pack(&pack, &rules, &pack_config.pack.ignore)?;

        let mut files = Vec::new();
        for m in &matches {
            let handler = registry.get(m.handler.as_str());
            let deployed = if let Some(h) = handler {
                h.check_status(&m.absolute_path, &pack.name, ctx.datastore.as_ref())
                    .map(|s| s.deployed)
                    .unwrap_or(false)
            } else {
                false
            };

            files.push(DisplayFile {
                name: m.relative_path.to_string_lossy().into_owned(),
                symbol: handler_symbol(&m.handler).into(),
                description: handler_description(
                    &m.handler,
                    &m.relative_path.to_string_lossy(),
                    None,
                ),
                status: status_style(deployed).into(),
                status_label: status_label(&m.handler, deployed),
                handler: m.handler.clone(),
            });
        }

        display_packs.push(DisplayPack {
            name: pack.name.clone(),
            files,
            error: None,
        });
    }

    Ok(PackStatusResult {
        message: None,
        dry_run: false,
        packs: display_packs,
    })
}

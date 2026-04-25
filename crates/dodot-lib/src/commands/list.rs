//! `list` command — show all available packs.

use serde::Serialize;

use crate::packs;
use crate::packs::orchestration::ExecutionContext;
use crate::Result;

#[derive(Debug, Clone, Serialize)]
pub struct ListResult {
    pub packs: Vec<ListPack>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListPack {
    /// User-facing pack name. For prefixed packs (`010-nvim`) this is
    /// the stripped form (`nvim`); for unprefixed packs it equals the
    /// directory name.
    pub name: String,
    pub ignored: bool,
}

/// List all packs in the dotfiles root.
///
/// Packs appear in the order dodot would apply them (lexicographic by
/// on-disk directory name); see the `packs` module docs for the pack
/// ordering contract. Goes through [`packs::scan_packs`] so list
/// output respects the same `pack.ignore` patterns and surfaces the
/// same scan-time errors (empty-stem prefix directories, ordering
/// collisions) that every other command does — `dodot list` should
/// never show ambiguous duplicates that `dodot up` would refuse.
pub fn list(ctx: &ExecutionContext) -> Result<ListResult> {
    let root_config = ctx.config_manager.root_config()?;
    let scanned = packs::scan_packs(
        ctx.fs.as_ref(),
        ctx.paths.dotfiles_root(),
        &root_config.pack.ignore,
    )?;

    // Two streams in: active packs (already carry display_name) and
    // dodotignore-marked dirs (raw names). Merge into one list with
    // the `ignored` flag, preserving lex order on the on-disk name
    // so the displayed order still matches deploy order.
    let mut entries: Vec<(String, ListPack)> = Vec::new();
    for p in scanned.packs {
        entries.push((
            p.name.clone(),
            ListPack {
                name: p.display_name,
                ignored: false,
            },
        ));
    }
    for dir in scanned.ignored {
        let display = packs::display_name_for(&dir).to_string();
        entries.push((
            dir,
            ListPack {
                name: display,
                ignored: true,
            },
        ));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    Ok(ListResult {
        packs: entries.into_iter().map(|(_, p)| p).collect(),
    })
}

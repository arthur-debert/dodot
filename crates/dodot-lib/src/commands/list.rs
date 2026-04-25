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
/// ordering contract.
pub fn list(ctx: &ExecutionContext) -> Result<ListResult> {
    let _root_config = ctx.config_manager.root_config()?;

    // Get all directories (including ignored ones for display)
    let entries = ctx.fs.read_dir(ctx.paths.dotfiles_root())?;
    let mut entries: Vec<_> = entries
        .into_iter()
        .filter(|e| {
            e.is_dir
                && (!e.name.starts_with('.') || e.name == ".config")
                && is_valid_pack_name(&e.name)
        })
        .collect();

    // Sort by raw on-disk name so the displayed order matches deploy order.
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    let packs = entries
        .into_iter()
        .map(|entry| {
            let ignored = ctx.fs.exists(&entry.path.join(".dodotignore"));
            ListPack {
                name: packs::display_name_for(&entry.name).to_string(),
                ignored,
            }
        })
        .collect();

    Ok(ListResult { packs })
}

fn is_valid_pack_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
}

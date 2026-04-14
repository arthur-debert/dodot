//! `list` command — show all available packs.

use serde::Serialize;

use crate::packs::orchestration::ExecutionContext;
use crate::Result;

#[derive(Debug, Clone, Serialize)]
pub struct ListResult {
    pub packs: Vec<ListPack>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListPack {
    pub name: String,
    pub ignored: bool,
}

/// List all packs in the dotfiles root.
pub fn list(ctx: &ExecutionContext) -> Result<ListResult> {
    let _root_config = ctx.config_manager.root_config()?;

    // Get all directories (including ignored ones for display)
    let entries = ctx.fs.read_dir(ctx.paths.dotfiles_root())?;
    let mut packs = Vec::new();

    for entry in entries {
        if !entry.is_dir {
            continue;
        }
        if entry.name.starts_with('.') && entry.name != ".config" {
            continue;
        }
        if !is_valid_pack_name(&entry.name) {
            continue;
        }

        let ignored = ctx.fs.exists(&entry.path.join(".dodotignore"));
        packs.push(ListPack {
            name: entry.name,
            ignored,
        });
    }

    packs.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(ListResult { packs })
}

fn is_valid_pack_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
}

//! Pack-name resolution.
//!
//! User-supplied pack identifiers come in two flavours: the on-disk
//! directory name (`010-nvim`) and the display name (`nvim`). These
//! helpers map either form onto a concrete on-disk directory, and
//! validate a list of names before any orchestration loop touches the
//! filesystem.

use crate::packs;
use crate::packs::context::ExecutionContext;

/// Resolve a user-typed pack identifier to its on-disk directory
/// name. Tries display name first, falls back to the raw on-disk
/// name — so `dodot adopt nvim` and `dodot adopt 010-nvim` both find
/// the same pack on disk.
///
/// Use this in commands that take a single pack-name argument and
/// then need the directory path for filesystem or datastore work
/// (`adopt`, `addignore`, `fill`). Errors with [`DodotError::PackNotFound`]
/// when no match exists, and resolves through both active and ignored
/// packs (the caller decides whether being ignored is fatal).
pub fn resolve_pack_dir_name(input: &str, ctx: &ExecutionContext) -> crate::Result<String> {
    let root_config = ctx.config_manager.root_config()?;
    let scanned = packs::scan_packs(
        ctx.fs.as_ref(),
        ctx.paths.dotfiles_root(),
        &root_config.pack.ignore,
    )?;
    if let Some(p) = scanned
        .packs
        .iter()
        .find(|p| p.display_name == *input || p.name == *input)
    {
        return Ok(p.name.clone());
    }
    if let Some(dir) = scanned
        .ignored
        .iter()
        .find(|d| d.as_str() == input || packs::display_name_for(d) == input)
    {
        return Ok(dir.clone());
    }
    Err(crate::DodotError::PackNotFound { name: input.into() })
}

/// Validate that requested pack names exist. Returns error for nonexistent
/// packs and collects warnings for ignored packs.
///
/// Names resolve against the pack's *display name* (e.g. `nvim` for an
/// on-disk `010-nvim`) first, then fall back to the raw on-disk name —
/// so `dodot up nvim` and `dodot up 010-nvim` both find the same pack.
/// The display name is the recommended form.
pub fn validate_pack_names(names: &[String], ctx: &ExecutionContext) -> crate::Result<Vec<String>> {
    let root_config = ctx.config_manager.root_config()?;
    let scanned = packs::scan_packs(
        ctx.fs.as_ref(),
        ctx.paths.dotfiles_root(),
        &root_config.pack.ignore,
    )?;

    let mut warnings = Vec::new();
    for input in names {
        if scanned
            .packs
            .iter()
            .any(|p| p.display_name == *input || p.name == *input)
        {
            continue;
        }
        if scanned
            .ignored
            .iter()
            .any(|dir| dir == input || packs::display_name_for(dir) == input)
        {
            warnings.push(format!("warning: pack '{}' is ignored, skipping", input));
            continue;
        }
        return Err(crate::DodotError::PackNotFound {
            name: input.clone(),
        });
    }
    Ok(warnings)
}

#[cfg(test)]
mod tests {
    #![allow(unused_imports)]

    use super::super::execute;
    use super::super::test_support::{make_context, TestUpCommand};
    use super::resolve_pack_dir_name;
    use crate::testing::TempEnvironment;

    #[test]
    fn resolve_pack_dir_name_finds_pack_by_display_name() {
        let env = TempEnvironment::builder()
            .pack("010-nvim")
            .file("init.lua", "x")
            .done()
            .build();

        let ctx = make_context(&env);
        let resolved = resolve_pack_dir_name("nvim", &ctx).unwrap();
        assert_eq!(resolved, "010-nvim");
    }

    #[test]
    fn resolve_pack_dir_name_finds_pack_by_raw_directory_name() {
        let env = TempEnvironment::builder()
            .pack("010-nvim")
            .file("init.lua", "x")
            .done()
            .build();

        let ctx = make_context(&env);
        let resolved = resolve_pack_dir_name("010-nvim", &ctx).unwrap();
        assert_eq!(resolved, "010-nvim");
    }

    #[test]
    fn resolve_pack_dir_name_errors_on_unknown_pack() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .build();

        let ctx = make_context(&env);
        let err = resolve_pack_dir_name("nope", &ctx).unwrap_err();
        assert!(matches!(
            err,
            crate::DodotError::PackNotFound { ref name } if name == "nope"
        ));
    }
}

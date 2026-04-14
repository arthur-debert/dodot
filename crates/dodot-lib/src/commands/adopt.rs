//! `adopt` command — move existing files into a pack, creating symlinks back.

use std::path::PathBuf;

use serde::Serialize;

use crate::packs::orchestration::ExecutionContext;
use crate::{DodotError, Result};

#[derive(Debug, Clone, Serialize)]
pub struct AdoptResult {
    pub message: String,
    pub details: Vec<String>,
}

/// Move files from their current location into a pack, creating
/// symlinks from the original location back to the pack.
pub fn adopt(
    pack_name: &str,
    files: &[PathBuf],
    force: bool,
    ctx: &ExecutionContext,
) -> Result<AdoptResult> {
    let pack_path = ctx.paths.pack_path(pack_name);

    if !ctx.fs.exists(&pack_path) {
        return Err(DodotError::PackNotFound {
            name: pack_name.into(),
        });
    }

    let mut details = Vec::new();

    for file in files {
        if !ctx.fs.exists(file) {
            return Err(DodotError::Fs {
                path: file.clone(),
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
            });
        }

        let filename = file
            .file_name()
            .ok_or_else(|| DodotError::Other(format!("no filename: {}", file.display())))?;

        // Strip leading dot for pack organization
        let pack_filename = filename.to_string_lossy();
        let pack_filename = pack_filename.strip_prefix('.').unwrap_or(&pack_filename);
        let dest = pack_path.join(pack_filename);

        if ctx.fs.exists(&dest) && !force {
            return Err(DodotError::SymlinkConflict { path: dest });
        }

        // Move file into pack
        ctx.fs.rename(file, &dest)?;

        // Create symlink from original location to pack
        ctx.fs.symlink(&dest, file)?;

        details.push(format!(
            "{} → {}/{}",
            file.display(),
            pack_name,
            pack_filename
        ));
    }

    Ok(AdoptResult {
        message: format!(
            "Adopted {} file(s) into '{pack_name}'.",
            files.len()
        ),
        details,
    })
}

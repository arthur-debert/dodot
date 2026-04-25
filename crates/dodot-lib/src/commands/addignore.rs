//! `addignore` command — mark a pack as ignored.

use serde::Serialize;

use crate::packs::orchestration::{self, ExecutionContext};
use crate::{DodotError, Result};

#[derive(Debug, Clone, Serialize)]
pub struct AddIgnoreResult {
    pub message: String,
    pub details: Vec<String>,
}

/// Create a `.dodotignore` file in the pack directory.
pub fn addignore(pack_name: &str, ctx: &ExecutionContext) -> Result<AddIgnoreResult> {
    // Resolve the user's input (display name or raw directory name) to
    // the on-disk directory name; the datastore is keyed by the latter.
    let pack_dir = orchestration::resolve_pack_dir_name(pack_name, ctx)?;
    let pack_path = ctx.paths.pack_path(&pack_dir);

    if !ctx.fs.exists(&pack_path) {
        return Err(DodotError::PackNotFound {
            name: pack_name.into(),
        });
    }

    let display = crate::packs::display_name_for(&pack_dir);
    let ignore_path = pack_path.join(".dodotignore");

    if ctx.fs.exists(&ignore_path) {
        return Ok(AddIgnoreResult {
            message: format!("Pack '{display}' is already ignored."),
            details: vec![],
        });
    }

    ctx.fs.write_file(&ignore_path, b"")?;

    // Check if pack is currently deployed and warn (#18)
    let handlers = ctx.datastore.list_pack_handlers(&pack_dir)?;
    let mut details = vec![format!("Created {}", ignore_path.display())];
    if !handlers.is_empty() {
        details.push(format!(
            "Warning: pack '{}' is currently deployed. Run 'dodot down {}' to clean up.",
            display, display
        ));
    }

    Ok(AddIgnoreResult {
        message: format!("Pack '{display}' marked as ignored."),
        details,
    })
}

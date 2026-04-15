//! `addignore` command — mark a pack as ignored.

use serde::Serialize;

use crate::packs::orchestration::ExecutionContext;
use crate::{DodotError, Result};

#[derive(Debug, Clone, Serialize)]
pub struct AddIgnoreResult {
    pub message: String,
    pub details: Vec<String>,
}

/// Create a `.dodotignore` file in the pack directory.
pub fn addignore(pack_name: &str, ctx: &ExecutionContext) -> Result<AddIgnoreResult> {
    let pack_path = ctx.paths.pack_path(pack_name);

    if !ctx.fs.exists(&pack_path) {
        return Err(DodotError::PackNotFound {
            name: pack_name.into(),
        });
    }

    let ignore_path = pack_path.join(".dodotignore");

    if ctx.fs.exists(&ignore_path) {
        return Ok(AddIgnoreResult {
            message: format!("Pack '{pack_name}' is already ignored."),
            details: vec![],
        });
    }

    ctx.fs.write_file(&ignore_path, b"")?;

    // Check if pack is currently deployed and warn (#18)
    let handlers = ctx.datastore.list_pack_handlers(pack_name)?;
    let mut details = vec![format!("Created {}", ignore_path.display())];
    if !handlers.is_empty() {
        details.push(format!(
            "Warning: pack '{}' is currently deployed. Run 'dodot down {}' to clean up.",
            pack_name, pack_name
        ));
    }

    Ok(AddIgnoreResult {
        message: format!("Pack '{pack_name}' marked as ignored."),
        details,
    })
}

//! `init` command — create a new pack with default structure.

use serde::Serialize;

use crate::packs::orchestration::ExecutionContext;
use crate::{DodotError, Result};

#[derive(Debug, Clone, Serialize)]
pub struct InitResult {
    pub message: String,
    pub details: Vec<String>,
}

/// Create a new pack directory with default structure.
pub fn init(pack_name: &str, ctx: &ExecutionContext) -> Result<InitResult> {
    let pack_path = ctx.paths.pack_path(pack_name);

    if ctx.fs.exists(&pack_path) {
        return Err(DodotError::PackInvalid {
            name: pack_name.into(),
            reason: "directory already exists".into(),
        });
    }

    ctx.fs.mkdir_all(&pack_path)?;

    // Write default .dodot.toml
    let config_content = format!(
        r#"# dodot configuration for {pack_name}
# See: dodot config gen --help

[pack]
# ignore = ["*.bak", "*.tmp"]

[symlink]
# force_home = []
# protected_paths = []

[mappings]
# install = "install.sh"
# shell = ["aliases.sh"]
# homebrew = "Brewfile"
# skip = []
"#
    );
    ctx.fs
        .write_file(&pack_path.join(".dodot.toml"), config_content.as_bytes())?;

    let details = vec![
        format!("Created {}", pack_path.display()),
        format!("Created {}/.dodot.toml", pack_path.display()),
    ];

    Ok(InitResult {
        message: format!("Pack '{pack_name}' initialized."),
        details,
    })
}

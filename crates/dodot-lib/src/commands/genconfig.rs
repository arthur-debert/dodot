//! `genconfig` command — generate default configuration.

use serde::Serialize;

use crate::packs::orchestration::ExecutionContext;
use crate::Result;

#[derive(Debug, Clone, Serialize)]
pub struct GenConfigResult {
    pub message: Option<String>,
    pub content: String,
}

/// Default configuration template with comments.
const DEFAULT_CONFIG: &str = r#"# dodot configuration
# Place this file at your dotfiles root as .dodot.toml
# or in a pack directory for pack-specific overrides.

[pack]
# Glob patterns for pack directories to ignore
# ignore = ["scratch", "*.bak"]

[symlink]
# Files/directories forced to $HOME instead of $XDG_CONFIG_HOME
# force_home = ["ssh", "bashrc", "zshrc"]

# Paths that must never be symlinked (security)
# protected_paths = [".ssh/id_rsa", ".gnupg"]

[mappings]
# File-to-handler mappings (override defaults)
# path = "bin"
# install = "install.sh"
# shell = ["aliases.sh", "profile.sh"]
# homebrew = "Brewfile"
# ignore = ["*.tmp"]
"#;

/// Generate default configuration, optionally writing to a file.
pub fn genconfig(write: bool, ctx: &ExecutionContext) -> Result<GenConfigResult> {
    if write {
        let config_path = ctx.paths.dotfiles_root().join(".dodot.toml");
        ctx.fs.write_file(&config_path, DEFAULT_CONFIG.as_bytes())?;

        Ok(GenConfigResult {
            message: Some(format!("Config written to {}", config_path.display())),
            content: DEFAULT_CONFIG.into(),
        })
    } else {
        Ok(GenConfigResult {
            message: None,
            content: DEFAULT_CONFIG.into(),
        })
    }
}

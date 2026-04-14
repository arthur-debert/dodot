//! Public command API — the entry points for all dodot operations.
//!
//! Each function returns a `Result<T>` where `T: Serialize`. These
//! types are the contract with standout's rendering layer — they
//! carry everything needed to produce both human-readable (template)
//! and machine-readable (JSON) output.

pub mod addignore;
pub mod adopt;
pub mod down;
pub mod fill;
pub mod genconfig;
pub mod init;
pub mod list;
pub mod status;
pub mod up;

#[cfg(test)]
mod tests;

use serde::Serialize;

// ── Shared display types ────────────────────────────────────────

/// Handler symbols matching the Go implementation.
pub fn handler_symbol(handler: &str) -> &'static str {
    match handler {
        "symlink" => "➞",
        "shell" => "⚙",
        "path" => "+",
        "homebrew" => "⚙",
        "install" => "×",
        _ => "?",
    }
}

/// Human-readable status label.
pub fn status_label(handler: &str, deployed: bool) -> String {
    match (handler, deployed) {
        ("symlink", true) => "deployed".into(),
        ("symlink", false) => "pending".into(),
        ("shell", true) => "sourced".into(),
        ("shell", false) => "not sourced".into(),
        ("path", true) => "in PATH".into(),
        ("path", false) => "not in PATH".into(),
        ("install", true) => "installed".into(),
        ("install", false) => "never run".into(),
        ("homebrew", true) => "installed".into(),
        ("homebrew", false) => "not installed".into(),
        (_, true) => "deployed".into(),
        (_, false) => "pending".into(),
    }
}

/// Status string for standout template tag matching (maps to theme style names).
pub fn status_style(deployed: bool) -> &'static str {
    if deployed {
        "deployed"
    } else {
        "pending"
    }
}

/// Human-readable handler description for a file.
pub fn handler_description(handler: &str, rel_path: &str, user_target: Option<&str>) -> String {
    match handler {
        "symlink" => {
            if let Some(target) = user_target {
                target.to_string()
            } else {
                format!("~/.{rel_path}")
            }
        }
        "shell" => "shell profile".into(),
        "path" => format!("$PATH/{rel_path}"),
        "install" => "run script".into(),
        "homebrew" => "brew install".into(),
        _ => String::new(),
    }
}

/// A file entry for pack status display.
#[derive(Debug, Clone, Serialize)]
pub struct DisplayFile {
    pub name: String,
    pub symbol: String,
    pub description: String,
    pub status: String,
    pub status_label: String,
    pub handler: String,
}

/// A pack entry for status display.
#[derive(Debug, Clone, Serialize)]
pub struct DisplayPack {
    pub name: String,
    pub files: Vec<DisplayFile>,
}

/// Result type for commands that display pack status
/// (status, up, down).
#[derive(Debug, Clone, Serialize)]
pub struct PackStatusResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub dry_run: bool,
    pub packs: Vec<DisplayPack>,
}

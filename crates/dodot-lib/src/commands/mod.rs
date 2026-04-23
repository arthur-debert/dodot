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
                // Apply dot. prefix convention: dot.bashrc → ~/.bashrc
                let display_path = if !rel_path.contains('/') && rel_path.starts_with("dot.") {
                    format!(".{}", &rel_path[4..])
                } else {
                    format!(".{rel_path}")
                };
                format!("~/{display_path}")
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
    /// Per-pack footnotes referenced by `(N)` markers in `status_label`.
    /// Each entry is the body text only (no leading `(N)`); the renderer
    /// pairs them with the marker number assigned at assembly time.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub footnotes: Vec<String>,
}

/// One claimant of a cross-pack conflict, formatted for display.
#[derive(Debug, Clone, Serialize)]
pub struct DisplayClaimant {
    /// Pack name.
    pub pack: String,
    /// Short, pack-relative source description (e.g. `git/env.sh`).
    pub source: String,
}

/// A single cross-pack conflict, flattened for template rendering.
#[derive(Debug, Clone, Serialize)]
pub struct DisplayConflict {
    /// Conflict kind. Serializes as `"symlink"` or `"path"` so the
    /// template can branch on it.
    pub kind: String,
    /// Human-readable target (path for symlink, executable name for path).
    pub target: String,
    pub claimants: Vec<DisplayClaimant>,
}

impl DisplayConflict {
    /// Convert a detection-layer conflict into its display form,
    /// shortening paths relative to `home` when possible.
    pub fn from_conflict(c: &crate::conflicts::Conflict, home: &std::path::Path) -> Self {
        let kind = match c.kind {
            crate::conflicts::ConflictKind::SymlinkTarget => "symlink",
            crate::conflicts::ConflictKind::PathExecutable => "path",
        };
        let target = match c.kind {
            crate::conflicts::ConflictKind::SymlinkTarget => shorten_path(&c.target, home),
            crate::conflicts::ConflictKind::PathExecutable => c
                .target
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| c.target.display().to_string()),
        };
        let claimants = c
            .claimants
            .iter()
            .map(|cl| DisplayClaimant {
                pack: cl.pack.clone(),
                source: pack_relative_source(&cl.source, &cl.pack),
            })
            .collect();
        DisplayConflict {
            kind: kind.into(),
            target,
            claimants,
        }
    }
}

fn shorten_path(p: &std::path::Path, home: &std::path::Path) -> String {
    if let Ok(rel) = p.strip_prefix(home) {
        format!("~/{}", rel.display())
    } else {
        p.display().to_string()
    }
}

/// Render a claimant source as `<pack>/<relative-path>` when possible,
/// falling back to just the filename.
fn pack_relative_source(source: &std::path::Path, pack: &str) -> String {
    let s = source.to_string_lossy();
    let marker = format!("/{pack}/");
    if let Some(idx) = s.rfind(&marker) {
        let rel = &s[idx + 1..];
        return rel.to_string();
    }
    // Fallback: pack/filename
    let fname = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("{pack}/{fname}")
}

/// Result type for commands that display pack status
/// (status, up, down).
#[derive(Debug, Clone, Serialize)]
pub struct PackStatusResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub dry_run: bool,
    pub packs: Vec<DisplayPack>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Cross-pack conflicts to display at the end of the output.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<DisplayConflict>,
    /// Names of pack-shaped directories skipped because they carry a
    /// `.dodotignore` marker. Surfaced by `status` so users aren't
    /// baffled when a directory they expected doesn't appear.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ignored_packs: Vec<String>,
}

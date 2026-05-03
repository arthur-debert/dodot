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
pub mod git_alias;
pub mod git_filters;
pub mod init;
pub mod list;
pub mod probe;
pub mod prompts;
pub mod refresh;
pub mod secret;
pub mod status;
pub mod template_clean;
pub mod template_install_filter;
pub mod transform;
pub mod tutorial;
pub mod up;

#[cfg(test)]
mod tests;

use serde::Serialize;

/// Shared `{message, details}` result for commands whose output is a
/// short headline plus a list of secondary lines. Renders through the
/// `message` template. Per-command result types (e.g. `InitResult`,
/// `AddIgnoreResult`) exist for backwards compatibility and where
/// commands grow additional fields; use [`MessageResult`] for new
/// commands that need only headline + lines.
#[derive(Debug, Clone, Serialize)]
pub struct MessageResult {
    pub message: String,
    pub details: Vec<String>,
}

// ── Shared display types ────────────────────────────────────────

/// Handler symbols matching the Go implementation.
pub fn handler_symbol(handler: &str) -> &'static str {
    match handler {
        "symlink" => "➞",
        "shell" => "⚙",
        "path" => "+",
        "homebrew" => "⚙",
        "install" => "×",
        "skip" => "·",
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
            // Callers normally pass a fully-resolved user_target (computed
            // by `resolve_target` with the pack name in scope). The
            // pack-namespaced XDG default cannot be reconstructed from
            // `rel_path` alone, so when no target is provided we fall
            // back to a generic "<symlink>" placeholder rather than
            // guessing a wrong `~/.<name>` path.
            user_target
                .map(str::to_string)
                .unwrap_or_else(|| "<symlink>".to_string())
        }
        "shell" => "shell profile".into(),
        "path" => format!("$PATH/{rel_path}"),
        "install" => "run script".into(),
        "homebrew" => "brew install".into(),
        "skip" => "not deployed".into(),
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
    /// 1-based index into `PackStatusResult.notes`. `Some(N)` means the
    /// row has a command-wide error/note attached; the template renders
    /// `[N]` next to the status label and the body appears in the notes
    /// section at the bottom of the output. Indices are assigned at
    /// assembly time and are stable within a single command invocation.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub note_ref: Option<u32>,
}

/// A pack entry for status display.
#[derive(Debug, Clone, Serialize)]
pub struct DisplayPack {
    pub name: String,
    pub files: Vec<DisplayFile>,
    /// Aggregated pack-level status, one of `"error"`, `"pending"`,
    /// `"deployed"`. Rollup rules: `error` ← any file with `error` or
    /// `broken`; otherwise `pending` ← any file with `pending`,
    /// `warning`, or `stale`; otherwise `deployed`. Always populated so
    /// JSON consumers and short-mode templates can use it uniformly.
    pub summary_status: String,
    /// Number of files in the pack whose status rolls up to
    /// `summary_status`. Displayed as `(N)` in short-mode output.
    pub summary_count: usize,
}

impl DisplayPack {
    /// Compute aggregated status and count from the pack's files.
    pub fn new(name: String, files: Vec<DisplayFile>) -> Self {
        let (summary_status, summary_count) = aggregate_status(&files);
        DisplayPack {
            name,
            files,
            summary_status,
            summary_count,
        }
    }

    /// Recompute `summary_status` / `summary_count` after mutating
    /// `files` (e.g. after `overlay_errors` flips a row to `error` or
    /// adopt failures synthesize new rows).
    pub fn recompute_summary(&mut self) {
        let (status, count) = aggregate_status(&self.files);
        self.summary_status = status;
        self.summary_count = count;
    }
}

/// Roll up per-file statuses into one of `error`, `pending`, `deployed`
/// (precedence: error > pending > deployed). Returns the bucket name
/// and the number of files that fall into it.
fn aggregate_status(files: &[DisplayFile]) -> (String, usize) {
    let mut errors = 0usize;
    let mut pendings = 0usize;
    let mut deployeds = 0usize;
    for f in files {
        match f.status.as_str() {
            "error" | "broken" => errors += 1,
            "pending" | "warning" | "stale" => pendings += 1,
            "deployed" => deployeds += 1,
            _ => {}
        }
    }
    if errors > 0 {
        ("error".into(), errors)
    } else if pendings > 0 {
        ("pending".into(), pendings)
    } else {
        ("deployed".into(), deployeds)
    }
}

/// A command-wide note (error / inline conflict) referenced by
/// `DisplayFile.note_ref`. Indices into `PackStatusResult.notes` are
/// 1-based; position in the vec matches the `[N]` shown inline.
#[derive(Debug, Clone, Serialize)]
pub struct DisplayNote {
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hint: Option<String>,
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
    /// Informational command-level messages not attached to any row
    /// (e.g. "pack X is ignored, skipping"). Real errors belong in
    /// `notes` so they can be referenced from an item row.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Command-wide error/note list. Each entry is referenced by a
    /// `DisplayFile.note_ref` (1-based). Rendered at the end of the
    /// output so per-item rows stay single-line and column-aligned.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<DisplayNote>,
    /// Cross-pack conflicts to display at the end of the output.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<DisplayConflict>,
    /// Names of pack-shaped directories skipped because they carry a
    /// `.dodotignore` marker. Surfaced by `status` so users aren't
    /// baffled when a directory they expected doesn't appear.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ignored_packs: Vec<String>,
    /// `"full"` (default) shows per-file listing; `"short"` collapses
    /// each pack to a single summary line.
    pub view_mode: String,
    /// `"name"` (default) lists packs in their discovery order;
    /// `"status"` groups packs under Ignored / Deployed / Pending /
    /// Error banners.
    pub group_mode: String,
}

/// View style for pack-status output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    Full,
    Short,
}

impl ViewMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ViewMode::Full => "full",
            ViewMode::Short => "short",
        }
    }
}

/// Grouping style for pack-status output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupMode {
    #[default]
    Name,
    Status,
}

impl GroupMode {
    pub fn as_str(self) -> &'static str {
        match self {
            GroupMode::Name => "name",
            GroupMode::Status => "status",
        }
    }
}

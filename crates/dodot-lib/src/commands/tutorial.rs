//! `tutorial` — data and introspection for the interactive tutorial.
//!
//! The interactive driver lives in `dodot-cli`; this module provides
//! the building blocks it composes: pack classification, shell-
//! integration detection (read-only inspection plus an explicit
//! append helper), JSON state persistence for resume, and the
//! serializable [`TutorialCtx`] that the CLI passes to step
//! templates. Reads are pure; writes (`append_shell_integration`,
//! `save_state`, `clear_state`) only run on explicit user consent
//! from the driver.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::packs;
use crate::packs::orchestration::ExecutionContext;
use crate::paths::Pather;
use crate::Result;

// ── Pack classification ─────────────────────────────────────────

/// Coarse categorisation of a pack used by the tutorial to pick a
/// good starter. Names match human prose: "config-only" / "shell" /
/// "install".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackKind {
    /// Only files that map to the default symlink handler.
    ConfigOnly,
    /// Has shell-integration files (`aliases.sh`, …) and/or `bin/`.
    ConfigPlusShell,
    /// Has install scripts and/or `Brewfile`.
    ConfigPlusInstall,
    /// Has both shell-integration and provisioning files.
    ConfigPlusShellAndInstall,
    /// Pack is essentially empty (no top-level files at all).
    Empty,
}

impl PackKind {
    pub fn label(self) -> &'static str {
        match self {
            PackKind::ConfigOnly => "config only",
            PackKind::ConfigPlusShell => "config + shell",
            PackKind::ConfigPlusInstall => "config + install",
            PackKind::ConfigPlusShellAndInstall => "config + shell + install",
            PackKind::Empty => "empty",
        }
    }

    /// Lower number = better starter pack for a first-time user.
    fn starter_rank(self) -> u8 {
        match self {
            PackKind::ConfigOnly => 0,
            PackKind::ConfigPlusShell => 1,
            PackKind::ConfigPlusInstall => 2,
            PackKind::ConfigPlusShellAndInstall => 3,
            PackKind::Empty => 99,
        }
    }
}

/// Classify a pack by inspecting its top-level files.
///
/// Mirrors the default rules in `config::mappings_to_rules` rather
/// than re-running the rules scanner, because the tutorial only
/// needs a coarse summary and we want to stay independent of any
/// custom rules a user may have added.
pub fn classify_pack(pack: &packs::Pack) -> PackKind {
    let entries = match std::fs::read_dir(&pack.path) {
        Ok(e) => e,
        Err(_) => return PackKind::Empty,
    };

    let mut has_install = false;
    let mut has_shell = false;
    let mut any = false;

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        any = true;
        let path = entry.path();
        let is_dir = path.is_dir();

        if !is_dir {
            if matches!(
                name.as_str(),
                "install.sh" | "install.bash" | "install.zsh" | "Brewfile"
            ) {
                has_install = true;
            } else if is_shell_filename(&name) {
                has_shell = true;
            }
            // Otherwise it's a default-symlink file — no flag to set;
            // any non-empty pack with no shell/install evidence falls
            // through to ConfigOnly below.
        } else if name == "bin" {
            has_shell = true;
        }
    }

    if !any {
        return PackKind::Empty;
    }
    match (has_shell, has_install) {
        (false, false) => PackKind::ConfigOnly,
        (true, false) => PackKind::ConfigPlusShell,
        (false, true) => PackKind::ConfigPlusInstall,
        (true, true) => PackKind::ConfigPlusShellAndInstall,
    }
}

fn is_shell_filename(name: &str) -> bool {
    let stems = ["aliases", "profile", "login", "env"];
    let exts = [".sh", ".bash", ".zsh"];
    for stem in stems {
        for ext in exts {
            if name == format!("{stem}{ext}") {
                return true;
            }
        }
    }
    false
}

// ── Discover & recommend ────────────────────────────────────────

/// Summary line for one pack as shown in the tutorial.
#[derive(Debug, Clone, Serialize)]
pub struct TutorialPack {
    pub name: String,
    pub kind: String,
    pub recommended: bool,
}

/// Discover packs in the active context and classify each one.
///
/// Returns the list in scan order with the recommended starter pack
/// flagged. If no pack is recommendable (only empty packs), no entry
/// has `recommended = true`.
pub fn discover_and_classify(ctx: &ExecutionContext) -> Result<Vec<TutorialPack>> {
    let root_config = ctx.config_manager.root_config()?;
    let scanned = packs::scan_packs(
        ctx.fs.as_ref(),
        ctx.paths.dotfiles_root(),
        &root_config.pack.ignore,
    )?;

    let mut entries: Vec<(String, PackKind, packs::Pack)> = scanned
        .packs
        .into_iter()
        .map(|p| {
            let kind = classify_pack(&p);
            (p.display_name.clone(), kind, p)
        })
        .collect();

    // Pick the recommended starter — best rank wins. Ties broken
    // by scan order (which is alphabetical).
    let recommended_idx = entries
        .iter()
        .enumerate()
        .filter(|(_, (_, kind, _))| !matches!(kind, PackKind::Empty))
        .min_by_key(|(_, (_, kind, _))| kind.starter_rank())
        .map(|(i, _)| i);

    let result = entries
        .drain(..)
        .enumerate()
        .map(|(i, (name, kind, _))| TutorialPack {
            name,
            kind: kind.label().to_string(),
            recommended: Some(i) == recommended_idx,
        })
        .collect();

    Ok(result)
}

// ── Shell integration detection ─────────────────────────────────

/// What we found out about the `eval "$(dodot init-sh)"` line.
#[derive(Debug, Clone, Serialize)]
pub struct ShellIntegration {
    /// Detected user shell (`zsh`, `bash`, `fish`, `unknown`).
    pub shell_kind: String,
    /// Path to the rc file we'd suggest editing (display form).
    pub rc_path: String,
    /// Absolute path of the rc file, for actual writes.
    #[serde(skip)]
    pub rc_path_abs: PathBuf,
    /// True if the eval line is already present in the rc file.
    pub line_present: bool,
    /// The full eval line we'd suggest adding.
    pub eval_line: String,
}

/// Detect the shell init situation for the user.
///
/// Reads `$SHELL`, picks a likely rc file, checks whether the
/// `dodot init-sh` eval line is already there. Pure read-only.
pub fn detect_shell_integration(home: &Path) -> ShellIntegration {
    let shell_env = std::env::var("SHELL").unwrap_or_default();
    let shell_kind = shell_env.rsplit('/').next().unwrap_or("").to_lowercase();

    let (kind, rc_rel) = match shell_kind.as_str() {
        "zsh" => ("zsh", ".zshrc"),
        "bash" => ("bash", ".bashrc"),
        "fish" => ("fish", ".config/fish/config.fish"),
        _ => ("unknown", ".profile"),
    };

    let rc_path_abs = home.join(rc_rel);
    let display = format!("~/{rc_rel}");
    let eval_line = if kind == "fish" {
        "dodot init-sh | source".to_string()
    } else {
        r#"eval "$(dodot init-sh)""#.to_string()
    };

    let line_present = std::fs::read_to_string(&rc_path_abs)
        .map(|c| c.contains("dodot init-sh"))
        .unwrap_or(false);

    ShellIntegration {
        shell_kind: kind.to_string(),
        rc_path: display,
        rc_path_abs,
        line_present,
        eval_line,
    }
}

/// Append the eval line to the user's rc file with a header comment.
/// Idempotent: returns Ok without writing if the line is already there.
pub fn append_shell_integration(integ: &ShellIntegration) -> Result<()> {
    if integ.line_present {
        return Ok(());
    }
    if let Some(parent) = integ.rc_path_abs.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| crate::DodotError::Other(format!("create rc parent: {e}")))?;
        }
    }
    let existing = std::fs::read_to_string(&integ.rc_path_abs).unwrap_or_default();
    let mut new = existing;
    if !new.is_empty() && !new.ends_with('\n') {
        new.push('\n');
    }
    new.push_str("\n# dodot — load packs into this shell session\n");
    new.push_str(&integ.eval_line);
    new.push('\n');
    std::fs::write(&integ.rc_path_abs, new)
        .map_err(|e| crate::DodotError::Other(format!("write rc: {e}")))?;
    Ok(())
}

// ── State persistence ───────────────────────────────────────────

/// Persisted between tutorial invocations so users can resume.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TutorialState {
    pub step_id: String,
    pub pack: Option<String>,
    pub started_at: Option<String>,
}

/// Path where tutorial state is stored.
pub fn state_path(paths: &dyn Pather) -> PathBuf {
    paths.data_dir().join("tutorial.json")
}

pub fn load_state(paths: &dyn Pather) -> Option<TutorialState> {
    let path = state_path(paths);
    let contents = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&contents).ok()
}

pub fn save_state(paths: &dyn Pather, state: &TutorialState) -> Result<()> {
    let path = state_path(paths);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| crate::DodotError::Other(format!("create state dir: {e}")))?;
    }
    let s = serde_json::to_string_pretty(state)
        .map_err(|e| crate::DodotError::Other(format!("serialize state: {e}")))?;
    std::fs::write(&path, s).map_err(|e| crate::DodotError::Other(format!("write state: {e}")))?;
    Ok(())
}

pub fn clear_state(paths: &dyn Pather) -> Result<()> {
    let path = state_path(paths);
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| crate::DodotError::Other(format!("remove state: {e}")))?;
    }
    Ok(())
}

// ── Tutorial Ctx ────────────────────────────────────────────────

/// Serializable context passed to step templates. The CLI driver
/// mutates this between steps; templates read fields by name.
#[derive(Debug, Clone, Serialize, Default)]
pub struct TutorialCtx {
    pub dotfiles_root: String,
    pub via: String,
    pub packs: Vec<TutorialPack>,
    pub chosen_pack: Option<String>,
    pub chosen_pack_kind: Option<String>,
    pub has_shell_files: bool,
    pub has_install_files: bool,
    pub status_output: Option<String>,
    pub dry_run_output: Option<String>,
    pub up_output: Option<String>,
    pub shell_integration: Option<ShellIntegration>,
    pub eval_line: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    fn write(p: &PathBuf, body: &str) {
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, body).unwrap();
    }

    #[test]
    fn classify_config_only_pack() {
        let dir = tempfile::tempdir().unwrap();
        let pack_path = dir.path().join("vim");
        std::fs::create_dir_all(&pack_path).unwrap();
        write(&pack_path.join("vimrc"), "set nu");
        let pack = packs::Pack::new(
            "vim".into(),
            pack_path,
            crate::handlers::HandlerConfig::default(),
        );
        assert_eq!(classify_pack(&pack), PackKind::ConfigOnly);
    }

    #[test]
    fn classify_config_plus_shell_pack() {
        let dir = tempfile::tempdir().unwrap();
        let pack_path = dir.path().join("zsh");
        std::fs::create_dir_all(&pack_path).unwrap();
        write(&pack_path.join("aliases.sh"), "alias ll='ls -l'");
        let pack = packs::Pack::new(
            "zsh".into(),
            pack_path,
            crate::handlers::HandlerConfig::default(),
        );
        assert_eq!(classify_pack(&pack), PackKind::ConfigPlusShell);
    }

    #[test]
    fn classify_config_plus_install_pack() {
        let dir = tempfile::tempdir().unwrap();
        let pack_path = dir.path().join("dev");
        std::fs::create_dir_all(&pack_path).unwrap();
        write(&pack_path.join("install.sh"), "echo");
        write(&pack_path.join("config"), "k=v");
        let pack = packs::Pack::new(
            "dev".into(),
            pack_path,
            crate::handlers::HandlerConfig::default(),
        );
        assert_eq!(classify_pack(&pack), PackKind::ConfigPlusInstall);
    }

    #[test]
    fn classify_empty_pack() {
        let dir = tempfile::tempdir().unwrap();
        let pack_path = dir.path().join("empty");
        std::fs::create_dir_all(&pack_path).unwrap();
        let pack = packs::Pack::new(
            "empty".into(),
            pack_path,
            crate::handlers::HandlerConfig::default(),
        );
        assert_eq!(classify_pack(&pack), PackKind::Empty);
    }

    #[test]
    fn detect_shell_with_no_rc_file_reports_absent() {
        let dir = tempfile::tempdir().unwrap();
        // Force a known shell — .zshrc doesn't exist in this temp HOME.
        std::env::set_var("SHELL", "/bin/zsh");
        let integ = detect_shell_integration(dir.path());
        assert_eq!(integ.shell_kind, "zsh");
        assert!(!integ.line_present);
        assert!(integ.eval_line.contains("dodot init-sh"));
    }
}

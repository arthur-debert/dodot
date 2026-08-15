//! `tutorial` — data and introspection for the interactive tutorial.
//!
//! The interactive driver lives in `dodot-cli`; this module provides
//! the building blocks it composes: pack classification, shell-
//! hookup detection (read-only — [`crate::shell::rc`] is both the rc
//! ladder and the hook classifier it defers to; the tutorial owns no
//! rc writer, the driver routes any write through the real
//! `dodot install --write` path), JSON state persistence for resume,
//! and the serializable [`TutorialCtx`] that the CLI passes to step
//! templates. Reads are pure; writes (`save_state`, `clear_state`)
//! only run on explicit user consent from the driver.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fs::Fs;
use crate::packs;
use crate::packs::orchestration::ExecutionContext;
use crate::paths::Pather;
use crate::shell::rc::{self, ShellEnv};
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

// Mirrors the default `[mappings] shell = ["*.sh", "*.bash", "*.zsh"]`.
// `install.{sh,bash,zsh}` is filtered by the caller before reaching this check
// (priority-20 install rule vs priority-10 shell wildcard).
fn is_shell_filename(name: &str) -> bool {
    name.ends_with(".sh") || name.ends_with(".bash") || name.ends_with(".zsh")
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

/// What we found out about the user's shell hookup.
///
/// Read-only facts for the shell-integration step. The tutorial owns
/// no rc writer: when the user consents, the driver runs the real
/// `dodot install --write` path (issue #281), so these fields only
/// describe — they never name a file the tutorial itself would touch.
/// [`Self::line_present`] answers the question the step actually
/// turns on — "is this user wired up at all?" — and counts either
/// hookup form.
#[derive(Debug, Clone, Serialize)]
pub struct ShellIntegration {
    /// Detected user shell: `zsh` / `bash` when dodot can wire it,
    /// otherwise the raw `$SHELL` basename (`fish`, …) or `unknown`.
    pub shell_kind: String,
    /// Whether `dodot install` can wire this shell (bash/zsh). When
    /// false the step is informational only: dodot refuses to guess
    /// an rc file for other shells, so there is nothing to offer.
    pub supported: bool,
    /// The rc file the shell actually reads at startup, display form
    /// (`~/`-relative) — resolved by [`crate::shell::rc::resolve_rc`],
    /// the same ladder `dodot install` writes through (`ZDOTDIR`,
    /// symlinks, the lot). Empty for unsupported shells.
    pub rc_path: String,
    /// True if *either* hookup form is already in the resolved rc
    /// file: the hand-written eval line, or the block `dodot install
    /// --write` writes. Classified by
    /// [`crate::shell::rc::scan_hook_file`], so a commented-out line
    /// does not count. Drives whether the step prompts at all.
    pub line_present: bool,
    /// The manual hookup line, shown only for shells `dodot install`
    /// refuses to wire (matching the command's own refusal message).
    pub eval_line: String,
}

/// Detect the shell init situation for the user. Pure read-only.
///
/// `shell::rc` is the single answer to both halves of the question:
/// [`crate::shell::rc::resolve_rc`] names the rc file the shell
/// actually reads — honouring `ZDOTDIR` (including the `~/.zshenv`
/// peek) and following a symlinked rc — and
/// [`crate::shell::rc::scan_hook_file`] classifies what is in it, so
/// *either* hookup form counts: the hand-written `dodot init-sh`
/// eval line, or the marked block `dodot install --write` splices
/// in. A flat `~/.zshrc` guess here would report an unwired `ZDOTDIR`
/// user as wired (or vice versa) — the silent dead hookup epic INS01
/// exists to eliminate (issue #281).
///
/// For shells dodot refuses to guess an rc file for (`fish`, plain
/// `sh`, unset `$SHELL`) no path is resolved and no scan runs:
/// `supported` is false and the step only shows the manual line.
pub fn detect_shell_integration(
    fs: &dyn Fs,
    home: &Path,
    shell_env: &ShellEnv,
) -> ShellIntegration {
    let shell = shell_env.hookup_shell();

    let shell_kind = match shell {
        Some(s) => s.as_str().to_string(),
        None => shell_env
            .shell
            .as_deref()
            .and_then(|p| Path::new(p).file_name())
            .and_then(|n| n.to_str())
            .map(|s| s.trim_start_matches('-').to_lowercase())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".to_string()),
    };
    let eval_line = if shell_kind == "fish" {
        "dodot init-sh | source".to_string()
    } else {
        r#"eval "$(dodot init-sh)""#.to_string()
    };

    let (rc_path, line_present) = match shell {
        Some(s) => {
            let target = rc::resolve_rc(fs, home, Some(s), shell_env, None);
            (
                rc::display_home_relative(target.nominal(), home),
                rc::scan_hook_file(fs, &target.path).is_present(),
            )
        }
        None => (String::new(), false),
    };

    ShellIntegration {
        shell_kind,
        supported: shell.is_some(),
        rc_path,
        line_present,
        eval_line,
    }
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
    fn classify_pack_with_arbitrary_shell_extension_filenames() {
        // Per the wildcard `*.{sh,bash,zsh}` shell default, names beyond
        // the legacy `aliases`/`profile`/`login`/`env` allowlist must
        // also classify as ConfigPlusShell.
        let dir = tempfile::tempdir().unwrap();
        let pack_path = dir.path().join("shell");
        std::fs::create_dir_all(&pack_path).unwrap();
        write(&pack_path.join("path.sh"), "export PATH=...");
        write(&pack_path.join("functions.zsh"), "function f() {}");
        write(&pack_path.join("50_prompt.bash"), "PS1='>'");
        let pack = packs::Pack::new(
            "shell".into(),
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

    /// Detection is pure over an explicit [`ShellEnv`] — no process
    /// env mutation, no `$SHELL` mutex.
    fn zsh_env() -> ShellEnv {
        ShellEnv {
            shell: Some("/bin/zsh".into()),
            zdotdir: None,
        }
    }

    fn os_fs() -> crate::fs::OsFs {
        crate::fs::OsFs::new()
    }

    #[test]
    fn detect_shell_with_no_rc_file_reports_absent() {
        let dir = tempfile::tempdir().unwrap();
        let integ = detect_shell_integration(&os_fs(), dir.path(), &zsh_env());
        assert_eq!(integ.shell_kind, "zsh");
        assert!(integ.supported);
        assert_eq!(integ.rc_path, "~/.zshrc");
        assert!(!integ.line_present);
    }

    #[test]
    fn detect_shell_sees_both_hookup_forms() {
        // The tutorial's shell step exists to stop a user shipping a
        // pack whose shell snippets never load. Either hookup form
        // means that job is done — the managed block `dodot install
        // --write` writes sources the script by path and never spells
        // out `dodot init-sh`, so matching only the eval line would
        // re-prompt a user who is already wired up.
        for rc in [
            "eval \"$(dodot init-sh)\"\n",
            "# >>> dodot shell hookup >>>\n\
             [ -f \"$HOME/.local/share/dodot/shell/dodot-init.sh\" ] \
             && . \"$HOME/.local/share/dodot/shell/dodot-init.sh\"\n\
             # <<< dodot shell hookup <<<\n",
        ] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join(".zshrc"), rc).unwrap();
            assert!(
                detect_shell_integration(&os_fs(), dir.path(), &zsh_env()).line_present,
                "should recognise this hookup: {rc}"
            );
        }
    }

    #[test]
    fn detect_shell_ignores_commented_out_hookups() {
        // Detection routes through `shell::rc::scan_hook_file`, so a
        // hook a user commented out — or merely mentioned in prose —
        // is not a hookup. Counting it would silently skip the step
        // for the user who most needs it.
        for rc in [
            "# eval \"$(dodot init-sh)\"\n",
            "  #  . \"$HOME/.local/share/dodot/shell/dodot-init.sh\"\n",
            "# see docs: dodot init-sh prints the script\n",
        ] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join(".zshrc"), rc).unwrap();
            assert!(
                !detect_shell_integration(&os_fs(), dir.path(), &zsh_env()).line_present,
                "should not count this as a hookup: {rc}"
            );
        }
    }

    #[test]
    fn detect_shell_honours_zdotdir() {
        // The issue-#281 reproduction: with ZDOTDIR set, zsh never
        // reads ~/.zshrc. Detection must look where the shell looks —
        // a hook in $ZDOTDIR/.zshrc counts, a hook in ~/.zshrc (the
        // file the tutorial's retired flat guess pointed at) does not.
        let dir = tempfile::tempdir().unwrap();
        let zdot = dir.path().join("cfg/zsh");
        let env = ShellEnv {
            shell: Some("/bin/zsh".into()),
            zdotdir: Some(zdot.display().to_string()),
        };

        write(&zdot.join(".zshrc"), "eval \"$(dodot init-sh)\"\n");
        let integ = detect_shell_integration(&os_fs(), dir.path(), &env);
        assert!(integ.line_present, "hook in $ZDOTDIR/.zshrc must count");

        std::fs::remove_file(zdot.join(".zshrc")).unwrap();
        write(&dir.path().join(".zshrc"), "eval \"$(dodot init-sh)\"\n");
        let integ = detect_shell_integration(&os_fs(), dir.path(), &env);
        assert!(
            !integ.line_present,
            "a hook in ~/.zshrc is dead when ZDOTDIR is set — it must not count"
        );
    }

    #[test]
    fn detect_shell_follows_symlinked_rc() {
        // The normal dodot case: ~/.zshrc is a symlink into the
        // managed dotfiles repo. Detection reads through it, same as
        // `dodot install` writes through it.
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("dotfiles/zshrc");
        write(&real, "eval \"$(dodot init-sh)\"\n");
        std::os::unix::fs::symlink(&real, dir.path().join(".zshrc")).unwrap();

        let integ = detect_shell_integration(&os_fs(), dir.path(), &zsh_env());
        assert!(integ.line_present, "hook behind a symlinked rc must count");
        assert_eq!(
            integ.rc_path, "~/.zshrc",
            "display names the file zsh opens"
        );
    }

    #[test]
    fn detect_unsupported_shell_is_informational_only() {
        // dodot refuses to guess an rc file for fish (same refusal as
        // `dodot install`), so the step has no path to resolve and no
        // write to offer — just the manual line, in fish syntax.
        let dir = tempfile::tempdir().unwrap();
        let env = ShellEnv {
            shell: Some("/usr/bin/fish".into()),
            zdotdir: None,
        };
        let integ = detect_shell_integration(&os_fs(), dir.path(), &env);
        assert_eq!(integ.shell_kind, "fish");
        assert!(!integ.supported);
        assert!(integ.rc_path.is_empty());
        assert!(!integ.line_present);
        assert_eq!(integ.eval_line, "dodot init-sh | source");
    }
}

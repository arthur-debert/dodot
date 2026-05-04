//! `dodot git-show-alias` and `dodot git-install-alias` — the
//! Tier 2 shell-side glue for the template-magic flow.
//!
//! Tier 1 (R4) gets you commit-time correctness via the pre-commit
//! hook. Tier 2 makes interactive `git status` and `git diff` show
//! the truth between commits too, by wrapping git in a shell alias
//! that runs `dodot refresh --quiet` first. The clean filter (R6)
//! does the heavy lifting once the source mtime is fresh; this
//! alias is what nudges the mtime on every interactive git
//! invocation.
//!
//! Two forms:
//!
//! - `dodot git-show-alias` — print the alias for the user's shell
//!   (detected from `$SHELL`) so they can copy-paste into their rc
//!   file. No filesystem mutation.
//! - `dodot git-install-alias` — write the alias to the user's rc
//!   file (`~/.bashrc` or `~/.zshrc`) with an idempotent guard
//!   block, mirroring the pre-commit hook installer's pattern.
//!
//! Only affects interactive shells. Scripts, editors, and CI that
//! shell out to `git` directly are unaffected — exactly what we
//! want: non-interactive callers get predictable behaviour,
//! interactive use gets the magic.

use serde::Serialize;
use std::path::PathBuf;

use crate::packs::orchestration::ExecutionContext;
use crate::{DodotError, Result};

/// The guard line that opens our managed alias block in a shell rc
/// file. Detection of this string is what makes `install_alias`
/// idempotent.
pub(crate) const ALIAS_GUARD_START: &str =
    "# >>> dodot git alias (managed by `dodot git-install-alias`) >>>";

/// The guard line that closes our managed alias block.
pub(crate) const ALIAS_GUARD_END: &str = "# <<< dodot git alias <<<";

/// Which shell we're targeting. Detected from `$SHELL` for the
/// bare `show`/`install` flow; the user can override with `--shell`
/// at the CLI layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Shell {
    Bash,
    Zsh,
}

impl Shell {
    /// Detect from `$SHELL`. Returns `None` when `$SHELL` is unset
    /// or names a shell we don't support — the caller (typically
    /// [`resolve_shell`]) surfaces a clear error rather than
    /// silently writing a bashrc snippet for a fish/nu user.
    ///
    /// Why fail explicitly: silently falling back to `Bash` means
    /// `dodot git-install-alias` happily writes `~/.bashrc` for a
    /// fish user, who then never sees the alias take effect.
    /// Better to refuse with a message that points at `--shell`.
    pub fn detect() -> Option<Self> {
        std::env::var("SHELL").ok().and_then(|s| {
            if s.ends_with("/zsh") || s == "zsh" {
                Some(Shell::Zsh)
            } else if s.ends_with("/bash") || s == "bash" {
                Some(Shell::Bash)
            } else {
                None
            }
        })
    }

    /// Parse a CLI `--shell` value. Returns `None` for unknown
    /// shells; the CLI layer surfaces a clear error.
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "bash" => Some(Shell::Bash),
            "zsh" => Some(Shell::Zsh),
            _ => None,
        }
    }

    /// Path to the rc file we write to for this shell, relative to
    /// `$HOME`. Used by [`install_alias`] to compute the absolute
    /// path. We pick the most universally-sourced file: `.bashrc`
    /// on bash, `.zshrc` on zsh. Users with non-standard setups can
    /// run `git-show-alias` and paste manually.
    pub fn rc_relative_path(self) -> &'static str {
        match self {
            Shell::Bash => ".bashrc",
            Shell::Zsh => ".zshrc",
        }
    }

    /// The alias line for this shell. Both bash and zsh accept the
    /// same `alias` syntax; we pin them per-shell anyway because
    /// future shells (fish, nu) will need different forms and
    /// keeping the dispatch on a single match is the cleanest
    /// extension point.
    pub fn alias_line(self) -> &'static str {
        match self {
            Shell::Bash | Shell::Zsh => "alias git='dodot refresh --quiet && command git'",
        }
    }
}

/// The full guarded block that `install_alias` writes (or that
/// `show_alias` prints for copy-paste). Mirrors the pre-commit
/// hook's `managed_block` shape.
pub fn managed_block(shell: Shell) -> String {
    format!(
        "{guard_start}\n\
         # Wraps `git` to run `dodot refresh` first, so `git status` and\n\
         # `git diff` show deployed-side template edits between commits.\n\
         # Only affects interactive shells. Remove this block to opt out.\n\
         {alias}\n\
         {guard_end}\n",
        guard_start = ALIAS_GUARD_START,
        guard_end = ALIAS_GUARD_END,
        alias = shell.alias_line(),
    )
}

// ── show ────────────────────────────────────────────────────────

/// Result of `dodot git-show-alias` — the alias body the user can
/// paste, plus the rc file we'd write it to (so the rendered output
/// can show "add this to ~/.zshrc").
#[derive(Debug, Clone, Serialize)]
pub struct ShowAliasResult {
    pub shell: Shell,
    pub alias_block: String,
    /// `alias_block` split by line, so the template can iterate
    /// directly without calling `.split` (which minijinja doesn't
    /// expose). Same trick `git-filters` uses for the same reason.
    pub alias_block_lines: Vec<String>,
    pub rc_path_display: String,
    /// True iff the rc file currently contains our managed block.
    /// Drives the rendered output: "you've already installed this"
    /// vs "add this to your rc file".
    pub already_installed: bool,
}

pub fn show_alias(ctx: &ExecutionContext, shell: Shell) -> Result<ShowAliasResult> {
    let rc_path = ctx.paths.home_dir().join(shell.rc_relative_path());
    let already_installed = if ctx.fs.exists(&rc_path) {
        ctx.fs
            .read_to_string(&rc_path)
            .map(|s| s.contains(ALIAS_GUARD_START))
            .unwrap_or(false)
    } else {
        false
    };
    let alias_block = managed_block(shell);
    let alias_block_lines: Vec<String> = alias_block
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    Ok(ShowAliasResult {
        shell,
        alias_block,
        alias_block_lines,
        rc_path_display: render_home_relative(&rc_path, ctx.paths.home_dir()),
        already_installed,
    })
}

// ── install ─────────────────────────────────────────────────────

/// Outcome of `dodot git-install-alias`. Mirrors `InstallHookOutcome`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallAliasOutcome {
    /// rc file did not exist; we created it with our block.
    Created,
    /// rc file existed; we appended our block to it. Existing
    /// content preserved.
    Appended,
    /// rc file already contains the current managed block — no
    /// change.
    AlreadyInstalled,
    /// rc file had an older managed block; we replaced it in place.
    /// Existing non-managed content preserved.
    Updated,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallAliasResult {
    pub shell: Shell,
    pub outcome: InstallAliasOutcome,
    pub rc_path: String,
    pub rc_path_display: String,
    /// The shell sourcing command the user needs to pick up the
    /// alias *now* (without restarting the shell). Surfaced so the
    /// rendered message can suggest e.g. `source ~/.zshrc`.
    pub source_command: String,
}

pub fn install_alias(ctx: &ExecutionContext, shell: Shell) -> Result<InstallAliasResult> {
    let rc_path = ctx.paths.home_dir().join(shell.rc_relative_path());
    let block = managed_block(shell);

    let outcome = if ctx.fs.exists(&rc_path) {
        let existing = ctx.fs.read_to_string(&rc_path)?;
        if let Some((start_byte, end_byte)) = find_managed_block(&existing) {
            let current_block = &existing[start_byte..end_byte];
            if current_block == block {
                InstallAliasOutcome::AlreadyInstalled
            } else {
                let mut new_content = String::with_capacity(existing.len() + block.len());
                new_content.push_str(&existing[..start_byte]);
                new_content.push_str(&block);
                new_content.push_str(&existing[end_byte..]);
                ctx.fs.write_file(&rc_path, new_content.as_bytes())?;
                InstallAliasOutcome::Updated
            }
        } else {
            // Append, preserving existing rc content. Add a leading
            // blank line so the block is visually separate from
            // whatever the user has above.
            let mut new_content = existing.clone();
            if !new_content.ends_with('\n') {
                new_content.push('\n');
            }
            if !new_content.ends_with("\n\n") {
                new_content.push('\n');
            }
            new_content.push_str(&block);
            ctx.fs.write_file(&rc_path, new_content.as_bytes())?;
            InstallAliasOutcome::Appended
        }
    } else {
        // Create the rc file with just our block. Most users will
        // already have one; this branch covers the rare empty-home
        // setup or a truly fresh shell install.
        ctx.fs.write_file(&rc_path, block.as_bytes())?;
        InstallAliasOutcome::Created
    };

    Ok(InstallAliasResult {
        shell,
        outcome,
        rc_path: rc_path.display().to_string(),
        rc_path_display: render_home_relative(&rc_path, ctx.paths.home_dir()),
        source_command: format!(
            "source {}",
            render_home_relative(&rc_path, ctx.paths.home_dir())
        ),
    })
}

// ── helpers ─────────────────────────────────────────────────────

fn render_home_relative(p: &std::path::Path, home: &std::path::Path) -> String {
    if let Ok(rel) = p.strip_prefix(home) {
        format!("~/{}", rel.display())
    } else {
        p.display().to_string()
    }
}

/// Locate the byte range of our managed block inside `text`. Same
/// shape as the pre-commit hook's `find_managed_block` — find the
/// first guard-start, then the first guard-end after it, return the
/// inclusive byte range (with the trailing newline if any). Returns
/// `None` if either guard is missing.
fn find_managed_block(text: &str) -> Option<(usize, usize)> {
    let start = text.find(ALIAS_GUARD_START)?;
    let after_start = start + ALIAS_GUARD_START.len();
    let end_rel = text[after_start..].find(ALIAS_GUARD_END)?;
    let end_guard_start = after_start + end_rel;
    let end_byte = end_guard_start + ALIAS_GUARD_END.len();
    let end_byte = if text.as_bytes().get(end_byte) == Some(&b'\n') {
        end_byte + 1
    } else {
        end_byte
    };
    Some((start, end_byte))
}

/// Diagnostic helper for the CLI: detect or validate a shell from
/// the `--shell` CLI value, surfacing a clear error for unknown
/// shells. Returns the resolved [`Shell`] or a `DodotError::Other`.
///
/// When `explicit` is `None` and `Shell::detect()` returns `None`
/// (unsupported `$SHELL`), errors out asking the user to pass
/// `--shell bash` or `--shell zsh`. Better than silently falling
/// back to bash on a fish/nu setup, which would produce an alias
/// that never fires.
pub fn resolve_shell(explicit: Option<&str>) -> Result<Shell> {
    if let Some(name) = explicit {
        return Shell::from_str_opt(name).ok_or_else(|| {
            DodotError::Other(format!(
                "unsupported shell {name:?}: dodot can install the git alias for `bash` or `zsh`. \
                 For other shells, run `dodot git-show-alias --shell bash` and adapt the snippet."
            ))
        });
    }
    Shell::detect().ok_or_else(|| {
        let detected = std::env::var("SHELL").unwrap_or_default();
        if detected.is_empty() {
            DodotError::Other(
                "$SHELL is unset; pass `--shell bash` or `--shell zsh` so dodot knows which \
                 rc file to write."
                    .into(),
            )
        } else {
            DodotError::Other(format!(
                "could not detect shell from $SHELL ({detected:?}): dodot can install the git \
                 alias for `bash` or `zsh`. Pass `--shell bash` or `--shell zsh` explicitly, or \
                 run `dodot git-show-alias --shell bash` and adapt the snippet for your shell."
            ))
        }
    })
}

/// Cheap "is this rc file already wrapping git via our alias?"
/// check, used by the future post-`up` prompt. Reads the rc file
/// and looks for the guard. Doesn't error out if the file is
/// missing — that's a normal "not installed" state.
pub fn is_installed(ctx: &ExecutionContext, shell: Shell) -> bool {
    let rc_path = ctx.paths.home_dir().join(shell.rc_relative_path());
    if !ctx.fs.exists(&rc_path) {
        return false;
    }
    ctx.fs
        .read_to_string(&rc_path)
        .map(|s| s.contains(ALIAS_GUARD_START))
        .unwrap_or(false)
}

// Mirror of crate's PathBuf re-export; localised to keep the
// `use` block at the top of the file readable.
#[allow(dead_code)]
fn _path_buf_anchor(_: PathBuf) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::Fs;
    use crate::paths::Pather;
    use crate::testing::TempEnvironment;

    fn make_ctx(env: &TempEnvironment) -> ExecutionContext {
        use crate::config::ConfigManager;
        use crate::datastore::{CommandOutput, CommandRunner, FilesystemDataStore};
        use crate::fs::Fs;
        use crate::paths::Pather;
        use std::sync::Arc;

        struct NoopRunner;
        impl CommandRunner for NoopRunner {
            fn run(&self, _e: &str, _a: &[String]) -> Result<CommandOutput> {
                Ok(CommandOutput {
                    exit_code: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                })
            }
        }
        let runner: Arc<dyn CommandRunner> = Arc::new(NoopRunner);
        let datastore = Arc::new(FilesystemDataStore::new(
            env.fs.clone(),
            env.paths.clone(),
            runner.clone(),
        ));
        let config_manager = Arc::new(ConfigManager::new(&env.dotfiles_root).unwrap());
        ExecutionContext {
            fs: env.fs.clone() as Arc<dyn Fs>,
            datastore,
            paths: env.paths.clone() as Arc<dyn Pather>,
            config_manager,
            syntax_checker: Arc::new(crate::shell::NoopSyntaxChecker),
            command_runner: runner,
            dry_run: false,
            no_provision: true,
            provision_rerun: false,
            force: false,
            view_mode: crate::commands::ViewMode::Full,
            group_mode: crate::commands::GroupMode::Name,
            verbose: false,
            host_facts: Arc::new(crate::gates::HostFacts::detect()),
        }
    }

    // ── Shell detection / parsing ───────────────────────────────

    #[test]
    fn from_str_opt_recognises_known_shells() {
        assert_eq!(Shell::from_str_opt("bash"), Some(Shell::Bash));
        assert_eq!(Shell::from_str_opt("BASH"), Some(Shell::Bash));
        assert_eq!(Shell::from_str_opt("zsh"), Some(Shell::Zsh));
        assert_eq!(Shell::from_str_opt("fish"), None);
        assert_eq!(Shell::from_str_opt("Powershell"), None);
    }

    #[test]
    fn rc_paths_match_shell_conventions() {
        assert_eq!(Shell::Bash.rc_relative_path(), ".bashrc");
        assert_eq!(Shell::Zsh.rc_relative_path(), ".zshrc");
    }

    #[test]
    fn alias_line_runs_refresh_then_command_git() {
        // Both shells use the same alias body for now; pin the
        // exact form so a stray edit doesn't break the wrap.
        for sh in [Shell::Bash, Shell::Zsh] {
            assert_eq!(
                sh.alias_line(),
                "alias git='dodot refresh --quiet && command git'"
            );
        }
    }

    #[test]
    fn resolve_shell_explicit_unknown_returns_error() {
        let err = resolve_shell(Some("fish")).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("fish"), "msg: {msg}");
        assert!(
            msg.contains("bash"),
            "msg should suggest supported shells: {msg}"
        );
    }

    #[test]
    fn resolve_shell_explicit_known_returns_match() {
        assert_eq!(resolve_shell(Some("bash")).unwrap(), Shell::Bash);
        assert_eq!(resolve_shell(Some("Zsh")).unwrap(), Shell::Zsh);
    }

    // Env-driven detect/resolve_shell tests use the shared
    // `ShellEnvGuard` from `crate::testing`. The guard is RAII
    // (restores `$SHELL` on drop, including on panic) AND holds a
    // process-wide mutex, so any test in the binary touching
    // `$SHELL` is serialised. We can split these into one
    // `#[test]` per scenario again, since the guard handles both
    // the panic-safety and cross-test-races concerns Copilot
    // raised on R8.

    use crate::testing::ShellEnvGuard;

    #[test]
    fn detect_returns_some_for_bash() {
        let _g = ShellEnvGuard::set("/bin/bash");
        assert_eq!(Shell::detect(), Some(Shell::Bash));
    }

    #[test]
    fn detect_returns_some_for_zsh() {
        let _g = ShellEnvGuard::set("/usr/local/bin/zsh");
        assert_eq!(Shell::detect(), Some(Shell::Zsh));
    }

    #[test]
    fn detect_returns_none_for_unknown_shell() {
        // fish/nu/etc. don't auto-detect — the caller must `--shell`.
        let _g = ShellEnvGuard::set("/usr/bin/fish");
        assert_eq!(Shell::detect(), None);
    }

    #[test]
    fn resolve_shell_no_explicit_unsupported_shell_errors() {
        // The PR-review fix from R7: a fish/nu user running
        // `dodot git-show-alias` (no --shell) gets a clear error
        // pointing at `--shell bash|zsh`, NOT a silent fall-
        // through to bash that writes a useless ~/.bashrc.
        let _g = ShellEnvGuard::set("/usr/bin/fish");
        let err = resolve_shell(None).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("fish"), "msg: {msg}");
        assert!(msg.contains("--shell"), "msg should suggest --shell: {msg}");
    }

    #[test]
    fn resolve_shell_no_explicit_unset_shell_errors() {
        // $SHELL unset entirely. Same disposition: clear pointer
        // at --shell.
        let _g = ShellEnvGuard::unset();
        let err = resolve_shell(None).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("$SHELL"), "msg: {msg}");
        assert!(msg.contains("--shell"), "msg: {msg}");
    }

    // ── managed_block + find_managed_block ──────────────────────

    #[test]
    fn managed_block_is_self_contained_and_grep_detectable() {
        let block = managed_block(Shell::Bash);
        assert!(block.starts_with(ALIAS_GUARD_START));
        assert!(block.trim_end().ends_with(ALIAS_GUARD_END));
        assert!(block.contains(Shell::Bash.alias_line()));
    }

    #[test]
    fn find_managed_block_locates_byte_range() {
        let block = managed_block(Shell::Bash);
        let text = format!("# rc preamble\n{block}# rc postamble\n");
        let (start, end) = find_managed_block(&text).expect("must find block");
        assert_eq!(&text[start..end], block);
    }

    #[test]
    fn find_managed_block_returns_none_when_absent() {
        assert!(find_managed_block("nothing here").is_none());
        let only_start = format!("{ALIAS_GUARD_START}\nstuff\n");
        assert!(find_managed_block(&only_start).is_none());
    }

    // ── install_alias ───────────────────────────────────────────

    #[test]
    fn install_alias_creates_rc_file_when_absent() {
        // Fresh home, no rc file. install_alias must create it
        // with just our block (rare but possible — empty-home or
        // truly fresh shell setup).
        let env = TempEnvironment::builder().build();
        let ctx = make_ctx(&env);
        let rc_path = env.paths.home_dir().join(".zshrc");
        assert!(!env.fs.exists(&rc_path));

        let r = install_alias(&ctx, Shell::Zsh).unwrap();
        assert!(matches!(r.outcome, InstallAliasOutcome::Created));
        assert!(env.fs.exists(&rc_path));
        let body = env.fs.read_to_string(&rc_path).unwrap();
        assert!(body.contains(ALIAS_GUARD_START));
        assert!(body.contains(Shell::Zsh.alias_line()));
    }

    #[test]
    fn install_alias_appends_to_existing_rc() {
        let env = TempEnvironment::builder().build();
        let rc_path = env.paths.home_dir().join(".bashrc");
        let existing = "export PATH=\"/usr/local/bin:$PATH\"\nalias ll='ls -l'\n";
        env.fs.mkdir_all(rc_path.parent().unwrap()).unwrap();
        env.fs.write_file(&rc_path, existing.as_bytes()).unwrap();

        let ctx = make_ctx(&env);
        let r = install_alias(&ctx, Shell::Bash).unwrap();
        assert!(matches!(r.outcome, InstallAliasOutcome::Appended));

        let body = env.fs.read_to_string(&rc_path).unwrap();
        assert!(body.starts_with(existing), "user content lost: {body:?}");
        assert!(body.contains(Shell::Bash.alias_line()));
    }

    #[test]
    fn install_alias_is_idempotent_on_current_block() {
        let env = TempEnvironment::builder().build();
        let ctx = make_ctx(&env);

        let r1 = install_alias(&ctx, Shell::Zsh).unwrap();
        assert!(matches!(r1.outcome, InstallAliasOutcome::Created));

        let body_after_first = env
            .fs
            .read_to_string(&env.paths.home_dir().join(".zshrc"))
            .unwrap();

        let r2 = install_alias(&ctx, Shell::Zsh).unwrap();
        assert!(matches!(r2.outcome, InstallAliasOutcome::AlreadyInstalled));

        let body_after_second = env
            .fs
            .read_to_string(&env.paths.home_dir().join(".zshrc"))
            .unwrap();
        assert_eq!(body_after_first, body_after_second);
    }

    #[test]
    fn install_alias_updates_a_stale_block() {
        // If the block exists but doesn't match `managed_block(...)`
        // (e.g. an older form before we added a comment line, or
        // shell-specific changes), install_alias rewrites it in
        // place. Same upgrade path the hook installer uses.
        let env = TempEnvironment::builder().build();
        let rc_path = env.paths.home_dir().join(".zshrc");
        let stale = format!(
            "export PATH=\"/usr/local/bin:$PATH\"\n\
             \n\
             {start}\n\
             # An old, simpler form of the alias block.\n\
             alias git='dodot refresh && git'\n\
             {end}\n\
             alias ll='ls -l'\n",
            start = ALIAS_GUARD_START,
            end = ALIAS_GUARD_END,
        );
        env.fs.mkdir_all(rc_path.parent().unwrap()).unwrap();
        env.fs.write_file(&rc_path, stale.as_bytes()).unwrap();

        let ctx = make_ctx(&env);
        let r = install_alias(&ctx, Shell::Zsh).unwrap();
        assert!(matches!(r.outcome, InstallAliasOutcome::Updated));

        let body = env.fs.read_to_string(&rc_path).unwrap();
        // New shape:
        assert!(body.contains(Shell::Zsh.alias_line()));
        // User content (before AND after the block) survived:
        assert!(body.contains("export PATH"));
        assert!(body.contains("alias ll='ls -l'"));
        // Exactly one managed block.
        assert_eq!(body.matches(ALIAS_GUARD_START).count(), 1);
    }

    #[test]
    fn is_installed_reflects_state() {
        let env = TempEnvironment::builder().build();
        let ctx = make_ctx(&env);
        assert!(!is_installed(&ctx, Shell::Bash));
        install_alias(&ctx, Shell::Bash).unwrap();
        assert!(is_installed(&ctx, Shell::Bash));
        // The other shell's state is independent.
        assert!(!is_installed(&ctx, Shell::Zsh));
    }

    // ── show_alias ──────────────────────────────────────────────

    #[test]
    fn show_alias_renders_block_without_writing() {
        let env = TempEnvironment::builder().build();
        let ctx = make_ctx(&env);
        let rc_path = env.paths.home_dir().join(".zshrc");
        assert!(!env.fs.exists(&rc_path));

        let r = show_alias(&ctx, Shell::Zsh).unwrap();
        assert!(r.alias_block.contains(Shell::Zsh.alias_line()));
        assert_eq!(r.rc_path_display, "~/.zshrc");
        assert!(!r.already_installed);
        // Critically: file must NOT have been created.
        assert!(!env.fs.exists(&rc_path));
    }

    #[test]
    fn show_alias_reports_already_installed_when_block_present() {
        let env = TempEnvironment::builder().build();
        let ctx = make_ctx(&env);
        install_alias(&ctx, Shell::Bash).unwrap();
        let r = show_alias(&ctx, Shell::Bash).unwrap();
        assert!(r.already_installed);
    }
}

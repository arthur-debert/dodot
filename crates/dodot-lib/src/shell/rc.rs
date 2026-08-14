//! Where the shell hookup lives — shell detection, the rc-file
//! ladder, and dodot's marked block
//! (`docs/proposals/shell-hookup.lex` §4).
//!
//! Two consumers share this module, and the split matters:
//!
//! - `dodot install` uses it to *choose a write target* and to write
//!   the marked block ([`resolve_rc`], [`apply_block`]).
//! - The verification probe uses it to *turn a failed measurement into
//!   a diagnosis* ([`scan_hook`]) — hook absent from the expected rc
//!   versus hook present but never reached.
//!
//! Those are the only two jobs a static rc scan is allowed to do
//! (spec §2.2). It is never the verdict on whether shells activate:
//! rc files chain, hide behind symlinks, and get sourced from places
//! no scan enumerates. The probe measures; this module guesses, and
//! the marked block makes the guess reversible.
//!
//! Everything here is a pure function of its inputs plus an injected
//! [`Fs`] — `$SHELL` and `$ZDOTDIR` arrive as a [`ShellEnv`] value
//! rather than being read at the point of use, so the ladder is
//! table-testable without mutating process-global environment.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::fs::Fs;
use crate::Result;

/// Opening marker of dodot's managed hook block.
pub const BLOCK_START: &str = "# >>> dodot shell hookup >>>";

/// Closing marker of dodot's managed hook block.
pub const BLOCK_END: &str = "# <<< dodot shell hookup <<<";

/// The line a macOS `.bash_profile` needs so Terminal's login shells
/// reach `.bashrc` — where the hook block lands (spec §4.3 rung 2).
pub const BASH_CHAIN_LINE: &str = "[ -f \"$HOME/.bashrc\" ] && . \"$HOME/.bashrc\"";

/// Guard against a symlink cycle while resolving an rc path. Well
/// past any legitimate dotfiles-repo chain.
const MAX_SYMLINK_HOPS: usize = 32;

// ── Shell detection ─────────────────────────────────────────────

/// A shell dodot knows how to wire up.
///
/// Deliberately narrower than the set dodot *generates init for*
/// (POSIX sh, bash, zsh): the resolution ladder in spec §4.3 defines
/// a canonical interactive rc for bash and zsh only. A plain `sh`
/// reads whatever `$ENV` points at, which is not a guess worth making
/// — that user gets the honest refusal plus the hook line, same as a
/// fish user, and `--rc` if they know better.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HookupShell {
    Bash,
    Zsh,
}

impl HookupShell {
    /// Detect from a `$SHELL`-style path by basename.
    ///
    /// A leading `-` is stripped first: that is the login-shell
    /// `argv[0]` convention (`-zsh`), and it shows up in `$SHELL` on
    /// hosts that copy `argv[0]` into it.
    pub fn from_shell_path(path: &str) -> Option<Self> {
        let base = Path::new(path).file_name()?.to_str()?;
        match base.strip_prefix('-').unwrap_or(base) {
            "bash" => Some(HookupShell::Bash),
            "zsh" => Some(HookupShell::Zsh),
            _ => None,
        }
    }

    /// Stable lowercase name, used in output and serialization.
    pub fn as_str(self) -> &'static str {
        match self {
            HookupShell::Bash => "bash",
            HookupShell::Zsh => "zsh",
        }
    }
}

/// The process-environment inputs the rc ladder reads.
///
/// Passed as a value so the ladder stays pure: production snapshots
/// it once via [`ShellEnv::from_process`], tests construct one
/// directly instead of mutating `$SHELL` / `$ZDOTDIR` under a global
/// lock.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShellEnv {
    /// The user's shell — `$SHELL`, or the login shell from the user
    /// database when `$SHELL` is unset (spec §4.3 rung 1).
    pub shell: Option<String>,
    /// `$ZDOTDIR`, when the environment carries one. Absent means the
    /// zsh rung peeks at `~/.zshenv` instead.
    pub zdotdir: Option<String>,
}

impl ShellEnv {
    /// Snapshot the real environment, applying the login-shell
    /// fallback for an unset (or empty) `$SHELL`.
    pub fn from_process() -> Self {
        let shell = std::env::var("SHELL")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(login_shell_from_user_db);
        Self {
            shell,
            zdotdir: std::env::var("ZDOTDIR").ok().filter(|s| !s.is_empty()),
        }
    }

    /// The shell this environment names, or `None` when it is unset or
    /// one dodot refuses to guess an rc file for.
    pub fn hookup_shell(&self) -> Option<HookupShell> {
        self.shell.as_deref().and_then(HookupShell::from_shell_path)
    }
}

/// The login shell recorded for this uid in the user database.
///
/// The fallback for an unset `$SHELL` (spec §4.3 rung 1). Reads the
/// passwd entry rather than `/etc/passwd` directly because macOS
/// serves user records from Directory Services, where the file is
/// nearly empty.
///
/// `getpwuid` returns a pointer into libc's static storage, so it is
/// not thread-safe against a concurrent caller in the same process.
/// dodot is a one-shot CLI that calls this at most once per
/// invocation, from context construction, before any worker threads
/// exist.
fn login_shell_from_user_db() -> Option<String> {
    // SAFETY: `getpwuid` takes a plain uid and returns either null or
    // a pointer to a `passwd` in libc-owned static storage, valid
    // until the next passwd-database call in this process. We read
    // `pw_shell` (a NUL-terminated C string) and copy it out
    // immediately, before any such call can happen.
    unsafe {
        let entry = libc::getpwuid(libc::getuid());
        if entry.is_null() || (*entry).pw_shell.is_null() {
            return None;
        }
        std::ffi::CStr::from_ptr((*entry).pw_shell)
            .to_str()
            .ok()
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    }
}

// ── The rc ladder ───────────────────────────────────────────────

/// The rc file `dodot install` reads and (with `--write`) writes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RcTarget {
    /// The file to actually touch — symlinks fully resolved, because
    /// a symlinked rc is written *through* (spec §4.3 rung 4).
    pub path: PathBuf,
    /// The path the ladder picked before symlink resolution, when the
    /// two differ. `Some` here is what the caller announces: landing
    /// the hook in someone's dotfiles repo is the desired outcome,
    /// doing it silently is how trust is lost.
    pub link_source: Option<PathBuf>,
    /// Whether [`path`](Self::path) exists today. A missing rc is
    /// still the right target — a fresh machine has no `.zshrc` —
    /// and `--write` creates it (rung 3).
    pub exists: bool,
    /// Everything about this resolution the user should be told:
    /// where `ZDOTDIR` came from, which symlink was followed.
    pub notes: Vec<String>,
}

/// Resolve the rc file for `shell`, honoring an explicit `--rc`
/// override (spec §4.3).
///
/// `explicit` short-circuits rungs 1–2 entirely, but still goes
/// through symlink resolution: `--rc` says *which* file, not "skip
/// the write-through announcement".
pub fn resolve_rc(
    fs: &dyn Fs,
    home: &Path,
    shell: Option<HookupShell>,
    env: &ShellEnv,
    explicit: Option<&Path>,
) -> RcTarget {
    let mut notes = Vec::new();

    let nominal = match (explicit, shell) {
        (Some(p), _) => absolutize(p, home),
        (None, Some(HookupShell::Bash)) => home.join(".bashrc"),
        (None, Some(HookupShell::Zsh)) => {
            let (dir, note) = zdotdir(fs, home, env);
            if let Some(note) = note {
                notes.push(note);
            }
            dir.join(".zshrc")
        }
        // Callers refuse before reaching here; `~/.profile` keeps the
        // type total without inventing a shell-specific guess.
        (None, None) => home.join(".profile"),
    };

    let resolved = resolve_symlinks(fs, &nominal);
    let link_source = (resolved != nominal).then(|| nominal.clone());
    if let Some(src) = &link_source {
        notes.push(format!(
            "{} → {} — writing there",
            display_home_relative(src, home),
            display_home_relative(&resolved, home)
        ));
    }

    RcTarget {
        exists: fs.exists(&resolved),
        link_source,
        notes,
        path: resolved,
    }
}

/// The directory zsh reads `.zshrc` from, and how we know.
///
/// `$ZDOTDIR` when the environment carries it; otherwise a peek at
/// `~/.zshenv`, since that is the one file zsh always reads and thus
/// the only legal place `ZDOTDIR` can be set before `.zshrc` is
/// looked up (spec §4.3 rung 2).
fn zdotdir(fs: &dyn Fs, home: &Path, env: &ShellEnv) -> (PathBuf, Option<String>) {
    if let Some(z) = env.zdotdir.as_deref() {
        let dir = expand_home(z, home);
        return (
            dir.clone(),
            Some(format!("ZDOTDIR is set: zsh reads {}", dir.display())),
        );
    }
    let zshenv = home.join(".zshenv");
    if fs.exists(&zshenv) {
        if let Ok(text) = fs.read_to_string(&zshenv) {
            if let Some(raw) = parse_zdotdir_assignment(&text) {
                let dir = expand_home(&raw, home);
                return (
                    dir.clone(),
                    Some(format!(
                        "~/.zshenv sets ZDOTDIR={}: zsh reads {}",
                        raw,
                        dir.display()
                    )),
                );
            }
        }
    }
    (home.to_path_buf(), None)
}

/// Extract the value of the last `ZDOTDIR=` assignment in `.zshenv`
/// text, ignoring comments. Quotes are stripped; `$HOME` / `~` stay
/// in the returned string for [`expand_home`] to deal with.
pub fn parse_zdotdir_assignment(text: &str) -> Option<String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with('#') {
                return None;
            }
            let rest = line
                .strip_prefix("export ")
                .unwrap_or(line)
                .trim_start()
                .strip_prefix("ZDOTDIR=")?;
            // Stop at a trailing comment or a command separator; an
            // unquoted value cannot contain either.
            let value = rest.split(['#', ';']).next().unwrap_or(rest).trim();
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(value);
            (!value.is_empty()).then(|| value.to_string())
        })
        .next_back()
}

/// Expand a leading `$HOME`, `${HOME}`, or `~` against `home`.
fn expand_home(raw: &str, home: &Path) -> PathBuf {
    for prefix in ["${HOME}", "$HOME", "~"] {
        if let Some(rest) = raw.strip_prefix(prefix) {
            let rest = rest.trim_start_matches('/');
            return if rest.is_empty() {
                home.to_path_buf()
            } else {
                home.join(rest)
            };
        }
    }
    absolutize(Path::new(raw), home)
}

/// Anchor a relative path at `home`; absolute paths pass through.
fn absolutize(path: &Path, home: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        home.join(path)
    }
}

/// Follow a chain of symlinks to the file that actually holds the
/// bytes, so a write lands in the dotfiles repo rather than replacing
/// the link.
///
/// Bounded and total: a cycle, a broken link, or an unreadable link
/// stops the walk and yields the last path we could name, which is
/// the same path a plain write would have used.
fn resolve_symlinks(fs: &dyn Fs, path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    for _ in 0..MAX_SYMLINK_HOPS {
        if !fs.is_symlink(&current) {
            break;
        }
        let Ok(target) = fs.readlink(&current) else {
            break;
        };
        let next = if target.is_absolute() {
            target
        } else {
            current
                .parent()
                .map(|p| p.join(&target))
                .unwrap_or_else(|| target.clone())
        };
        if next == current {
            break;
        }
        current = next;
    }
    current
}

/// Render a path as `~/…` when it sits under `home`.
pub fn display_home_relative(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rel) => format!("~/{}", rel.display()),
        Err(_) => path.display().to_string(),
    }
}

// ── The marked block ────────────────────────────────────────────

/// What writing the block did to an rc file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BlockOutcome {
    /// The rc file did not exist; it now holds just our block.
    Created,
    /// The block was added to an rc file that had no dodot block.
    Appended,
    /// An existing block was rewritten in place — never duplicated.
    Replaced,
    /// The file already carried exactly this block. No write.
    Unchanged,
}

impl BlockOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            BlockOutcome::Created => "created",
            BlockOutcome::Appended => "appended",
            BlockOutcome::Replaced => "replaced",
            BlockOutcome::Unchanged => "unchanged",
        }
    }
}

/// Render the managed block around one or more body lines.
pub fn render_block(body: &[&str]) -> String {
    let mut out = String::with_capacity(BLOCK_START.len() + BLOCK_END.len() + 128);
    out.push_str(BLOCK_START);
    out.push('\n');
    for line in body {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(BLOCK_END);
    out.push('\n');
    out
}

/// Byte range of the managed block inside `text`, trailing newline
/// included. `None` when either marker is missing — a half-written
/// block is treated as "no block", so the next `--write` appends a
/// complete one instead of splicing into garbage.
pub fn find_block(text: &str) -> Option<(usize, usize)> {
    let start = text.find(BLOCK_START)?;
    let after_start = start + BLOCK_START.len();
    let end_rel = text[after_start..].find(BLOCK_END)?;
    let end = after_start + end_rel + BLOCK_END.len();
    let end = if text.as_bytes().get(end) == Some(&b'\n') {
        end + 1
    } else {
        end
    };
    Some((start, end))
}

/// Splice `block` into `existing` rc text (or into nothing, for a file
/// that does not exist yet), returning the new content and what
/// happened.
///
/// Strictly idempotent: an existing block is replaced where it stands,
/// never duplicated, and identical content is a no-op. Everything
/// outside the markers is preserved byte for byte — dodot writes one
/// block and touches nothing else (spec §1.5).
pub fn apply_block(existing: Option<&str>, block: &str) -> (String, BlockOutcome) {
    let Some(existing) = existing else {
        return (block.to_string(), BlockOutcome::Created);
    };
    if let Some((start, end)) = find_block(existing) {
        if &existing[start..end] == block {
            return (existing.to_string(), BlockOutcome::Unchanged);
        }
        let mut out = String::with_capacity(existing.len() + block.len());
        out.push_str(&existing[..start]);
        out.push_str(block);
        out.push_str(&existing[end..]);
        return (out, BlockOutcome::Replaced);
    }
    let mut out = existing.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.is_empty() && !out.ends_with("\n\n") {
        out.push('\n');
    }
    out.push_str(block);
    (out, BlockOutcome::Appended)
}

/// Write the managed block to `path`, creating the file (and its
/// parent) when missing. Returns without writing when the block is
/// already exactly right.
pub fn write_block(fs: &dyn Fs, path: &Path, block: &str) -> Result<BlockOutcome> {
    let existing = if fs.exists(path) {
        Some(fs.read_to_string(path)?)
    } else {
        None
    };
    let (content, outcome) = apply_block(existing.as_deref(), block);
    if outcome != BlockOutcome::Unchanged {
        if let Some(parent) = path.parent() {
            fs.mkdir_all(parent)?;
        }
        fs.write_file(path, content.as_bytes())?;
    }
    Ok(outcome)
}

// ── The static scan ─────────────────────────────────────────────

/// What an rc file says about the hookup — configuration state, never
/// activation state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HookPresence {
    /// dodot's managed block is there.
    ManagedBlock,
    /// A hook is there, wired by hand — `eval "$(dodot init-sh)"` or a
    /// direct source of the generated script. Left strictly alone:
    /// existing hookups keep working untouched (spec §6).
    Manual,
    /// Nothing in this file mentions dodot's init script.
    Absent,
}

impl HookPresence {
    pub fn as_str(self) -> &'static str {
        match self {
            HookPresence::ManagedBlock => "managed-block",
            HookPresence::Manual => "manual",
            HookPresence::Absent => "absent",
        }
    }

    /// Whether the hook is wired at all, however it got there.
    pub fn is_present(self) -> bool {
        !matches!(self, HookPresence::Absent)
    }
}

/// Classify rc text by which kind of dodot hook it carries.
pub fn scan_hook(text: &str) -> HookPresence {
    if find_block(text).is_some() {
        return HookPresence::ManagedBlock;
    }
    if text.lines().any(is_manual_hook_line) {
        return HookPresence::Manual;
    }
    HookPresence::Absent
}

/// Read `path` and classify it. A missing or unreadable file reads as
/// [`HookPresence::Absent`] — for the diagnosis split, "we could not
/// see a hook" and "there is no hook" call for the same next step.
pub fn scan_hook_file(fs: &dyn Fs, path: &Path) -> HookPresence {
    if !fs.exists(path) {
        return HookPresence::Absent;
    }
    fs.read_to_string(path)
        .map(|t| scan_hook(&t))
        .unwrap_or(HookPresence::Absent)
}

/// A hand-wired hook line: either form, as long as it is not
/// commented out.
fn is_manual_hook_line(line: &str) -> bool {
    let line = line.trim();
    if line.starts_with('#') {
        return false;
    }
    line.contains("dodot init-sh") || line.contains("dodot-init.sh")
}

// ── macOS bash chain ────────────────────────────────────────────

/// The profile that needs a `.bashrc` chain, or `None` when one
/// already chains (spec §4.3 rung 2).
///
/// macOS Terminal spawns *login* shells, which read `.bash_profile` /
/// `.profile` and never `.bashrc` — where the hook block lands. If
/// neither profile reaches `.bashrc`, the hook is written but never
/// read, which is exactly the silent failure this epic exists to kill.
pub fn bash_chain_target(fs: &dyn Fs, home: &Path) -> Option<PathBuf> {
    for name in [".bash_profile", ".profile"] {
        let path = home.join(name);
        if !fs.exists(&path) {
            continue;
        }
        let chains = fs
            .read_to_string(&path)
            .map(|t| {
                t.lines()
                    .any(|l| !l.trim().starts_with('#') && l.contains(".bashrc"))
            })
            .unwrap_or(false);
        if chains {
            return None;
        }
    }
    Some(home.join(".bash_profile"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempEnvironment;

    fn env_zsh() -> ShellEnv {
        ShellEnv {
            shell: Some("/bin/zsh".into()),
            zdotdir: None,
        }
    }

    // ── Shell detection ─────────────────────────────────────────

    #[test]
    fn shell_detection_reads_the_basename_and_refuses_the_rest() {
        let cases = [
            ("/bin/zsh", Some(HookupShell::Zsh)),
            ("/usr/local/bin/bash", Some(HookupShell::Bash)),
            ("zsh", Some(HookupShell::Zsh)),
            // Login-shell argv[0] convention.
            ("-zsh", Some(HookupShell::Zsh)),
            ("/usr/bin/fish", None),
            ("/bin/sh", None),
            ("/opt/nu", None),
            ("", None),
        ];
        for (raw, expected) in cases {
            assert_eq!(HookupShell::from_shell_path(raw), expected, "raw={raw:?}");
        }
    }

    // ── The ladder ──────────────────────────────────────────────

    #[test]
    fn zsh_targets_zshrc_and_bash_targets_bashrc() {
        let env = TempEnvironment::builder().build();
        let home = env.home.clone();

        let zsh = resolve_rc(
            env.fs.as_ref(),
            &home,
            Some(HookupShell::Zsh),
            &env_zsh(),
            None,
        );
        assert_eq!(zsh.path, home.join(".zshrc"));
        assert!(!zsh.exists, "a fresh home has no .zshrc — still the target");
        assert!(zsh.notes.is_empty());

        let bash = resolve_rc(
            env.fs.as_ref(),
            &home,
            Some(HookupShell::Bash),
            &ShellEnv {
                shell: Some("/bin/bash".into()),
                zdotdir: None,
            },
            None,
        );
        assert_eq!(bash.path, home.join(".bashrc"));
    }

    #[test]
    fn zdotdir_from_the_environment_moves_the_target_and_is_announced() {
        let env = TempEnvironment::builder().build();
        let home = env.home.clone();
        let shell_env = ShellEnv {
            shell: Some("/bin/zsh".into()),
            zdotdir: Some("$HOME/.config/zsh".into()),
        };

        let target = resolve_rc(
            env.fs.as_ref(),
            &home,
            Some(HookupShell::Zsh),
            &shell_env,
            None,
        );
        assert_eq!(target.path, home.join(".config/zsh/.zshrc"));
        assert!(
            target.notes.iter().any(|n| n.contains("ZDOTDIR")),
            "the move must be announced: {:?}",
            target.notes
        );
    }

    #[test]
    fn zdotdir_is_peeked_out_of_zshenv_when_the_environment_is_silent() {
        // `.zshenv` is the one file zsh always reads, so it is the
        // legal place ZDOTDIR gets set — and dodot, invoked from
        // anywhere, won't have it in its own environment.
        let env = TempEnvironment::builder().build();
        let home = env.home.clone();
        env.fs
            .write_file(
                &home.join(".zshenv"),
                b"# my zshenv\nexport ZDOTDIR=\"$HOME/dotfiles/zsh\"\n",
            )
            .unwrap();

        let target = resolve_rc(
            env.fs.as_ref(),
            &home,
            Some(HookupShell::Zsh),
            &env_zsh(),
            None,
        );
        assert_eq!(target.path, home.join("dotfiles/zsh/.zshrc"));
        assert!(
            target.notes.iter().any(|n| n.contains(".zshenv")),
            "notes should say where ZDOTDIR came from: {:?}",
            target.notes
        );
    }

    #[test]
    fn zdotdir_parsing_handles_quotes_comments_and_overrides() {
        assert_eq!(
            parse_zdotdir_assignment("export ZDOTDIR=\"$HOME/a\"\n"),
            Some("$HOME/a".into())
        );
        assert_eq!(
            parse_zdotdir_assignment("ZDOTDIR='/tmp/z'\n"),
            Some("/tmp/z".into())
        );
        assert_eq!(
            parse_zdotdir_assignment("ZDOTDIR=~/zsh # trailing comment\n"),
            Some("~/zsh".into())
        );
        // Commented-out assignments don't count.
        assert_eq!(parse_zdotdir_assignment("# ZDOTDIR=/nope\n"), None);
        assert_eq!(parse_zdotdir_assignment("export PATH=/bin\n"), None);
        // Last assignment wins, same as the shell would see it.
        assert_eq!(
            parse_zdotdir_assignment("ZDOTDIR=/first\nZDOTDIR=/second\n"),
            Some("/second".into())
        );
    }

    #[test]
    fn a_symlinked_rc_is_written_through_and_announced() {
        let env = TempEnvironment::builder().build();
        let home = env.home.clone();
        let repo_rc = home.join("dotfiles/zsh/zshrc");
        env.fs.mkdir_all(repo_rc.parent().unwrap()).unwrap();
        env.fs.write_file(&repo_rc, b"# real rc\n").unwrap();
        env.fs.symlink(&repo_rc, &home.join(".zshrc")).unwrap();

        let target = resolve_rc(
            env.fs.as_ref(),
            &home,
            Some(HookupShell::Zsh),
            &env_zsh(),
            None,
        );
        assert_eq!(
            target.path, repo_rc,
            "the write must land in the repo, not replace the link"
        );
        assert_eq!(target.link_source, Some(home.join(".zshrc")));
        assert!(target.exists);
        let note = target.notes.join(" ");
        assert!(
            note.contains("~/.zshrc") && note.contains("dotfiles/zsh/zshrc"),
            "write-through must be announced, not silent: {note}"
        );
    }

    #[test]
    fn explicit_rc_overrides_the_whole_ladder() {
        let env = TempEnvironment::builder().build();
        let home = env.home.clone();
        let custom = home.join("elsewhere/rc.sh");

        let target = resolve_rc(
            env.fs.as_ref(),
            &home,
            Some(HookupShell::Zsh),
            &ShellEnv {
                shell: Some("/bin/zsh".into()),
                zdotdir: Some("/somewhere/else".into()),
            },
            Some(&custom),
        );
        assert_eq!(target.path, custom);
    }

    // ── The marked block ────────────────────────────────────────

    #[test]
    fn block_is_created_appended_replaced_and_then_left_alone() {
        let block = render_block(&["HOOK v1"]);

        let (created, outcome) = apply_block(None, &block);
        assert_eq!(outcome, BlockOutcome::Created);
        assert_eq!(created, block);

        let (appended, outcome) = apply_block(Some("export PATH=/bin\n"), &block);
        assert_eq!(outcome, BlockOutcome::Appended);
        assert!(appended.starts_with("export PATH=/bin\n"));
        assert!(appended.contains("HOOK v1"));

        let (unchanged, outcome) = apply_block(Some(&appended), &block);
        assert_eq!(outcome, BlockOutcome::Unchanged);
        assert_eq!(unchanged, appended);

        let v2 = render_block(&["HOOK v2"]);
        let (replaced, outcome) = apply_block(Some(&appended), &v2);
        assert_eq!(outcome, BlockOutcome::Replaced);
        assert!(replaced.contains("HOOK v2"));
        assert!(!replaced.contains("HOOK v1"));
        assert_eq!(
            replaced.matches(BLOCK_START).count(),
            1,
            "a re-run replaces, it never duplicates"
        );
        assert!(replaced.starts_with("export PATH=/bin\n"));
    }

    #[test]
    fn content_around_the_block_survives_a_replacement() {
        let before = "# top\n";
        let after = "# bottom\n";
        let text = format!("{before}{}{after}", render_block(&["OLD"]));
        let (out, outcome) = apply_block(Some(&text), &render_block(&["NEW"]));
        assert_eq!(outcome, BlockOutcome::Replaced);
        assert!(out.starts_with(before), "{out:?}");
        assert!(out.ends_with(after), "{out:?}");
    }

    #[test]
    fn a_half_written_block_is_appended_to_not_spliced_into() {
        let broken = format!("{BLOCK_START}\ntruncated\n");
        assert!(find_block(&broken).is_none());
        let (out, outcome) = apply_block(Some(&broken), &render_block(&["HOOK"]));
        assert_eq!(outcome, BlockOutcome::Appended);
        assert!(out.contains("HOOK"));
    }

    #[test]
    fn write_block_creates_the_file_and_stays_idempotent_on_disk() {
        let env = TempEnvironment::builder().build();
        let rc = env.home.join(".config/zsh/.zshrc");
        let block = render_block(&["HOOK"]);

        assert_eq!(
            write_block(env.fs.as_ref(), &rc, &block).unwrap(),
            BlockOutcome::Created
        );
        let first = env.fs.read_to_string(&rc).unwrap();
        assert_eq!(
            write_block(env.fs.as_ref(), &rc, &block).unwrap(),
            BlockOutcome::Unchanged
        );
        assert_eq!(env.fs.read_to_string(&rc).unwrap(), first);
    }

    // ── The scan ────────────────────────────────────────────────

    #[test]
    fn scan_tells_managed_manual_and_absent_apart() {
        assert_eq!(
            scan_hook(&render_block(&["whatever"])),
            HookPresence::ManagedBlock
        );
        assert_eq!(
            scan_hook("eval \"$(dodot init-sh)\"\n"),
            HookPresence::Manual
        );
        assert_eq!(
            scan_hook(". \"$HOME/.local/share/dodot/shell/dodot-init.sh\"\n"),
            HookPresence::Manual
        );
        assert_eq!(scan_hook("alias ll='ls -l'\n"), HookPresence::Absent);
        // A commented-out hook is exactly the "broken hook" story —
        // it must not read as present.
        assert_eq!(
            scan_hook("# eval \"$(dodot init-sh)\"\n"),
            HookPresence::Absent
        );
    }

    #[test]
    fn scanning_a_missing_file_is_absent_not_an_error() {
        let env = TempEnvironment::builder().build();
        assert_eq!(
            scan_hook_file(env.fs.as_ref(), &env.home.join(".zshrc")),
            HookPresence::Absent
        );
    }

    // ── macOS bash chain ────────────────────────────────────────

    #[test]
    fn bash_chain_is_needed_until_some_profile_sources_bashrc() {
        let env = TempEnvironment::builder().build();
        let home = env.home.clone();

        // No profile at all: Terminal's login shell would never read
        // .bashrc, so the chain is needed.
        assert_eq!(
            bash_chain_target(env.fs.as_ref(), &home),
            Some(home.join(".bash_profile"))
        );

        // A .profile that chains counts — we don't insist on our own file.
        env.fs
            .write_file(&home.join(".profile"), b". \"$HOME/.bashrc\"\n")
            .unwrap();
        assert_eq!(bash_chain_target(env.fs.as_ref(), &home), None);
    }

    #[test]
    fn a_commented_chain_does_not_count() {
        let env = TempEnvironment::builder().build();
        env.fs
            .write_file(&env.home.join(".bash_profile"), b"# . ~/.bashrc\n")
            .unwrap();
        assert!(bash_chain_target(env.fs.as_ref(), &env.home).is_some());
    }
}

//! Built-in Homebrew bootstrap for the generated init script
//! (`shell-hookup-ergonomics.lex` §4).
//!
//! On macOS essentially every pack downstream needs Homebrew's
//! environment, and `/opt/homebrew/bin` is not on `PATH` until
//! *something* puts it there. dodot owns that bootstrap so the user's
//! rc file can stay empty and no `001-homebrew` pack is required.
//!
//! # Ask brew, don't guess
//!
//! `brew shellenv <shell>` already emits the block, and Homebrew is the
//! authority on its own bootstrap. We capture that output when `dodot
//! up` / `dodot down` regenerate the init script and persist it to the
//! datastore, next to the generated script
//! ([`crate::paths::Pather::homebrew_cache_path`]). Every generation
//! path — the baked-in block in `dodot-init.sh` and `dodot init-sh`'s
//! stdout alike — emits from that cached text, so sourcing asks brew
//! nothing and a brew that changes its block is picked up by the next
//! `up`. A hand-written approximation would silently rot, which is why
//! there is exactly one mechanism here and no fallback emitter
//! (`pack-ordering.lex` §2.2).
//!
//! Never reach for `eval "$(brew shellenv)"` *at shell time*: that runs
//! brew itself on every shell start. Capture-at-`up` is the whole
//! point.
//!
//! # Cache invalidation
//!
//! The cache refreshes on the next `up`/`down` — the same contract as
//! every other artifact dodot generates, and deliberately not a
//! freshness check against brew (a second mechanism doing the job the
//! first already does). The one guard is the resolved prefix: it is
//! stored with the captured text, and a cache whose prefix no longer
//! matches the host's detected prefix (an Intel → arm move, a
//! relocated install) is a miss, never served.
//!
//! # Two blocks, one guard
//!
//! The shell argument is what selects the shell-specific lines —
//! `brew shellenv zsh` carries `fpath[1,0]=` and `export FPATH`, which
//! `sh` and `bash` must not see. So we capture twice (`sh` and `zsh`)
//! and emit both under a `$ZSH_VERSION` guard. Homebrew's own man page
//! documents `eval "$(brew shellenv zsh)"` as the recommended form,
//! noting the shell "should be specified explicitly … but will be
//! detected automatically if not provided (but this may not be
//! correct)".
//!
//! # Verbatim, and first
//!
//! Captured output is emitted **byte-for-byte** — not reindented, not
//! filtered. In particular the `path_helper` re-exec line stays. An
//! older concern, that it demotes dodot's own PATH entries, no longer
//! holds: brew now passes `PATH_HELPER_ROOT`, which only prepends
//! brew's own paths and leaves existing entries in order. Measured:
//!
//! ```text
//! input:  /aaa-first:/usr/bin:/bin:/zzz-last
//! output: /opt/homebrew/bin:/opt/homebrew/sbin:/aaa-first:/usr/bin:/bin:/zzz-last
//! ```
//!
//! (Plain `path_helper`, which `/etc/zprofile` runs, *does* reorder —
//! that is where the older advice came from.) Do not edit brew's output
//! to avoid a problem that no longer exists.
//!
//! The block is emitted *first*, above dodot's own PATH additions, so
//! dodot's contributions are always the last word. That ordering is
//! what makes the built-in work with no pack-ordering choreography.
//!
//! # Cost
//!
//! This is the first command execution dodot has ever put on the shell
//! startup path — brew's choice, not dodot's, but ours to state
//! plainly. Two costs, identical under both hook shapes:
//!
//! - *The emitted block*, paid by every shell that sources the script:
//!   two process spawns (`env` plus `path_helper`). Measured on an
//!   Apple-silicon mac over 200 iterations: ~3.1 ms wall per
//!   invocation, ~2.7 ms net of the ~0.5 ms `$( )` subshell baseline.
//! - *The capture itself*, two `brew shellenv` runs (`sh` and `zsh`),
//!   ~10-20 ms each on the same machine — paid by `dodot up`/`down`
//!   ([`capture_and_persist`]) and cached in the datastore, so a shell
//!   never pays it. The hand-wired `eval "$(dodot init-sh)"` hook
//!   regenerates the script on every shell start, but generation emits
//!   from the cache ([`cached_or_capture`]) and spawns brew zero
//!   times. The one exception is a cold cache — the first `init-sh`
//!   before any `up` has run: that shell captures in memory, emits,
//!   and does not persist (`init-sh` is passive — #121); the next `up`
//!   warms the cache.
//!
//! `[shell] homebrew = "off"` turns all of it off in one config key.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::config::DodotConfig;
use crate::datastore::CommandRunner;
use crate::fs::Fs;
use crate::paths::Pather;
use crate::{DodotError, Result};

/// Standard Homebrew install prefixes, in probe order: Apple silicon
/// first, then the Intel/`/usr/local` layout.
pub const DEFAULT_PREFIXES: [&str; 2] = ["/opt/homebrew", "/usr/local"];

/// The `[shell] homebrew` config key — the gate on the whole mechanism.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrewBootstrapMode {
    /// Emit the block when this host has an executable `brew` under a
    /// known prefix.
    #[default]
    Auto,
    /// Emit nothing, ever.
    Off,
}

impl BrewBootstrapMode {
    /// Parse the config string, rejecting anything outside the
    /// vocabulary rather than silently falling back — a typo'd `of`
    /// should say so, not quietly disable the bootstrap.
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "auto" => Ok(Self::Auto),
            "off" => Ok(Self::Off),
            other => Err(DodotError::Config(format!(
                "unknown `[shell] homebrew` value `{other}`: must be one of auto, off"
            ))),
        }
    }
}

/// Host inputs brew detection depends on, injected rather than read
/// from the process so tests never need brew installed — or a mac.
#[derive(Debug, Clone)]
pub struct BrewHost {
    /// Homebrew's bootstrap is a macOS concern; off the platform we
    /// emit nothing regardless of what is installed.
    pub is_macos: bool,
    /// Prefix candidates in probe order. The first whose `bin/brew` is
    /// an executable file wins.
    pub prefix_candidates: Vec<PathBuf>,
}

impl BrewHost {
    /// Snapshot the running host: `$HOMEBREW_PREFIX` first when set,
    /// then [`DEFAULT_PREFIXES`].
    pub fn detect() -> Self {
        Self::new(
            cfg!(target_os = "macos"),
            std::env::var("HOMEBREW_PREFIX").ok(),
        )
    }

    /// Build the candidate list from its two inputs.
    ///
    /// `$HOMEBREW_PREFIX` leads when set, but does not short-circuit:
    /// each candidate still has to hold an executable `bin/brew`, so a
    /// stale or mistyped export falls through to the standard prefixes
    /// instead of suppressing the block.
    pub fn new(is_macos: bool, prefix_env: Option<String>) -> Self {
        let mut prefix_candidates: Vec<PathBuf> = Vec::with_capacity(3);
        if let Some(prefix) = prefix_env.filter(|p| !p.trim().is_empty()) {
            prefix_candidates.push(PathBuf::from(prefix));
        }
        prefix_candidates.extend(DEFAULT_PREFIXES.iter().map(PathBuf::from));
        Self {
            is_macos,
            prefix_candidates,
        }
    }
}

/// One host's captured `brew shellenv` output, per shell dialect.
///
/// Also the datastore cache's serialized shape (JSON, at
/// [`Pather::homebrew_cache_path`]): what `up` persisted is exactly
/// what emission reads back, so the emitted block is byte-identical
/// whether it came from the cache or from a live capture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrewBlocks {
    /// The prefix the blocks were captured from. Emitted as a comment
    /// so a reader of `dodot-init.sh` can see which brew answered.
    pub prefix: PathBuf,
    /// `brew shellenv sh` — used for everything that is not zsh.
    pub sh: String,
    /// `brew shellenv zsh` — carries the zsh-only `fpath`/`FPATH` lines.
    pub zsh: String,
}

/// Capture the bootstrap live and persist it to the datastore cache —
/// the entry point for `up` and `down`, the commands that regenerate
/// the init script. The capture belongs to them, not to every
/// generation.
///
/// One call spawns `brew` **twice**, once per shell dialect. The
/// result lands at [`Pather::homebrew_cache_path`] so passive
/// generation paths ([`cached_or_capture`]) emit without spawning
/// brew. A capture with nothing to emit — mode `off`, not macOS, no
/// brew — *removes* the cache, keeping it in lockstep with what the
/// written script carries: a host that lost its brew can never serve
/// a stale block.
///
/// The only capture error is a bad `[shell] homebrew` value; a host
/// without brew is `Ok(None)`. Cache IO surfaces its own errors.
pub fn capture_and_persist(
    fs: &dyn Fs,
    runner: &dyn CommandRunner,
    config: &DodotConfig,
    paths: &dyn Pather,
) -> Result<Option<BrewBlocks>> {
    let mode = BrewBootstrapMode::parse(&config.shell.homebrew)?;
    let blocks = capture(fs, runner, mode, &BrewHost::detect());
    persist_cache(fs, &paths.homebrew_cache_path(), blocks.as_ref())?;
    Ok(blocks)
}

/// Emit-side entry point for passive commands (`dodot init-sh`): serve
/// the cached capture when it is warm and its prefix still matches the
/// host; capture in memory on a miss; never write the datastore.
///
/// With a warm cache this spawns brew **zero** times — prefix
/// detection is a couple of `stat` calls. On a miss (no `up` has run
/// since the cache was cleared, or the prefix moved) it pays one live
/// capture and deliberately does not persist it: `init-sh` is passive,
/// and passive commands writing the datastore is a standing defect
/// class (#121). The next `up` warms the cache.
///
/// The only error it can return is a bad `[shell] homebrew` value; a
/// host without brew is `Ok(None)`.
pub fn cached_or_capture(
    fs: &dyn Fs,
    runner: &dyn CommandRunner,
    config: &DodotConfig,
    paths: &dyn Pather,
) -> Result<Option<BrewBlocks>> {
    let mode = BrewBootstrapMode::parse(&config.shell.homebrew)?;
    Ok(cached_or_capture_at(
        fs,
        runner,
        mode,
        &BrewHost::detect(),
        &paths.homebrew_cache_path(),
    ))
}

/// [`cached_or_capture`] with the host and cache path injected, so
/// tests drive it without a mac, a brew, or a real datastore.
fn cached_or_capture_at(
    fs: &dyn Fs,
    runner: &dyn CommandRunner,
    mode: BrewBootstrapMode,
    host: &BrewHost,
    cache_path: &Path,
) -> Option<BrewBlocks> {
    if mode == BrewBootstrapMode::Off || !host.is_macos {
        return None;
    }
    let prefix = detect_prefix(fs, host)?;
    if let Some(cached) = load_cache(fs, cache_path) {
        if cached.prefix == prefix {
            return Some(cached);
        }
        debug!(
            cached = %cached.prefix.display(),
            detected = %prefix.display(),
            "homebrew cache is for another prefix; capturing live"
        );
    }
    capture_at(runner, &prefix)
}

/// Read the cached capture back, or `None` when it is absent or
/// unreadable. Unreadable includes unparseable: a corrupt or
/// old-format cache is a miss (live capture), never an error — the
/// next `up` rewrites it wholesale.
fn load_cache(fs: &dyn Fs, path: &Path) -> Option<BrewBlocks> {
    let text = fs.read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Write the captured blocks to the cache, or remove the cache when
/// there is nothing to emit. Only `up`/`down` reach this, via
/// [`capture_and_persist`] — the emit side never writes (#121).
///
/// The write is atomic ([`Fs::write_atomic`]), never straight onto the
/// cache path. A truncating write is visible to a concurrent reader,
/// and the reader here is *every shell the user opens*: a torn read
/// parses as a miss ([`load_cache`]), and a miss costs a live
/// `brew shellenv` on every `init-sh` until the next `up` rewrites the
/// file — the exact cost this cache exists to remove, lost quietly,
/// because the fallback is correct-but-slow rather than broken.
fn persist_cache(fs: &dyn Fs, path: &Path, blocks: Option<&BrewBlocks>) -> Result<()> {
    match blocks {
        Some(blocks) => {
            if let Some(parent) = path.parent() {
                fs.mkdir_all(parent)?;
            }
            let json = serde_json::to_string_pretty(blocks)
                .map_err(|e| DodotError::Other(format!("serialize homebrew cache: {e}")))?;
            fs.write_atomic(path, json.as_bytes())
        }
        None => {
            if fs.exists(path) {
                fs.remove_file(path)?;
            }
            Ok(())
        }
    }
}

/// Capture Homebrew's bootstrap for this host, or `None` when there is
/// nothing to emit.
///
/// `None` — never an error — for every "no brew here" case: the mode is
/// `off`, the host is not macOS, no candidate prefix holds an executable
/// `bin/brew`, or brew itself fails to answer. A missing brew is not a
/// broken deployment, and the next `dodot up` heals the script once brew
/// is installed.
pub fn capture(
    fs: &dyn Fs,
    runner: &dyn CommandRunner,
    mode: BrewBootstrapMode,
    host: &BrewHost,
) -> Option<BrewBlocks> {
    if mode == BrewBootstrapMode::Off {
        return None;
    }
    if !host.is_macos {
        return None;
    }

    let prefix = detect_prefix(fs, host)?;
    capture_at(runner, &prefix)
}

/// Ask the brew at `prefix` for both dialect blocks — the two spawns
/// every "capture" in this module ultimately means.
fn capture_at(runner: &dyn CommandRunner, prefix: &Path) -> Option<BrewBlocks> {
    let brew = brew_binary(prefix);
    let sh = shellenv(runner, &brew, "sh")?;
    let zsh = shellenv(runner, &brew, "zsh")?;
    Some(BrewBlocks {
        prefix: prefix.to_path_buf(),
        sh,
        zsh,
    })
}

/// First candidate prefix holding an executable `bin/brew`.
fn detect_prefix(fs: &dyn Fs, host: &BrewHost) -> Option<PathBuf> {
    host.prefix_candidates
        .iter()
        .find(|prefix| is_executable_file(fs, &brew_binary(prefix)))
        .cloned()
}

/// Whether `path` is a regular file carrying an execute bit — the check
/// behind every "holds a `bin/brew`" claim in this module.
///
/// Existence alone is not the question, because the path is about to be
/// *run*: a non-executable leftover, a directory, or a dangling symlink
/// at `bin/brew` must not stop the probe, so it is skipped and the next
/// candidate gets its turn. [`Fs::stat`] follows symlinks, which is what
/// a `bin/brew` symlinked into a Cellar needs.
fn is_executable_file(fs: &dyn Fs, path: &Path) -> bool {
    matches!(fs.stat(path), Ok(meta) if meta.is_file && meta.mode & 0o111 != 0)
}

fn brew_binary(prefix: &Path) -> PathBuf {
    prefix.join("bin").join("brew")
}

/// Run `brew shellenv <shell>` and return its stdout verbatim.
///
/// Any failure — spawn error, non-zero exit, empty output — degrades to
/// `None` so a half-broken brew cannot break `dodot up`.
fn shellenv(runner: &dyn CommandRunner, brew: &Path, shell: &str) -> Option<String> {
    let executable = brew.to_string_lossy().to_string();
    let args = vec!["shellenv".to_string(), shell.to_string()];
    match runner.run(&executable, &args) {
        Ok(out) if out.exit_code == 0 && !out.stdout.trim().is_empty() => Some(out.stdout),
        Ok(out) => {
            debug!(
                brew = %executable,
                shell,
                exit_code = out.exit_code,
                "brew shellenv produced nothing usable; skipping the Homebrew block"
            );
            None
        }
        Err(e) => {
            debug!(
                brew = %executable,
                shell,
                error = %e,
                "brew shellenv failed; skipping the Homebrew block"
            );
            None
        }
    }
}

/// Emit the captured blocks under a `$ZSH_VERSION` guard.
///
/// Both branches carry brew's output byte-for-byte, unindented — see
/// the module docs on why nothing here rewrites what brew said. The
/// guard is the only thing dodot contributes, and it is what keeps the
/// zsh-only `fpath[1,0]=` / `export FPATH` lines away from `sh` and
/// `bash`.
pub(super) fn emit_homebrew_block(script: &mut String, blocks: &BrewBlocks) {
    writeln!(script, "# ── Homebrew environment ──").unwrap();
    writeln!(
        script,
        "# Captured from `brew shellenv` at {} on the last `dodot up`.",
        blocks.prefix.display()
    )
    .unwrap();
    writeln!(
        script,
        "# Emitted verbatim and first, so dodot's own PATH additions below win."
    )
    .unwrap();
    writeln!(script, "if [ -n \"${{ZSH_VERSION:-}}\" ]; then").unwrap();
    write_block(script, &blocks.zsh);
    writeln!(script, "else").unwrap();
    write_block(script, &blocks.sh);
    writeln!(script, "fi").unwrap();
    writeln!(script).unwrap();
}

/// Append a captured block, guaranteeing it ends on its own line so the
/// `else`/`fi` that follows is not swallowed by an unterminated last
/// line.
fn write_block(script: &mut String, block: &str) {
    script.push_str(block);
    if !block.ends_with('\n') {
        script.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datastore::CommandOutput;
    use crate::testing::TempEnvironment;
    use crate::Result;
    use std::sync::Mutex;

    const SH_BLOCK: &str = "export HOMEBREW_PREFIX=\"/opt/homebrew\";\n\
         eval \"$(/usr/bin/env PATH_HELPER_ROOT=\"/opt/homebrew\" /usr/libexec/path_helper -s)\"\n";
    const ZSH_BLOCK: &str = "export HOMEBREW_PREFIX=\"/opt/homebrew\";\n\
         fpath[1,0]=\"/opt/homebrew/share/zsh/site-functions\";\n\
         export FPATH;\n\
         eval \"$(/usr/bin/env PATH_HELPER_ROOT=\"/opt/homebrew\" /usr/libexec/path_helper -s)\"\n";

    /// Stands in for a real `brew`: answers `shellenv <shell>` with a
    /// canned block and records every invocation. No test in this file
    /// needs Homebrew installed.
    struct FakeBrew {
        calls: Mutex<Vec<String>>,
        exit_code: i32,
        stdout: Option<String>,
    }

    impl FakeBrew {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                exit_code: 0,
                stdout: None,
            }
        }

        fn failing(exit_code: i32) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                exit_code,
                stdout: Some(String::new()),
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl CommandRunner for FakeBrew {
        fn run(&self, executable: &str, arguments: &[String]) -> Result<CommandOutput> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("{executable} {}", arguments.join(" ")));
            let stdout = self.stdout.clone().unwrap_or_else(|| {
                match arguments.get(1).map(String::as_str) {
                    Some("zsh") => ZSH_BLOCK,
                    _ => SH_BLOCK,
                }
                .to_string()
            });
            Ok(CommandOutput {
                exit_code: self.exit_code,
                stdout,
                stderr: String::new(),
            })
        }
    }

    /// A brew-shaped install under a temp root, so prefix detection has
    /// something to find without touching the real `/opt/homebrew`.
    /// Executable, because that is what detection requires.
    fn install_brew(env: &TempEnvironment, prefix: &Path) {
        install_brew_with_mode(env, prefix, 0o755);
    }

    /// The same install at an arbitrary mode, for proving what a
    /// non-executable `bin/brew` does.
    fn install_brew_with_mode(env: &TempEnvironment, prefix: &Path, mode: u32) {
        env.fs.mkdir_all(&prefix.join("bin")).unwrap();
        env.fs
            .write_file_with_mode(&brew_binary(prefix), b"#!/bin/sh\n", mode)
            .unwrap();
    }

    fn host_with(prefixes: &[&Path]) -> BrewHost {
        BrewHost {
            is_macos: true,
            prefix_candidates: prefixes.iter().map(PathBuf::from).collect(),
        }
    }

    #[test]
    fn mode_parses_its_vocabulary_and_rejects_the_rest() {
        assert_eq!(
            BrewBootstrapMode::parse("auto").unwrap(),
            BrewBootstrapMode::Auto
        );
        assert_eq!(
            BrewBootstrapMode::parse("off").unwrap(),
            BrewBootstrapMode::Off
        );
        assert_eq!(BrewBootstrapMode::default(), BrewBootstrapMode::Auto);

        let err = BrewBootstrapMode::parse("of").unwrap_err().to_string();
        assert!(err.contains("auto, off"), "error was: {err}");
    }

    #[test]
    fn homebrew_prefix_env_leads_the_candidates() {
        let host = BrewHost::new(true, Some("/custom/brew".to_string()));
        assert_eq!(
            host.prefix_candidates,
            vec![
                PathBuf::from("/custom/brew"),
                PathBuf::from("/opt/homebrew"),
                PathBuf::from("/usr/local"),
            ]
        );
    }

    #[test]
    fn unset_or_blank_prefix_env_leaves_the_defaults() {
        for env_value in [None, Some(String::new()), Some("   ".to_string())] {
            let host = BrewHost::new(true, env_value);
            assert_eq!(
                host.prefix_candidates,
                vec![PathBuf::from("/opt/homebrew"), PathBuf::from("/usr/local")]
            );
        }
    }

    #[test]
    fn first_candidate_holding_brew_wins() {
        let env = TempEnvironment::builder().build();
        let first = env.dotfiles_root.join("opt/homebrew");
        let second = env.dotfiles_root.join("usr/local");
        install_brew(&env, &second);

        let host = host_with(&[&first, &second]);
        assert_eq!(detect_prefix(env.fs.as_ref(), &host), Some(second.clone()));

        // Once the earlier candidate exists too, it takes precedence.
        install_brew(&env, &first);
        assert_eq!(detect_prefix(env.fs.as_ref(), &host), Some(first));
    }

    #[test]
    fn a_non_executable_brew_is_not_a_brew() {
        let env = TempEnvironment::builder().build();
        let first = env.dotfiles_root.join("opt/homebrew");
        let second = env.dotfiles_root.join("usr/local");
        // A `bin/brew` that exists but cannot be run — a half-finished
        // install, a leftover. Detection must step over it rather than
        // hand `capture` a path it will fail to spawn.
        install_brew_with_mode(&env, &first, 0o644);
        install_brew(&env, &second);

        let host = host_with(&[&first, &second]);
        assert_eq!(detect_prefix(env.fs.as_ref(), &host), Some(second));

        // A directory at `bin/brew` is not one either.
        let only_a_dir = env.dotfiles_root.join("bare");
        env.fs.mkdir_all(&brew_binary(&only_a_dir)).unwrap();
        assert_eq!(
            detect_prefix(env.fs.as_ref(), &host_with(&[&only_a_dir])),
            None
        );
    }

    #[test]
    fn capture_asks_brew_once_per_shell_dialect() {
        let env = TempEnvironment::builder().build();
        let prefix = env.dotfiles_root.join("opt/homebrew");
        install_brew(&env, &prefix);
        let runner = FakeBrew::new();

        let blocks = capture(
            env.fs.as_ref(),
            &runner,
            BrewBootstrapMode::Auto,
            &host_with(&[&prefix]),
        )
        .expect("brew is installed under the temp prefix");

        let brew = brew_binary(&prefix).display().to_string();
        assert_eq!(
            runner.calls(),
            vec![
                format!("{brew} shellenv sh"),
                format!("{brew} shellenv zsh"),
            ]
        );
        assert_eq!(blocks.prefix, prefix);
        // Verbatim: what brew said is what we hold.
        assert_eq!(blocks.sh, SH_BLOCK);
        assert_eq!(blocks.zsh, ZSH_BLOCK);
    }

    #[test]
    fn off_emits_nothing_and_never_spawns_brew() {
        let env = TempEnvironment::builder().build();
        let prefix = env.dotfiles_root.join("opt/homebrew");
        install_brew(&env, &prefix);
        let runner = FakeBrew::new();

        assert!(capture(
            env.fs.as_ref(),
            &runner,
            BrewBootstrapMode::Off,
            &host_with(&[&prefix]),
        )
        .is_none());
        assert!(runner.calls().is_empty(), "`off` must not run brew at all");
    }

    #[test]
    fn non_macos_emits_nothing() {
        let env = TempEnvironment::builder().build();
        let prefix = env.dotfiles_root.join("opt/homebrew");
        install_brew(&env, &prefix);
        let runner = FakeBrew::new();
        let host = BrewHost {
            is_macos: false,
            prefix_candidates: vec![prefix],
        };

        assert!(capture(env.fs.as_ref(), &runner, BrewBootstrapMode::Auto, &host).is_none());
        assert!(runner.calls().is_empty());
    }

    #[test]
    fn brew_absent_emits_nothing() {
        let env = TempEnvironment::builder().build();
        let missing = env.dotfiles_root.join("opt/homebrew");
        let runner = FakeBrew::new();

        assert!(capture(
            env.fs.as_ref(),
            &runner,
            BrewBootstrapMode::Auto,
            &host_with(&[&missing]),
        )
        .is_none());
        assert!(runner.calls().is_empty());
    }

    #[test]
    fn brew_that_fails_to_answer_emits_nothing() {
        let env = TempEnvironment::builder().build();
        let prefix = env.dotfiles_root.join("opt/homebrew");
        install_brew(&env, &prefix);
        let runner = FakeBrew::failing(1);

        assert!(capture(
            env.fs.as_ref(),
            &runner,
            BrewBootstrapMode::Auto,
            &host_with(&[&prefix]),
        )
        .is_none());
    }

    // ── Datastore cache (shell-hookup-ergonomics.lex §4, #294) ──────

    /// What `up` persisted is exactly what emission reads back —
    /// `BrewBlocks` equality is byte equality on the captured text,
    /// and emission is a pure function of it, so this is the
    /// "byte-identical whether cached or live" guarantee.
    #[test]
    fn cache_round_trips_the_capture_byte_for_byte() {
        let env = TempEnvironment::builder().build();
        let path = env.paths.homebrew_cache_path();
        let blocks = BrewBlocks {
            prefix: PathBuf::from("/opt/homebrew"),
            sh: SH_BLOCK.to_string(),
            zsh: ZSH_BLOCK.to_string(),
        };

        persist_cache(env.fs.as_ref(), &path, Some(&blocks)).unwrap();
        assert_eq!(load_cache(env.fs.as_ref(), &path), Some(blocks));
    }

    /// The cache is *replaced* by rename, never truncated in place, so
    /// a concurrent reader — every shell the user opens — can never
    /// catch it half-written and fall back to spawning brew.
    ///
    /// A hard link is the proof: it shares the file's inode, so a
    /// truncating write rewrites what the link sees, while a rename
    /// swaps the directory entry and leaves the link on the old inode.
    #[test]
    fn persist_replaces_the_cache_by_rename_not_by_truncating_it() {
        let env = TempEnvironment::builder().build();
        let cache = env.paths.homebrew_cache_path();
        let first = BrewBlocks {
            prefix: PathBuf::from("/usr/local"),
            sh: "export FIRST=1;\n".to_string(),
            zsh: "export FIRST=1;\n".to_string(),
        };
        let second = BrewBlocks {
            prefix: PathBuf::from("/opt/homebrew"),
            sh: SH_BLOCK.to_string(),
            zsh: ZSH_BLOCK.to_string(),
        };
        persist_cache(env.fs.as_ref(), &cache, Some(&first)).unwrap();

        let witness = cache.parent().unwrap().join("witness.json");
        std::fs::hard_link(&cache, &witness).unwrap();

        persist_cache(env.fs.as_ref(), &cache, Some(&second)).unwrap();

        assert_eq!(load_cache(env.fs.as_ref(), &cache), Some(second));
        assert_eq!(
            load_cache(env.fs.as_ref(), &witness),
            Some(first),
            "the old inode was rewritten: the cache is being truncated \
             in place, so a reader can see a half-written file"
        );
    }

    /// A successful persist leaves nothing but the cache: the temp it
    /// wrote through is renamed away, not abandoned.
    #[test]
    fn persist_leaves_no_temp_file_behind() {
        let env = TempEnvironment::builder().build();
        let cache = env.paths.homebrew_cache_path();
        let blocks = BrewBlocks {
            prefix: PathBuf::from("/opt/homebrew"),
            sh: SH_BLOCK.to_string(),
            zsh: ZSH_BLOCK.to_string(),
        };

        persist_cache(env.fs.as_ref(), &cache, Some(&blocks)).unwrap();
        persist_cache(env.fs.as_ref(), &cache, Some(&blocks)).unwrap();

        let names: Vec<String> = env
            .fs
            .read_dir(cache.parent().unwrap())
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        assert_eq!(names, vec!["brew-shellenv.json".to_string()]);
    }

    /// The steady state: a warm cache whose prefix still matches the
    /// host serves the block with **zero** brew invocations — counted
    /// through the injectable runner, not timed.
    #[test]
    fn warm_cache_emits_without_spawning_brew() {
        let env = TempEnvironment::builder().build();
        let prefix = env.dotfiles_root.join("opt/homebrew");
        install_brew(&env, &prefix);
        let cache = env.paths.homebrew_cache_path();
        let cached = BrewBlocks {
            prefix: prefix.clone(),
            sh: SH_BLOCK.to_string(),
            zsh: ZSH_BLOCK.to_string(),
        };
        persist_cache(env.fs.as_ref(), &cache, Some(&cached)).unwrap();
        let runner = FakeBrew::new();

        let served = cached_or_capture_at(
            env.fs.as_ref(),
            &runner,
            BrewBootstrapMode::Auto,
            &host_with(&[&prefix]),
            &cache,
        );

        assert_eq!(served, Some(cached));
        assert!(
            runner.calls().is_empty(),
            "a warm cache must not spawn brew: {:?}",
            runner.calls()
        );
    }

    /// A cold cache — first `init-sh` before any `up` — captures in
    /// memory and emits correct content, but leaves the datastore
    /// untouched: `init-sh` is passive (#121).
    #[test]
    fn cache_miss_captures_live_and_does_not_persist() {
        let env = TempEnvironment::builder().build();
        let prefix = env.dotfiles_root.join("opt/homebrew");
        install_brew(&env, &prefix);
        let cache = env.paths.homebrew_cache_path();
        let runner = FakeBrew::new();

        let served = cached_or_capture_at(
            env.fs.as_ref(),
            &runner,
            BrewBootstrapMode::Auto,
            &host_with(&[&prefix]),
            &cache,
        )
        .expect("live capture must fill the miss");

        assert_eq!(served.sh, SH_BLOCK);
        assert_eq!(served.zsh, ZSH_BLOCK);
        assert_eq!(runner.calls().len(), 2, "one capture, two dialects");
        assert!(
            !env.fs.exists(&cache),
            "a passive command must never write the cache"
        );
    }

    /// A cache captured from another prefix (Intel → arm, a relocated
    /// install) is a miss, never served — and the mismatching file is
    /// left for the next `up` to rewrite, not touched here.
    #[test]
    fn cache_for_another_prefix_is_invalidated_not_served() {
        let env = TempEnvironment::builder().build();
        let old_prefix = env.dotfiles_root.join("usr/local");
        let new_prefix = env.dotfiles_root.join("opt/homebrew");
        install_brew(&env, &new_prefix); // brew moved; nothing at the old prefix
        let cache = env.paths.homebrew_cache_path();
        persist_cache(
            env.fs.as_ref(),
            &cache,
            Some(&BrewBlocks {
                prefix: old_prefix.clone(),
                sh: "export STALE=1;\n".to_string(),
                zsh: "export STALE=1;\n".to_string(),
            }),
        )
        .unwrap();
        let runner = FakeBrew::new();

        let served = cached_or_capture_at(
            env.fs.as_ref(),
            &runner,
            BrewBootstrapMode::Auto,
            &host_with(&[&old_prefix, &new_prefix]),
            &cache,
        )
        .expect("the moved brew must be captured live");

        assert_eq!(served.prefix, new_prefix);
        assert_eq!(served.sh, SH_BLOCK, "stale block must never be served");
        assert_eq!(runner.calls().len(), 2);
        // Passive path: the stale file stays until the next `up`.
        assert_eq!(
            load_cache(env.fs.as_ref(), &cache).unwrap().prefix,
            old_prefix
        );
    }

    /// A corrupt or old-format cache file is a miss (live capture),
    /// never an error.
    #[test]
    fn corrupt_cache_is_a_miss_not_an_error() {
        let env = TempEnvironment::builder().build();
        let prefix = env.dotfiles_root.join("opt/homebrew");
        install_brew(&env, &prefix);
        let cache = env.paths.homebrew_cache_path();
        env.fs.mkdir_all(cache.parent().unwrap()).unwrap();
        env.fs.write_file(&cache, b"not json {").unwrap();
        let runner = FakeBrew::new();

        let served = cached_or_capture_at(
            env.fs.as_ref(),
            &runner,
            BrewBootstrapMode::Auto,
            &host_with(&[&prefix]),
            &cache,
        );

        assert!(served.is_some());
        assert_eq!(runner.calls().len(), 2);
    }

    /// `off` and a non-macOS host ignore even a warm cache: the gate
    /// runs before the cache is consulted, exactly as it runs before a
    /// live capture.
    #[test]
    fn off_and_non_macos_ignore_a_warm_cache() {
        let env = TempEnvironment::builder().build();
        let prefix = env.dotfiles_root.join("opt/homebrew");
        install_brew(&env, &prefix);
        let cache = env.paths.homebrew_cache_path();
        persist_cache(
            env.fs.as_ref(),
            &cache,
            Some(&BrewBlocks {
                prefix: prefix.clone(),
                sh: SH_BLOCK.to_string(),
                zsh: ZSH_BLOCK.to_string(),
            }),
        )
        .unwrap();
        let runner = FakeBrew::new();

        assert!(cached_or_capture_at(
            env.fs.as_ref(),
            &runner,
            BrewBootstrapMode::Off,
            &host_with(&[&prefix]),
            &cache,
        )
        .is_none());
        let non_mac = BrewHost {
            is_macos: false,
            prefix_candidates: vec![prefix],
        };
        assert!(cached_or_capture_at(
            env.fs.as_ref(),
            &runner,
            BrewBootstrapMode::Auto,
            &non_mac,
            &cache,
        )
        .is_none());
        assert!(runner.calls().is_empty());
    }

    /// A capture with nothing to emit clears the cache, keeping it in
    /// lockstep with the script `up` just wrote: a host that lost its
    /// brew (or turned the bootstrap off) can never serve a stale
    /// block.
    #[test]
    fn persisting_an_empty_capture_clears_the_cache() {
        let env = TempEnvironment::builder().build();
        let cache = env.paths.homebrew_cache_path();
        persist_cache(
            env.fs.as_ref(),
            &cache,
            Some(&BrewBlocks {
                prefix: PathBuf::from("/opt/homebrew"),
                sh: SH_BLOCK.to_string(),
                zsh: ZSH_BLOCK.to_string(),
            }),
        )
        .unwrap();
        assert!(env.fs.exists(&cache));

        persist_cache(env.fs.as_ref(), &cache, None).unwrap();
        assert!(!env.fs.exists(&cache));
        // And clearing an already-absent cache is a no-op, not an error.
        persist_cache(env.fs.as_ref(), &cache, None).unwrap();
    }

    #[test]
    fn zsh_only_lines_stay_under_the_zsh_guard() {
        let mut script = String::new();
        emit_homebrew_block(
            &mut script,
            &BrewBlocks {
                prefix: PathBuf::from("/opt/homebrew"),
                sh: SH_BLOCK.to_string(),
                zsh: ZSH_BLOCK.to_string(),
            },
        );

        let guard = script.find("if [ -n \"${ZSH_VERSION:-}\" ]; then").unwrap();
        let else_at = script.find("\nelse\n").unwrap();
        let fi_at = script.find("\nfi\n").unwrap();
        let zsh_branch = &script[guard..else_at];
        let sh_branch = &script[else_at..fi_at];

        assert!(zsh_branch.contains("fpath[1,0]="), "script:\n{script}");
        assert!(zsh_branch.contains("export FPATH;"), "script:\n{script}");
        assert!(!sh_branch.contains("fpath["), "script:\n{script}");
        assert!(!sh_branch.contains("FPATH"), "script:\n{script}");
        // The `path_helper` re-exec is brew's line to keep — see the
        // module docs. Both branches carry it untouched.
        assert!(zsh_branch.contains("PATH_HELPER_ROOT"));
        assert!(sh_branch.contains("PATH_HELPER_ROOT"));
    }

    #[test]
    fn captured_output_is_emitted_byte_for_byte() {
        let mut script = String::new();
        let sh = "export ONE=1;\nexport TWO=2;";
        emit_homebrew_block(
            &mut script,
            &BrewBlocks {
                prefix: PathBuf::from("/opt/homebrew"),
                sh: sh.to_string(),
                zsh: "export ZED=1;\n".to_string(),
            },
        );

        // Unindented and unedited, with only the missing trailing
        // newline supplied so `fi` starts its own line.
        assert!(
            script.contains("export ONE=1;\nexport TWO=2;\nfi\n"),
            "script:\n{script}"
        );
        assert!(
            script.contains("export ZED=1;\nelse\n"),
            "script:\n{script}"
        );
    }
}

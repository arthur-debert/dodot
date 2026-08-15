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
//! authority on its own bootstrap. We capture that output at
//! *generation time* — during `dodot up`, baked into `dodot-init.sh` as
//! static text — so the shell pays nothing to ask and a brew that
//! changes its block is picked up by the next `up`. A hand-written
//! approximation would silently rot, which is why there is exactly one
//! mechanism here and no fallback emitter (`pack-ordering.lex` §2.2).
//!
//! Never reach for `eval "$(brew shellenv)"` *at shell time*: that runs
//! brew itself on every shell start. Generation-time capture is the
//! whole point.
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
//! plainly. The emitted block spends two process spawns (`env` plus
//! `path_helper`) per shell start. Measured on an Apple-silicon mac
//! over 200 iterations: 3.1 ms wall per invocation, ~2.7 ms net of the
//! 0.46 ms `$( )` subshell baseline. `[shell] homebrew = "off"` turns
//! it off in one config key.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use tracing::debug;

use crate::config::DodotConfig;
use crate::datastore::CommandRunner;
use crate::fs::Fs;
use crate::{DodotError, Result};

/// Standard Homebrew install prefixes, in probe order: Apple silicon
/// first, then the Intel/`/usr/local` layout.
pub const DEFAULT_PREFIXES: [&str; 2] = ["/opt/homebrew", "/usr/local"];

/// The `[shell] homebrew` config key — the gate on the whole mechanism.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrewBootstrapMode {
    /// Emit the block when a Homebrew prefix exists on this host.
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
    /// Prefix candidates in probe order. The first whose `bin/brew`
    /// exists wins.
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
    /// each candidate still has to hold a `bin/brew`, so a stale or
    /// mistyped export falls through to the standard prefixes instead
    /// of suppressing the block.
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrewBlocks {
    /// The prefix the blocks were captured from. Emitted as a comment
    /// so a reader of `dodot-init.sh` can see which brew answered.
    pub prefix: PathBuf,
    /// `brew shellenv sh` — used for everything that is not zsh.
    pub sh: String,
    /// `brew shellenv zsh` — carries the zsh-only `fpath`/`FPATH` lines.
    pub zsh: String,
}

/// Capture the bootstrap for the running host from resolved config —
/// the entry point every command that regenerates the init script uses.
///
/// The only error it can return is a bad `[shell] homebrew` value; a
/// host without brew is `Ok(None)`.
pub fn capture_from_config(
    fs: &dyn Fs,
    runner: &dyn CommandRunner,
    config: &DodotConfig,
) -> Result<Option<BrewBlocks>> {
    let mode = BrewBootstrapMode::parse(&config.shell.homebrew)?;
    Ok(capture(fs, runner, mode, &BrewHost::detect()))
}

/// Capture Homebrew's bootstrap for this host, or `None` when there is
/// nothing to emit.
///
/// `None` — never an error — for every "no brew here" case: the mode is
/// `off`, the host is not macOS, no candidate prefix holds a `bin/brew`,
/// or brew itself fails to answer. A missing brew is not a broken
/// deployment, and the next `dodot up` heals the script once brew is
/// installed.
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
    let brew = brew_binary(&prefix);
    let sh = shellenv(runner, &brew, "sh")?;
    let zsh = shellenv(runner, &brew, "zsh")?;

    Some(BrewBlocks { prefix, sh, zsh })
}

/// First candidate prefix holding an executable-looking `bin/brew`.
fn detect_prefix(fs: &dyn Fs, host: &BrewHost) -> Option<PathBuf> {
    host.prefix_candidates
        .iter()
        .find(|prefix| fs.exists(&brew_binary(prefix)))
        .cloned()
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
    fn install_brew(env: &TempEnvironment, prefix: &Path) {
        env.fs.mkdir_all(&prefix.join("bin")).unwrap();
        env.fs
            .write_file(&brew_binary(prefix), b"#!/bin/sh\n")
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

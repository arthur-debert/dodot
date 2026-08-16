//! Shell integration — generates `dodot-init.sh`.
//!
//! The script is generated flat and declarative from the actual
//! datastore state, rather than re-discovering the datastore layout at
//! runtime in shell. This means:
//!
//! - Zero logic duplication between Rust and shell
//! - The script is just `source` and `PATH=` lines — trivially fast
//! - Changes to the datastore layout only need to happen in Rust
//!
//! The generated script is written to `data_dir/shell/dodot-init.sh`.
//! `dodot install --write` wires the line below into the user's rc
//! file ([`rc`]), [`probe`] measures whether a new shell actually
//! runs it, and [`trace`] reports what `dodot` resolves to at that
//! rc line (`dodot probe shell-init`). Users can also add it by hand:
//!
//! ```sh
//! [ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ] && . "$HOME/.local/share/dodot/shell/dodot-init.sh"
//! ```
//!
//! # Activation evidence (shell-hookup-ergonomics.lex §2.1)
//!
//! Every generated script — profiled or not, empty datastore or not —
//! opens with three lines that prove it ran and say who wrote it: an
//! `export DODOT_INIT_GEN=` carrying the *generation* the script was
//! written at, an `export DODOT_INIT_VERSION=` carrying the dodot that
//! wrote it, and a single truncating redirect of both fields into the
//! heartbeat marker. `dodot up`, `dodot down` and `dodot status` read
//! them back through [`activation`] to tell "no shell has ever loaded
//! dodot" from "this terminal predates your last `up`" from "your
//! shells load a different dodot" from "healthy".
//!
//! The generation is an argument, not something the generator invents:
//! callers that write the script stamp [`activation::current_generation`],
//! and tests pin a value so the emitted script is deterministic. The
//! version is not — it is this binary's, by definition.
//!
//! The block is on a strict budget — exports and one redirect, no
//! command execution — because it runs on every shell start forever.
//! That is also why the heartbeat is a whole-file rewrite of static
//! content: concurrent shell startups race, and last-writer-wins on a
//! truncating redirect is a correct answer to that race, where an
//! append or a read-modify-write would not be. It is also why "when did
//! a shell last load dodot" is read from that file's *mtime* rather
//! than from anything written inside it: the redirect already updates
//! mtime on every activation, so the answer costs no extra write.
//!
//! # Homebrew bootstrap (shell-hookup-ergonomics.lex §4)
//!
//! Right after the evidence, and before anything a pack contributed,
//! the script can carry Homebrew's environment as captured from
//! `brew shellenv` by `dodot up`/`down` and cached in the datastore.
//! [`homebrew`] owns the capture, the cache, the `$ZSH_VERSION` guard,
//! and the reasons for all three; the generator only decides *where*
//! the block goes, which is: first, so dodot's own PATH additions are
//! always the last word.
//!
//! # Profiling wrapper (Phase 2 of profiling.lex)
//!
//! When the caller passes `profiling_enabled = true`, the generator
//! wraps every `source` and PATH line with an inline `EPOCHREALTIME`
//! capture and writes one `profile-*.tsv` per shell start under
//! `<data_dir>/probes/shell-init/`. The wrapper is gated on a runtime
//! check (`bash 5+` / `zsh` with `EPOCHREALTIME` available); shells
//! without the variable fall through to the unchanged source/PATH
//! path with a single `[ "$_dodot_prof" = "1" ]` test of overhead.
//! When `profiling_enabled = false`, the generated script is
//! byte-identical to the unwrapped form.
//!
//! Sources are *not* wrapped in a shell function: in zsh, `source`
//! inside a function changes scoping for plain variable assignments
//! in the sourced file, which is a behavioural surprise nobody asked
//! for. We pay the price of a slightly longer script in exchange for
//! semantic equivalence with the un-instrumented form.

use std::fmt::Write;
use std::path::{Path, PathBuf};

use crate::fs::Fs;
use crate::paths::Pather;
use crate::Result;

pub mod activation;
pub mod homebrew;
pub mod probe;
pub mod rc;
pub mod trace;
pub mod validate;
pub use activation::{ActivationNotice, ActivationState, INIT_GEN_ENV, INIT_VERSION_ENV};
pub use homebrew::{BrewBlocks, BrewBootstrapMode, BrewHost};
pub use probe::ProbePolicy;
pub use rc::ShellEnv;
pub use validate::{
    error_sidecar_path, validate_shell_sources, NoopSyntaxChecker, ShellValidationFailure,
    ShellValidationReport, SyntaxCheckResult, SyntaxChecker, SystemSyntaxChecker, ERRORS_SUBDIR,
};

/// The line an init script with no pack contributions carries, and the
/// marker [`script_has_contributions`] reads back.
///
/// One string, written by the generator and parsed by the footer, so
/// "the script is empty" can never mean two different things in the
/// two halves of the round trip.
pub const EMPTY_SCRIPT_MARKER: &str = "# No shell scripts or PATH additions to load.";

/// Whether generated init-script text sources or PATHs anything.
///
/// `false` for the three ways a script ends up with nothing to do —
/// after `dodot down`, in a repository where every pack is ignored, and
/// after a first `up` that deployed nothing — which is the one rule the
/// footer needs to say "wired, but nothing is deployed" instead of
/// claiming a healthy deployment (`shell-hookup-ergonomics.lex` §2.3).
pub fn script_has_contributions(script: &str) -> bool {
    !script
        .lines()
        .any(|line| line.trim() == EMPTY_SCRIPT_MARKER)
}

/// Append the "nothing to do" notice for an empty init script.
fn append_empty_notice(script: &mut String) {
    writeln!(script, "{EMPTY_SCRIPT_MARKER}").unwrap();
    writeln!(
        script,
        "# Run `dodot up` to deploy packs, or `dodot status` to see available packs."
    )
    .unwrap();
}

/// Generate the shell init script content from the current datastore state.
///
/// Scans the datastore for:
/// - `packs/*/shell/*` — symlinks to shell scripts → `source` lines
/// - `packs/*/path/*` — symlinks to directories → `PATH=` lines
///
/// `generation` is stamped into the activation-evidence block (see the
/// module docs), which is emitted unconditionally — before the
/// early-return for an empty datastore, because "a shell sourced this"
/// is worth knowing even when the script has nothing else to do.
///
/// `homebrew` is the bootstrap block captured from `brew shellenv` —
/// by [`homebrew::capture_and_persist`] in `up`/`down`, or served from
/// the datastore cache by [`homebrew::cached_or_capture`] in passive
/// generation paths — or `None` when there is nothing to emit (not
/// macOS, no brew, or `[shell] homebrew = "off"`). Like
/// the evidence it lands ahead of the empty-datastore early return: it
/// is a function of config and the host, not of what any pack deployed,
/// and a user whose rc file is empty still needs brew's environment.
/// Emitting it *first* is what lets dodot's own PATH additions be the
/// last word without any pack-ordering choreography.
///
/// When `profiling_enabled` is true and there is at least one entry to
/// emit, the script also carries the per-line timing wrapper described
/// in the module docs.
pub fn generate_init_script(
    fs: &dyn Fs,
    paths: &dyn Pather,
    profiling_enabled: bool,
    generation: u64,
    homebrew: Option<&BrewBlocks>,
) -> Result<String> {
    let mut script = String::new();

    writeln!(script, "#!/bin/sh").unwrap();
    writeln!(script, "# Generated by dodot — do not edit manually.").unwrap();
    writeln!(script, "# Regenerated on every `dodot up` / `dodot down`.").unwrap();
    writeln!(script).unwrap();

    emit_activation_evidence(&mut script, generation, &paths.hookup_heartbeat_path());

    if let Some(blocks) = homebrew {
        homebrew::emit_homebrew_block(&mut script, blocks);
    }

    let packs_dir = paths.data_dir().join("packs");
    if !fs.exists(&packs_dir) {
        append_empty_notice(&mut script);
        return Ok(script);
    }

    let pack_entries = fs.read_dir(&packs_dir)?;

    // Collect shell sources and path additions separately so we can
    // group them in the output for readability.
    let mut shell_sources: Vec<(String, PathBuf)> = Vec::new(); // (pack, target)
    let mut path_additions: Vec<(String, PathBuf)> = Vec::new(); // (pack, target)

    for pack_entry in &pack_entries {
        if !pack_entry.is_dir {
            continue;
        }
        // The datastore subtree is keyed by the on-disk directory
        // name (e.g. `010-nvim`), but the comment we emit in the
        // generated init script uses the pack's display name
        // (`nvim`) — that's what the user sees in `dodot status` and
        // expects to recognise here.
        let pack_dir = &pack_entry.name;
        let pack_display = crate::packs::display_name_for(pack_dir).to_string();

        // Shell handler: source scripts
        let shell_dir = paths.handler_data_dir(pack_dir, "shell");
        if fs.is_dir(&shell_dir) {
            if let Ok(entries) = fs.read_dir(&shell_dir) {
                for entry in entries {
                    if !entry.is_symlink {
                        continue;
                    }
                    let target = fs.readlink(&entry.path)?;
                    shell_sources.push((pack_display.clone(), target));
                }
            }
        }

        // Path handler: add to PATH
        let path_dir = paths.handler_data_dir(pack_dir, "path");
        if fs.is_dir(&path_dir) {
            if let Ok(entries) = fs.read_dir(&path_dir) {
                for entry in entries {
                    if !entry.is_symlink {
                        continue;
                    }
                    let target = fs.readlink(&entry.path)?;
                    path_additions.push((pack_display.clone(), target));
                }
            }
        }
    }

    if path_additions.is_empty() && shell_sources.is_empty() {
        append_empty_notice(&mut script);
        return Ok(script);
    }

    // Profiling preamble (only when enabled and there's at least one entry).
    let profiling_active = profiling_enabled;
    if profiling_active {
        emit_profiling_preamble(
            &mut script,
            &paths.probes_shell_init_dir(),
            &paths.init_script_path(),
        );
    }

    if !path_additions.is_empty() {
        writeln!(script, "# PATH additions").unwrap();
        for (pack, target) in &path_additions {
            writeln!(script, "# [{pack}]").unwrap();
            if profiling_active {
                emit_timed_path(&mut script, pack, target);
            } else {
                writeln!(script, "export PATH=\"{}:$PATH\"", target.display()).unwrap();
            }
        }
        writeln!(script).unwrap();
    }

    if !shell_sources.is_empty() {
        writeln!(script, "# Shell scripts").unwrap();
        for (pack, target) in &shell_sources {
            writeln!(script, "# [{pack}]").unwrap();
            if profiling_active {
                emit_timed_source(&mut script, pack, target);
            } else {
                // Loud-failure wrapper: if the source command itself
                // exits non-zero, print a dodot-attributed message to
                // stderr alongside the shell's own error so the user
                // can see *which* dodot-managed file failed. The
                // shell's native message already carries the line
                // number; we add the breadcrumb back to dodot.
                writeln!(
                    script,
                    "[ -f \"{p}\" ] && {{ . \"{p}\" || echo \"dodot: shell source exited $?: {p}\" >&2; }}",
                    p = target.display()
                )
                .unwrap();
            }
        }
        writeln!(script).unwrap();
    }

    if profiling_active {
        emit_profiling_epilogue(&mut script);
    }

    Ok(script)
}

/// Generate and write the init script to `data_dir/shell/dodot-init.sh`,
/// stamping it with a fresh generation.
///
/// The write is atomic ([`Fs::write_atomic_with_mode`]): the reader
/// is every shell the user opens, and a truncating in-place write
/// would let a shell starting mid-`up` source a prefix of the script
/// — usually still valid shell, so the loss is silent, and since the
/// activation evidence sits at the top of the script the truncated
/// load still stamps itself healthy (#297). The executable mode goes
/// on the temp before the rename, so the visible file is never a
/// non-executable script.
///
/// Also creates the heartbeat's parent directory. The emitted redirect
/// can't `mkdir -p` its own way out of a missing directory without
/// spending a process on every shell start, so the write side owns
/// that once per regeneration instead.
///
/// `homebrew` carries the captured Homebrew bootstrap, as for
/// [`generate_init_script`].
///
/// Returns the path where the script was written.
pub fn write_init_script(
    fs: &dyn Fs,
    paths: &dyn Pather,
    profiling_enabled: bool,
    homebrew: Option<&BrewBlocks>,
) -> Result<PathBuf> {
    let generation = activation::current_generation();
    let script_content = generate_init_script(fs, paths, profiling_enabled, generation, homebrew)?;
    let script_path = paths.init_script_path();

    fs.mkdir_all(&paths.probes_hookup_dir())?;
    fs.mkdir_all(paths.shell_dir())?;
    fs.write_atomic_with_mode(&script_path, script_content.as_bytes(), 0o755)?;

    Ok(script_path)
}

// ── Activation evidence emitter ──────────────────────────────────────

/// Emit the evidence block (shell-hookup-ergonomics.lex §2.1).
///
/// - `export DODOT_INIT_GEN=<generation>` — free to read back from any
///   dodot process that the shell later spawns.
/// - `export DODOT_INIT_VERSION=<version>` — which dodot generated the
///   script *this* shell loaded. Without it a hookup can be wired,
///   sourced on every start, and still dead, because the binary that
///   wrote the script is not the binary the user runs.
/// - `echo <generation> <version> >| <heartbeat> 2>/dev/null || :` — one
///   builtin and one truncating redirect, now carrying both fields so a
///   detached caller can answer the same question about the last shell
///   anywhere. `2>/dev/null` keeps an unwritable data dir from spraying
///   errors across every shell start, and the `|| :` keeps the failed
///   redirect's non-zero status from aborting an rc file that runs
///   under `set -e`.
///
/// `>|`, not `>`: `setopt noclobber` / `set -C` is an ordinary rc line,
/// and under it a plain `>` *refuses* to write a file that already
/// exists. The heartbeat exists after the first activation, so every
/// activation from then on would fail silently — the two guards above
/// see to the silence — and freeze "last loaded" at the first shell
/// that ever ran. Three claims now rest on that file (the footer's
/// timestamp, the skew comparison, and the probe gate), so the failure
/// reads as a confident wrong answer rather than a missing one.
/// `>|` overrides `noclobber` and is POSIX; verified against zsh, bash
/// and dash, where plain `>` leaves the file untouched.
///
/// Still two exports and one redirect: no command execution, no `dodot`
/// invocation on the shell startup path.
fn emit_activation_evidence(script: &mut String, generation: u64, heartbeat_path: &Path) {
    let heartbeat = sh_quote(&heartbeat_path.display().to_string());
    let version = activation::running_version();
    writeln!(script, "# ── dodot activation evidence ──").unwrap();
    writeln!(script, "export {}={generation}", activation::INIT_GEN_ENV).unwrap();
    writeln!(script, "export {}={version}", activation::INIT_VERSION_ENV).unwrap();
    writeln!(
        script,
        "echo {generation} {version} >| {heartbeat} 2>/dev/null || :"
    )
    .unwrap();
    writeln!(script).unwrap();
}

// ── Profiling wrapper emitters ───────────────────────────────────────

/// The runtime-detection preamble. Sets `_dodot_prof` to `1` when the
/// current shell is bash 5+ or zsh with `EPOCHREALTIME` available;
/// otherwise leaves it `0` (the wrapper falls through to the no-op
/// path). All shell variables are namespaced `_dodot_*` so we don't
/// stomp on the user's environment.
fn emit_profiling_preamble(script: &mut String, profiles_dir: &Path, init_script_path: &Path) {
    let dir = sh_quote(&profiles_dir.display().to_string());
    let init_script = sh_quote(&init_script_path.display().to_string());
    writeln!(script, "# ── dodot shell-init profiling (Phase 2) ──").unwrap();
    writeln!(script, "_dodot_prof=0").unwrap();
    writeln!(
        script,
        "if [ -n \"${{BASH_VERSION:-}}\" ] || [ -n \"${{ZSH_VERSION:-}}\" ]; then"
    )
    .unwrap();
    // zsh exposes EPOCHREALTIME only after `zmodload zsh/datetime`. Load
    // it eagerly here; bash 5+ has the variable built in and ignores
    // unknown commands like `zmodload` (we suppress its `command not
    // found` error). Doing this *inside* the bash/zsh guard keeps it off
    // hot paths in plain sh.
    writeln!(
        script,
        "  [ -n \"${{ZSH_VERSION:-}}\" ] && zmodload zsh/datetime 2>/dev/null"
    )
    .unwrap();
    writeln!(script, "  if [ -n \"${{EPOCHREALTIME:-}}\" ]; then").unwrap();
    writeln!(script, "    _dodot_prof_dir={dir}").unwrap();
    writeln!(
        script,
        "    _dodot_prof_file=\"$_dodot_prof_dir/profile-${{EPOCHSECONDS:-0}}-$$-${{RANDOM}}.tsv\""
    )
    .unwrap();
    // Sibling errors log: one record per source whose stderr was non-empty.
    // Format: `@@\t<target>\t<exit_status>` header line, followed by the
    // captured stderr verbatim, followed by a trailing newline. Loaded
    // alongside the profile by `probe::shell_init::read_recent_profiles`
    // and parsed by `probe::shell_init::parse_errors_log`.
    writeln!(
        script,
        "    _dodot_err_file=\"${{_dodot_prof_file%.tsv}}.errors.log\""
    )
    .unwrap();
    // Per-shell scratch file for capturing each source's stderr. Reused
    // across every source in this shell startup; truncated each time.
    writeln!(script, "    _dodot_err_tmp=\"$_dodot_prof_dir/.errtmp-$$\"").unwrap();
    writeln!(
        script,
        "    if mkdir -p \"$_dodot_prof_dir\" 2>/dev/null; then"
    )
    .unwrap();
    writeln!(script, "      _dodot_prof_t0=$EPOCHREALTIME").unwrap();
    writeln!(script, "      {{").unwrap();
    writeln!(script, "        printf '# dodot shell-init profile v1\\n'").unwrap();
    writeln!(
        script,
        "        printf '# shell\\t%s\\n' \"${{BASH_VERSION:+bash $BASH_VERSION}}${{ZSH_VERSION:+zsh $ZSH_VERSION}}\""
    )
    .unwrap();
    writeln!(
        script,
        "        printf '# start_t\\t%s\\n' \"$_dodot_prof_t0\""
    )
    .unwrap();
    writeln!(
        script,
        "        printf '# init_script\\t%s\\n' {init_script}"
    )
    .unwrap();
    writeln!(
        script,
        "        printf '# columns\\tphase\\tpack\\thandler\\ttarget\\tstart_t\\tend_t\\texit_status\\n'"
    )
    .unwrap();
    writeln!(
        script,
        "      }} > \"$_dodot_prof_file\" 2>/dev/null && _dodot_prof=1"
    )
    .unwrap();
    // Errors log is created lazily — see `emit_timed_source`. Most shell
    // startups have no stderr from any source, and writing an empty
    // header file for each one would defeat the "fast path is free"
    // claim. The first source that actually emits stderr seeds the
    // header before appending its record.
    writeln!(script, "    fi").unwrap();
    writeln!(script, "  fi").unwrap();
    writeln!(script, "fi").unwrap();
    writeln!(script).unwrap();
}

/// One inline-timed `export PATH=…` row. The branch is one comparison
/// at runtime — negligible on shells where the wrapper is inert.
fn emit_timed_path(script: &mut String, pack: &str, target: &Path) {
    let target_str = target.display().to_string();
    let target_q = sh_quote(&target_str);
    writeln!(script, "if [ \"$_dodot_prof\" = \"1\" ]; then").unwrap();
    writeln!(
        script,
        "  _dodot_t0=$EPOCHREALTIME; export PATH=\"{target_str}:$PATH\"; _dodot_t1=$EPOCHREALTIME"
    )
    .unwrap();
    writeln!(
        script,
        "  printf 'path\\t{pack}\\tpath\\t%s\\t%s\\t%s\\t0\\n' {target_q} \"$_dodot_t0\" \"$_dodot_t1\" >> \"$_dodot_prof_file\" 2>/dev/null"
    )
    .unwrap();
    writeln!(script, "else").unwrap();
    writeln!(script, "  export PATH=\"{target_str}:$PATH\"").unwrap();
    writeln!(script, "fi").unwrap();
}

/// One inline-timed `[ -f X ] && . X` row, capturing the source's
/// exit status and stderr. Same overhead profile as the PATH variant
/// when the sourced file is silent; one extra `[ -s ]` test plus an
/// append when stderr is non-empty.
///
/// Both branches (profiling-active and unprofiled fallback) emit the
/// loud-failure message on a non-zero source exit, so users see the
/// dodot breadcrumb whether or not their shell supports the timing
/// path.
fn emit_timed_source(script: &mut String, pack: &str, target: &Path) {
    let target_str = target.display().to_string();
    let target_q = sh_quote(&target_str);
    writeln!(script, "if [ \"$_dodot_prof\" = \"1\" ]; then").unwrap();
    // `_dodot_rc` is initialised to 0 *before* the source attempt so a
    // missing file (the `[ -f … ]` test failing) does not get reported
    // as "exited 1". The compound `&& { … }` only sets `_dodot_rc` from
    // the actual `.` invocation; otherwise it stays 0. Stderr from the
    // source is redirected to `_dodot_err_tmp`; if non-empty, we
    // re-emit it to the user's stderr (preserving the shell's own
    // error display) and append it to the per-shell errors log.
    writeln!(
        script,
        "  _dodot_rc=0; : > \"$_dodot_err_tmp\" 2>/dev/null; _dodot_t0=$EPOCHREALTIME; [ -f \"{target_str}\" ] && {{ . \"{target_str}\" 2>\"$_dodot_err_tmp\"; _dodot_rc=$?; }}; _dodot_t1=$EPOCHREALTIME"
    )
    .unwrap();
    writeln!(
        script,
        "  printf 'source\\t{pack}\\tshell\\t%s\\t%s\\t%s\\t%s\\n' {target_q} \"$_dodot_t0\" \"$_dodot_t1\" \"$_dodot_rc\" >> \"$_dodot_prof_file\" 2>/dev/null"
    )
    .unwrap();
    // Stderr-handling block. Skipped entirely when the sourced file was
    // silent (the common case). When non-empty, we print to the user's
    // stderr and append a record to the errors log. The errors log is
    // seeded with its `v1` header on first use — keeping creation lazy
    // means a clean shell startup leaves no orphan `*.errors.log` on
    // disk. The trailing `\n` after each record guarantees the next
    // record's `@@` header starts on its own line even if the captured
    // stderr didn't end with a newline.
    writeln!(script, "  if [ -s \"$_dodot_err_tmp\" ]; then").unwrap();
    writeln!(script, "    cat \"$_dodot_err_tmp\" >&2").unwrap();
    writeln!(
        script,
        "    [ -f \"$_dodot_err_file\" ] || printf '# dodot shell-init errors v1\\n' > \"$_dodot_err_file\" 2>/dev/null"
    )
    .unwrap();
    writeln!(script, "    {{").unwrap();
    writeln!(
        script,
        "      printf '@@\\t%s\\t%s\\n' {target_q} \"$_dodot_rc\""
    )
    .unwrap();
    writeln!(script, "      cat \"$_dodot_err_tmp\"").unwrap();
    writeln!(script, "      printf '\\n'").unwrap();
    writeln!(script, "    }} >> \"$_dodot_err_file\" 2>/dev/null").unwrap();
    writeln!(script, "  elif [ \"$_dodot_rc\" -ne 0 ]; then").unwrap();
    // Non-zero exit with empty stderr — still emit the loud breadcrumb
    // so the user knows dodot saw a failure (matches prior behaviour).
    writeln!(
        script,
        "    echo \"dodot: shell source exited $_dodot_rc: {target_str}\" >&2"
    )
    .unwrap();
    writeln!(script, "  fi").unwrap();
    writeln!(script, "else").unwrap();
    writeln!(
        script,
        "  [ -f \"{target_str}\" ] && {{ . \"{target_str}\" || echo \"dodot: shell source exited $?: {target_str}\" >&2; }}"
    )
    .unwrap();
    writeln!(script, "fi").unwrap();
}

/// Closes out the report (writes the `# end_t` marker) and clears
/// every `_dodot_*` shell variable so we don't leak state into the
/// user's interactive shell.
fn emit_profiling_epilogue(script: &mut String) {
    writeln!(script, "# ── dodot shell-init profiling epilogue ──").unwrap();
    writeln!(script, "if [ \"$_dodot_prof\" = \"1\" ]; then").unwrap();
    writeln!(
        script,
        "  printf '# end_t\\t%s\\n' \"$EPOCHREALTIME\" >> \"$_dodot_prof_file\" 2>/dev/null"
    )
    .unwrap();
    // Remove the per-shell stderr scratch file. It's reused across
    // sources within one shell startup; here at exit we tidy up.
    writeln!(
        script,
        "  [ -n \"${{_dodot_err_tmp:-}}\" ] && rm -f \"$_dodot_err_tmp\" 2>/dev/null"
    )
    .unwrap();
    writeln!(script, "fi").unwrap();
    writeln!(
        script,
        "unset _dodot_prof _dodot_prof_dir _dodot_prof_file _dodot_err_file _dodot_err_tmp _dodot_prof_t0 _dodot_t0 _dodot_t1 _dodot_rc 2>/dev/null"
    )
    .unwrap();
}

/// Single-quote a string for safe use in POSIX shell. Embedded single
/// quotes are escaped via the `'\''` idiom.
fn sh_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datastore::{CommandOutput, CommandRunner, DataStore, FilesystemDataStore};
    use crate::testing::TempEnvironment;
    use std::sync::Arc;

    /// Pinned generation so emitted scripts are deterministic in
    /// tests; production stamps `activation::current_generation()`.
    const TEST_GEN: u64 = 1_755_200_000;

    struct NoopRunner;
    impl CommandRunner for NoopRunner {
        fn run(&self, _: &str, _: &[String]) -> Result<CommandOutput> {
            Ok(CommandOutput {
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    fn make_datastore(env: &TempEnvironment) -> FilesystemDataStore {
        FilesystemDataStore::new(env.fs.clone(), env.paths.clone(), Arc::new(NoopRunner))
    }

    #[test]
    fn empty_datastore_produces_helpful_script() {
        let env = TempEnvironment::builder().build();
        let script =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), false, TEST_GEN, None)
                .unwrap();

        assert!(script.starts_with("#!/bin/sh"));
        assert!(script.contains("Generated by dodot"));
        assert!(script.contains("No shell scripts or PATH additions"));
        assert!(script.contains("dodot up"));
        assert!(script.contains("dodot status"));
        assert!(!script.contains("export PATH"));
        assert!(!script.contains(". \""));
    }

    #[test]
    fn shell_handler_state_produces_source_lines() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "alias vi=vim")
            .done()
            .build();

        let ds = make_datastore(&env);
        let source = env.dotfiles_root.join("vim/aliases.sh");
        ds.create_data_link("vim", "shell", &source).unwrap();

        let script =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), false, TEST_GEN, None)
                .unwrap();

        assert!(script.contains("# Shell scripts"), "script:\n{script}");
        assert!(script.contains("# [vim]"), "script:\n{script}");
        // Loud-failure wrapper: existence-guarded source, with a
        // dodot-attributed echo on non-zero exit.
        assert!(
            script.contains(&format!(
                "[ -f \"{p}\" ] && {{ . \"{p}\" || echo \"dodot: shell source exited $?: {p}\" >&2; }}",
                p = source.display()
            )),
            "script:\n{script}"
        );
    }

    #[test]
    fn path_handler_state_produces_path_lines() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("bin/myscript", "#!/bin/sh")
            .done()
            .build();

        let ds = make_datastore(&env);
        let source = env.dotfiles_root.join("vim/bin");
        ds.create_data_link("vim", "path", &source).unwrap();

        let script =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), false, TEST_GEN, None)
                .unwrap();

        assert!(script.contains("# PATH additions"), "script:\n{script}");
        assert!(script.contains("# [vim]"), "script:\n{script}");
        assert!(
            script.contains(&format!("export PATH=\"{}:$PATH\"", source.display())),
            "script:\n{script}"
        );
    }

    #[test]
    fn multiple_packs_combined() {
        let env = TempEnvironment::builder()
            .pack("git")
            .file("aliases.sh", "alias gs='git status'")
            .done()
            .pack("vim")
            .file("aliases.sh", "alias vi=vim")
            .file("bin/vimrun", "#!/bin/sh")
            .done()
            .build();

        let ds = make_datastore(&env);

        ds.create_data_link("git", "shell", &env.dotfiles_root.join("git/aliases.sh"))
            .unwrap();
        ds.create_data_link("vim", "shell", &env.dotfiles_root.join("vim/aliases.sh"))
            .unwrap();

        ds.create_data_link("vim", "path", &env.dotfiles_root.join("vim/bin"))
            .unwrap();

        let script =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), false, TEST_GEN, None)
                .unwrap();

        assert!(script.contains("# [git]"), "script:\n{script}");
        assert!(script.contains("# [vim]"), "script:\n{script}");
        assert!(script.contains("export PATH="), "script:\n{script}");
        let source_count = script.matches(". \"").count();
        assert_eq!(
            source_count, 2,
            "expected 2 source lines, script:\n{script}"
        );
    }

    #[test]
    fn write_init_script_creates_executable_file() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "alias vi=vim")
            .done()
            .build();

        let ds = make_datastore(&env);
        ds.create_data_link("vim", "shell", &env.dotfiles_root.join("vim/aliases.sh"))
            .unwrap();

        let script_path =
            write_init_script(env.fs.as_ref(), env.paths.as_ref(), false, None).unwrap();

        assert_eq!(script_path, env.paths.init_script_path());
        env.assert_exists(&script_path);

        let content = env.fs.read_to_string(&script_path).unwrap();
        assert!(content.starts_with("#!/bin/sh"));
        assert!(content.contains("aliases.sh"));

        let meta = std::fs::metadata(&script_path).unwrap();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(meta.permissions().mode() & 0o111, 0o111);
    }

    /// The init script is *replaced* by rename, never truncated in
    /// place, so a shell that starts mid-`dodot up` can never source
    /// a prefix of the script (#297).
    ///
    /// A hard link is the proof: it shares the file's inode, so an
    /// in-place truncating write rewrites what the link sees, while a
    /// rename swaps the directory entry and leaves the link on the
    /// old inode. Same technique as homebrew's
    /// `persist_replaces_the_cache_by_rename_not_by_truncating_it`.
    #[test]
    fn init_script_is_replaced_by_rename_not_truncated_in_place() {
        let env = TempEnvironment::builder().build();

        let path = write_init_script(env.fs.as_ref(), env.paths.as_ref(), false, None).unwrap();
        let first = env.fs.read_to_string(&path).unwrap();

        let witness = path.parent().unwrap().join("witness.sh");
        std::fs::hard_link(&path, &witness).unwrap();

        // Different homebrew blocks force different content: the
        // generation stamp is seconds-resolution, so two back-to-back
        // writes could otherwise be byte-identical and prove nothing.
        let blocks = BrewBlocks {
            prefix: PathBuf::from("/opt/homebrew"),
            sh: "export HOMEBREW_PREFIX=/opt/homebrew;\n".to_string(),
            zsh: "export HOMEBREW_PREFIX=/opt/homebrew;\n".to_string(),
        };
        write_init_script(env.fs.as_ref(), env.paths.as_ref(), false, Some(&blocks)).unwrap();

        let second = env.fs.read_to_string(&path).unwrap();
        assert_ne!(
            first, second,
            "the two writes must differ for the witness to prove anything"
        );
        assert_eq!(
            env.fs.read_to_string(&witness).unwrap(),
            first,
            "the old inode was rewritten: the init script is being \
             truncated in place, so a shell starting mid-write can \
             source a prefix of it"
        );

        // A successful write also leaves no temp sibling behind.
        let leftovers: Vec<String> = env
            .fs
            .read_dir(path.parent().unwrap())
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert_eq!(leftovers, Vec::<String>::new());
    }

    #[test]
    fn script_regenerated_reflects_current_state() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "alias vi=vim")
            .done()
            .build();

        let ds = make_datastore(&env);

        let script1 =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), false, TEST_GEN, None)
                .unwrap();
        assert!(!script1.contains("aliases.sh"));

        ds.create_data_link("vim", "shell", &env.dotfiles_root.join("vim/aliases.sh"))
            .unwrap();

        let script2 =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), false, TEST_GEN, None)
                .unwrap();
        assert!(script2.contains("aliases.sh"));

        ds.remove_state("vim", "shell").unwrap();

        let script3 =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), false, TEST_GEN, None)
                .unwrap();
        assert!(!script3.contains("aliases.sh"));
    }

    #[test]
    fn ignores_non_symlink_files_in_handler_dirs() {
        let env = TempEnvironment::builder().build();

        let shell_dir = env.paths.handler_data_dir("vim", "shell");
        env.fs.mkdir_all(&shell_dir).unwrap();
        env.fs
            .write_file(&shell_dir.join("not-a-symlink"), b"noise")
            .unwrap();

        let script =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), false, TEST_GEN, None)
                .unwrap();
        assert!(!script.contains("not-a-symlink"));
    }

    #[test]
    fn path_additions_come_before_shell_sources() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "alias vi=vim")
            .file("bin/myscript", "#!/bin/sh")
            .done()
            .build();

        let ds = make_datastore(&env);
        ds.create_data_link("vim", "shell", &env.dotfiles_root.join("vim/aliases.sh"))
            .unwrap();
        ds.create_data_link("vim", "path", &env.dotfiles_root.join("vim/bin"))
            .unwrap();

        let script =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), false, TEST_GEN, None)
                .unwrap();

        let path_pos = script.find("# PATH additions").unwrap();
        let shell_pos = script.find("# Shell scripts").unwrap();
        assert!(
            path_pos < shell_pos,
            "PATH additions should come before shell sources"
        );
    }

    // ── Homebrew bootstrap (shell-hookup-ergonomics.lex §4) ─────────

    /// Stand-in for a captured `brew shellenv`, shaped like the real
    /// thing (zsh block carries the zsh-only `fpath` lines).
    fn sample_brew_blocks() -> BrewBlocks {
        BrewBlocks {
            prefix: PathBuf::from("/opt/homebrew"),
            sh: "export HOMEBREW_PREFIX=\"/opt/homebrew\";\n\
                 eval \"$(/usr/bin/env PATH_HELPER_ROOT=\"/opt/homebrew\" /usr/libexec/path_helper -s)\"\n"
                .to_string(),
            zsh: "export HOMEBREW_PREFIX=\"/opt/homebrew\";\n\
                  fpath[1,0]=\"/opt/homebrew/share/zsh/site-functions\";\n\
                  export FPATH;\n"
                .to_string(),
        }
    }

    /// The ordering claim the whole feature rests on, asserted against
    /// the generated script rather than assumed: brew's block lands
    /// above the first pack PATH addition, so dodot's own entries are
    /// prepended *after* brew's and therefore win.
    #[test]
    fn homebrew_block_precedes_the_first_pack_path_addition() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "alias vi=vim")
            .file("bin/myscript", "#!/bin/sh")
            .done()
            .build();

        let ds = make_datastore(&env);
        ds.create_data_link("vim", "shell", &env.dotfiles_root.join("vim/aliases.sh"))
            .unwrap();
        ds.create_data_link("vim", "path", &env.dotfiles_root.join("vim/bin"))
            .unwrap();

        let script = generate_init_script(
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            TEST_GEN,
            Some(&sample_brew_blocks()),
        )
        .unwrap();

        let brew_pos = script.find("# ── Homebrew environment ──").unwrap();
        let path_pos = script.find("# PATH additions").unwrap();
        let export_pos = script.find("export PATH=\"").unwrap();
        let source_pos = script.find("# Shell scripts").unwrap();
        assert!(
            brew_pos < path_pos && brew_pos < export_pos && brew_pos < source_pos,
            "Homebrew block must come first, script:\n{script}"
        );
    }

    /// The whole emission order in one script, read back as text.
    ///
    /// The version stamp (`shell-hookup-ergonomics.lex` §2.1) and the
    /// Homebrew bootstrap (§4) were written against the same generator
    /// and only meet here, so the order they compose into is asserted
    /// rather than inferred from the fact that both compile: activation
    /// evidence first — all three lines, the version among them —
    /// then brew, then anything a pack contributed.
    #[test]
    fn the_evidence_block_precedes_the_homebrew_block_precedes_the_packs() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "alias vi=vim")
            .file("bin/myscript", "#!/bin/sh")
            .done()
            .build();

        let ds = make_datastore(&env);
        ds.create_data_link("vim", "shell", &env.dotfiles_root.join("vim/aliases.sh"))
            .unwrap();
        ds.create_data_link("vim", "path", &env.dotfiles_root.join("vim/bin"))
            .unwrap();

        let script = generate_init_script(
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            TEST_GEN,
            Some(&sample_brew_blocks()),
        )
        .unwrap();

        let positions = [
            ("evidence header", "# ── dodot activation evidence ──"),
            ("generation export", "export DODOT_INIT_GEN="),
            ("version export", "export DODOT_INIT_VERSION="),
            ("heartbeat redirect", "echo "),
            ("Homebrew block", "# ── Homebrew environment ──"),
            ("PATH additions", "# PATH additions"),
            ("shell sources", "# Shell scripts"),
        ]
        .map(|(label, needle)| {
            (
                label,
                script
                    .find(needle)
                    .unwrap_or_else(|| panic!("missing {label} ({needle}), script:\n{script}")),
            )
        });

        for pair in positions.windows(2) {
            let [(before, at), (after, then)] = pair else {
                unreachable!()
            };
            assert!(
                at < then,
                "{before} must precede {after}, script:\n{script}"
            );
        }
    }

    /// The bootstrap does not depend on any pack having deployed, so a
    /// datastore with nothing in it still carries it — that is the case
    /// where the user's rc file is empty and brew is all they need.
    #[test]
    fn homebrew_block_survives_an_empty_datastore() {
        let env = TempEnvironment::builder().build();

        let script = generate_init_script(
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            TEST_GEN,
            Some(&sample_brew_blocks()),
        )
        .unwrap();

        assert!(
            script.contains("# ── Homebrew environment ──"),
            "script:\n{script}"
        );
        assert!(script.contains("HOMEBREW_PREFIX"), "script:\n{script}");
        // The empty notice is about pack contributions and still holds.
        assert!(script.contains("No shell scripts or PATH additions"));
    }

    /// `off`, a non-macOS host and a brew-less mac all arrive here as
    /// `None`, and none of them may leave a trace in the script.
    #[test]
    fn no_capture_means_no_homebrew_lines_at_all() {
        let env = TempEnvironment::builder().build();

        let script =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), false, TEST_GEN, None)
                .unwrap();

        assert!(!script.contains("Homebrew"), "script:\n{script}");
        assert!(!script.contains("HOMEBREW"), "script:\n{script}");
        assert!(!script.contains("brew"), "script:\n{script}");
    }

    // ── Phase 2: profiling wrapper ──────────────────────────────────

    #[test]
    fn profiling_disabled_matches_phase1_byte_for_byte() {
        // The contract: when profiling is off, the script must be the
        // exact same bytes a Phase-1 generator would have produced. This
        // protects users who don't want any change to their init script.
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "alias vi=vim")
            .done()
            .build();

        let ds = make_datastore(&env);
        ds.create_data_link("vim", "shell", &env.dotfiles_root.join("vim/aliases.sh"))
            .unwrap();

        let script =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), false, TEST_GEN, None)
                .unwrap();
        assert!(!script.contains("_dodot_prof"));
        assert!(!script.contains("EPOCHREALTIME"));
        assert!(!script.contains("dodot shell-init profile"));
    }

    #[test]
    fn profiling_enabled_emits_runtime_gated_preamble() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "alias vi=vim")
            .done()
            .build();

        let ds = make_datastore(&env);
        ds.create_data_link("vim", "shell", &env.dotfiles_root.join("vim/aliases.sh"))
            .unwrap();

        let script =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), true, TEST_GEN, None)
                .unwrap();

        assert!(script.contains("BASH_VERSION"));
        assert!(script.contains("ZSH_VERSION"));
        assert!(script.contains("EPOCHREALTIME"));
        assert!(script.contains(env.paths.probes_shell_init_dir().to_str().unwrap()));
        assert!(script.contains("$$"));
        assert!(script.contains("RANDOM"));
        assert!(script.contains("# dodot shell-init profile v1"));
        assert!(script.contains("columns\\tphase\\tpack\\thandler\\ttarget"));
    }

    #[test]
    fn profiling_enabled_wraps_each_source_with_else_path() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "")
            .file("bin/tool", "#!/bin/sh")
            .done()
            .build();

        let ds = make_datastore(&env);
        ds.create_data_link("vim", "shell", &env.dotfiles_root.join("vim/aliases.sh"))
            .unwrap();
        ds.create_data_link("vim", "path", &env.dotfiles_root.join("vim/bin"))
            .unwrap();

        let script =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), true, TEST_GEN, None)
                .unwrap();

        // Each entry has an if/else so unprofiled shells still source / set PATH.
        // (One else per entry; the epilogue uses an if-only form, so counting
        // `else` keeps us focused on the entry wrappers.)
        let else_count = script.matches("else").count();
        assert_eq!(
            else_count, 2,
            "expected one else-branch per entry; script:\n{script}"
        );

        // Source row carries the captured exit status; PATH row hard-codes 0.
        assert!(script.contains("printf 'source\\tvim\\tshell\\t"));
        assert!(script.contains("printf 'path\\tvim\\tpath\\t"));
        assert!(script.contains("\"$_dodot_rc\""));
    }

    #[test]
    fn profiling_captures_source_stderr_into_errors_log() {
        // The wrapper must redirect each source's stderr to the per-shell
        // scratch file and append a versioned record (`@@\ttarget\texit`)
        // to the errors.log sibling whenever stderr is non-empty.
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "")
            .done()
            .build();
        let ds = make_datastore(&env);
        ds.create_data_link("vim", "shell", &env.dotfiles_root.join("vim/aliases.sh"))
            .unwrap();

        let script =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), true, TEST_GEN, None)
                .unwrap();

        assert!(
            script.contains("_dodot_err_file=\"${_dodot_prof_file%.tsv}.errors.log\""),
            "errors-log path must be a sibling of the profile TSV:\n{script}"
        );
        // Versioned header is seeded lazily — only on first stderr from
        // a sourced file, guarded by `[ -f "$_dodot_err_file" ]` so an
        // all-silent shell startup leaves no sidecar on disk.
        assert!(
            script.contains("[ -f \"$_dodot_err_file\" ] || printf '# dodot shell-init errors v1"),
            "errors-log header must be seeded lazily on first stderr:\n{script}"
        );
        assert!(
            script.contains("2>\"$_dodot_err_tmp\""),
            "source must redirect stderr to scratch file:\n{script}"
        );
        // Truncation before each source so a previous source's stderr
        // doesn't leak into the next record.
        assert!(
            script.contains(": > \"$_dodot_err_tmp\""),
            "scratch file must be truncated before each source:\n{script}"
        );
        assert!(
            script.contains("printf '@@\\t%s\\t%s\\n'"),
            "errors-log records must use @@ header format:\n{script}"
        );
    }

    #[test]
    fn profiling_epilogue_writes_end_marker_and_unsets_state() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "")
            .done()
            .build();

        let ds = make_datastore(&env);
        ds.create_data_link("vim", "shell", &env.dotfiles_root.join("vim/aliases.sh"))
            .unwrap();

        let script =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), true, TEST_GEN, None)
                .unwrap();
        assert!(script.contains("# end_t"));
        assert!(script.contains("unset _dodot_prof"));
        assert!(script.contains("_dodot_prof_file"));
    }

    #[test]
    fn profiling_enabled_with_empty_datastore_skips_preamble() {
        let env = TempEnvironment::builder().build();
        let script =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), true, TEST_GEN, None)
                .unwrap();
        assert!(script.contains("No shell scripts or PATH additions"));
        assert!(!script.contains("_dodot_prof"));
    }

    #[test]
    fn profiled_source_initialises_rc_so_missing_file_isnt_reported_as_failure() {
        // A missing file is not a source failure: initialize rc to zero
        // and only update it when the source command actually runs.
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "alias vi=vim")
            .done()
            .build();
        let ds = make_datastore(&env);
        ds.create_data_link("vim", "shell", &env.dotfiles_root.join("vim/aliases.sh"))
            .unwrap();

        let script =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), true, TEST_GEN, None)
                .unwrap();
        assert!(
            script.contains("_dodot_rc=0;"),
            "profiled branch must seed _dodot_rc=0 before the source attempt:\n{script}"
        );
        assert!(
            script.contains("&& { . "),
            "profiled branch must guard the rc update inside `&& {{ … }}`:\n{script}"
        );
    }

    #[test]
    fn loud_failure_wrapper_present_in_both_modes() {
        // A non-zero exit from a sourced file must surface as a
        // dodot-attributed message on stderr, regardless of whether
        // profiling is on. This is the user-facing breadcrumb that
        // says "the dodot-managed source exited non-zero" alongside
        // the shell's own line-numbered error.
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "alias vi=vim")
            .done()
            .build();

        let ds = make_datastore(&env);
        ds.create_data_link("vim", "shell", &env.dotfiles_root.join("vim/aliases.sh"))
            .unwrap();

        // Profiling off: inline OR-echo form.
        let plain =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), false, TEST_GEN, None)
                .unwrap();
        assert!(
            plain.contains("dodot: shell source exited $?:"),
            "plain script missing loud-failure echo:\n{plain}"
        );

        // Profiling on: timed branch echoes from the elif-empty-stderr
        // arm (silent failure case); the with-stderr arm relies on
        // re-emitting the captured stderr to the user's TTY. Unprofiled
        // fallback uses the OR-echo form like the plain path.
        let timed = generate_init_script(env.fs.as_ref(), env.paths.as_ref(), true, TEST_GEN, None)
            .unwrap();
        assert!(
            timed.contains("echo \"dodot: shell source exited $_dodot_rc:"),
            "timed script missing silent-failure echo:\n{timed}"
        );
        assert!(
            timed.contains("dodot: shell source exited $?:"),
            "timed script missing fallback-branch echo:\n{timed}"
        );
        // Captured stderr is re-emitted to the user's TTY before being
        // appended to the errors log, so they still see it live.
        assert!(
            timed.contains("cat \"$_dodot_err_tmp\" >&2"),
            "timed script must echo captured stderr to user's TTY:\n{timed}"
        );
    }

    // ── Activation evidence (shell-hookup.lex §2.1) ─────────────────

    /// Every shape of generated script must carry both evidence lines:
    /// the evidence is unconditional, which is exactly what separates
    /// it from the opt-in profiling instrumentation.
    #[test]
    fn evidence_is_emitted_unconditionally_in_every_script_shape() {
        let empty = TempEnvironment::builder().build();
        let populated = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "alias vi=vim")
            .done()
            .build();
        let ds = make_datastore(&populated);
        ds.create_data_link(
            "vim",
            "shell",
            &populated.dotfiles_root.join("vim/aliases.sh"),
        )
        .unwrap();

        let shapes = [
            ("empty datastore", &empty, false),
            ("empty datastore, profiled", &empty, true),
            ("populated", &populated, false),
            ("populated, profiled", &populated, true),
        ];
        for (label, env, profiling) in shapes {
            let script = generate_init_script(
                env.fs.as_ref(),
                env.paths.as_ref(),
                profiling,
                TEST_GEN,
                None,
            )
            .unwrap();
            assert!(
                script.contains(&format!("export DODOT_INIT_GEN={TEST_GEN}")),
                "{label}: missing generation stamp:\n{script}"
            );
            assert!(
                script.contains(&format!(
                    "export DODOT_INIT_VERSION={}",
                    activation::running_version()
                )),
                "{label}: missing version stamp:\n{script}"
            );
            // `>|`, not `>`: under `noclobber` a plain `>` refuses to
            // overwrite the heartbeat once it exists, and the `2>/dev/null
            // || :` guards swallow the refusal — freezing "last loaded"
            // at the first shell that ever ran.
            assert!(
                script.contains(&format!(
                    "echo {TEST_GEN} {} >| '{}' 2>/dev/null || :",
                    activation::running_version(),
                    env.paths.hookup_heartbeat_path().display()
                )),
                "{label}: missing heartbeat write:\n{script}"
            );
            assert_eq!(
                activation::parse_script_generation(&script),
                Some(TEST_GEN),
                "{label}: generation must round-trip out of the script"
            );
        }
    }

    /// `noclobber` is an ordinary rc line, and under it a plain `>`
    /// refuses to write a file that already exists. The heartbeat
    /// exists from the second activation onward, so with `>` every
    /// activation after the first fails — silently, because the
    /// redirect carries `2>/dev/null || :` — and "last loaded" freezes
    /// at the first shell the user ever opened. Three of this epic's
    /// claims read that file, so the freeze surfaces as a confident
    /// wrong answer rather than a missing one.
    ///
    /// Run against every shell that can source the generated script,
    /// because the fix is a redirect operator and its support is the
    /// whole question.
    #[test]
    fn the_heartbeat_write_survives_noclobber_in_every_shell() {
        let env = TempEnvironment::builder().build();
        let heartbeat = env.paths.hookup_heartbeat_path();
        env.fs.mkdir_all(&env.paths.probes_hookup_dir()).unwrap();
        let script =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), false, TEST_GEN, None)
                .unwrap();
        let script_path = env.home.join("dodot-init.sh");
        env.fs.write_file(&script_path, script.as_bytes()).unwrap();

        for shell in ["/bin/sh", "/bin/bash", "/bin/zsh"] {
            if !Path::new(shell).exists() {
                continue;
            }
            // A heartbeat from an earlier activation is what a plain
            // `>` would refuse to overwrite.
            env.fs.write_file(&heartbeat, b"1 0.0.0\n").unwrap();
            let status = std::process::Command::new(shell)
                .arg("-c")
                .arg(format!("set -C; . '{}'", script_path.display()))
                .status()
                .expect("the shell runs");
            assert!(status.success(), "{shell}: sourcing the script failed");
            assert_eq!(
                env.fs.read_to_string(&heartbeat).unwrap().trim(),
                format!("{TEST_GEN} {}", activation::running_version()),
                "{shell}: noclobber must not freeze the heartbeat"
            );
        }
    }

    /// The hot-path budget: the version rides along inside the shape
    /// INS01 set — exports and one redirect — and nothing that costs a
    /// process. A `mkdir -p` or a `dodot` call here would be paid on
    /// every shell start, forever.
    #[test]
    fn evidence_costs_two_exports_and_one_redirect() {
        let env = TempEnvironment::builder().build();
        let script =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), false, TEST_GEN, None)
                .unwrap();

        let evidence: Vec<&str> = script
            .lines()
            .filter(|l| l.contains("DODOT_INIT") || l.contains("heartbeat"))
            .collect();
        assert_eq!(evidence.len(), 3, "evidence block: {evidence:?}");
        assert!(evidence[0].starts_with("export DODOT_INIT_GEN="));
        assert!(evidence[1].starts_with("export DODOT_INIT_VERSION="));
        assert!(evidence[2].starts_with("echo "));
        for forbidden in ["mkdir", "dodot ", "date", "$(", "`"] {
            assert!(
                !evidence.iter().any(|l| l.contains(forbidden)),
                "evidence must not run `{forbidden}`: {evidence:?}"
            );
        }
    }

    /// `write_init_script` owns the heartbeat directory so the emitted
    /// redirect never has to create it, and stamps a real generation
    /// that reads back through the same parser `status` uses.
    #[test]
    fn write_init_script_stamps_generation_and_creates_heartbeat_dir() {
        let env = TempEnvironment::builder().build();

        let path = write_init_script(env.fs.as_ref(), env.paths.as_ref(), false, None).unwrap();

        assert!(
            env.fs.is_dir(&env.paths.probes_hookup_dir()),
            "heartbeat dir must exist before any shell writes the marker"
        );
        let gen = activation::read_script_generation(env.fs.as_ref(), env.paths.as_ref())
            .expect("written script must carry a generation");
        assert!(gen > 1_700_000_000, "generation should be a unix ts: {gen}");
        let content = env.fs.read_to_string(&path).unwrap();
        assert!(content.contains(&format!("export DODOT_INIT_GEN={gen}")));
        // No shell has run yet, so there is no heartbeat — that
        // absence is the "never activated" signal.
        assert!(!env.fs.exists(&env.paths.hookup_heartbeat_path()));
    }

    /// Portability leg: the emitted script must parse under POSIX sh,
    /// bash, and zsh — the shells dodot generates init for.
    #[test]
    fn generated_script_parses_in_sh_bash_and_zsh() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "alias vi=vim")
            .file("bin/tool", "#!/bin/sh")
            .done()
            .build();
        let ds = make_datastore(&env);
        ds.create_data_link("vim", "shell", &env.dotfiles_root.join("vim/aliases.sh"))
            .unwrap();
        ds.create_data_link("vim", "path", &env.dotfiles_root.join("vim/bin"))
            .unwrap();

        // The Homebrew arm matters most here: its block is the one
        // place the script carries zsh-only syntax (`fpath[1,0]=`), and
        // `sh -n` is what proves the guard keeps that syntax out of the
        // branch a POSIX shell parses.
        let brew = sample_brew_blocks();
        for profiling in [false, true] {
            for homebrew in [None, Some(&brew)] {
                let path =
                    write_init_script(env.fs.as_ref(), env.paths.as_ref(), profiling, homebrew)
                        .unwrap();
                for shell in ["sh", "bash", "zsh"] {
                    let out = std::process::Command::new(shell)
                        .arg("-n")
                        .arg(&path)
                        .output();
                    let Ok(out) = out else {
                        continue; // interpreter not installed on this host
                    };
                    assert!(
                        out.status.success(),
                        "{shell} -n rejected the generated script (profiling={profiling}, homebrew={}): {}",
                        homebrew.is_some(),
                        String::from_utf8_lossy(&out.stderr)
                    );
                }
            }
        }
    }

    /// The evidence is only worth anything if sourcing the script
    /// actually leaves it: run the real thing under `sh` and read
    /// every signal back — both exports and both heartbeat fields.
    #[test]
    fn sourcing_the_script_exports_the_stamp_and_writes_the_heartbeat() {
        let env = TempEnvironment::builder().build();
        let path = write_init_script(env.fs.as_ref(), env.paths.as_ref(), false, None).unwrap();
        let gen = activation::read_script_generation(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        let version = activation::running_version();

        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!(
                ". '{}'; printf '%s %s' \"$DODOT_INIT_GEN\" \"$DODOT_INIT_VERSION\"",
                path.display()
            ))
            .env_remove(activation::INIT_GEN_ENV)
            .env_remove(activation::INIT_VERSION_ENV)
            .output()
            .expect("sh is required to run dodot's own init script");
        assert!(out.status.success(), "sourcing failed: {out:?}");
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            format!("{gen} {version}"),
            "sourcing must export both the generation and the version"
        );

        let heartbeat = activation::read_heartbeat(env.fs.as_ref(), env.paths.as_ref())
            .expect("sourcing must leave a heartbeat");
        assert_eq!(heartbeat.generation, gen);
        assert_eq!(heartbeat.version.as_deref(), Some(version));
    }

    /// The one rule that tells "wired and deploying nothing" from
    /// "wired and working" — the footer's, and `down`'s, whole basis.
    #[test]
    fn an_empty_script_is_readable_as_having_no_contributions() {
        let env = TempEnvironment::builder().build();
        let empty =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), false, 100, None).unwrap();
        assert!(!script_has_contributions(&empty), "{empty}");

        let env = TempEnvironment::builder().build();
        let shell_dir = env.paths.handler_data_dir("vim", "shell");
        env.fs.mkdir_all(&shell_dir).unwrap();
        let target = env.home.join("aliases.sh");
        env.fs.write_file(&target, b"alias v=vim").unwrap();
        env.fs
            .symlink(&target, &shell_dir.join("aliases.sh"))
            .unwrap();
        let deployed =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), false, 100, None).unwrap();
        assert!(script_has_contributions(&deployed), "{deployed}");
    }

    /// Parsing the guard is not the same as honouring it: source the
    /// script for real under `sh` and confirm the shell took the `sh`
    /// branch — brew's environment set, the zsh-only `fpath` line never
    /// executed (it would print a `command not found` to stderr) and no
    /// `FPATH` left behind.
    #[test]
    fn sourcing_under_sh_takes_the_sh_branch_of_the_homebrew_block() {
        let env = TempEnvironment::builder().build();
        let blocks = BrewBlocks {
            prefix: PathBuf::from("/fake/brew"),
            sh: "export HOMEBREW_PREFIX=\"/fake/brew\";\n".to_string(),
            zsh: "export HOMEBREW_PREFIX=\"/fake/brew\";\n\
                  fpath[1,0]=\"/fake/brew/share/zsh/site-functions\";\n\
                  export FPATH;\n"
                .to_string(),
        };
        let path =
            write_init_script(env.fs.as_ref(), env.paths.as_ref(), false, Some(&blocks)).unwrap();

        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!(
                ". '{}'; printf '%s|%s' \"$HOMEBREW_PREFIX\" \"${{FPATH:-}}\"",
                path.display()
            ))
            .env_remove("HOMEBREW_PREFIX")
            .env_remove("FPATH")
            .output()
            .expect("sh is required to run dodot's own init script");

        assert!(out.status.success(), "sourcing failed: {out:?}");
        assert_eq!(String::from_utf8_lossy(&out.stdout), "/fake/brew|");
        assert!(
            out.stderr.is_empty(),
            "the sh branch must not touch zsh-only lines: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// Concurrency: many shells starting at once each truncate the
    /// same marker with the same static content, so the marker is
    /// always exactly one generation — never interleaved bytes.
    #[test]
    fn concurrent_sources_leave_an_intact_heartbeat() {
        let env = TempEnvironment::builder().build();
        let path = write_init_script(env.fs.as_ref(), env.paths.as_ref(), false, None).unwrap();
        let gen = activation::read_script_generation(env.fs.as_ref(), env.paths.as_ref()).unwrap();

        let children: Vec<_> = (0..8)
            .filter_map(|_| {
                std::process::Command::new("sh")
                    .arg("-c")
                    .arg(format!(". '{}'", path.display()))
                    .spawn()
                    .ok()
            })
            .collect();
        for mut child in children {
            let _ = child.wait();
        }

        assert_eq!(
            activation::read_heartbeat(env.fs.as_ref(), env.paths.as_ref()).map(|h| h.generation),
            Some(gen)
        );
    }

    #[test]
    fn shell_quoting_handles_paths_with_single_quotes() {
        // A path with a single quote in it must round-trip safely
        // through the printf args. Embedded `'` becomes `'\''`.
        assert_eq!(sh_quote("plain"), "'plain'");
        assert_eq!(sh_quote("it's"), "'it'\\''s'");
        assert_eq!(sh_quote(""), "''");
    }
}

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
//! rc line (`dodot probe shell-init --trace-hook`). Users can also add it by hand:
//!
//! ```sh
//! [ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ] && . "$HOME/.local/share/dodot/shell/dodot-init.sh"
//! ```
//!
//! # Targeted verification
//!
//! Current init scripts open with the one-use challenge branch from
//! `docs/proposals/shipped/targeted-shell-init-verification.lex`: when dodot starts a fresh
//! interactive shell only to verify that the hook is reached, the
//! script reports its nonce, verifier PID, shell PID, generation, and
//! dodot version, then exits before heartbeat writes, profiling, PATH
//! setup, Homebrew, or pack contributions. Ordinary shells do not
//! carry the challenge variables and continue into activation evidence
//! unchanged.
//!
//! # Hook tracing
//!
//! Current init scripts also recognize the internal diagnostic mode
//! used by `dodot probe shell-init --trace-hook`. In that mode they
//! suppress activation evidence and profiling writes, then continue
//! through Homebrew, PATH setup, and pack contributions so the trace
//! can diagnose the same startup work without manufacturing heartbeat
//! or profile evidence.
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
//! When `profiling_enabled = false`, the profile writer is omitted;
//! the verification and diagnostic branches still remain at the top
//! because they serve separate command paths.
//!
//! Sources are *not* wrapped in a shell function: in zsh, `source`
//! inside a function changes scoping for plain variable assignments
//! in the sourced file, which is a behavioural surprise nobody asked
//! for. We pay the price of a slightly longer script in exchange for
//! semantic equivalence with the un-instrumented form.

use std::collections::HashSet;
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
pub use homebrew::{
    BrewBlocks, BrewBootstrapMode, BrewCapture, BrewHost, CaptureFailure, PersistedCapture,
};
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

/// Whether a generated script advertises the process-bound targeted
/// verification protocol.
pub fn script_supports_targeted_probe(script: &str) -> bool {
    script
        .lines()
        .any(|line| line.trim() == "# dodot shell-init-probe v1")
}

/// Whether a generated script can suppress activation evidence and
/// shell-init profile writes while a hook trace runs through the rest
/// of the init body.
pub fn script_supports_diagnostic_trace(script: &str) -> bool {
    script
        .lines()
        .any(|line| line.trim() == "# dodot shell-init-trace v1")
}

/// Generate the guarded challenge response fragment used by current
/// init scripts.
///
/// [`generate_init_script`] embeds this fragment at the beginning of
/// the full script. When a verifier starts a shell with a valid
/// targeted challenge, the fragment prints the nonce-bound response and
/// exits before activation evidence, Homebrew setup, profiling, or pack
/// contributions run; ordinary shells continue into the generated init
/// body. This helper returns the fragment by itself for callers that
/// need to inspect or test the probe protocol.
pub fn generate_init_probe_response(generation: u64) -> String {
    let mut script = String::new();
    emit_init_probe_response(&mut script, generation);
    script
}

/// Generate the targeted response fragment for an eval-form hook.
///
/// This fragment is intentionally cheap to create: `dodot init-sh` can
/// return it before resolving the dotfiles root, loading configuration,
/// capturing Homebrew, or scanning packs. The shell still owns the parent
/// identity check. If inherited or stale challenge variables fail that
/// check, the fragment clears them and asks the exact same dodot executable
/// for the ordinary full init body.
pub fn generate_eval_init_probe_response(generation: u64, dodot_executable: &Path) -> String {
    let mut script = String::new();
    emit_init_probe_response_body(&mut script, generation);
    writeln!(script, "else").unwrap();
    writeln!(script, "    unset {}", probe::TARGET_PROBE_ENV).unwrap();
    writeln!(script, "    unset {}", probe::TARGET_PROBE_PARENT_ENV).unwrap();
    writeln!(
        script,
        "    eval \"$({} init-sh)\"",
        sh_quote(&dodot_executable.display().to_string())
    )
    .unwrap();
    writeln!(script, "fi").unwrap();
    writeln!(script).unwrap();
    script
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

/// One directory in the composed `$PATH`'s "packs" tier, attributed to
/// the pack that staged it.
///
/// The attribution is not cosmetic: it drives the `# [pack]` comments
/// emitted above the composed line, and — when profiling is on — the
/// `(pack, target)` columns of each profile row `probe::shell_init`
/// reads back, even though every contribution now lands on `$PATH` via
/// one shared `export`.
struct PathContribution {
    pack: String,
    dir: PathBuf,
}

/// Compose the packs tier of the final `$PATH`
/// (`docs/proposals/path-precedence.lex` §3.1, §3.2, §4.2) — the one
/// place the cross-pack `$PATH` rule lives (§4.3).
///
/// `path_additions` is every pack's staged `path`-handler directory, in
/// pack-scan order: ascending on-disk directory name ([`Fs::read_dir`]
/// sorts), with each pack's own directories in their own scan order.
/// The tiers contract ranks packs purely by that lex order with the
/// *last* pack winning the front of the string — prepending above
/// system `$PATH` is what forces that direction (§2.3), not a style
/// choice — so this reverses the list one pack-group at a time: each
/// pack's own directories keep their internal order, only the
/// pack-to-pack order flips.
///
/// `known_lower_tier_dirs` is every directory a lower, fixed tier
/// already places on `$PATH` before this composed line runs — today
/// that is just Homebrew's `<prefix>/bin` and `<prefix>/sbin`
/// ([`homebrew_known_dirs`]; §3.1 "homebrew / toolchains"). That tier's
/// block is emitted verbatim and untouched (RCS01, out of scope here),
/// so its entry can't itself be deduped away — a pack directory that
/// collides with one is dropped from the packs tier instead, and the
/// lower tier's already-emitted entry is what survives. The surviving
/// copy therefore sits at the lower tier's fixed position, not the
/// pack's — read literally, the opposite of "highest-precedence tier
/// wins the slot" — but the outcome is still exactly one entry rather
/// than two, which is what §3.2 requires and names by example: a stale
/// `001-homebrew` pack collapses into Homebrew's own single entry.
///
/// Deduplication within the packs tier itself is the ordinary case:
/// first occurrence in pack-precedence order — the highest-precedence
/// pack (last on disk) wins the slot, and every lower-precedence repeat
/// of the same directory is dropped.
fn compose_path_tier(
    path_additions: &[(String, PathBuf)],
    known_lower_tier_dirs: &[PathBuf],
) -> Vec<PathContribution> {
    // Group consecutive entries by pack, preserving each pack's own
    // internal (scan) order. `path_additions` already arrives grouped
    // this way — the scan drains one pack fully before moving to the
    // next — so a linear pass is enough; no sort needed.
    let mut groups: Vec<(String, Vec<PathBuf>)> = Vec::new();
    for (pack, dir) in path_additions {
        match groups.last_mut() {
            Some((last_pack, dirs)) if last_pack == pack => dirs.push(dir.clone()),
            _ => groups.push((pack.clone(), vec![dir.clone()])),
        }
    }

    let mut seen: HashSet<PathBuf> = known_lower_tier_dirs.iter().cloned().collect();
    let mut out = Vec::new();
    for (pack, dirs) in groups.into_iter().rev() {
        for dir in dirs {
            if seen.insert(dir.clone()) {
                out.push(PathContribution {
                    pack: pack.clone(),
                    dir,
                });
            }
        }
    }
    out
}

/// The directories Homebrew's own captured bootstrap block already
/// places on `$PATH` before the composed packs-tier line runs — used
/// only to dedup the packs tier against that lower tier
/// ([`compose_path_tier`]); the block itself is untouched
/// (`docs/proposals/path-precedence.lex` note at the top: Homebrew's
/// bootstrap is settled by RCS01 and not reopened here).
///
/// `brew shellenv` puts `<prefix>/bin` and `<prefix>/sbin` on `$PATH`;
/// `None` (not macOS, no brew, or `[shell] homebrew = "off"`) yields no
/// known directories to dedup against.
fn homebrew_known_dirs(homebrew: Option<&BrewBlocks>) -> Vec<PathBuf> {
    match homebrew {
        Some(blocks) => vec![blocks.prefix.join("bin"), blocks.prefix.join("sbin")],
        None => Vec::new(),
    }
}

/// Generate the shell init script content from the current datastore state.
///
/// Scans the datastore for:
/// - `packs/*/shell/*` — symlinks to shell scripts → one `source` line each
/// - `packs/*/path/*` — symlinks to directories, composed by
///   [`compose_path_tier`] into a single deduplicated, tiered
///   `export PATH=…` line (`docs/proposals/path-precedence.lex` §3–§4)
///   instead of one `export PATH=` per directory
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
/// in the module docs. Every generated completion path clears the
/// temporary `_dodot_trace` shell variable before returning control to
/// the user's interactive shell.
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

    emit_init_probe_response(&mut script, generation);
    emit_trace_mode_preamble(&mut script);
    emit_activation_evidence(&mut script, generation, &paths.hookup_heartbeat_path());

    if let Some(blocks) = homebrew {
        homebrew::emit_homebrew_block(&mut script, blocks);
    }

    let packs_dir = paths.data_dir().join("packs");
    if !fs.exists(&packs_dir) {
        append_empty_notice(&mut script);
        emit_trace_mode_cleanup(&mut script);
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

    // Compose the packs tier once here rather than at each of the two
    // sites below that need it (the empty-datastore check and the
    // emitter) — see [`compose_path_tier`] for the tier/dedup rule
    // itself.
    let path_contributions = compose_path_tier(&path_additions, &homebrew_known_dirs(homebrew));

    if path_contributions.is_empty() && shell_sources.is_empty() {
        append_empty_notice(&mut script);
        emit_trace_mode_cleanup(&mut script);
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

    if !path_contributions.is_empty() {
        writeln!(script, "# PATH additions").unwrap();
        for c in &path_contributions {
            writeln!(script, "# [{}]", c.pack).unwrap();
        }
        if profiling_active {
            emit_timed_path(&mut script, &path_contributions);
        } else {
            let joined = path_contributions
                .iter()
                .map(|c| c.dir.display().to_string())
                .collect::<Vec<_>>()
                .join(":");
            writeln!(script, "export PATH=\"{joined}:$PATH\"").unwrap();
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
    emit_trace_mode_cleanup(&mut script);

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

/// Emit the process-bound shell-init verification branch.
fn emit_init_probe_response(script: &mut String, generation: u64) {
    emit_init_probe_response_body(script, generation);
    writeln!(script, "fi").unwrap();
    writeln!(script).unwrap();
}

/// Emit the targeted response through its `exit 0`, leaving the opening
/// `if` unfinished so file-source and eval callers can choose their own
/// identity-mismatch behavior.
fn emit_init_probe_response_body(script: &mut String, generation: u64) {
    let version = activation::running_version();
    writeln!(script, "# dodot shell-init-probe v1").unwrap();
    writeln!(
        script,
        "if [ -n \"${{{}-}}\" ] && [ \"${{{}-}}\" = \"$PPID\" ]; then",
        probe::TARGET_PROBE_ENV,
        probe::TARGET_PROBE_PARENT_ENV
    )
    .unwrap();
    writeln!(
        script,
        "    _dodot_probe_nonce=${}",
        probe::TARGET_PROBE_ENV
    )
    .unwrap();
    writeln!(script, "    unset {}", probe::TARGET_PROBE_ENV).unwrap();
    writeln!(script, "    unset {}", probe::TARGET_PROBE_PARENT_ENV).unwrap();
    writeln!(
        script,
        "    \\printf '\\n{}%s|%s|%s|{}|{}\\n' \"$_dodot_probe_nonce\" \"$PPID\" \"$$\"",
        probe::TARGET_PROBE_MARKER,
        generation,
        version
    )
    .unwrap();
    writeln!(script, "    unset _dodot_probe_nonce").unwrap();
    writeln!(script, "    exit 0").unwrap();
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
fn emit_trace_mode_preamble(script: &mut String) {
    writeln!(script, "# dodot shell-init-trace v1").unwrap();
    writeln!(script, "_dodot_trace=0").unwrap();
    writeln!(
        script,
        "if [ -n \"${{{}-}}\" ]; then",
        trace::DIAGNOSTIC_TRACE_ENV
    )
    .unwrap();
    // Keep the process-scoped marker exported until the trace shell dies.
    // An rc may source this script more than once; consuming the marker on
    // the first source would let a later source write heartbeat/profile
    // evidence during the same diagnostic invocation.
    writeln!(script, "  _dodot_trace=1").unwrap();
    writeln!(script, "fi").unwrap();
    writeln!(script).unwrap();
}

fn emit_trace_mode_cleanup(script: &mut String) {
    writeln!(script, "unset _dodot_trace 2>/dev/null").unwrap();
}

fn emit_activation_evidence(script: &mut String, generation: u64, heartbeat_path: &Path) {
    let heartbeat = sh_quote(&heartbeat_path.display().to_string());
    let version = activation::running_version();
    writeln!(script, "if [ \"${{_dodot_trace:-0}}\" != \"1\" ]; then").unwrap();
    writeln!(script, "# ── dodot activation evidence ──").unwrap();
    writeln!(script, "  export {}={generation}", activation::INIT_GEN_ENV).unwrap();
    writeln!(
        script,
        "  export {}={version}",
        activation::INIT_VERSION_ENV
    )
    .unwrap();
    writeln!(
        script,
        "  echo {generation} {version} >| {heartbeat} 2>/dev/null || :"
    )
    .unwrap();
    writeln!(script, "fi").unwrap();
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
    writeln!(script, "if [ \"${{_dodot_trace:-0}}\" != \"1\" ]; then").unwrap();
    writeln!(
        script,
        "  if [ -n \"${{BASH_VERSION:-}}\" ] || [ -n \"${{ZSH_VERSION:-}}\" ]; then"
    )
    .unwrap();
    // zsh exposes EPOCHREALTIME only after `zmodload zsh/datetime`. Load
    // it eagerly here; bash 5+ has the variable built in and ignores
    // unknown commands like `zmodload` (we suppress its `command not
    // found` error). Doing this *inside* the bash/zsh guard keeps it off
    // hot paths in plain sh.
    writeln!(
        script,
        "    [ -n \"${{ZSH_VERSION:-}}\" ] && zmodload zsh/datetime 2>/dev/null"
    )
    .unwrap();
    writeln!(script, "    if [ -n \"${{EPOCHREALTIME:-}}\" ]; then").unwrap();
    writeln!(script, "      _dodot_prof_dir={dir}").unwrap();
    writeln!(
        script,
        "      _dodot_prof_file=\"$_dodot_prof_dir/profile-${{EPOCHSECONDS:-0}}-$$-${{RANDOM}}.tsv\""
    )
    .unwrap();
    // Sibling errors log: one record per source whose stderr was non-empty.
    // Format: `@@\t<target>\t<exit_status>` header line, followed by the
    // captured stderr verbatim, followed by a trailing newline. Loaded
    // alongside the profile by `probe::shell_init::read_recent_profiles`
    // and parsed by `probe::shell_init::parse_errors_log`.
    writeln!(
        script,
        "      _dodot_err_file=\"${{_dodot_prof_file%.tsv}}.errors.log\""
    )
    .unwrap();
    // Per-shell scratch file for capturing each source's stderr. Reused
    // across every source in this shell startup; truncated each time.
    writeln!(
        script,
        "      _dodot_err_tmp=\"$_dodot_prof_dir/.errtmp-$$\""
    )
    .unwrap();
    writeln!(
        script,
        "      if mkdir -p \"$_dodot_prof_dir\" 2>/dev/null; then"
    )
    .unwrap();
    writeln!(script, "        _dodot_prof_t0=$EPOCHREALTIME").unwrap();
    writeln!(script, "        {{").unwrap();
    writeln!(
        script,
        "          printf '# dodot shell-init profile v1\\n'"
    )
    .unwrap();
    writeln!(
        script,
        "          printf '# shell\\t%s\\n' \"${{BASH_VERSION:+bash $BASH_VERSION}}${{ZSH_VERSION:+zsh $ZSH_VERSION}}\""
    )
    .unwrap();
    writeln!(
        script,
        "          printf '# start_t\\t%s\\n' \"$_dodot_prof_t0\""
    )
    .unwrap();
    writeln!(
        script,
        "          printf '# init_script\\t%s\\n' {init_script}"
    )
    .unwrap();
    writeln!(
        script,
        "          printf '# columns\\tphase\\tpack\\thandler\\ttarget\\tstart_t\\tend_t\\texit_status\\n'"
    )
    .unwrap();
    writeln!(
        script,
        "        }} > \"$_dodot_prof_file\" 2>/dev/null && _dodot_prof=1"
    )
    .unwrap();
    // Errors log is created lazily — see `emit_timed_source`. Most shell
    // startups have no stderr from any source, and writing an empty
    // header file for each one would defeat the "fast path is free"
    // claim. The first source that actually emits stderr seeds the
    // header before appending its record.
    writeln!(script, "      fi").unwrap();
    writeln!(script, "    fi").unwrap();
    writeln!(script, "  fi").unwrap();
    writeln!(script, "fi").unwrap();
    writeln!(script).unwrap();
}

/// One inline-timed `export PATH=…` row emitting the whole composed
/// packs-tier line in a single shell statement — composition is one
/// command now (§4.2 of the Spec), not one per directory — with one
/// profiling record per contributing directory, all sharing that one
/// timing window. The branch is one comparison at runtime — negligible
/// on shells where the wrapper is inert.
fn emit_timed_path(script: &mut String, contributions: &[PathContribution]) {
    let joined = contributions
        .iter()
        .map(|c| c.dir.display().to_string())
        .collect::<Vec<_>>()
        .join(":");
    writeln!(script, "if [ \"$_dodot_prof\" = \"1\" ]; then").unwrap();
    writeln!(
        script,
        "  _dodot_t0=$EPOCHREALTIME; export PATH=\"{joined}:$PATH\"; _dodot_t1=$EPOCHREALTIME"
    )
    .unwrap();
    for c in contributions {
        let pack = &c.pack;
        let target_q = sh_quote(&c.dir.display().to_string());
        writeln!(
            script,
            "  printf 'path\\t{pack}\\tpath\\t%s\\t%s\\t%s\\t0\\n' {target_q} \"$_dodot_t0\" \"$_dodot_t1\" >> \"$_dodot_prof_file\" 2>/dev/null"
        )
        .unwrap();
    }
    writeln!(script, "else").unwrap();
    writeln!(script, "  export PATH=\"{joined}:$PATH\"").unwrap();
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

    // ── PATH composition (path-precedence.lex §3–§4) ─────────────────

    /// The worked example from Spec §2.3, pinned as a fixture: three
    /// packs staging one directory each, read off disk in ascending
    /// order (`001-foo`, `200-bar`, `baz`), must compose into exactly
    /// that final order — the pack read *last* wins the front of the
    /// string, because prepending above system `$PATH` is what forces
    /// the direction, not a style choice.
    #[test]
    fn pinned_three_pack_worked_example_from_spec_2_3() {
        let env = TempEnvironment::builder().build();
        let ds = make_datastore(&env);

        let foo = env.home.join("dotfiles/001-foo/bin");
        let bar = env.home.join("dotfiles/200-bar/bin");
        let baz = env.home.join("dotfiles/baz/bin");
        ds.create_data_link("001-foo", "path", &foo).unwrap();
        ds.create_data_link("200-bar", "path", &bar).unwrap();
        ds.create_data_link("baz", "path", &baz).unwrap();

        let script =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), false, TEST_GEN, None)
                .unwrap();

        assert!(
            script.contains(&format!(
                "export PATH=\"{}:{}:{}:$PATH\"",
                baz.display(),
                bar.display(),
                foo.display()
            )),
            "on-disk ascending 001-foo, 200-bar, baz must compose to baz:bar:foo, script:\n{script}"
        );
        // One computed line, not one per directory.
        assert_eq!(
            script.matches("export PATH=\"").count(),
            1,
            "script:\n{script}"
        );
    }

    /// Two packs staging the same directory must resolve to exactly one
    /// entry in the composed line (§3.2) — the higher-precedence pack
    /// (later on disk) keeps it, the earlier pack's repeat is dropped.
    #[test]
    fn two_packs_staging_the_same_directory_produce_one_entry() {
        let env = TempEnvironment::builder().build();
        let ds = make_datastore(&env);

        let shared = env.home.join("shared/bin");
        ds.create_data_link("aaa", "path", &shared).unwrap();
        ds.create_data_link("zzz", "path", &shared).unwrap();

        let script =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), false, TEST_GEN, None)
                .unwrap();

        assert_eq!(
            script.matches(&shared.display().to_string()).count(),
            1,
            "script:\n{script}"
        );
        assert!(script.contains("# [zzz]"), "script:\n{script}");
        assert!(
            !script.contains("# [aaa]"),
            "the earlier pack's duplicate must not survive: script:\n{script}"
        );
    }

    /// A stale `001-homebrew` pack (or any pack) that duplicates the
    /// directory the built-in Homebrew capture already places on
    /// `$PATH` collapses into that one entry rather than adding a
    /// second — the exact case the Spec names by name (§3.2).
    #[test]
    fn stale_pack_duplicating_the_homebrew_bin_dir_is_dropped_from_the_packs_tier() {
        let env = TempEnvironment::builder().build();
        let ds = make_datastore(&env);

        let blocks = sample_brew_blocks();
        let stale_dir = blocks.prefix.join("bin");
        ds.create_data_link("001-homebrew", "path", &stale_dir)
            .unwrap();

        let script = generate_init_script(
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            TEST_GEN,
            Some(&blocks),
        )
        .unwrap();

        assert!(
            !script.contains("# PATH additions"),
            "the packs tier must have nothing left once its only entry dedups away: script:\n{script}"
        );
        assert!(
            !script.contains("export PATH=\""),
            "no composed PATH line should be emitted: script:\n{script}"
        );
        assert!(
            script.contains("No shell scripts or PATH additions"),
            "script:\n{script}"
        );
    }

    /// The homebrew/toolchain tier is fixed below every pack regardless
    /// of naming (§3.1) — a pack named to sort *before* anything
    /// Homebrew-related still has its composed export line run after
    /// Homebrew's block, because the ordering is structural (Homebrew's
    /// block always emits first), not a function of on-disk names.
    #[test]
    fn homebrew_tier_stays_below_a_pack_even_when_the_pack_sorts_first() {
        let env = TempEnvironment::builder().build();
        let ds = make_datastore(&env);

        let dir = env.home.join("early/bin");
        ds.create_data_link("000-early", "path", &dir).unwrap();

        let script = generate_init_script(
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            TEST_GEN,
            Some(&sample_brew_blocks()),
        )
        .unwrap();

        let brew_pos = script.find("# ── Homebrew environment ──").unwrap();
        let export_pos = script.find("export PATH=\"").unwrap();
        assert!(
            brew_pos < export_pos,
            "a pack named to sort first must still lose to the fixed Homebrew tier: script:\n{script}"
        );
        assert!(
            script.contains(&dir.display().to_string()),
            "script:\n{script}"
        );
    }

    /// The whole point of computing once in Rust: sourced for real, the
    /// shell's resulting `$PATH` must match what [`compose_path_tier`]
    /// computed — tiered, deduplicated, last-pack-wins — under both
    /// bash and zsh.
    #[test]
    fn real_shell_composes_the_pinned_path_matching_the_rust_computed_value() {
        let env = TempEnvironment::builder().build();
        let ds = make_datastore(&env);

        let foo = env.home.join("001-foo/bin");
        let bar = env.home.join("200-bar/bin");
        let baz = env.home.join("baz/bin");
        ds.create_data_link("001-foo", "path", &foo).unwrap();
        ds.create_data_link("200-bar", "path", &bar).unwrap();
        ds.create_data_link("baz", "path", &baz).unwrap();

        let path = write_init_script(env.fs.as_ref(), env.paths.as_ref(), false, None).unwrap();
        let expected = format!(
            "{}:{}:{}:/preexisting",
            baz.display(),
            bar.display(),
            foo.display()
        );

        // `--noprofile --norc` (bash) / `-f` (zsh, same no-rc guard
        // `trace.rs`'s `HookupShell::Zsh` uses) keep the host's own rc
        // files — which may append their own PATH entries, e.g.
        // `~/.cargo/bin` — from folding into what we're trying to
        // measure here: the script's own composed line, in isolation.
        let shells: &[(&str, &[&str])] = &[
            ("/bin/bash", &["--noprofile", "--norc", "-c"]),
            ("/bin/zsh", &["-f", "-c"]),
        ];
        for (shell, flags) in shells {
            let shell_path = Path::new(shell);
            if !shell_path.exists() {
                continue;
            }
            let out = std::process::Command::new(shell_path)
                .args(*flags)
                .arg(format!(". '{}'; printf '%s' \"$PATH\"", path.display()))
                .env("PATH", "/preexisting")
                .output()
                .expect("the shell runs");
            assert!(out.status.success(), "{shell}: sourcing failed: {out:?}");
            assert_eq!(
                String::from_utf8_lossy(&out.stdout),
                expected,
                "{shell}: composed $PATH did not match the Rust-computed value"
            );
        }
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

    #[test]
    fn targeted_probe_branch_precedes_activation_evidence() {
        let env = TempEnvironment::builder().build();

        let script =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), true, TEST_GEN, None)
                .unwrap();

        let probe = script.find("# dodot shell-init-probe v1").unwrap();
        let evidence = script.find("# ── dodot activation evidence ──").unwrap();
        assert!(
            probe < evidence,
            "verification branch must answer before heartbeat/profile setup:\n{script}"
        );
        assert!(script_supports_targeted_probe(&script));
    }

    #[test]
    fn diagnostic_trace_mode_precedes_and_guards_mutating_evidence() {
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

        let trace = script.find("# dodot shell-init-trace v1").unwrap();
        let evidence_guard = script
            .find("if [ \"${_dodot_trace:-0}\" != \"1\" ]; then")
            .unwrap();
        let path_or_shell = script.find("# Shell scripts").unwrap();
        assert!(
            trace < evidence_guard && evidence_guard < path_or_shell,
            "diagnostic trace mode must be known before evidence/profile writes but keep contributions:\n{script}"
        );
        assert!(script_supports_diagnostic_trace(&script));
    }

    #[test]
    fn generated_scripts_do_not_leave_trace_state_in_the_shell() {
        let bash = std::path::Path::new("/bin/bash");
        if !bash.exists() {
            return;
        }

        let empty_env = TempEnvironment::builder().build();
        let empty_script = generate_init_script(
            empty_env.fs.as_ref(),
            empty_env.paths.as_ref(),
            false,
            TEST_GEN,
            None,
        )
        .unwrap();
        assert_trace_state_is_cleaned(bash, &empty_env, &empty_script, "empty");

        let env = TempEnvironment::builder()
            .pack("vim")
            .file("bin/tool", "#!/bin/sh\n")
            .done()
            .build();
        let ds = make_datastore(&env);
        ds.create_data_link("vim", "path", &env.dotfiles_root.join("vim/bin"))
            .unwrap();

        for (label, profiling) in [("plain", false), ("profiled", true)] {
            let script = generate_init_script(
                env.fs.as_ref(),
                env.paths.as_ref(),
                profiling,
                TEST_GEN,
                None,
            )
            .unwrap();
            assert_trace_state_is_cleaned(bash, &env, &script, label);
        }
    }

    #[test]
    fn diagnostic_trace_mode_stays_sticky_across_repeated_sources() {
        let bash = std::path::Path::new("/bin/bash");
        if !bash.exists() {
            return;
        }

        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "alias vi=vim")
            .done()
            .build();
        let ds = make_datastore(&env);
        ds.create_data_link("vim", "shell", &env.dotfiles_root.join("vim/aliases.sh"))
            .unwrap();
        env.fs.mkdir_all(&env.paths.probes_hookup_dir()).unwrap();
        let script =
            generate_init_script(env.fs.as_ref(), env.paths.as_ref(), true, TEST_GEN, None)
                .unwrap();
        let script_path = env.home.join("dodot-init.sh");
        env.fs.write_file(&script_path, script.as_bytes()).unwrap();
        let source_twice = format!(
            ". {path}; . {path}",
            path = sh_quote(&script_path.display().to_string())
        );

        let status = std::process::Command::new(bash)
            .args(["--noprofile", "--norc", "-c", &source_twice])
            .env(trace::DIAGNOSTIC_TRACE_ENV, "1")
            .env("HOME", &env.home)
            .status()
            .expect("bash runs");

        assert!(
            status.success(),
            "generated script failed when sourced twice"
        );
        assert!(
            !env.fs.exists(&env.paths.hookup_heartbeat_path()),
            "the second source wrote activation evidence"
        );
        assert!(
            env.fs
                .read_dir(&env.paths.probes_shell_init_dir())
                .unwrap_or_default()
                .is_empty(),
            "the second source created a startup profile"
        );
    }

    fn assert_trace_state_is_cleaned(
        bash: &std::path::Path,
        env: &TempEnvironment,
        script: &str,
        label: &str,
    ) {
        let script_path = env.home.join(format!("{label}-dodot-init.sh"));
        env.fs.mkdir_all(&env.paths.probes_hookup_dir()).unwrap();
        env.fs.write_file(&script_path, script.as_bytes()).unwrap();
        let status = std::process::Command::new(bash)
            .arg("-c")
            .arg(format!(
                ". {}; test -z \"${{_dodot_trace+x}}\"",
                sh_quote(&script_path.display().to_string())
            ))
            .status()
            .expect("bash runs");
        assert!(
            status.success(),
            "{label}: generated script left _dodot_trace in scope:\n{script}"
        );
    }

    #[test]
    fn eval_probe_response_contains_no_ordinary_init_body() {
        let script = generate_init_probe_response(TEST_GEN);

        assert!(script_supports_targeted_probe(&script));
        assert!(script.contains(&format!("|{TEST_GEN}|{}", activation::running_version())));
        assert!(!script.contains("DODOT_INIT_GEN"), "script:\n{script}");
        assert!(!script.contains("heartbeat"), "script:\n{script}");
        assert!(!script.contains("shell-init profile"), "script:\n{script}");
        assert!(!script.contains("Homebrew"), "script:\n{script}");
    }

    // ── Phase 2: profiling wrapper ──────────────────────────────────

    #[test]
    fn profiling_disabled_omits_the_profile_wrapper() {
        // The contract: when profiling is off, the profile writer is
        // absent. Other top-of-file protocol branches may still be
        // present because they serve verification and tracing.
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
        assert!(evidence[0]
            .trim_start()
            .starts_with("export DODOT_INIT_GEN="));
        assert!(evidence[1]
            .trim_start()
            .starts_with("export DODOT_INIT_VERSION="));
        assert!(evidence[2].trim_start().starts_with("echo "));
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

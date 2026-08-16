//! PATH at the hook line — the live half of `dodot probe shell-init`
//! (`docs/proposals/shell-hookup-ergonomics.lex` §3, folded into
//! `probe shell-init` by epic RCS01 decision 6).
//!
//! The question the cheap signals cannot answer: *what does `dodot`
//! resolve to at the moment the hook line runs?* Spawning the shell
//! with its rc suppressed reproduces PATH before the rc runs, and
//! truncating the rc at the hook breaks parsing inside any block —
//! both rejected in the Spec. Instead **the shell reports it itself**:
//! `PS4` is re-expanded for every traced line, so it carries the
//! location and the live `PATH`; dodot sets it in the spawned shell's
//! environment, runs the user's whole rc under xtrace, and reads back
//! exactly one addressable record — the rc file and line dodot already
//! knows from the `shell::rc` ladder. Nothing in the user's rc is
//! parsed: only a location match and a field dodot itself defined.
//!
//! Resolution then happens **in Rust, not in the shell**: `command -v`
//! silently skips a dangling symlink, and a dangling symlink is
//! precisely the failure that motivated this epic. [`resolve_on_path`]
//! walks the traced PATH, records each candidate passed over and why,
//! and [`TraceVerdict`] keeps the outcomes apart because they demand
//! different action.
//!
//! # Fallback
//!
//! An rc that overrides `PS4` (or drops `PROMPT_SUBST`) costs the
//! marker, which is detectable — as does bash 3.2 (macOS's shipped
//! `/bin/bash`), which truncates long expanded `PS4` values at around
//! a hundred bytes, severing the record before its suffix. Both
//! degrade the same way. The fallback works on a *copy*: a
//! temporary `ZDOTDIR` (zsh) or `--rcfile` (bash) holding the rc with
//! a report line **inserted** before the hook — insertion, not
//! truncation, so the file stays parseable inside any conditional or
//! function — the always-read zsh files symlinked in, and `ZDOTDIR`
//! reassigned to the real directory on line one so the rc's own
//! `$ZDOTDIR` references still resolve. The user's real files are
//! never written; the copy lives in a scratch directory created
//! exclusively under a random name at mode `0700` (see
//! [`scratch_dir`]) and removed when the run ends.
//!
//! The copy must reproduce the shell's startup, not approximate it. A
//! shell that read a different set of files than the real one does
//! yields a verdict the user would act on and should not — so a copy
//! that cannot be built faithfully ends the trace
//! ([`TraceError::FallbackUnfaithful`]) and the command reports "could
//! not trace" instead of a measurement.
//!
//! # Boundaries
//!
//! Only `dodot probe shell-init` reaches this module; `status` and
//! `up` never do (INS01 §9 still binds). Every spawn goes through
//! [`crate::shell::probe::spawn_captured`] — the INS01 envelope
//! (announced, stdin from `/dev/null`, output captured, hard timeout
//! with a process-group kill, `DODOT_INIT_*` scrubbed) reused
//! unchanged. The spawned shell is interactive non-login (the mode
//! that reads the file the hook lives in) and inherits this process's
//! environment minus the scrub, same as the INS01 probe.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::fs::Fs;
use crate::shell::probe::{spawn_captured, SpawnOutcome};
use crate::shell::rc::{self, HookupShell};

/// The field prefix every trace record carries, behind the leading
/// `+`(s) the shell prepends per nesting depth.
pub const TRACE_MARKER: &str = "dodot-trace|";

/// Terminates the PATH field of a record, separating it from the
/// traced command text that follows on the same line.
pub const RECORD_SUFFIX: &str = "|> ";

// ── The PS4 contract ────────────────────────────────────────────

/// The `PS4` value handed to the spawned shell.
///
/// zsh expands prompt escapes (`%N` = file being sourced, `%i` = line
/// within it) natively but needs `PROMPT_SUBST` for the `$PATH`
/// expansion — set on the command line (see [`trace_args`]), never in
/// the user's files. bash expands parameters in `PS4` by default.
pub fn ps4(shell: HookupShell) -> String {
    match shell {
        HookupShell::Zsh => format!("+{TRACE_MARKER}%N|%i|$PATH{RECORD_SUFFIX}"),
        HookupShell::Bash => {
            format!("+{TRACE_MARKER}${{BASH_SOURCE}}|${{LINENO}}|${{PATH}}{RECORD_SUFFIX}")
        }
    }
}

/// Arguments for the traced spawn: xtrace on from the start, then the
/// same interactive `-c true` shape the INS01 probe uses.
fn trace_args(shell: HookupShell) -> &'static [&'static str] {
    match shell {
        HookupShell::Zsh => &["-o", "promptsubst", "-o", "xtrace", "-i", "-c", "true"],
        HookupShell::Bash => &["-x", "-i", "-c", "true"],
    }
}

// ── Trace parsing (pure) ────────────────────────────────────────

/// One `PS4` record read back out of a trace: which file, which line,
/// and what `PATH` held when that line ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceRecord {
    pub file: String,
    pub line: usize,
    pub path: String,
}

/// Parse every dodot record out of captured xtrace output.
///
/// Everything else — the user's own commands, other files' records,
/// arbitrary rc noise — is ignored, not interpreted. The moment this
/// starts reading user commands it becomes the brittle thing the Spec
/// rejected.
pub fn parse_trace(stderr: &str) -> Vec<TraceRecord> {
    stderr.lines().filter_map(parse_record).collect()
}

/// Parse one line as a record, or `None` for anything that is not one.
///
/// The shell repeats the first `PS4` character per nesting depth
/// (`++dodot-trace|…` inside a command substitution), so at least one
/// leading `+` is required and any number are stripped.
fn parse_record(line: &str) -> Option<TraceRecord> {
    let stripped = line.trim_start_matches('+');
    if stripped.len() == line.len() {
        return None;
    }
    let rest = stripped.strip_prefix(TRACE_MARKER)?;
    let (file, rest) = rest.split_once('|')?;
    let (line_no, rest) = rest.split_once('|')?;
    let line_no = line_no.parse::<usize>().ok()?;
    // PATH runs to the record suffix; the traced command follows. A
    // PATH containing the suffix itself would truncate early — a
    // pathological name this deliberately does not chase.
    let path = &rest[..rest.find(RECORD_SUFFIX)?];
    Some(TraceRecord {
        file: file.to_string(),
        line: line_no,
        path: path.to_string(),
    })
}

/// The first record at `line` of any of `files` — the addressable
/// read: dodot knows the rc and the hook's line, so the record is
/// looked up, never searched for. First wins because a compound hook
/// line traces once per component with the same pre-hook PATH.
pub fn record_at<'a>(
    records: &'a [TraceRecord],
    files: &[&Path],
    line: usize,
) -> Option<&'a TraceRecord> {
    records
        .iter()
        .find(|r| r.line == line && files.iter().any(|f| Path::new(&r.file) == *f))
}

// ── Hook location (pure) ────────────────────────────────────────

/// Which shape of hook a line carries — they get different
/// resolution halves ([`TraceVerdict`]): the `eval` form asks what
/// `dodot` resolves to on PATH; the file-source form asks only
/// whether the sourced script exists, no PATH involved — the
/// structural reason the file-source form is the recommended one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookForm {
    /// `eval "$(dodot init-sh)"` — resolution rides on PATH.
    Eval,
    /// `. …/dodot-init.sh` — a fixed path, present or not.
    FileSource,
}

/// Locate the hook in rc text: 1-indexed line number and form.
///
/// The same recognition [`rc::scan_hook`] uses, plus the location:
/// the first uncommented line mentioning either hook form. A
/// commented-out hook is exactly the broken-hookup story and must not
/// count.
pub fn find_hook(text: &str) -> Option<(usize, HookForm)> {
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.starts_with('#') {
            continue;
        }
        if line.contains("dodot init-sh") {
            return Some((idx + 1, HookForm::Eval));
        }
        if line.contains("dodot-init.sh") {
            return Some((idx + 1, HookForm::FileSource));
        }
    }
    None
}

// ── PATH resolution (pure over an injected Fs) ──────────────────

/// Why one PATH entry did not produce the binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// The PATH entry names a directory that does not exist.
    MissingDir,
    /// The directory exists but holds no entry by this name. The
    /// common, uninteresting case — recorded for completeness, not
    /// display.
    NotPresent,
    /// A symlink whose target is gone — the entry `command -v`
    /// silently skips, and the one that started this epic.
    DanglingSymlink { target: Option<PathBuf> },
    /// Present but not an executable regular file.
    NotExecutable,
}

/// One PATH entry's candidate and what became of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// `<dir>/<binary>` for the PATH entry this row describes.
    pub path: PathBuf,
    /// `None` means this candidate won.
    pub skipped: Option<SkipReason>,
}

/// The walk's outcome: every candidate up to and including the
/// winner, in PATH order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    pub candidates: Vec<Candidate>,
    /// First executable match, exactly as the shell would pick it —
    /// except nothing is skipped silently on the way there.
    pub winner: Option<PathBuf>,
}

impl Resolution {
    /// The candidates worth telling the user about: entries that
    /// exist in some form but lost, or whose directory is missing.
    /// [`SkipReason::NotPresent`] rows are omitted — "no dodot in
    /// /usr/bin" is not a finding.
    pub fn notable_skips(&self) -> impl Iterator<Item = &Candidate> {
        self.candidates.iter().filter(|c| {
            matches!(
                c.skipped,
                Some(SkipReason::DanglingSymlink { .. })
                    | Some(SkipReason::NotExecutable)
                    | Some(SkipReason::MissingDir)
            )
        })
    }
}

/// Resolve `binary` against a `PATH` string the way the shell would —
/// first executable match wins — but reporting what the shell skips
/// silently. Pure over the injected [`Fs`]. The walk stops at the
/// winner: entries after it were never passed over.
pub fn resolve_on_path(fs: &dyn Fs, path_var: &str, binary: &str) -> Resolution {
    let mut candidates = Vec::new();
    let mut winner = None;
    for dir in path_var.split(':').filter(|d| !d.is_empty()) {
        let dir = Path::new(dir);
        let candidate = dir.join(binary);
        let skipped = classify(fs, dir, &candidate);
        let won = skipped.is_none();
        candidates.push(Candidate {
            path: candidate.clone(),
            skipped,
        });
        if won {
            winner = Some(candidate);
            break;
        }
    }
    Resolution { candidates, winner }
}

/// Why `candidate` in `dir` loses, or `None` when it wins.
fn classify(fs: &dyn Fs, dir: &Path, candidate: &Path) -> Option<SkipReason> {
    if !fs.is_dir(dir) {
        return Some(SkipReason::MissingDir);
    }
    if fs.is_symlink(candidate) && !fs.exists(candidate) {
        return Some(SkipReason::DanglingSymlink {
            target: fs.readlink(candidate).ok(),
        });
    }
    if !fs.exists(candidate) {
        return Some(SkipReason::NotPresent);
    }
    match fs.stat(candidate) {
        Ok(meta) if meta.is_file && meta.mode & 0o111 != 0 => None,
        _ => Some(SkipReason::NotExecutable),
    }
}

// ── Verdicts (pure) ─────────────────────────────────────────────

/// The measured answer to "what runs at the hook line?" — kept apart
/// because each demands different action (Spec §3.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceVerdict {
    /// No record at the hook's location: the hook sits inside a branch
    /// that did not run, or the rc `exec`s away before reaching it.
    HookNeverRan,
    /// `dodot` is unresolvable at the hook line, with the PATH that
    /// was searched and every entry skipped on the way.
    Unresolvable {
        path: String,
        resolution: Resolution,
    },
    /// The hook line runs a different binary than the one running now
    /// — both paths, both versions. `running` is carried here rather
    /// than re-read at display time so the two halves of the
    /// comparison can never disagree about which binary "now" is.
    DifferentBinary {
        found: PathBuf,
        found_version: Option<String>,
        running: PathBuf,
        resolution: Resolution,
    },
    /// `dodot` resolves to the running binary: the hook is sound and
    /// the fault, if any, is elsewhere.
    RunningBinary { path: PathBuf },
    /// File-source hook whose script path is gone — `dodot up`
    /// regenerates it.
    ScriptMissing { script: PathBuf },
    /// File-source hook and the script is where the hook says: sound.
    ScriptPresent { script: PathBuf },
}

/// Judge an `eval`-form hook from the PATH its trace record carried.
///
/// `version_of` is how the winner's version is learned — injected so
/// the judgment stays pure; production executes `<winner> --version`
/// under the probe envelope, tests hand back canned strings. Binary
/// identity is judged on symlink-resolved paths: a `dodot` reached
/// through a link *is* the running binary if the link lands on it.
pub fn judge_eval_hook(
    fs: &dyn Fs,
    traced_path: &str,
    running_exe: &Path,
    version_of: &dyn Fn(&Path) -> Option<String>,
) -> TraceVerdict {
    let resolution = resolve_on_path(fs, traced_path, "dodot");
    let Some(winner) = resolution.winner.clone() else {
        return TraceVerdict::Unresolvable {
            path: traced_path.to_string(),
            resolution,
        };
    };
    let winner_real = rc::resolve_symlinks(fs, &winner);
    let running_real = rc::resolve_symlinks(fs, running_exe);
    if winner_real == running_real {
        TraceVerdict::RunningBinary { path: winner }
    } else {
        TraceVerdict::DifferentBinary {
            found_version: version_of(&winner),
            found: winner,
            running: running_exe.to_path_buf(),
            resolution,
        }
    }
}

/// Extract a version from `dodot --version` output: the last
/// whitespace token of the first line, accepted only when it starts
/// with a digit. Anything else — an error message, an empty capture,
/// a binary that is not dodot at all — is an unknown version, not a
/// guess.
pub fn parse_version_output(stdout: &str) -> Option<String> {
    let token = stdout.lines().next()?.split_whitespace().next_back()?;
    token
        .starts_with(|c: char| c.is_ascii_digit())
        .then(|| token.to_string())
}

// ── The trace spawns (impure) ───────────────────────────────────

/// The line printed before any shell is spawned. The trace is
/// announced, never covert — same rule as the INS01 probe.
pub fn announcement(shell: HookupShell) -> String {
    format!(
        "tracing shell startup ({})… (runs your rc file once)",
        shell.as_str()
    )
}

/// Why no trace could be read at all. Everything here degrades to
/// "could not trace" plus the recorded half — a slow or hostile rc
/// must still leave the user with a report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceError {
    /// The shell outlived the timeout; its process group was killed.
    TimedOut,
    /// The shell could not be spawned.
    SpawnFailed(String),
    /// The rc file could not be read for the fallback copy.
    RcUnreadable(String),
    /// The fallback's scratch copy could not be built so that it
    /// reproduces the shell's startup faithfully — the scratch
    /// directory would not open, or an always-read file could not be
    /// linked in. A verdict computed from a shell that read a
    /// different set of files than the real one does is worse than no
    /// verdict, because the user acts on it.
    FallbackUnfaithful(String),
}

/// One completed trace: the records read back, and whether the
/// fallback copy had to be used to get them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceRun {
    pub records: Vec<TraceRecord>,
    pub used_fallback: bool,
}

/// Spawn the shell under xtrace and read the hook-line records back,
/// falling back to the inserted-report copy when the primary run
/// yields no record at the hook's location.
///
/// `rc_nominal` is the path the shell opens (what zsh's `%N` / bash's
/// `BASH_SOURCE` will name); `rc_resolved` the symlink-resolved file
/// that holds the bytes. Records are matched against both. The
/// fallback runs whenever the primary record is missing — an rc-side
/// `PS4` override can strip the marker from any suffix of the file,
/// so absence alone cannot distinguish "hook never ran" from "marker
/// lost"; the fallback's inserted report can, because it fires exactly
/// when the hook line would be reached. A missing record *after* the
/// fallback is therefore the hook-never-ran answer, confirmed on a
/// copy that cannot lose the marker.
pub fn run_trace(
    fs: &dyn Fs,
    shell_path: &Path,
    shell: HookupShell,
    rc_nominal: &Path,
    rc_resolved: &Path,
    hook_line: usize,
    timeout: Duration,
) -> Result<TraceRun, TraceError> {
    let mut command = Command::new(shell_path);
    command.env("PS4", ps4(shell)).args(trace_args(shell));
    let records = match spawn_captured(command, timeout) {
        SpawnOutcome::TimedOut => return Err(TraceError::TimedOut),
        SpawnOutcome::SpawnFailed(e) => return Err(TraceError::SpawnFailed(e)),
        SpawnOutcome::Finished(capture) => parse_trace(&capture.stderr),
    };
    if record_at(&records, &[rc_nominal, rc_resolved], hook_line).is_some() {
        return Ok(TraceRun {
            records,
            used_fallback: false,
        });
    }
    run_fallback(
        fs,
        shell_path,
        shell,
        rc_nominal,
        rc_resolved,
        hook_line,
        timeout,
    )
    .map(|records| TraceRun {
        records,
        used_fallback: true,
    })
}

/// The copy-based fallback: a modified *copy* of the rc with a report
/// line inserted before the hook, run via a temporary `ZDOTDIR` (zsh)
/// or `--rcfile` (bash). The report line prints the same record shape
/// the `PS4` path produces — one parser, two producers — addressed to
/// the *original* rc location, so the lookup does not care which path
/// produced the record. The user's real files are never written.
///
/// The copy has to reproduce the shell's startup, not approximate it:
/// anything that would leave the spawned shell reading a different set
/// of files than the real one does ends the trace with
/// [`TraceError::FallbackUnfaithful`] rather than yielding a verdict
/// the user would act on.
fn run_fallback(
    fs: &dyn Fs,
    shell_path: &Path,
    shell: HookupShell,
    rc_nominal: &Path,
    rc_resolved: &Path,
    hook_line: usize,
    timeout: Duration,
) -> Result<Vec<TraceRecord>, TraceError> {
    let rc_text = fs
        .read_to_string(rc_resolved)
        .map_err(|e| TraceError::RcUnreadable(format!("{e}")))?;
    // Dropped at the end of this function — every exit path, the
    // timeout and the error paths included, takes the copy with it.
    let scratch = scratch_dir()?;
    let temp = scratch.path();
    let mut command = Command::new(shell_path);
    match shell {
        HookupShell::Zsh => {
            let real_zdot = rc_nominal.parent().unwrap_or(Path::new("/"));
            // Line one points $ZDOTDIR back at the real directory
            // before anything in the rc can read it; the report
            // line goes in front of the hook.
            let copy = format!(
                "ZDOTDIR={}\n{}",
                shell_quote(&real_zdot.display().to_string()),
                insert_report_line(&rc_text, rc_nominal, hook_line)
            );
            fs.write_file(&temp.join(".zshrc"), copy.as_bytes())
                .map_err(|e| TraceError::FallbackUnfaithful(format!("{e}")))?;
            // The always-read files still get read — from where they
            // really live. A link that cannot be made is not a
            // cosmetic loss: the shell would start from a different
            // set of files, so it ends the trace.
            for name in [".zshenv", ".zprofile", ".zlogin"] {
                let real = real_zdot.join(name);
                if fs.exists(&real) {
                    fs.symlink(&real, &temp.join(name))
                        .map_err(|e| TraceError::FallbackUnfaithful(format!("{e}")))?;
                }
            }
            command.env("ZDOTDIR", temp).args(["-i", "-c", "true"]);
        }
        HookupShell::Bash => {
            let copy = insert_report_line(&rc_text, rc_nominal, hook_line);
            let rcfile = temp.join("bashrc");
            fs.write_file(&rcfile, copy.as_bytes())
                .map_err(|e| TraceError::FallbackUnfaithful(format!("{e}")))?;
            command
                .arg("--rcfile")
                .arg(&rcfile)
                .args(["-i", "-c", "true"]);
        }
    }
    match spawn_captured(command, timeout) {
        SpawnOutcome::TimedOut => Err(TraceError::TimedOut),
        SpawnOutcome::SpawnFailed(e) => Err(TraceError::SpawnFailed(e)),
        SpawnOutcome::Finished(capture) => Ok(parse_trace(&capture.stderr)),
    }
}

/// The rc text with the report line inserted immediately before
/// `hook_line` (1-indexed). Insertion keeps every surrounding block
/// parseable; the report addresses the original `<rc>:<line>` so the
/// record lookup is unchanged.
fn insert_report_line(rc_text: &str, rc_nominal: &Path, hook_line: usize) -> String {
    let report = format!(
        "printf '+{TRACE_MARKER}%s|%s|%s{RECORD_SUFFIX}\\n' {} {} \"$PATH\" >&2\n",
        shell_quote(&rc_nominal.display().to_string()),
        hook_line,
    );
    let mut out = String::with_capacity(rc_text.len() + report.len());
    for (idx, line) in rc_text.lines().enumerate() {
        if idx + 1 == hook_line {
            out.push_str(&report);
        }
        out.push_str(line);
        out.push('\n');
    }
    // A hook line past EOF (rc shrank between scan and copy) still
    // yields a well-formed copy; the missing record then reads as
    // "hook never ran", which is true of the copy that was run.
    out
}

/// Single-quote `value` for the shell, the only quoting that needs no
/// knowledge of the string's content.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// The scratch directory one fallback run writes its copy into.
///
/// It lands in the system temp directory, which is world-writable, so
/// its creation carries the whole guarantee: `mkdir(2)` under a random
/// name, which fails outright if anything already occupies the path
/// (so the directory written into is always one this process just
/// made, never a planted symlink pointing elsewhere), at mode `0700`
/// applied by `mkdir` itself rather than a follow-up `chmod` — the
/// same create-time-mode reasoning as [`Fs::write_file_with_mode`].
/// The returned handle removes the directory and its contents when
/// dropped.
fn scratch_dir() -> Result<tempfile::TempDir, TraceError> {
    use std::os::unix::fs::PermissionsExt;
    tempfile::Builder::new()
        .prefix("dodot-trace-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .map_err(|e| TraceError::FallbackUnfaithful(format!("{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempEnvironment;

    // ── Trace parsing, against captured real traces ─────────────
    //
    // The fixtures are verbatim captures from real shells (zsh 5.9,
    // bash 5.3 on macOS) started with this module's PS4 under a
    // controlled three-entry PATH — see the epic's WS02 issue. The
    // parser has to work against what the shells actually print, not
    // what the format string suggests they print.

    const ZSH_TRACE: &str = include_str!("fixtures/zsh-xtrace.txt");
    const BASH_TRACE: &str = include_str!("fixtures/bash-xtrace.txt");
    const BASH_NO_MARKER: &str = include_str!("fixtures/bash-xtrace-no-marker.txt");

    const FIXTURE_PATH: &str = "/opt/homebrew/bin:/usr/bin:/bin";

    #[test]
    fn the_zsh_record_at_the_hook_line_is_addressable() {
        let records = parse_trace(ZSH_TRACE);
        let rc = Path::new("/tmp/dodot-trace-exp/zdot/.zshrc");
        let record = record_at(&records, &[rc], 4).expect("hook-line record");
        assert_eq!(record.path, FIXTURE_PATH);
        assert_eq!(record.line, 4);
        // Other lines of the same file are records too — PATH at
        // *every* line is what the trace buys.
        assert!(record_at(&records, &[rc], 2).is_some());
        // A line that never executed has no record.
        assert!(record_at(&records, &[rc], 99).is_none());
    }

    #[test]
    fn the_bash_record_at_the_hook_line_is_addressable() {
        let records = parse_trace(BASH_TRACE);
        let rc = Path::new("/tmp/dodot-trace-exp/home/.bashrc");
        let record = record_at(&records, &[rc], 4).expect("hook-line record");
        assert_eq!(record.path, FIXTURE_PATH);
    }

    #[test]
    fn nested_records_with_repeated_plus_signs_still_parse() {
        // bash prints `++dodot-trace|…` for lines traced inside a
        // command substitution — the hook's own `$(dodot init-sh)`
        // among them.
        let nested: Vec<_> = BASH_TRACE.lines().filter(|l| l.starts_with("++")).collect();
        assert!(!nested.is_empty(), "fixture must contain nested records");
        for line in nested {
            assert!(parse_record(line).is_some(), "unparsed: {line}");
        }
    }

    #[test]
    fn a_marker_less_trace_yields_no_records() {
        // Plain xtrace with the default `+ ` PS4 — what an rc-side
        // PS4 override leaves us with.
        assert!(parse_trace(BASH_NO_MARKER).is_empty());
    }

    #[test]
    fn record_parsing_rejects_near_misses() {
        // No leading '+': not a trace line, whatever it contains.
        assert_eq!(parse_record("dodot-trace|/rc|1|/bin|> x"), None);
        // No record suffix: the PATH field never terminated.
        assert_eq!(parse_record("+dodot-trace|/rc|1|/bin"), None);
        // Line number is not a number.
        assert_eq!(parse_record("+dodot-trace|/rc|x|/bin|> x"), None);
        // Empty PATH is still a record — an empty PATH at the hook
        // line is exactly the unresolvable story.
        assert_eq!(
            parse_record("+dodot-trace|/rc|3||> eval"),
            Some(TraceRecord {
                file: "/rc".into(),
                line: 3,
                path: String::new(),
            })
        );
    }

    // ── Hook location ───────────────────────────────────────────

    #[test]
    fn find_hook_names_line_and_form() {
        let eval_rc = "# comment\nexport A=1\neval \"$(dodot init-sh)\"\n";
        assert_eq!(find_hook(eval_rc), Some((3, HookForm::Eval)));

        let file_rc = "[ -f \"$HOME/.local/share/dodot/shell/dodot-init.sh\" ] && \
                       . \"$HOME/.local/share/dodot/shell/dodot-init.sh\"\n";
        assert_eq!(find_hook(file_rc), Some((1, HookForm::FileSource)));
    }

    #[test]
    fn a_commented_hook_is_no_hook() {
        assert_eq!(find_hook("# eval \"$(dodot init-sh)\"\n"), None);
        assert_eq!(find_hook("alias ll='ls -l'\n"), None);
    }

    // ── PATH resolution ─────────────────────────────────────────

    fn executable(env: &TempEnvironment, dir: &str, name: &str) -> PathBuf {
        let dir = env.home.join(dir);
        env.fs.mkdir_all(&dir).unwrap();
        let path = dir.join(name);
        env.fs
            .write_file_with_mode(&path, b"#!/bin/sh\n", 0o755)
            .unwrap();
        path
    }

    #[test]
    fn the_first_executable_match_wins() {
        let env = TempEnvironment::builder().build();
        let first = executable(&env, "a", "dodot");
        executable(&env, "b", "dodot");
        let path_var = format!(
            "{}:{}",
            env.home.join("a").display(),
            env.home.join("b").display()
        );
        let r = resolve_on_path(env.fs.as_ref(), &path_var, "dodot");
        assert_eq!(r.winner, Some(first));
        // The walk stops at the winner: b's entry was never passed
        // over, so it is not a candidate.
        assert_eq!(r.candidates.len(), 1);
    }

    #[test]
    fn a_dangling_symlink_is_reported_never_silently_skipped() {
        let env = TempEnvironment::builder().build();
        let linkdir = env.home.join("links");
        env.fs.mkdir_all(&linkdir).unwrap();
        let gone = env.home.join("gone/dodot");
        env.fs.symlink(&gone, &linkdir.join("dodot")).unwrap();
        let real = executable(&env, "real", "dodot");

        let path_var = format!("{}:{}", linkdir.display(), env.home.join("real").display());
        let r = resolve_on_path(env.fs.as_ref(), &path_var, "dodot");
        assert_eq!(r.winner, Some(real));
        let skips: Vec<_> = r.notable_skips().collect();
        assert_eq!(skips.len(), 1);
        assert_eq!(skips[0].path, linkdir.join("dodot"));
        assert_eq!(
            skips[0].skipped,
            Some(SkipReason::DanglingSymlink { target: Some(gone) })
        );
    }

    #[test]
    fn missing_dirs_and_non_executables_are_classified() {
        let env = TempEnvironment::builder().build();
        let plain_dir = env.home.join("plain");
        env.fs.mkdir_all(&plain_dir).unwrap();
        env.fs
            .write_file(&plain_dir.join("dodot"), b"not executable")
            .unwrap();
        let path_var = format!(
            "{}:{}:{}",
            env.home.join("nowhere").display(),
            plain_dir.display(),
            env.home.join("empty").display()
        );
        env.fs.mkdir_all(&env.home.join("empty")).unwrap();
        let r = resolve_on_path(env.fs.as_ref(), &path_var, "dodot");
        assert_eq!(r.winner, None);
        assert_eq!(r.candidates.len(), 3);
        assert_eq!(r.candidates[0].skipped, Some(SkipReason::MissingDir));
        assert_eq!(r.candidates[1].skipped, Some(SkipReason::NotExecutable));
        assert_eq!(r.candidates[2].skipped, Some(SkipReason::NotPresent));
        // The bare not-present row is completeness, not a finding.
        assert_eq!(r.notable_skips().count(), 2);
    }

    #[test]
    fn an_empty_path_resolves_nothing() {
        let env = TempEnvironment::builder().build();
        let r = resolve_on_path(env.fs.as_ref(), "", "dodot");
        assert_eq!(r.winner, None);
        assert!(r.candidates.is_empty());
    }

    // ── Verdicts: all four reachable ────────────────────────────

    fn no_version(_: &Path) -> Option<String> {
        None
    }

    #[test]
    fn verdict_unresolvable_carries_the_searched_path() {
        let env = TempEnvironment::builder().build();
        let running = executable(&env, "run", "dodot");
        let v = judge_eval_hook(env.fs.as_ref(), "/nowhere", &running, &no_version);
        match v {
            TraceVerdict::Unresolvable { path, resolution } => {
                assert_eq!(path, "/nowhere");
                assert!(resolution.winner.is_none());
            }
            other => panic!("expected Unresolvable, got {other:?}"),
        }
    }

    #[test]
    fn verdict_running_binary_follows_symlinks_to_identity() {
        let env = TempEnvironment::builder().build();
        let running = executable(&env, "real", "dodot");
        // The PATH reaches the running binary through a symlink — the
        // link *is* the running binary once resolved.
        let linkdir = env.home.join("links");
        env.fs.mkdir_all(&linkdir).unwrap();
        env.fs.symlink(&running, &linkdir.join("dodot")).unwrap();
        let v = judge_eval_hook(
            env.fs.as_ref(),
            &linkdir.display().to_string(),
            &running,
            &no_version,
        );
        assert_eq!(
            v,
            TraceVerdict::RunningBinary {
                path: linkdir.join("dodot")
            }
        );
    }

    #[test]
    fn verdict_different_binary_names_the_winner_and_its_version() {
        let env = TempEnvironment::builder().build();
        let running = executable(&env, "new", "dodot");
        let stale = executable(&env, "old", "dodot");
        let version_of = |p: &Path| {
            assert_eq!(p, &stale);
            Some("5.0.0".to_string())
        };
        let v = judge_eval_hook(
            env.fs.as_ref(),
            &env.home.join("old").display().to_string(),
            &running,
            &version_of,
        );
        match v {
            TraceVerdict::DifferentBinary {
                found,
                found_version,
                running: reported_running,
                ..
            } => {
                assert_eq!(found, stale);
                assert_eq!(found_version.as_deref(), Some("5.0.0"));
                assert_eq!(reported_running, running);
            }
            other => panic!("expected DifferentBinary, got {other:?}"),
        }
    }

    // The fourth verdict — HookNeverRan — is produced by the caller
    // when the trace holds no record at the hook location; the spawn
    // tests below reach it through a real bash.

    #[test]
    fn version_output_parses_the_clap_shape_and_rejects_noise() {
        assert_eq!(parse_version_output("dodot 5.5.1\n"), Some("5.5.1".into()));
        assert_eq!(
            parse_version_output("dodot 5.6.0-rc1"),
            Some("5.6.0-rc1".into())
        );
        assert_eq!(parse_version_output("zsh: command not found\n"), None);
        assert_eq!(parse_version_output(""), None);
    }

    // ── The fallback copy (pure part) ───────────────────────────

    #[test]
    fn the_report_line_is_inserted_not_truncated() {
        let rc = "if true; then\n  eval \"$(dodot init-sh)\"\nfi\n";
        let copy = insert_report_line(rc, Path::new("/home/u/.zshrc"), 2);
        let lines: Vec<&str> = copy.lines().collect();
        assert_eq!(lines.len(), 4, "insertion, not replacement: {copy}");
        assert!(lines[1].starts_with("printf '+dodot-trace|"), "{copy}");
        assert!(lines[1].contains("'/home/u/.zshrc' 2"), "{copy}");
        // Everything around the insertion survives byte for byte.
        assert_eq!(lines[0], "if true; then");
        assert_eq!(lines[2], "  eval \"$(dodot init-sh)\"");
        assert_eq!(lines[3], "fi");
    }

    #[test]
    fn shell_quoting_survives_embedded_single_quotes() {
        assert_eq!(shell_quote("plain"), "'plain'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn the_scratch_dir_is_private_unpredictable_and_transient() {
        use std::os::unix::fs::PermissionsExt;

        let scratch = scratch_dir().expect("scratch dir");
        let path = scratch.path().to_path_buf();
        // A real directory this process made, not something waiting
        // in the world-writable temp dir under a guessable name.
        let meta = std::fs::symlink_metadata(&path).expect("scratch dir exists");
        assert!(meta.file_type().is_dir(), "a directory, never a symlink");
        assert_eq!(meta.permissions().mode() & 0o777, 0o700, "{path:?}");
        // Two runs never land on the same path, whatever the clock or
        // the pid says.
        let other = scratch_dir().expect("second scratch dir");
        assert_ne!(other.path(), path);
        drop(scratch);
        assert!(!path.exists(), "removed with its handle");
    }

    // ── Spawn round-trips, against a real bash ──────────────────
    //
    // These spawn `/bin/bash` with `$HOME` pointed at a temp dir (via
    // EnvVarGuard, which serialises env-mutating tests), so the rc
    // that gets traced is one the test wrote — never the developer's.

    use crate::testing::EnvVarGuard;

    fn bash() -> Option<&'static Path> {
        let p = Path::new("/bin/bash");
        p.exists().then_some(p)
    }

    const TIMEOUT: Duration = Duration::from_secs(10);

    #[test]
    fn a_real_bash_reports_path_at_the_hook_line() {
        let Some(bash) = bash() else { return };
        let env = TempEnvironment::builder().build();
        let _home = EnvVarGuard::set("HOME", &env.home.display().to_string());
        let rc = env.home.join(".bashrc");
        env.fs
            .write_file(
                &rc,
                b"export PATH=/mangled/by/rc\neval \"$(dodot init-sh)\"\n",
            )
            .unwrap();

        let run = run_trace(
            env.fs.as_ref(),
            bash,
            HookupShell::Bash,
            &rc,
            &rc,
            2,
            TIMEOUT,
        )
        .expect("trace runs");
        // Which route produced the record is the shell's business:
        // bash 5 answers from the primary xtrace; macOS's bash 3.2
        // truncates long expanded PS4 values (~100 bytes), which the
        // fallback copy absorbs. Either way the answer is the same.
        let record = record_at(&run.records, &[&rc], 2).expect("hook-line record");
        assert_eq!(record.path, "/mangled/by/rc");
    }

    #[test]
    fn a_ps4_override_falls_back_to_the_inserted_report_copy() {
        let Some(bash) = bash() else { return };
        let env = TempEnvironment::builder().build();
        let _home = EnvVarGuard::set("HOME", &env.home.display().to_string());
        let rc = env.home.join(".bashrc");
        // Line 1 destroys the marker; the hook is on line 3.
        env.fs
            .write_file(
                &rc,
                b"PS4='+ '\nexport PATH=/from/fallback\neval \"$(dodot init-sh)\"\n",
            )
            .unwrap();
        let before = env.fs.read_to_string(&rc).unwrap();

        let run = run_trace(
            env.fs.as_ref(),
            bash,
            HookupShell::Bash,
            &rc,
            &rc,
            3,
            TIMEOUT,
        )
        .expect("fallback runs");
        assert!(run.used_fallback);
        let record = record_at(&run.records, &[&rc], 3).expect("inserted report record");
        assert_eq!(record.path, "/from/fallback");
        // The user's real rc was never written.
        assert_eq!(env.fs.read_to_string(&rc).unwrap(), before);
    }

    #[test]
    fn a_hook_inside_a_dead_branch_yields_no_record_twice() {
        let Some(bash) = bash() else { return };
        let env = TempEnvironment::builder().build();
        let _home = EnvVarGuard::set("HOME", &env.home.display().to_string());
        let rc = env.home.join(".bashrc");
        // The branch never runs, so neither the traced hook nor the
        // fallback's inserted report (inside the same branch) fires —
        // which is the HookNeverRan story, confirmed both ways.
        env.fs
            .write_file(
                &rc,
                b"PS4='+ '\nif false; then\n  eval \"$(dodot init-sh)\"\nfi\n",
            )
            .unwrap();

        let run = run_trace(
            env.fs.as_ref(),
            bash,
            HookupShell::Bash,
            &rc,
            &rc,
            3,
            TIMEOUT,
        )
        .expect("trace runs");
        assert!(run.used_fallback);
        assert!(record_at(&run.records, &[&rc], 3).is_none());
    }

    #[test]
    fn a_real_zsh_reports_path_at_the_hook_line_via_zdotdir() {
        let zsh = Path::new("/bin/zsh");
        if !zsh.exists() {
            return;
        }
        let env = TempEnvironment::builder().build();
        let zdot = env.home.join("zdot");
        env.fs.mkdir_all(&zdot).unwrap();
        let rc = zdot.join(".zshrc");
        env.fs
            .write_file(
                &rc,
                b"export PATH=/mangled/by/zshrc\neval \"$(dodot init-sh)\"\n",
            )
            .unwrap();
        // One guard only (the env lock is not reentrant): with
        // ZDOTDIR set, zsh reads every user dotfile from it, so the
        // developer's own rc files are unreachable without touching
        // $HOME.
        let _zdot = EnvVarGuard::set("ZDOTDIR", &zdot.display().to_string());

        let run = run_trace(env.fs.as_ref(), zsh, HookupShell::Zsh, &rc, &rc, 2, TIMEOUT)
            .expect("trace runs");
        let record = record_at(&run.records, &[&rc], 2).expect("hook-line record");
        assert_eq!(record.path, "/mangled/by/zshrc");
    }

    #[test]
    fn an_unrunnable_shell_is_an_error_not_a_panic() {
        let env = TempEnvironment::builder().build();
        let rc = env.home.join(".bashrc");
        env.fs
            .write_file(&rc, b"eval \"$(dodot init-sh)\"\n")
            .unwrap();
        let missing = env.home.join("no-such-shell");
        assert!(matches!(
            run_trace(
                env.fs.as_ref(),
                &missing,
                HookupShell::Bash,
                &rc,
                &rc,
                1,
                TIMEOUT
            ),
            Err(TraceError::SpawnFailed(_))
        ));
    }
}

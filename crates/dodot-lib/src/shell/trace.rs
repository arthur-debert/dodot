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
//! `/bin/bash`), which truncates expanded `PS4` values at 100 bytes,
//! severing the record before its suffix. Carrying `PWD` as well as
//! `PATH` puts a typical record past that limit, so on macOS's system
//! bash the fallback is the normal route rather than the exception —
//! the report line it inserts is a `printf`, not a prompt, and is
//! never truncated. Both degrade the same way. The fallback works on a *copy*: a
//! temporary `ZDOTDIR` (zsh) or `--rcfile` (bash) holding the rc with
//! a report line **inserted** before the hook — insertion, not
//! truncation, so the file stays parseable inside any conditional or
//! function. For zsh the scratch directory also carries a `.zshenv`
//! that replays the real two-stage startup — the user's own first-stage
//! `.zshenv` sourced from where it really lives, then `ZDOTDIR` moved
//! to the scratch copy for stage two and restored inside it (see
//! [`zshenv_stage_one`]). The user's real files are never written; the
//! copy lives in a scratch directory created exclusively under a random
//! name at mode `0700` (see [`scratch_dir`]) and removed when the run
//! ends.
//!
//! For capability-unknown hooks, the copy prints the hook-line record
//! and kills the diagnostic shell before the hook itself. That is the
//! only trace shape that cannot create heartbeat/profile evidence
//! through a legacy init script that does not understand diagnostic
//! suppression.
//!
//! The copy must reproduce the shell's startup, not approximate it. A
//! shell that read a different set of files than the real one does
//! yields a verdict the user would act on and should not — so a copy
//! that cannot be built faithfully ends the trace
//! ([`TraceError::FallbackUnfaithful`]) and the command reports "could
//! not trace" instead of a measurement. That covers the copy the
//! shell *reads* as well as the files it opens: the inserted report
//! line can cost a `case` pattern or a heredoc its parse, so both the
//! original and the copy go through `<shell> -n` first
//! ([`insertion_broke_the_parse`]).
//!
//! # Known fidelity limits
//!
//! Two things about the traced shell differ from the real one, both
//! unavoidable and neither silent:
//!
//! - **Option state.** Tracing needs `xtrace` (and, for zsh,
//!   `promptsubst`), which a normal startup has off. An rc that
//!   branches on `$-` or on `setopt` output therefore takes a
//!   different path under the trace than it does in earnest. Reading
//!   `PATH` at the hook line is what this buys, and there is no way to
//!   buy it without the option set.
//! - **The rc runs up to twice for diagnostic-capable hooks.** The
//!   fallback re-runs it on a copy, so side effects before the hook
//!   can happen again. Capability-unknown hooks use only the copy and
//!   exit before the hook. [`announcement`] says the upper bound
//!   before anything is spawned.
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
use std::time::{Duration, Instant};

use crate::fs::Fs;
use crate::shell::activation::{self, EvidenceVersion};
use crate::shell::probe::{spawn_captured, SpawnOutcome};
use crate::shell::rc::{self, HookupShell};

/// The field prefix every trace record carries, behind the leading
/// `+`(s) the shell prepends per nesting depth.
pub const TRACE_MARKER: &str = "dodot-trace|";

/// Terminates the PATH field of a record, separating it from the
/// traced command text that follows on the same line.
pub const RECORD_SUFFIX: &str = "|> ";

/// Internal environment flag carried by `probe shell-init --trace-hook`.
/// Current init scripts consume it before pack contributions and use it
/// only to suppress heartbeat and profile writes while the diagnostic
/// shell continues through PATH and source work.
pub const DIAGNOSTIC_TRACE_ENV: &str = "DODOT_INTERNAL_SHELL_INIT_TRACE";

const REPORT_ONLY_TERMINATION: &str =
    "\\command kill -KILL \"$$\" 2>/dev/null || \\builtin kill -KILL \"$$\" 2>/dev/null || kill -KILL \"$$\"";

// ── The PS4 contract ────────────────────────────────────────────

/// The `PS4` value handed to the spawned shell.
///
/// zsh expands prompt escapes (`%N` = file being sourced, `%i` = line
/// within it) natively but needs `PROMPT_SUBST` for the `$PWD` and
/// `$PATH` expansions — set on the command line (see [`trace_args`]),
/// never in the user's files. bash expands parameters in `PS4` by
/// default.
///
/// `PWD` rides along because a `PATH` is not a set of locations on its
/// own: an empty entry means the working directory and a relative one
/// is resolved against it, so the same `PATH` string picks a different
/// binary depending on where the rc had `cd`'d to by the hook line
/// ([`resolve_on_path`]). `PATH` stays last so it remains the field
/// terminated by [`RECORD_SUFFIX`].
pub fn ps4(shell: HookupShell) -> String {
    match shell {
        HookupShell::Zsh => format!("+{TRACE_MARKER}%N|%i|$PWD|$PATH{RECORD_SUFFIX}"),
        HookupShell::Bash => {
            format!("+{TRACE_MARKER}${{BASH_SOURCE}}|${{LINENO}}|${{PWD}}|${{PATH}}{RECORD_SUFFIX}")
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
/// and what `PWD` and `PATH` held when that line ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceRecord {
    pub file: String,
    pub line: usize,
    /// The shell's working directory at that line — the directory an
    /// empty or relative `PATH` entry resolves against.
    pub cwd: String,
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
    let (cwd, rest) = rest.split_once('|')?;
    // PATH runs to the record suffix; the traced command follows. A
    // PATH containing the suffix itself would truncate early — a
    // pathological name this deliberately does not chase.
    let path = &rest[..rest.find(RECORD_SUFFIX)?];
    Some(TraceRecord {
        file: file.to_string(),
        line: line_no,
        cwd: cwd.to_string(),
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
/// whether the script *that line names* exists, no PATH involved — the
/// structural reason the file-source form is the recommended one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookForm {
    /// `eval "$(dodot init-sh)"` — resolution rides on PATH.
    Eval,
    /// `. …/dodot-init.sh` — a fixed path, carried here as the line
    /// itself gives it.
    FileSource(SourcedScript),
}

/// The script a file-source hook line sources.
///
/// Carried out of [`find_hook`] instead of recomputed downstream:
/// hook *recognition* is loose by design (any uncommented line
/// mentioning `dodot-init.sh`, matching [`rc::scan_hook`]), so the
/// line dodot found may well source a copy somewhere other than the
/// one `dodot up` writes — a stale one, or one that is gone. Judging
/// the datastore's path instead would report that hookup sound on the
/// strength of a file the line never mentions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourcedScript {
    /// The exact path the line sources, `$HOME` expanded.
    Path(PathBuf),
    /// The line sources something dodot will not resolve without
    /// interpreting shell — another variable, a command substitution,
    /// a relative path whose directory is the shell's business. The
    /// raw word is kept for the report; no verdict is drawn from it.
    Unresolved { raw: String },
}

/// Where the hook sits in an rc file, and what it does there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hook {
    /// 1-indexed line number — the address the trace record is looked
    /// up by.
    pub line: usize,
    /// Which shape the line takes, and for the file-source shape what
    /// it sources.
    pub form: HookForm,
}

/// Locate the hook in rc text.
///
/// The same recognition [`rc::scan_hook`] uses, plus the location and,
/// for the file-source form, the path the line actually sources: the
/// first uncommented line mentioning either hook form. A commented-out
/// hook is exactly the broken-hookup story and must not count.
///
/// `home` expands the one variable form dodot itself writes and users
/// copy — see [`expand_hook_path`] for why that is where the expansion
/// stops.
pub fn find_hook(text: &str, home: &Path) -> Option<Hook> {
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.starts_with('#') {
            continue;
        }
        let form = if line.contains("dodot init-sh") {
            HookForm::Eval
        } else if line.contains("dodot-init.sh") {
            HookForm::FileSource(sourced_script(line, home))
        } else {
            continue;
        };
        return Some(Hook {
            line: idx + 1,
            form,
        });
    }
    None
}

/// The script a file-source hook line sources, or why dodot will not
/// say.
///
/// The argument to the line's `.` / `source` builtin, never the first
/// path-looking word on it: dodot's own hook line names the same path
/// twice, once inside a `[ -f … ]` guard and once as the thing being
/// sourced, and only the second is what the shell reads.
fn sourced_script(line: &str, home: &Path) -> SourcedScript {
    let Some(raw) = source_argument(line) else {
        return SourcedScript::Unresolved {
            raw: line.to_string(),
        };
    };
    match expand_hook_path(&raw, home) {
        Some(path) => SourcedScript::Path(path),
        None => SourcedScript::Unresolved { raw },
    }
}

/// The word after the line's `.` or `source` builtin, unquoted.
fn source_argument(line: &str) -> Option<String> {
    let mut rest = line;
    loop {
        let (word, after) = next_word(rest)?;
        if word == "." || word == "source" {
            return next_word(after).map(|(argument, _)| argument);
        }
        rest = after;
    }
}

/// Split one shell word off the front of `rest`, honouring a single
/// layer of quoting, and return it beside what follows.
///
/// Enough to read the argument of a `source` line and no more, and it
/// says so by refusing the rest: a word that opens a quote it never
/// closes, or that glues a quoted part to an unquoted one
/// (`"$HOME"/x`), yields `None`. The caller lands on
/// [`SourcedScript::Unresolved`], which is the right answer — half a
/// path is not a path, and dodot is not writing a shell parser.
fn next_word(rest: &str) -> Option<(String, &str)> {
    let rest = rest.trim_start();
    if rest.is_empty() {
        return None;
    }
    for quote in ['"', '\''] {
        if let Some(body) = rest.strip_prefix(quote) {
            let end = body.find(quote)?;
            let after = &body[end + 1..];
            let word_ends_here = after.is_empty() || after.starts_with(char::is_whitespace);
            return word_ends_here.then(|| (body[..end].to_string(), after));
        }
    }
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    Some((rest[..end].to_string(), &rest[end..]))
}

/// Anything in a hook line's path that would need a running shell to
/// resolve: a variable, a substitution, a glob, or a separator the
/// word split would otherwise have swallowed into the path.
const NEEDS_A_SHELL: [char; 10] = ['$', '`', '*', '?', '~', ';', '&', '|', '<', '>'];

/// Expand a hook line's script path, or `None` when resolving it would
/// mean guessing.
///
/// `$HOME`, `${HOME}` and `~` are expanded because they are what
/// dodot's own hook line uses and what a hand-wired one copies.
/// Everything else is refused rather than approximated: the characters
/// in [`NEEDS_A_SHELL`] need a shell to resolve, and a relative path
/// needs the working directory of a shell that is not running yet. A
/// refusal costs the user one honest "could not tell"; a guess costs
/// them a verdict about a file the hook does not source.
///
/// The refusal is applied to the part that came off the line, never to
/// `home` — dodot resolved that itself, and a home directory with an
/// unusual character in it is not a line dodot cannot read.
fn expand_hook_path(raw: &str, home: &Path) -> Option<PathBuf> {
    let under_home = ["${HOME}", "$HOME", "~"]
        .iter()
        .find_map(|prefix| raw.strip_prefix(prefix));
    let rest = under_home.map_or(raw, |rest| rest.trim_start_matches('/'));
    if rest.contains(NEEDS_A_SHELL) {
        return None;
    }
    match under_home {
        Some(_) => Some(home.join(rest)),
        None => Path::new(rest).is_absolute().then(|| PathBuf::from(rest)),
    }
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
    /// The candidates worth telling the user about: entries where a
    /// `dodot` is present in some form and still lost the search.
    ///
    /// [`SkipReason::NotPresent`] is omitted because "no dodot in
    /// /usr/bin" is not a finding, and
    /// [`SkipReason::MissingDir`] is omitted for the same reason:
    /// an ordinary macOS `PATH` carries several directories that do
    /// not exist, so listing them costs six noise lines with the
    /// dangling-symlink row — the one line this whole walk exists to
    /// surface — buried in the middle. A report nobody reads to the
    /// end has the same value as no report.
    pub fn notable_skips(&self) -> impl Iterator<Item = &Candidate> {
        self.candidates.iter().filter(|c| {
            matches!(
                c.skipped,
                Some(SkipReason::DanglingSymlink { .. }) | Some(SkipReason::NotExecutable)
            )
        })
    }
}

/// Resolve `binary` against a `PATH` string the way the shell would —
/// first executable match wins — but reporting what the shell skips
/// silently. Pure over the injected [`Fs`]. The walk stops at the
/// winner: entries after it were never passed over.
///
/// `cwd` is the working directory at the traced line, and it is part
/// of the lookup rather than a nicety: an **empty** `PATH` component
/// means the working directory to every shell that implements the
/// POSIX rule — measured on bash 5 and zsh 5.9, for a leading,
/// trailing, doubled *and* wholly empty `PATH` — and a **relative**
/// component is resolved against it too. Dropping the empty entries
/// and reading the relative ones against dodot's own directory would
/// answer for a search the traced shell never performed, which is the
/// diagnostic being wrong in exactly the way it exists to catch.
pub fn resolve_on_path(fs: &dyn Fs, path_var: &str, cwd: &Path, binary: &str) -> Resolution {
    let mut candidates = Vec::new();
    let mut winner = None;
    for entry in path_var.split(':') {
        let dir = match entry {
            "" => cwd.to_path_buf(),
            other if Path::new(other).is_absolute() => PathBuf::from(other),
            relative => cwd.join(relative),
        };
        let candidate = dir.join(binary);
        let skipped = classify(fs, &dir, &candidate);
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
    /// File-source hook whose script path names nothing sourceable —
    /// `dodot up` regenerates the one it owns.
    ScriptMissing { script: PathBuf },
    /// File-source hook sourcing a script this dodot wrote, at the
    /// current generation: sound.
    ScriptPresent { script: PathBuf },
    /// File-source hook sourcing a real init script written by a
    /// *different* dodot — the epic's motivating failure, in the hook
    /// form the docs recommend.
    ScriptSkewed {
        script: PathBuf,
        version: EvidenceVersion,
    },
    /// File-source hook sourcing an init script older than the one
    /// `dodot up` maintains: a copy left behind somewhere.
    ScriptStale {
        script: PathBuf,
        found: u64,
        expected: u64,
    },
    /// File-source hook whose script exists but cannot be identified —
    /// unreadable, or carrying no dodot stamp at all. Not a judgment:
    /// "there is a file there" is a fact about the filesystem, and
    /// "the hookup is sound" is a claim about activation.
    ScriptUnverified { script: PathBuf, reason: String },
    /// File-source hook whose sourced path dodot declines to resolve
    /// (see [`SourcedScript::Unresolved`]). Deliberately not a
    /// judgment: an honest "could not tell" is the only answer a line
    /// dodot cannot read supports.
    ScriptUnresolved { raw: String },
}

/// Judge a file-source hook from the script the hook line names.
///
/// Two separate corrections live here, and they are the same one.
///
/// The path comes from the line, never from
/// [`Pather::init_script_path`](crate::paths::Pather::init_script_path):
/// a hand-wired hook may source a stale copy elsewhere while the
/// datastore's script sits there perfectly intact, and checking the
/// path dodot *would* have written reports that hookup sound on
/// evidence about a different file.
///
/// And the file's *existence* is not the verdict either. A regular
/// file at that path is a filesystem fact; "the hookup is sound" is a
/// claim about which dodot the user's shells activate. Deciding the
/// second from the first certified an init script written by dodot
/// 5.0.0 as sound while the footer on the same machine — reading the
/// heartbeat that same script had written — correctly reported version
/// skew. So the script is read and identified by the two fields every
/// other signal carries, then judged by the two rules every other
/// signal is judged by, in the same order: [`activation::is_skewed`]
/// first, then [`activation::classify_stamp`] against the generation
/// `dodot up` last wrote. Unreadable or unstamped is neither sound nor
/// broken — it is unverified.
pub fn judge_file_source_hook(
    fs: &dyn Fs,
    script: PathBuf,
    reference: Option<u64>,
    running: &str,
) -> TraceVerdict {
    // A directory or a dangling link is not something `.` can source.
    if !matches!(fs.stat(&script), Ok(meta) if meta.is_file) {
        return TraceVerdict::ScriptMissing { script };
    }
    let Ok(text) = fs.read_to_string(&script) else {
        return TraceVerdict::ScriptUnverified {
            script,
            reason: "it could not be read".into(),
        };
    };
    let Some(found) = activation::parse_script_generation(&text) else {
        return TraceVerdict::ScriptUnverified {
            script,
            reason: "it carries no dodot activation stamp, so it is not a script dodot generated"
                .into(),
        };
    };
    let version = EvidenceVersion::from_field(activation::parse_script_version(&text).as_deref());
    if activation::is_skewed(Some(&version), running) {
        return TraceVerdict::ScriptSkewed { script, version };
    }
    match activation::classify_stamp(Some(found), reference) {
        activation::StampState::Current => TraceVerdict::ScriptPresent { script },
        _ => TraceVerdict::ScriptStale {
            script,
            found,
            expected: reference.unwrap_or(found),
        },
    }
}

/// Judge an `eval`-form hook from the trace record at its line.
///
/// The whole record rather than its `path` field alone: the search
/// depends on the working directory as much as on `PATH`
/// ([`resolve_on_path`]), and taking both from one record is what keeps
/// them from being read off two different moments of the startup.
///
/// `version_of` is how the winner's version is learned — injected so
/// the judgment stays pure; production executes `<winner> --version`
/// under the probe envelope, tests hand back canned strings. Binary
/// identity is judged on symlink-resolved paths: a `dodot` reached
/// through a link *is* the running binary if the link lands on it.
pub fn judge_eval_hook(
    fs: &dyn Fs,
    record: &TraceRecord,
    running_exe: &Path,
    version_of: &dyn Fn(&Path) -> Option<String>,
) -> TraceVerdict {
    let resolution = resolve_on_path(fs, &record.path, Path::new(&record.cwd), "dodot");
    let Some(winner) = resolution.winner.clone() else {
        return TraceVerdict::Unresolvable {
            path: record.path.clone(),
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
///
/// It says "up to twice" because that is true: the fallback re-runs
/// the rc on a copy, and on macOS's `/bin/bash` it is the normal
/// route. This line is the user's only warning before their own rc's
/// side effects happen, so promising "once" understated the thing they
/// are being warned about — and the count is not knowable until the
/// first pass comes back unreadable.
pub fn announcement(shell: HookupShell) -> String {
    format!(
        "tracing shell startup ({})… (runs your rc file, up to twice)",
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
    /// directory would not open, or one of the files that stands in
    /// for a startup file could not be written. A verdict computed
    /// from a shell that read a different set of files than the real
    /// one does is worse than no verdict, because the user acts on it.
    FallbackUnfaithful(String),
}

/// One completed trace: the records read back, and whether the
/// fallback copy had to be used to get them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceRun {
    pub records: Vec<TraceRecord>,
    pub used_fallback: bool,
    pub elapsed_us: u64,
}

/// Everything one trace needs to know about the shell it is
/// reproducing.
#[derive(Debug, Clone, Copy)]
pub struct TraceRequest<'a> {
    /// The shell binary to spawn.
    pub shell_path: &'a Path,
    pub shell: HookupShell,
    /// `$HOME` — where zsh looks for the *first* file it reads,
    /// `.zshenv`, when the environment exports no `$ZDOTDIR`.
    pub home: &'a Path,
    /// `$ZDOTDIR` exactly as the shell would inherit it, when the
    /// environment carries one. Only the fallback consults it, and
    /// only for zsh — see [`run_fallback`].
    pub zdotdir: Option<&'a str>,
    /// The path the shell opens (what zsh's `%N` / bash's
    /// `BASH_SOURCE` will name).
    pub rc_nominal: &'a Path,
    /// The symlink-resolved file that holds the bytes.
    pub rc_resolved: &'a Path,
    /// 1-indexed line the hook sits on.
    pub hook_line: usize,
    pub timeout: Duration,
    pub execution: TraceExecution,
}

impl TraceRequest<'_> {
    /// The two paths a record at the hook line may be addressed by.
    fn rc_paths(&self) -> [&Path; 2] {
        [self.rc_nominal, self.rc_resolved]
    }
}

/// Whether it is safe to run the real hook during a trace.
///
/// Current generated scripts understand [`DIAGNOSTIC_TRACE_ENV`], so
/// primary tracing can run the hook without creating heartbeat/profile
/// evidence. Capability-unknown hooks are traced through the temporary
/// copy only, with a report line that exits before the hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceExecution {
    DiagnosticSupported,
    ReportOnly,
}

/// Spawn the shell under xtrace and read the hook-line records back,
/// falling back to the inserted-report copy when the primary run
/// yields no record at the hook's location.
///
/// Records are matched against both of the request's rc paths. The
/// fallback runs whenever the primary record is missing — an rc-side
/// `PS4` override can strip the marker from any suffix of the file,
/// so absence alone cannot distinguish "hook never ran" from "marker
/// lost"; the fallback's inserted report can, because it fires exactly
/// when the hook line would be reached. Capability-unknown hooks skip
/// the primary run and use that copy in report-only mode so a legacy
/// init script cannot write activation evidence before dodot has the
/// PATH answer.
pub fn run_trace(fs: &dyn Fs, req: &TraceRequest) -> Result<TraceRun, TraceError> {
    let started = Instant::now();
    if req.execution == TraceExecution::ReportOnly {
        return run_fallback(fs, req, true).map(|records| TraceRun {
            records,
            used_fallback: true,
            elapsed_us: elapsed_us(started),
        });
    }

    let mut command = Command::new(req.shell_path);
    command
        .env(DIAGNOSTIC_TRACE_ENV, "1")
        .env("PS4", ps4(req.shell))
        .args(trace_args(req.shell));
    let records = match spawn_captured(command, req.timeout) {
        SpawnOutcome::TimedOut => return Err(TraceError::TimedOut),
        SpawnOutcome::SpawnFailed(e) => return Err(TraceError::SpawnFailed(e)),
        SpawnOutcome::Finished(capture) => parse_trace(&capture.stderr),
    };
    if record_at(&records, &req.rc_paths(), req.hook_line).is_some() {
        return Ok(TraceRun {
            records,
            used_fallback: false,
            elapsed_us: elapsed_us(started),
        });
    }
    run_fallback(fs, req, false).map(|records| TraceRun {
        records,
        used_fallback: true,
        elapsed_us: elapsed_us(started),
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
    req: &TraceRequest,
    exit_before_hook: bool,
) -> Result<Vec<TraceRecord>, TraceError> {
    let rc_text = fs
        .read_to_string(req.rc_resolved)
        .map_err(|e| TraceError::RcUnreadable(format!("{e}")))?;
    // Dropped at the end of this function — every exit path, the
    // timeout and the error paths included, takes the copy with it.
    let scratch = scratch_dir()?;
    let temp = scratch.path();
    let unfaithful = |e: crate::DodotError| TraceError::FallbackUnfaithful(format!("{e}"));
    let mut command = Command::new(req.shell_path);
    if !exit_before_hook {
        command.env(DIAGNOSTIC_TRACE_ENV, "1");
    }
    let copy = insert_report_line(&rc_text, req.rc_nominal, req.hook_line, exit_before_hook);
    // The file that stands in for the user's rc — the one whose parse
    // has to survive the insertion.
    let copy_path;
    match req.shell {
        HookupShell::Zsh => {
            copy_path = temp.join(".zshrc");
            fs.write_file(&copy_path, zshrc_copy(&copy).as_bytes())
                .map_err(unfaithful)?;
            fs.write_file(
                &temp.join(".zshenv"),
                zshenv_stage_one(req, temp).as_bytes(),
            )
            .map_err(unfaithful)?;
            command.env("ZDOTDIR", temp).args(["-i", "-c", "true"]);
        }
        HookupShell::Bash => {
            copy_path = temp.join("bashrc");
            fs.write_file(&copy_path, copy.as_bytes())
                .map_err(unfaithful)?;
            command
                .arg("--rcfile")
                .arg(&copy_path)
                .args(["-i", "-c", "true"]);
        }
    }
    let copy_path = copy_path.as_path();
    if let Some(broken) = insertion_broke_the_parse(req, copy_path) {
        return Err(TraceError::FallbackUnfaithful(broken));
    }
    match spawn_captured(command, req.timeout) {
        SpawnOutcome::TimedOut => Err(TraceError::TimedOut),
        SpawnOutcome::SpawnFailed(e) => Err(TraceError::SpawnFailed(e)),
        SpawnOutcome::Finished(capture) => Ok(parse_trace(&capture.stderr)),
    }
}

/// Whether inserting the report line cost the copy its parse, and the
/// shell's complaint if so.
///
/// [`insert_report_line`] puts a whole command where the shell was
/// expecting the next line of something else, and "somewhere else" is
/// not always a statement boundary: a `case` pattern, a line
/// continuation, and the body of a heredoc all take the insertion as
/// part of themselves and stop parsing. The copy then produces no
/// record at all, which reads downstream as `hook-never-ran` — an
/// error-severity claim that the hook did not run, about a hook that
/// ran perfectly well. And on macOS's `/bin/bash` the fallback is the
/// normal route, not the exception, so that is not a rare shape.
///
/// `<shell> -n` reads a file and exits without running a line of it,
/// which is exactly the question and none of the risk. Both files are
/// checked because only a *newly introduced* error is dodot's: an rc
/// that already did not parse is the user's own breakage, still worth
/// tracing, and refusing it would replace a real finding with a
/// shrug. `None` from either check — the syntax check itself could not
/// run — is not evidence of anything and does not block the trace.
fn insertion_broke_the_parse(req: &TraceRequest, copy: &Path) -> Option<String> {
    let original = parses(req, req.rc_resolved)?;
    let copied = parses(req, copy)?;
    match (original.ok, copied.ok) {
        (true, false) => Some(format!(
            "inserting the trace's report line before {}:{} left the rc copy unparseable, so a \
             trace of it would describe a shell that never got past the insertion{}",
            req.rc_nominal.display(),
            req.hook_line,
            complaint(&copied.stderr),
        )),
        _ => None,
    }
}

/// What `<shell> -n <file>` said.
struct ParseCheck {
    ok: bool,
    stderr: String,
}

/// Ask the shell whether it can parse `file`, without running it.
///
/// `None` when the check could not be carried out — the shell would
/// not spawn, or outlived the timeout — which is a fact about the
/// check, never about the file.
fn parses(req: &TraceRequest, file: &Path) -> Option<ParseCheck> {
    let mut command = Command::new(req.shell_path);
    command.arg("-n").arg(file);
    match spawn_captured(command, req.timeout) {
        SpawnOutcome::Finished(capture) => Some(ParseCheck {
            ok: capture.status == Some(0),
            stderr: capture.stderr,
        }),
        _ => None,
    }
}

/// The shell's own diagnostic, trimmed to its first line and rendered
/// as a parenthetical — the answer to "unparseable *how*", which was
/// being captured and dropped on the floor.
fn complaint(stderr: &str) -> String {
    match stderr.lines().find(|l| !l.trim().is_empty()) {
        Some(line) => format!(" ({})", line.trim()),
        None => String::new(),
    }
}

/// Shell variable the fallback's `.zshenv` parks the effective
/// `ZDOTDIR` in, so the copied `.zshrc` can restore the value the real
/// startup would have reached by that point.
const EFFECTIVE_ZDOTDIR: &str = "DODOT_TRACE_ZDOTDIR";

/// The scratch `.zshenv`: stage one of zsh's startup, reproduced.
///
/// zsh's file lookup is two-stage, and the stages disagree about where
/// `ZDOTDIR` points. It reads `$ZDOTDIR/.zshenv` *first*, with
/// `ZDOTDIR` still holding whatever the environment exported — which
/// for most users is nothing, making the first file `~/.zshenv`. Only
/// then does it look up `$ZDOTDIR/.zshrc`, by which time that very
/// file may have moved `ZDOTDIR` somewhere else. Pointing `ZDOTDIR` at
/// the scratch directory from the start and linking the *final*
/// directory's `.zshenv` in — the shape this replaces — gets that
/// backwards twice over: it skips the `~/.zshenv` that establishes
/// `ZDOTDIR` (along with every `PATH` line in it, which is exactly what
/// the trace is measuring), and it reads a `.zshenv` beside the
/// `.zshrc` that real zsh never opens.
///
/// So the scratch `.zshenv` *is* stage one: it restores `ZDOTDIR` to
/// the value the real shell would have held, sources the real
/// first-stage file from where it really lives, records where that
/// left `ZDOTDIR`, and only then points `ZDOTDIR` at the scratch
/// directory so stage two finds the copied `.zshrc`.
fn zshenv_stage_one(req: &TraceRequest, temp: &Path) -> String {
    let inherited = match req.zdotdir {
        Some(dir) => format!("ZDOTDIR={}", shell_quote(dir)),
        None => "unset ZDOTDIR".to_string(),
    };
    let first_stage = req
        .zdotdir
        .map(PathBuf::from)
        .unwrap_or_else(|| req.home.to_path_buf())
        .join(".zshenv");
    format!(
        "{inherited}\n\
         [ -f {first} ] && . {first}\n\
         {EFFECTIVE_ZDOTDIR}=\"${{ZDOTDIR:-$HOME}}\"\n\
         ZDOTDIR={temp}\n",
        first = shell_quote(&first_stage.display().to_string()),
        temp = shell_quote(&temp.display().to_string()),
    )
}

/// The scratch `.zshrc`: the rc copy, preceded by the line that undoes
/// the scratch `ZDOTDIR`.
///
/// The rc's own `$ZDOTDIR` references have to resolve to what stage one
/// left behind, not to the scratch directory that got zsh here — and
/// the effective value is read back from the variable stage one
/// recorded rather than recomputed, so a `.zshenv` that derives
/// `ZDOTDIR` at runtime is reproduced as faithfully as one that
/// assigns it literally.
fn zshrc_copy(rc_copy: &str) -> String {
    format!("ZDOTDIR=\"${EFFECTIVE_ZDOTDIR}\"\nunset {EFFECTIVE_ZDOTDIR}\n{rc_copy}")
}

/// The rc text with the report line inserted immediately before
/// `hook_line` (1-indexed). The report addresses the original
/// `<rc>:<line>` so the record lookup is unchanged.
///
/// Insertion keeps the *common* shapes parseable — a hook inside a
/// conditional, a loop, a function — which is what truncation could
/// not do. It is not a guarantee: a `case` pattern, a line
/// continuation, and a heredoc body all swallow the inserted line and
/// stop the parse. [`insertion_broke_the_parse`] is what turns that
/// from a silent wrong verdict into a refusal to answer, because
/// nothing in this function can tell.
fn insert_report_line(
    rc_text: &str,
    rc_nominal: &Path,
    hook_line: usize,
    exit_before_hook: bool,
) -> String {
    let maybe_exit = if exit_before_hook {
        format!("{REPORT_ONLY_TERMINATION}\n")
    } else {
        String::new()
    };
    let report = format!(
        "printf '+{TRACE_MARKER}%s|%s|%s|%s{RECORD_SUFFIX}\\n' {} {} \"$PWD\" \"$PATH\" >&2\n{maybe_exit}",
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

fn elapsed_us(started: Instant) -> u64 {
    started.elapsed().as_micros().try_into().unwrap_or(u64::MAX)
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
    use crate::paths::Pather;
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
    const FIXTURE_CWD: &str = "/tmp/dodot-trace-exp";

    #[test]
    fn the_zsh_record_at_the_hook_line_is_addressable() {
        let records = parse_trace(ZSH_TRACE);
        let rc = Path::new("/tmp/dodot-trace-exp/zdot/.zshrc");
        let record = record_at(&records, &[rc], 4).expect("hook-line record");
        assert_eq!(record.path, FIXTURE_PATH);
        assert_eq!(record.cwd, FIXTURE_CWD);
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
        assert_eq!(record.cwd, FIXTURE_CWD);
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
        assert_eq!(parse_record("dodot-trace|/rc|1|/home|/bin|> x"), None);
        // No record suffix: the PATH field never terminated — the
        // shape bash 3.2's 100-byte PS4 truncation produces.
        assert_eq!(parse_record("+dodot-trace|/rc|1|/home|/bin"), None);
        // Truncated a field earlier: no PATH field at all.
        assert_eq!(parse_record("+dodot-trace|/rc|1|/home"), None);
        // Line number is not a number.
        assert_eq!(parse_record("+dodot-trace|/rc|x|/home|/bin|> x"), None);
        // Empty PATH is still a record — an empty PATH at the hook
        // line is exactly the unresolvable story.
        assert_eq!(
            parse_record("+dodot-trace|/rc|3|/home||> eval"),
            Some(TraceRecord {
                file: "/rc".into(),
                line: 3,
                cwd: "/home".into(),
                path: String::new(),
            })
        );
    }

    // ── Hook location ───────────────────────────────────────────

    const HOME: &str = "/home/u";

    fn hook_in(rc_text: &str) -> Option<Hook> {
        find_hook(rc_text, Path::new(HOME))
    }

    /// The path a file-source hook resolved to, or the test's own
    /// panic — every caller below asserts about one or the other.
    fn sourced(rc_text: &str) -> SourcedScript {
        match hook_in(rc_text).expect("a hook").form {
            HookForm::FileSource(script) => script,
            other => panic!("expected a file-source hook, got {other:?}"),
        }
    }

    #[test]
    fn find_hook_names_line_and_form() {
        let eval_rc = "# comment\nexport A=1\neval \"$(dodot init-sh)\"\n";
        assert_eq!(
            hook_in(eval_rc),
            Some(Hook {
                line: 3,
                form: HookForm::Eval
            })
        );

        // dodot's own hook line names the path twice — once in the
        // guard, once as the argument. The second one is what gets
        // sourced, and both happen to agree here.
        let file_rc = "[ -f \"$HOME/.local/share/dodot/shell/dodot-init.sh\" ] && \
                       . \"$HOME/.local/share/dodot/shell/dodot-init.sh\"\n";
        assert_eq!(
            hook_in(file_rc),
            Some(Hook {
                line: 1,
                form: HookForm::FileSource(SourcedScript::Path(
                    "/home/u/.local/share/dodot/shell/dodot-init.sh".into()
                ))
            })
        );
    }

    /// The finding this retained path exists for: hook recognition
    /// accepts any line mentioning `dodot-init.sh`, so the line may
    /// well source a copy somewhere other than the one `dodot up`
    /// writes. What comes back is that copy, not the datastore's.
    #[test]
    fn the_retained_path_is_the_one_the_line_sources() {
        for (rc, expected) in [
            (
                ". \"$HOME/old/dodot-init.sh\"\n",
                "/home/u/old/dodot-init.sh",
            ),
            (
                "source ${HOME}/old/dodot-init.sh\n",
                "/home/u/old/dodot-init.sh",
            ),
            (". ~/old/dodot-init.sh\n", "/home/u/old/dodot-init.sh"),
            (
                ". /opt/elsewhere/dodot-init.sh\n",
                "/opt/elsewhere/dodot-init.sh",
            ),
            // The guard names one file and the source another: the
            // sourced one is the only one that runs.
            (
                "[ -f \"$HOME/a/dodot-init.sh\" ] && . \"$HOME/b/dodot-init.sh\"\n",
                "/home/u/b/dodot-init.sh",
            ),
        ] {
            assert_eq!(
                sourced(rc),
                SourcedScript::Path(expected.into()),
                "rc: {rc}"
            );
        }
    }

    /// Lines dodot will not resolve without interpreting shell. Each
    /// yields an honest non-answer rather than a path — a guess here
    /// would be a verdict about a file the hook may not source.
    #[test]
    fn a_line_dodot_cannot_read_resolves_to_nothing_rather_than_a_guess() {
        for rc in [
            // A variable dodot does not expand.
            ". \"$XDG_DATA_HOME/dodot/shell/dodot-init.sh\"\n",
            // A command substitution.
            ". \"$(dirname \"$0\")/dodot-init.sh\"\n",
            // Relative: resolved against a working directory that
            // belongs to a shell which is not running yet.
            ". ./dodot-init.sh\n",
            // A glob.
            ". /opt/*/dodot-init.sh\n",
            // A separator the word split would otherwise have taken
            // for part of the filename.
            "if true; then . $HOME/dodot-init.sh; fi\n",
            // Quoted and unquoted parts glued together: dodot reads
            // one layer of quoting, not shell concatenation.
            ". \"$HOME\"/dodot-init.sh\n",
            // Mentioned but never sourced — no `.`/`source` argument
            // to read at all.
            "echo dodot-init.sh is missing\n",
            // A quote that never closes: half a path is not a path.
            ". \"/opt/dodot-init.sh\n",
        ] {
            assert!(
                matches!(sourced(rc), SourcedScript::Unresolved { .. }),
                "rc must not resolve: {rc}"
            );
        }
    }

    #[test]
    fn a_commented_hook_is_no_hook() {
        assert_eq!(hook_in("# eval \"$(dodot init-sh)\"\n"), None);
        assert_eq!(hook_in("alias ll='ls -l'\n"), None);
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

    /// A working directory no PATH entry in these tests refers to, so
    /// only the tests that mean to exercise it can be affected by it.
    fn nowhere() -> &'static Path {
        Path::new("/nonexistent-cwd")
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
        let r = resolve_on_path(env.fs.as_ref(), &path_var, nowhere(), "dodot");
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
        let r = resolve_on_path(env.fs.as_ref(), &path_var, nowhere(), "dodot");
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
        let r = resolve_on_path(env.fs.as_ref(), &path_var, nowhere(), "dodot");
        assert_eq!(r.winner, None);
        assert_eq!(r.candidates.len(), 3);
        assert_eq!(r.candidates[0].skipped, Some(SkipReason::MissingDir));
        assert_eq!(r.candidates[1].skipped, Some(SkipReason::NotExecutable));
        assert_eq!(r.candidates[2].skipped, Some(SkipReason::NotPresent));
        // Every candidate is classified; only the one where a `dodot`
        // is actually present and still lost is worth a report line.
        // A missing directory and a directory without a dodot are both
        // ordinary facts about anyone's PATH, and printing them buries
        // the row that matters.
        let notable: Vec<_> = r.notable_skips().collect();
        assert_eq!(notable.len(), 1);
        assert_eq!(notable[0].skipped, Some(SkipReason::NotExecutable));
    }

    /// An empty `PATH` component means the working directory — to
    /// bash 5, bash 3.2 and zsh 5.9 alike, verified against all three.
    /// Leading, trailing, doubled, and the wholly empty `PATH` that is
    /// one empty component: every one of them searches there, so
    /// filtering them out reports on a search the shell never did.
    #[test]
    fn empty_path_components_search_the_working_directory() {
        let env = TempEnvironment::builder().build();
        let here = executable(&env, "here", "dodot");
        let cwd = env.home.join("here");
        let elsewhere = env.home.join("elsewhere").display().to_string();

        for path_var in [
            "",
            ":",
            &format!(":{elsewhere}"),
            &format!("{elsewhere}:"),
            &format!("{elsewhere}::{elsewhere}"),
        ] {
            let r = resolve_on_path(env.fs.as_ref(), path_var, &cwd, "dodot");
            assert_eq!(r.winner, Some(here.clone()), "PATH={path_var:?}");
        }
    }

    /// A relative component is resolved against the traced shell's
    /// working directory, not dodot's — the rc may well have `cd`'d
    /// before the hook line, and dodot's own cwd has nothing to do
    /// with the search that happened.
    #[test]
    fn relative_path_components_resolve_against_the_traced_directory() {
        let env = TempEnvironment::builder().build();
        let nested = executable(&env, "project/bin", "dodot");
        let r = resolve_on_path(env.fs.as_ref(), "bin", &env.home.join("project"), "dodot");
        assert_eq!(r.winner, Some(nested));
        // The same PATH from anywhere else finds nothing: the answer
        // is a property of the pair, which is why they travel together
        // in one `TraceRecord`.
        let r = resolve_on_path(env.fs.as_ref(), "bin", &env.home, "dodot");
        assert_eq!(r.winner, None);
    }

    // ── Verdicts: all four reachable ────────────────────────────

    fn no_version(_: &Path) -> Option<String> {
        None
    }

    /// A record carrying `path` at the hook line, from a working
    /// directory the PATH entries under test do not depend on.
    fn record(path: &str) -> TraceRecord {
        TraceRecord {
            file: "/home/u/.zshrc".into(),
            line: 1,
            cwd: nowhere().display().to_string(),
            path: path.to_string(),
        }
    }

    #[test]
    fn verdict_unresolvable_carries_the_searched_path() {
        let env = TempEnvironment::builder().build();
        let running = executable(&env, "run", "dodot");
        let v = judge_eval_hook(env.fs.as_ref(), &record("/nowhere"), &running, &no_version);
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
            &record(&linkdir.display().to_string()),
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
            &record(&env.home.join("old").display().to_string()),
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

    /// The generation `dodot up` last wrote, for the file-source tests
    /// below — what a sound hook's script should be carrying.
    const CURRENT_GEN: u64 = 100;

    /// Write an init script carrying `generation` and, when given, a
    /// version export — the two fields every dodot init script stamps.
    fn init_script(env: &TempEnvironment, at: &Path, generation: u64, version: Option<&str>) {
        let mut text = format!("# dodot init\nexport DODOT_INIT_GEN={generation}\n");
        if let Some(v) = version {
            text.push_str(&format!("export DODOT_INIT_VERSION={v}\n"));
        }
        env.fs.mkdir_all(at.parent().unwrap()).unwrap();
        env.fs.write_file(at, text.as_bytes()).unwrap();
    }

    fn judge(env: &TempEnvironment, script: &Path) -> TraceVerdict {
        judge_file_source_hook(
            env.fs.as_ref(),
            script.to_path_buf(),
            Some(CURRENT_GEN),
            "5.6.0",
        )
    }

    /// The file-source verdict is about the path the *line* names.
    /// The regression it guards: a hook pointing at a copy elsewhere
    /// was reported sound because the datastore's own script existed —
    /// a verdict drawn from a file the hook never mentions.
    #[test]
    fn the_file_source_verdict_judges_the_sourced_path_not_the_expected_one() {
        use crate::paths::Pather;
        let env = TempEnvironment::builder().build();
        // The script `dodot up` writes, present and healthy.
        let datastore = env.paths.init_script_path();
        init_script(&env, &datastore, CURRENT_GEN, Some("5.6.0"));

        // A hand-wired hook sourcing a copy that is not there.
        let gone = env.home.join("old/dodot-init.sh");
        assert_eq!(
            judge(&env, &gone),
            TraceVerdict::ScriptMissing {
                script: gone.clone()
            },
            "the datastore's script existing says nothing about this hook"
        );

        // And the sound case, judged on the same path.
        assert_eq!(
            judge(&env, &datastore),
            TraceVerdict::ScriptPresent { script: datastore }
        );
    }

    /// The finding this file-source arm exists for, and the one the
    /// existence check could not see: the hook sources a *real* dodot
    /// init script — written by dodot 5.0.0. Every filesystem fact
    /// about it says "present", and the footer on the same machine,
    /// reading the heartbeat that same script writes, correctly says
    /// version skew. The two halves of the tool must not contradict
    /// each other.
    #[test]
    fn a_script_written_by_another_dodot_is_skew_not_soundness() {
        let env = TempEnvironment::builder().build();
        let foreign = env.home.join("old/dodot-init.sh");
        init_script(&env, &foreign, CURRENT_GEN, Some("5.0.0"));
        assert_eq!(
            judge(&env, &foreign),
            TraceVerdict::ScriptSkewed {
                script: foreign,
                version: EvidenceVersion::Known("5.0.0".into())
            }
        );

        // The version-less shape — a script from a dodot that predates
        // the field — is the same finding against the bound.
        let pre = env.home.join("older/dodot-init.sh");
        init_script(&env, &pre, CURRENT_GEN, None);
        assert_eq!(
            judge(&env, &pre),
            TraceVerdict::ScriptSkewed {
                script: pre,
                version: EvidenceVersion::PreVersion
            }
        );
    }

    /// Right dodot, older script: a copy left behind. Judged by
    /// `classify_stamp` against the generation `up` last wrote — the
    /// same rule, in the same order after skew, that the activation
    /// probe applies to what a spawned shell reports.
    #[test]
    fn a_script_older_than_the_one_up_maintains_is_stale_not_sound() {
        let env = TempEnvironment::builder().build();
        let old = env.home.join("old/dodot-init.sh");
        init_script(&env, &old, CURRENT_GEN - 10, Some("5.6.0"));
        assert_eq!(
            judge(&env, &old),
            TraceVerdict::ScriptStale {
                script: old,
                found: CURRENT_GEN - 10,
                expected: CURRENT_GEN
            }
        );
    }

    /// A file that is not a dodot init script at all cannot be
    /// certified as one. Neither sound nor broken — unverified.
    #[test]
    fn an_unstamped_file_is_unverified_never_sound() {
        let env = TempEnvironment::builder().build();
        let impostor = env.home.join("other/dodot-init.sh");
        env.fs.mkdir_all(impostor.parent().unwrap()).unwrap();
        env.fs
            .write_file(&impostor, b"# someone else's script\nexport PATH=/x\n")
            .unwrap();
        assert!(
            matches!(
                judge(&env, &impostor),
                TraceVerdict::ScriptUnverified { .. }
            ),
            "a file with no dodot stamp is not a dodot init script"
        );
    }

    /// Sourceable means a regular file. A directory or a dangling
    /// symlink at that path is not something `.` can read, and calling
    /// it present is the same false green wearing a different hat.
    #[test]
    fn a_path_that_is_not_a_readable_file_is_not_a_present_script() {
        let env = TempEnvironment::builder().build();
        let dir = env.home.join("dodot-init.sh");
        env.fs.mkdir_all(&dir).unwrap();
        assert!(matches!(
            judge(&env, &dir),
            TraceVerdict::ScriptMissing { .. }
        ));

        let dangling = env.home.join("link/dodot-init.sh");
        env.fs.mkdir_all(&env.home.join("link")).unwrap();
        env.fs
            .symlink(&env.home.join("gone.sh"), &dangling)
            .unwrap();
        assert!(matches!(
            judge(&env, &dangling),
            TraceVerdict::ScriptMissing { .. }
        ));
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
        let copy = insert_report_line(rc, Path::new("/home/u/.zshrc"), 2, false);
        let lines: Vec<&str> = copy.lines().collect();
        assert_eq!(lines.len(), 4, "insertion, not replacement: {copy}");
        assert!(lines[1].starts_with("printf '+dodot-trace|"), "{copy}");
        assert!(lines[1].contains("'/home/u/.zshrc' 2"), "{copy}");
        // The report produces the same four-field record the PS4 path
        // does — one parser, two producers.
        assert!(lines[1].contains("\"$PWD\" \"$PATH\""), "{copy}");
        // Everything around the insertion survives byte for byte.
        assert_eq!(lines[0], "if true; then");
        assert_eq!(lines[2], "  eval \"$(dodot init-sh)\"");
        assert_eq!(lines[3], "fi");
    }

    #[test]
    fn report_only_copy_stops_before_the_hook() {
        let rc = "eval \"$(dodot init-sh)\"\necho after\n";
        let copy = insert_report_line(rc, Path::new("/home/u/.bashrc"), 1, true);
        let lines: Vec<&str> = copy.lines().collect();
        assert!(lines[0].starts_with("printf '+dodot-trace|"), "{copy}");
        assert_eq!(lines[1], REPORT_ONLY_TERMINATION);
        assert_eq!(lines[2], "eval \"$(dodot init-sh)\"");
        assert_eq!(lines[3], "echo after");
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

    /// A request tracing `rc` at `hook_line` with the given shell,
    /// rooted at the test environment's home.
    fn request<'a>(
        env: &'a TempEnvironment,
        shell_path: &'a Path,
        shell: HookupShell,
        rc: &'a Path,
        hook_line: usize,
    ) -> TraceRequest<'a> {
        TraceRequest {
            shell_path,
            shell,
            home: &env.home,
            zdotdir: None,
            rc_nominal: rc,
            rc_resolved: rc,
            hook_line,
            timeout: TIMEOUT,
            execution: TraceExecution::DiagnosticSupported,
        }
    }

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
            &request(&env, bash, HookupShell::Bash, &rc, 2),
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
            &request(&env, bash, HookupShell::Bash, &rc, 3),
        )
        .expect("fallback runs");
        assert!(run.used_fallback);
        let record = record_at(&run.records, &[&rc], 3).expect("inserted report record");
        assert_eq!(record.path, "/from/fallback");
        // The user's real rc was never written.
        assert_eq!(env.fs.read_to_string(&rc).unwrap(), before);
    }

    #[test]
    fn report_only_trace_does_not_execute_a_capability_unknown_hook() {
        let Some(bash) = bash() else { return };
        let env = TempEnvironment::builder().build();
        let _home = EnvVarGuard::set("HOME", &env.home.display().to_string());
        let rc = env.home.join(".bashrc");
        let touched = env.home.join("touched");
        env.fs
            .write_file(
                &rc,
                format!(
                    "export PATH=/before-hook\necho should-not-run > {}\n",
                    touched.display()
                )
                .as_bytes(),
            )
            .unwrap();

        let mut req = request(&env, bash, HookupShell::Bash, &rc, 2);
        req.execution = TraceExecution::ReportOnly;
        let run = run_trace(env.fs.as_ref(), &req).expect("report-only trace runs");

        assert!(run.used_fallback);
        assert!(run.elapsed_us > 0);
        let record = record_at(&run.records, &[&rc], 2).expect("inserted report record");
        assert_eq!(record.path, "/before-hook");
        assert!(
            !env.fs.exists(&touched),
            "report-only fallback must exit before executing the unknown hook"
        );
    }

    #[test]
    fn report_only_trace_ignores_shadowed_exit_in_bash() {
        let Some(bash) = bash() else { return };
        report_only_trace_ignores_shadowed_exit(bash, HookupShell::Bash, ".bashrc");
    }

    #[test]
    fn report_only_trace_ignores_shadowed_exit_in_zsh() {
        let Some(zsh) = zsh() else { return };
        report_only_trace_ignores_shadowed_exit(zsh, HookupShell::Zsh, ".zshrc");
    }

    fn report_only_trace_ignores_shadowed_exit(
        shell_path: &Path,
        shell: HookupShell,
        rc_name: &str,
    ) {
        let env = TempEnvironment::builder().build();
        let _home = EnvVarGuard::set("HOME", &env.home.display().to_string());
        let rc = env.home.join(rc_name);
        let touched = env.home.join("hook-ran");
        env.fs
            .write_file(
                &rc,
                format!(
                    "exit() {{ echo shadowed-exit; return 0; }}\n\
                     alias exit='echo alias-exit'\n\
                     export PATH=/before-hook\n\
                     echo should-not-run > {}\n",
                    touched.display()
                )
                .as_bytes(),
            )
            .unwrap();

        let mut req = request(&env, shell_path, shell, &rc, 4);
        req.execution = TraceExecution::ReportOnly;
        let run = run_trace(env.fs.as_ref(), &req).expect("report-only trace runs");

        assert!(run.used_fallback);
        assert!(
            record_at(&run.records, &[&rc], 4).is_some(),
            "report-only trace must still print the hook-line record"
        );
        assert!(
            !env.fs.exists(&touched),
            "a shadowed exit must not let report-only tracing continue into the hook"
        );
    }

    #[test]
    fn diagnostic_supported_trace_leaves_evidence_untouched_and_runs_contributions() {
        let Some(bash) = bash() else { return };
        let env = TempEnvironment::builder()
            .pack("shell")
            .file(
                "trace.sh",
                "echo trace contribution > \"$HOME/trace-contribution\"",
            )
            .done()
            .build();
        let _home = EnvVarGuard::set("HOME", &env.home.display().to_string());

        let handler_dir = env.paths.handler_data_dir("shell", "shell");
        env.fs.mkdir_all(&handler_dir).unwrap();
        env.fs
            .symlink(
                &env.dotfiles_root.join("shell/trace.sh"),
                &handler_dir.join("trace.sh"),
            )
            .unwrap();
        crate::shell::write_init_script(env.fs.as_ref(), env.paths.as_ref(), true, None).unwrap();

        let rc = env.home.join(".bashrc");
        let init = env.paths.init_script_path();
        env.fs
            .write_file(
                &rc,
                format!(
                    "[ -f \"{}\" ] && . \"{}\"\n",
                    init.display(),
                    init.display()
                )
                .as_bytes(),
            )
            .unwrap();

        std::process::Command::new(bash)
            .arg("-ic")
            .arg("true")
            .status()
            .expect("seed shell runs");

        let heartbeat = env.paths.hookup_heartbeat_path();
        let heartbeat_contents = env.fs.read_to_string(&heartbeat).unwrap();
        let heartbeat_mtime = std::fs::metadata(&heartbeat).unwrap().modified().unwrap();
        let profiles_before = profile_snapshot(&env);
        let contribution = env.home.join("trace-contribution");
        std::fs::remove_file(&contribution).unwrap();

        let run = run_trace(
            env.fs.as_ref(),
            &request(&env, bash, HookupShell::Bash, &rc, 1),
        )
        .expect("diagnostic trace runs");

        assert!(run.elapsed_us > 0);
        assert!(
            record_at(&run.records, &[&rc], 1).is_some(),
            "hook-line record must be present"
        );
        assert!(
            env.fs.exists(&contribution),
            "diagnostic-capable trace should continue through pack contributions"
        );
        assert_eq!(
            env.fs.read_to_string(&heartbeat).unwrap(),
            heartbeat_contents
        );
        assert_eq!(
            std::fs::metadata(&heartbeat).unwrap().modified().unwrap(),
            heartbeat_mtime,
            "trace must not refresh activation heartbeat evidence"
        );
        assert_eq!(
            profile_snapshot(&env),
            profiles_before,
            "trace must not create or rewrite shell-init profiles"
        );
    }

    /// The insertion is not free. A `case` pattern line is a shape
    /// where putting a whole command in front of the hook costs the
    /// copy its parse: the shell stops, no record is produced, and the
    /// caller reads that absence as `hook-never-ran` — an
    /// error-severity claim that the hook did not run, about a hook
    /// that ran. `TraceError::FallbackUnfaithful` exists for exactly
    /// this and was never being reached.
    ///
    /// A `PS4` override forces the fallback, which is the only path
    /// this can happen on — and on macOS's `/bin/bash` the fallback is
    /// the ordinary path, not an edge case.
    #[test]
    fn an_insertion_that_breaks_the_copys_parse_refuses_to_answer() {
        let Some(bash) = bash() else { return };
        let env = TempEnvironment::builder().build();
        let _home = EnvVarGuard::set("HOME", &env.home.display().to_string());
        let rc = env.home.join(".bashrc");
        // The hook rides the `case` *pattern* line — the idiomatic
        // `case $- in *i*) …;; esac` interactive guard. The original
        // parses; a command inserted between `case … in` and its first
        // pattern does not.
        env.fs
            .write_file(
                &rc,
                b"PS4='+ '\ncase x in\n  x) eval \"$(dodot init-sh)\" ;;\nesac\n",
            )
            .unwrap();

        let err = run_trace(
            env.fs.as_ref(),
            &request(&env, bash, HookupShell::Bash, &rc, 3),
        )
        .expect_err("an unparseable copy is not a verdict");
        let TraceError::FallbackUnfaithful(reason) = err else {
            panic!("expected FallbackUnfaithful, got {err:?}");
        };
        assert!(reason.contains("unparseable"), "{reason}");
        // The shell's own complaint, which used to be captured and
        // dropped, is what makes the refusal actionable.
        assert!(
            reason.contains("syntax error") || reason.contains("unexpected"),
            "the shell's diagnostic must survive into the message: {reason}"
        );
    }

    /// An rc that *already* does not parse is the user's own breakage,
    /// and still worth tracing — refusing it would trade a real
    /// finding for a shrug. Only a parse error dodot introduced counts.
    #[test]
    fn an_rc_that_never_parsed_is_still_traced() {
        let Some(bash) = bash() else { return };
        let env = TempEnvironment::builder().build();
        let _home = EnvVarGuard::set("HOME", &env.home.display().to_string());
        let rc = env.home.join(".bashrc");
        env.fs
            .write_file(
                &rc,
                b"PS4='+ '\nif then fi oops(\neval \"$(dodot init-sh)\"\n",
            )
            .unwrap();

        // Whatever comes back, it is not a refusal to try.
        match run_trace(
            env.fs.as_ref(),
            &request(&env, bash, HookupShell::Bash, &rc, 3),
        ) {
            Ok(_) => {}
            Err(TraceError::FallbackUnfaithful(reason)) => {
                panic!("an already-broken rc must not read as dodot's doing: {reason}")
            }
            Err(_) => {}
        }
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
            &request(&env, bash, HookupShell::Bash, &rc, 3),
        )
        .expect("trace runs");
        assert!(run.used_fallback);
        assert!(record_at(&run.records, &[&rc], 3).is_none());
    }

    fn zsh() -> Option<&'static Path> {
        let p = Path::new("/bin/zsh");
        p.exists().then_some(p)
    }

    #[test]
    fn a_real_zsh_reports_path_at_the_hook_line_via_zdotdir() {
        let Some(zsh) = zsh() else { return };
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
        let zdot_display = zdot.display().to_string();
        let _zdot = EnvVarGuard::set("ZDOTDIR", &zdot_display);

        let mut req = request(&env, zsh, HookupShell::Zsh, &rc, 2);
        req.zdotdir = Some(&zdot_display);
        let run = run_trace(env.fs.as_ref(), &req).expect("trace runs");
        let record = record_at(&run.records, &[&rc], 2).expect("hook-line record");
        assert_eq!(record.path, "/mangled/by/zshrc");
    }

    /// The shape the fallback used to get wrong, end to end in a real
    /// zsh: `~/.zshenv` is what *sets* `ZDOTDIR`, and it exports PATH
    /// on the way. Real startup reads it first, from `$HOME`, and only
    /// then looks up `$ZDOTDIR/.zshrc`.
    ///
    /// The old fallback started with `ZDOTDIR` already pointed at the
    /// scratch directory and linked in `<final-ZDOTDIR>/.zshenv` — a
    /// file zsh never reads in this shape — so `~/.zshenv` never ran
    /// and its PATH never appeared. The hook-line PATH came back
    /// missing the entry every real shell has, presented as a faithful
    /// measurement. A `PS4` override forces the fallback, which is the
    /// only route this exercises.
    #[test]
    fn the_zsh_fallback_runs_the_zshenv_that_establishes_zdotdir() {
        let Some(zsh) = zsh() else { return };
        let env = TempEnvironment::builder().build();
        let zdot = env.home.join("config/zsh");
        env.fs.mkdir_all(&zdot).unwrap();
        // Stage one: sets ZDOTDIR *and* mutates PATH. Both must land.
        env.fs
            .write_file(
                &env.home.join(".zshenv"),
                format!(
                    "export ZDOTDIR={}\nexport PATH=/from/zshenv\n",
                    zdot.display()
                )
                .as_bytes(),
            )
            .unwrap();
        // Stage two, from the directory stage one named. The PS4
        // override costs the marker, so the fallback has to answer.
        let rc = zdot.join(".zshrc");
        env.fs
            .write_file(
                &rc,
                b"PS4='+ '\nexport PATH=$PATH:/from/zshrc\neval \"$(dodot init-sh)\"\n",
            )
            .unwrap();
        let _home = EnvVarGuard::set("HOME", &env.home.display().to_string());

        // No ZDOTDIR in the environment: `~/.zshenv` is where it comes
        // from, which is the whole point of the shape.
        let run = run_trace(
            env.fs.as_ref(),
            &request(&env, zsh, HookupShell::Zsh, &rc, 3),
        )
        .expect("trace runs");
        assert!(run.used_fallback);
        let record = record_at(&run.records, &[&rc], 3).expect("hook-line record");
        assert_eq!(
            record.path, "/from/zshenv:/from/zshrc",
            "the PATH at the hook line must include what ~/.zshenv exported"
        );
    }

    /// The rc's own `$ZDOTDIR` references have to resolve to the
    /// directory the real startup would have left there, not to the
    /// scratch copy that got zsh to read it.
    #[test]
    fn the_fallback_restores_zdotdir_inside_the_copy() {
        let Some(zsh) = zsh() else { return };
        let env = TempEnvironment::builder().build();
        let zdot = env.home.join("config/zsh");
        env.fs.mkdir_all(&zdot).unwrap();
        env.fs
            .write_file(
                &env.home.join(".zshenv"),
                format!("export ZDOTDIR={}\n", zdot.display()).as_bytes(),
            )
            .unwrap();
        let rc = zdot.join(".zshrc");
        // The rc reports its own $ZDOTDIR through PATH, which is the
        // field the record carries.
        env.fs
            .write_file(
                &rc,
                b"PS4='+ '\nexport PATH=$ZDOTDIR\neval \"$(dodot init-sh)\"\n",
            )
            .unwrap();
        let _home = EnvVarGuard::set("HOME", &env.home.display().to_string());

        let run = run_trace(
            env.fs.as_ref(),
            &request(&env, zsh, HookupShell::Zsh, &rc, 3),
        )
        .expect("trace runs");
        let record = record_at(&run.records, &[&rc], 3).expect("hook-line record");
        assert_eq!(record.path, zdot.display().to_string());
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
                &request(&env, &missing, HookupShell::Bash, &rc, 1)
            ),
            Err(TraceError::SpawnFailed(_))
        ));
    }

    fn profile_snapshot(env: &TempEnvironment) -> Vec<(String, String)> {
        let dir = env.paths.probes_shell_init_dir();
        let Ok(mut entries) = env.fs.read_dir(&dir) else {
            return Vec::new();
        };
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries
            .into_iter()
            .filter(|entry| entry.is_file && entry.name.starts_with("profile-"))
            .map(|entry| {
                (
                    entry.name,
                    env.fs.read_to_string(&entry.path).unwrap_or_default(),
                )
            })
            .collect()
    }
}

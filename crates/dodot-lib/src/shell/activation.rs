//! Activation evidence — is a shell actually loading dodot?
//!
//! `dodot up` proves the datastore is correct. It proves nothing about
//! whether any shell ever *sources* the generated init script, which is
//! the only layer users experience. This module carries the evidence
//! half of that gap (`docs/proposals/shipped/shell-hookup.lex` §2):
//!
//! - The **generation stamp** — the init script exports
//!   [`INIT_GEN_ENV`] with the generation it was written at. Any dodot
//!   command inherits the calling shell's environment, so it knows with
//!   certainty whether the invoking shell sourced init, and whether
//!   that init predates the last regeneration.
//! - The **heartbeat** — the init script truncates
//!   [`Pather::hookup_heartbeat_path`] to the same generation on every
//!   source. One redirect of static content, so parallel shell startups
//!   can't corrupt it: last writer wins.
//!
//! Both are emitted unconditionally by
//! [`crate::shell::generate_init_script`] — unlike the opt-in
//! profiling TSVs — and both stay on the "one export, one redirect"
//! hot-path budget: no command execution, no `dodot` invocation.
//!
//! # Evaluation
//!
//! [`classify_stamp`] and [`classify_heartbeat`] turn the two raw
//! signals into a state each, relative to a *reference generation*, and
//! [`evaluate`] folds the pair — plus one bit of *session* evidence,
//! whether dodot is attached to a terminal — into one user-facing
//! [`ActivationState`]. All three are pure — the IO lives in
//! [`collect_signals`].
//!
//! The reference generation is "the generation a healthy shell would
//! be running", and who supplies it differs by caller:
//!
//! - `dodot status` reads it off the init script on disk.
//! - `dodot up` captures it *before* regenerating the script.
//!   Comparing against the freshly written generation instead would
//!   mark the invoking shell stale on every single `up`, which is noise,
//!   not news.
//!
//! The empirical probe (spec §3) lives in [`crate::shell::probe`] and
//! the rc-file machinery behind `dodot install` (spec §4) in
//! [`crate::shell::rc`]. Everything below is decided on evidence
//! alone; a caller that is allowed to spawn a shell goes through
//! [`probe::notice_with_probe`](crate::shell::probe::notice_with_probe)
//! instead, which starts here and only measures when these signals
//! come back inconclusive.

use std::path::Path;

use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::fs::Fs;
use crate::paths::Pather;
use crate::shell::rc::{self, HookPresence};

/// Environment variable the generated init script exports, carrying
/// the generation it was written at.
pub const INIT_GEN_ENV: &str = "DODOT_INIT_GEN";

/// The generation to stamp into a script written now.
///
/// Unix seconds: monotonic enough for "is this stamp older than that
/// script", and readable in a heartbeat file during debugging. Two
/// regenerations within the same second collapse to one generation,
/// which is harmless — every comparison is `>=`, so the worst case is
/// calling a just-started shell current instead of stale.
pub fn current_generation() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Parse a generation from stamp/heartbeat text. Anything that isn't
/// ASCII decimal reads as "no signal" rather than as generation zero,
/// which would make a corrupt file look like the oldest possible
/// activation instead of no activation at all.
pub fn parse_generation(raw: &str) -> Option<u64> {
    raw.trim().parse::<u64>().ok()
}

/// Read the calling shell's generation stamp from the process
/// environment. `None` when unset or unparseable.
pub fn read_env_stamp() -> Option<u64> {
    std::env::var(INIT_GEN_ENV)
        .ok()
        .as_deref()
        .and_then(parse_generation)
}

/// Read the generation recorded by the last shell activation. `None`
/// when no shell has ever sourced the init script on this machine (or
/// the marker is unreadable).
pub fn read_heartbeat(fs: &dyn Fs, paths: &dyn Pather) -> Option<u64> {
    let path = paths.hookup_heartbeat_path();
    if !fs.exists(&path) {
        return None;
    }
    fs.read_to_string(&path)
        .ok()
        .as_deref()
        .and_then(parse_generation)
}

/// Read the generation stamped into the init script currently on disk
/// — the generation a shell started right now would pick up. `None`
/// when the script is missing (nothing has ever been deployed) or
/// carries no stamp.
pub fn read_script_generation(fs: &dyn Fs, paths: &dyn Pather) -> Option<u64> {
    let path = paths.init_script_path();
    if !fs.exists(&path) {
        return None;
    }
    let content = fs.read_to_string(&path).ok()?;
    parse_script_generation(&content)
}

/// Extract the generation from init-script text — the value of the
/// single `export DODOT_INIT_GEN=<n>` line the generator emits.
pub fn parse_script_generation(script: &str) -> Option<u64> {
    let prefix = format!("export {INIT_GEN_ENV}=");
    script
        .lines()
        .find_map(|line| line.trim().strip_prefix(&prefix))
        .and_then(parse_generation)
}

/// Signal 1: what the calling shell's environment says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StampState {
    /// Stamped at or after the reference generation — this shell is live.
    Current,
    /// Stamped, but older than the reference generation — this shell
    /// sourced an init script that has since been regenerated.
    Stale,
    /// No stamp: this process was not started by a shell that sourced
    /// init (or no shell ever has).
    Absent,
}

/// Signal 2: what the heartbeat says about *any* shell on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartbeatState {
    /// Some shell activated at or after the reference generation.
    Fresh,
    /// Some shell activated once, but not since the reference generation.
    Old,
    /// No shell has ever activated here.
    Absent,
}

/// The user-facing activation state (spec §5, plus [`ShellNotLoaded`]
/// from #279).
///
/// All but [`VerifiedBroken`] are decided on evidence alone — the two
/// generation signals plus, for a caller that supplies it, the
/// session's tty-attachment. [`VerifiedBroken`] is the one state only
/// a measurement can reach — see [`crate::shell::probe`] — and it
/// takes precedence over the others when the probe has run: with the
/// generation signals alone, a hookup that used to work and then
/// broke looks like a stale shell, so the user gets "open a new
/// shell" for a problem no new shell will fix. [`ShellNotLoaded`] is
/// the evidence path's answer to that same broken hookup for callers
/// that may not measure (`status`), built from the tty signal and the
/// static rc scan.
///
/// [`VerifiedBroken`]: ActivationState::VerifiedBroken
/// [`ShellNotLoaded`]: ActivationState::ShellNotLoaded
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationState {
    /// Evidence says shells are loading dodot at the current
    /// generation. Nothing to say.
    Healthy,
    /// Something activated, but not at the current generation — most
    /// often the terminal the user is typing in, which predates their
    /// last `up`.
    StaleShell,
    /// No evidence of any activation, ever. The new-user failure story.
    NeverActivated,
    /// dodot is attached to a terminal but inherited no stamp: the
    /// shell the user is typing in demonstrably did not load dodot,
    /// whatever the heartbeat claims about some past session (#279).
    /// The literal truth and nothing more — an IDE task shell with a
    /// pty legitimately reads no rc file, so the notice never asserts
    /// "your hookup is broken"; the rc scan supplies the next step.
    ShellNotLoaded,
    /// The probe spawned a shell and it did not load dodot. Measured,
    /// not inferred.
    VerifiedBroken,
}

impl ActivationState {
    /// Stable identifier for the state, used as the serialized
    /// discriminator and in tests.
    pub fn as_str(self) -> &'static str {
        match self {
            ActivationState::Healthy => "healthy",
            ActivationState::StaleShell => "stale-shell",
            ActivationState::NeverActivated => "never-activated",
            ActivationState::ShellNotLoaded => "shell-not-loaded",
            ActivationState::VerifiedBroken => "verified-broken",
        }
    }
}

/// Classify the environment stamp against the reference generation.
///
/// A `None` reference means no generation is known (no init script on
/// disk yet), so there is nothing to be stale against: a stamp that
/// exists at all counts as current.
pub fn classify_stamp(stamp: Option<u64>, reference: Option<u64>) -> StampState {
    match (stamp, reference) {
        (None, _) => StampState::Absent,
        (Some(_), None) => StampState::Current,
        (Some(s), Some(r)) => {
            if s >= r {
                StampState::Current
            } else {
                StampState::Stale
            }
        }
    }
}

/// Classify the heartbeat against the reference generation. Same
/// no-reference rule as [`classify_stamp`].
pub fn classify_heartbeat(heartbeat: Option<u64>, reference: Option<u64>) -> HeartbeatState {
    match (heartbeat, reference) {
        (None, _) => HeartbeatState::Absent,
        (Some(_), None) => HeartbeatState::Fresh,
        (Some(h), Some(r)) => {
            if h >= r {
                HeartbeatState::Fresh
            } else {
                HeartbeatState::Old
            }
        }
    }
}

/// Fold the two signals, plus the session's tty-attachment, into one
/// state.
///
/// The stamp wins when present: it is direct evidence about the shell
/// the user is typing in, which is the one they can act on. A current
/// stamp is healthy even with no heartbeat (this shell activated; the
/// marker write is best-effort), and a stale stamp is a stale shell
/// even when some *other* shell is current — "open a new shell" is
/// still the fix.
///
/// With no stamp, `tty` breaks the tie the heartbeat cannot (#279).
/// The heartbeat is a high-water mark: it proves some shell activated
/// once, never that shells still do — a hookup that breaks after a
/// heartbeat keeps its old certificate indefinitely. But a
/// tty-attached process with no stamp *is* direct evidence: the
/// session in front of the user did not load dodot, and that outranks
/// the heartbeat's claim about a past session — whether the heartbeat
/// is fresh (the dead-hookup case the epic was built to catch) or old
/// (where blind "open a new shell" advice may be wrong; the rc scan
/// decides).
///
/// Detached and stampless (a cron job, an editor's task runner — the
/// callers this arm exists for), the heartbeat still decides: fresh
/// means shells are activating and this process just isn't one of
/// them; old means nothing has activated since the last regeneration;
/// absent means nothing ever has.
pub fn evaluate(stamp: StampState, heartbeat: HeartbeatState, tty: bool) -> ActivationState {
    match (stamp, heartbeat) {
        (StampState::Current, _) => ActivationState::Healthy,
        (StampState::Stale, _) => ActivationState::StaleShell,
        (StampState::Absent, HeartbeatState::Absent) => ActivationState::NeverActivated,
        (StampState::Absent, _) if tty => ActivationState::ShellNotLoaded,
        (StampState::Absent, HeartbeatState::Fresh) => ActivationState::Healthy,
        (StampState::Absent, HeartbeatState::Old) => ActivationState::StaleShell,
    }
}

/// Read both evidence signals and evaluate them against `reference`.
///
/// `env_stamp` and `tty` are passed in rather than read here so
/// evaluation stays a function of its inputs: production snapshots
/// both once into
/// [`ExecutionContext`](crate::packs::orchestration::ExecutionContext)
/// (via [`read_env_stamp`] and an `isatty` check), tests hand over
/// values. Callers that judge on the classic two-signal ladder alone
/// (`up`, `install`) pass `tty: false`; only `status` supplies real
/// session evidence — see [`notice_for`].
pub fn collect_signals(
    fs: &dyn Fs,
    paths: &dyn Pather,
    env_stamp: Option<u64>,
    reference: Option<u64>,
    tty: bool,
) -> ActivationState {
    let heartbeat = read_heartbeat(fs, paths);
    evaluate(
        classify_stamp(env_stamp, reference),
        classify_heartbeat(heartbeat, reference),
        tty,
    )
}

/// Evaluate the evidence and render the message for it, or `None` when
/// there is nothing to say.
///
/// Silent before anything is deployed: with no init script on disk
/// there is no hookup to have, and "no shell has loaded dodot yet" on a
/// machine where `dodot up` has never run is a warning about a
/// non-problem. The signals are read the same way either way — this is
/// purely about whether the answer is worth showing.
///
/// `tty` is the session evidence (see [`evaluate`]); `shell_env` is
/// only consulted when it produces [`ActivationState::ShellNotLoaded`],
/// whose next step comes from the static rc scan
/// ([`rc::scan_expected_rc`]) — file reads, never a shell spawn, so
/// `status` stays inside spec §9. Callers that must not let the
/// invoking session color the verdict (`up`, `install`, whose probe
/// path measures instead) pass `tty: false` and keep today's
/// two-signal behavior.
///
/// `quiet_ok` asks for the healthy line; see
/// [`ActivationNotice::for_state`].
pub fn notice_for(
    fs: &dyn Fs,
    paths: &dyn Pather,
    env_stamp: Option<u64>,
    reference: Option<u64>,
    quiet_ok: bool,
    tty: bool,
    shell_env: &rc::ShellEnv,
) -> Option<ActivationNotice> {
    let init_script = paths.init_script_path();
    if !fs.exists(&init_script) {
        return None;
    }
    let state = collect_signals(fs, paths, env_stamp, reference, tty);
    let hook_line = hook_line(&init_script, paths.home_dir());
    if state == ActivationState::ShellNotLoaded {
        let scan = rc::scan_expected_rc(fs, paths.home_dir(), shell_env, None);
        return Some(ActivationNotice::for_shell_not_loaded(scan, &hook_line));
    }
    ActivationNotice::for_state(state, &hook_line, quiet_ok)
}

/// A rendered activation message for `up` / `status` output.
///
/// `severity` names the presentation, not a failure level: `warning`
/// is the prominent never-activated banner, `info` the one-line stale
/// hint, `ok` the quiet "hookup: ok" line that only `status` shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActivationNotice {
    /// [`ActivationState::as_str`] — lets JSON consumers switch on the
    /// state without parsing the prose.
    pub state: String,
    /// `"ok"` | `"info"` | `"warning"`.
    pub severity: String,
    /// The one-line message.
    pub message: String,
    /// Optional second line: what to do about it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl ActivationNotice {
    /// Build the notice for a state, or `None` when the state should
    /// stay silent.
    ///
    /// `hook_line` is the rc-file line the user needs when nothing has
    /// ever activated — see [`hook_line`]. `quiet_ok` asks for the
    /// healthy "shell hookup: ok" line: `status` passes true (it is a
    /// report), `up` passes false (a healthy hookup after a deploy is
    /// silence, spec §5).
    pub fn for_state(
        state: ActivationState,
        hook_line: &str,
        quiet_ok: bool,
    ) -> Option<ActivationNotice> {
        match state {
            ActivationState::Healthy if !quiet_ok => None,
            ActivationState::Healthy => Some(ActivationNotice {
                state: state.as_str().into(),
                severity: "ok".into(),
                message: "shell hookup: ok".into(),
                hint: None,
            }),
            ActivationState::StaleShell => Some(ActivationNotice {
                state: state.as_str().into(),
                severity: "info".into(),
                message: "This shell started before your last `dodot up`.".into(),
                hint: Some("Open a new shell to pick up the current deployment.".into()),
            }),
            ActivationState::NeverActivated => Some(ActivationNotice {
                state: state.as_str().into(),
                severity: "warning".into(),
                message: "Deployed, but no shell has loaded dodot yet.".into(),
                hint: Some(format!(
                    "Run `dodot install --write` to wire it up, or add this to your shell rc \
                     file yourself: {hook_line}"
                )),
            }),
            // The full notice needs the rc scan for its next step;
            // `notice_for` routes there ([`Self::for_shell_not_loaded`])
            // before reaching this fold. This arm answers for a caller
            // that has no scan to offer: same headline, scanless hint.
            ActivationState::ShellNotLoaded => {
                Some(ActivationNotice::for_shell_not_loaded(None, hook_line))
            }
            // Only the probe reaches this state, and it renders its own
            // diagnosis (`probe::Verdict::notice`). This arm is the
            // answer for a caller that folds the state through the
            // evidence path anyway: same headline, generic next step.
            ActivationState::VerifiedBroken => Some(ActivationNotice {
                state: state.as_str().into(),
                severity: "error".into(),
                message: VERIFIED_BROKEN_MESSAGE.into(),
                hint: Some(format!(
                    "Run `dodot install --write` to wire the hook, or add this to your shell rc \
                     file yourself: {hook_line}"
                )),
            }),
        }
    }

    /// Build the [`ActivationState::ShellNotLoaded`] notice.
    ///
    /// The headline states only what the evidence proves — *this*
    /// shell hasn't loaded dodot — because a tty is not proof of a
    /// shell session (an IDE task runner allocates one too, and
    /// legitimately reads no rc). The static scan of the rc the user's
    /// shell should be reading (`scan`: presence + display path, as
    /// returned by [`rc::scan_expected_rc`]) supplies the next step:
    ///
    /// - hook absent → the rc has no hook, so no new shell will fix
    ///   it; name the file and the fix (a warning).
    /// - hook present → this is most plausibly a shell opened before
    ///   the hook landed; open a new one, escalate to `dodot up` (which
    ///   may probe) if that changes nothing (info).
    /// - `None` (unsupported shell, no rc to name) → just the manual
    ///   line (info — with nothing scanned, the false-positive reading
    ///   stays plausible).
    pub fn for_shell_not_loaded(
        scan: Option<(HookPresence, String)>,
        hook_line: &str,
    ) -> ActivationNotice {
        let state = ActivationState::ShellNotLoaded;
        let (severity, hint) = match scan {
            Some((HookPresence::Absent, rc_path)) => (
                "warning",
                format!(
                    "{rc_path} doesn't have the dodot hook. Run `dodot install --write` to \
                     wire it up, or add this line yourself: {hook_line}"
                ),
            ),
            Some((_, rc_path)) => (
                "info",
                format!(
                    "The hook is in {rc_path}, so this shell probably predates it — open a \
                     new shell. If that changes nothing, run `dodot up` to diagnose."
                ),
            ),
            None => (
                "info",
                format!(
                    "dodot could not tell which rc file your shell reads. Make sure it has \
                     this line: {hook_line}"
                ),
            ),
        };
        ActivationNotice {
            state: state.as_str().into(),
            severity: severity.into(),
            message: "This shell hasn't loaded dodot.".into(),
            hint: Some(hint),
        }
    }
}

/// Headline for a hookup the probe measured as broken. Shared so the
/// evidence path and [`crate::shell::probe`] can never drift into two
/// different phrasings of the same finding.
pub const VERIFIED_BROKEN_MESSAGE: &str = "Deployed, but a new shell did not load dodot.";

/// The rc-file line that wires a shell up to the generated init
/// script, with the home prefix written back as `$HOME`.
///
/// `$HOME` rather than `~` because the path is quoted: a shell expands
/// `$HOME` inside double quotes but not `~`, so a tilde would make the
/// line source a literal `~` path once pasted. Quoting itself is not
/// optional — it is what keeps a home directory with spaces working.
///
/// The single source of truth for the hook line. `dodot install
/// --write` writes exactly this string inside its marked block
/// ([`crate::shell::rc`]), and every message that offers a manual
/// alternative prints it — one line, one definition.
pub fn hook_line(init_script_path: &Path, home: &Path) -> String {
    let shown = match init_script_path.strip_prefix(home) {
        Ok(rel) => format!("$HOME/{}", rel.display()),
        Err(_) => init_script_path.display().to_string(),
    };
    format!("[ -f \"{shown}\" ] && . \"{shown}\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempEnvironment;

    // ── The signal ladder, exhaustively ──────────────────────────────

    #[test]
    fn stamp_classification_covers_generation_comparisons() {
        let cases = [
            // (stamp, reference, expected)
            (None, Some(10), StampState::Absent),
            (None, None, StampState::Absent),
            (Some(10), Some(10), StampState::Current),
            (Some(11), Some(10), StampState::Current),
            (Some(9), Some(10), StampState::Stale),
            (Some(0), Some(10), StampState::Stale),
            // No script on disk: nothing to be stale against.
            (Some(9), None, StampState::Current),
        ];
        for (stamp, reference, expected) in cases {
            assert_eq!(
                classify_stamp(stamp, reference),
                expected,
                "stamp={stamp:?} reference={reference:?}"
            );
        }
    }

    #[test]
    fn heartbeat_classification_covers_generation_comparisons() {
        let cases = [
            (None, Some(10), HeartbeatState::Absent),
            (None, None, HeartbeatState::Absent),
            (Some(10), Some(10), HeartbeatState::Fresh),
            (Some(11), Some(10), HeartbeatState::Fresh),
            (Some(9), Some(10), HeartbeatState::Old),
            (Some(9), None, HeartbeatState::Fresh),
        ];
        for (heartbeat, reference, expected) in cases {
            assert_eq!(
                classify_heartbeat(heartbeat, reference),
                expected,
                "heartbeat={heartbeat:?} reference={reference:?}"
            );
        }
    }

    #[test]
    fn evaluation_covers_the_full_stamp_by_heartbeat_by_tty_matrix() {
        use ActivationState::*;
        use HeartbeatState as H;
        use StampState as S;
        // (stamp, heartbeat, detached expectation, tty expectation).
        // A stamp — direct evidence either way — makes tty irrelevant;
        // it only breaks the stampless tie (#279).
        let matrix = [
            (S::Current, H::Fresh, Healthy, Healthy),
            (S::Current, H::Old, Healthy, Healthy),
            (S::Current, H::Absent, Healthy, Healthy),
            (S::Stale, H::Fresh, StaleShell, StaleShell),
            (S::Stale, H::Old, StaleShell, StaleShell),
            (S::Stale, H::Absent, StaleShell, StaleShell),
            // The dead-hookup case: the heartbeat's old certificate
            // loses to the session in front of the user.
            (S::Absent, H::Fresh, Healthy, ShellNotLoaded),
            (S::Absent, H::Old, StaleShell, ShellNotLoaded),
            (S::Absent, H::Absent, NeverActivated, NeverActivated),
        ];
        for (stamp, heartbeat, detached, tty) in matrix {
            assert_eq!(
                evaluate(stamp, heartbeat, false),
                detached,
                "detached: stamp={stamp:?} heartbeat={heartbeat:?}"
            );
            assert_eq!(
                evaluate(stamp, heartbeat, true),
                tty,
                "tty: stamp={stamp:?} heartbeat={heartbeat:?}"
            );
        }
    }

    #[test]
    fn end_to_end_generation_matrix_through_collect_signals() {
        // Same matrix, driven by raw generations through the IO entry
        // point: reference 100, stamps/heartbeats above and below it.
        let cases = [
            (Some(100), Some(100), false, ActivationState::Healthy),
            (Some(99), Some(100), false, ActivationState::StaleShell),
            (None, Some(100), false, ActivationState::Healthy),
            (None, Some(99), false, ActivationState::StaleShell),
            (None, None, false, ActivationState::NeverActivated),
            (Some(100), None, false, ActivationState::Healthy),
            (Some(99), None, false, ActivationState::StaleShell),
            // Attached to a terminal, no stamp: the heartbeat cannot
            // certify this session, fresh or old.
            (None, Some(100), true, ActivationState::ShellNotLoaded),
            (None, Some(99), true, ActivationState::ShellNotLoaded),
            (None, None, true, ActivationState::NeverActivated),
            (Some(100), None, true, ActivationState::Healthy),
        ];
        for (stamp, heartbeat, tty, expected) in cases {
            let env = TempEnvironment::builder().build();
            if let Some(h) = heartbeat {
                env.fs.mkdir_all(&env.paths.probes_hookup_dir()).unwrap();
                env.fs
                    .write_file(&env.paths.hookup_heartbeat_path(), h.to_string().as_bytes())
                    .unwrap();
            }
            assert_eq!(
                collect_signals(env.fs.as_ref(), env.paths.as_ref(), stamp, Some(100), tty),
                expected,
                "stamp={stamp:?} heartbeat={heartbeat:?} tty={tty}"
            );
        }
    }

    // ── Parsing ──────────────────────────────────────────────────────

    #[test]
    fn unparseable_signals_read_as_absent_not_zero() {
        assert_eq!(parse_generation("  42\n"), Some(42));
        assert_eq!(parse_generation(""), None);
        assert_eq!(parse_generation("not-a-number"), None);
        assert_eq!(parse_generation("-1"), None);

        let env = TempEnvironment::builder().build();
        env.fs.mkdir_all(&env.paths.probes_hookup_dir()).unwrap();
        env.fs
            .write_file(&env.paths.hookup_heartbeat_path(), b"garbage")
            .unwrap();
        assert_eq!(read_heartbeat(env.fs.as_ref(), env.paths.as_ref()), None);
    }

    #[test]
    fn script_generation_is_read_back_from_the_export_line() {
        assert_eq!(
            parse_script_generation("#!/bin/sh\nexport DODOT_INIT_GEN=1755200000\n"),
            Some(1_755_200_000)
        );
        assert_eq!(parse_script_generation("#!/bin/sh\nexport PATH=x\n"), None);

        let env = TempEnvironment::builder().build();
        // Missing script: no generation known.
        assert_eq!(
            read_script_generation(env.fs.as_ref(), env.paths.as_ref()),
            None
        );
    }

    // ── Notices ──────────────────────────────────────────────────────

    #[test]
    fn healthy_is_silent_for_up_and_quiet_for_status() {
        assert_eq!(
            ActivationNotice::for_state(ActivationState::Healthy, "hook", false),
            None
        );
        let quiet = ActivationNotice::for_state(ActivationState::Healthy, "hook", true).unwrap();
        assert_eq!(quiet.severity, "ok");
        assert_eq!(quiet.state, "healthy");
    }

    #[test]
    fn never_activated_is_prominent_and_names_the_manual_hook() {
        let hook = hook_line(
            Path::new("/home/u/.local/share/dodot/shell/dodot-init.sh"),
            Path::new("/home/u"),
        );
        // `$HOME`, not `~`: the paths are quoted, and a quoted tilde
        // stays literal when the user pastes the line into their rc.
        assert_eq!(
            hook,
            "[ -f \"$HOME/.local/share/dodot/shell/dodot-init.sh\" ] && . \"$HOME/.local/share/dodot/shell/dodot-init.sh\""
        );

        let notice =
            ActivationNotice::for_state(ActivationState::NeverActivated, &hook, true).unwrap();
        assert_eq!(notice.severity, "warning");
        assert!(notice.message.contains("no shell has loaded dodot yet"));
        let hint = notice.hint.unwrap();
        assert!(
            hint.contains(&hook),
            "hint should carry the hook line: {hint}"
        );
        // WS02 ships `dodot install`, so the hint now leads with the
        // command that does this for you — the manual line stays for
        // users who would rather wire it themselves.
        assert!(hint.contains("dodot install --write"), "hint: {hint}");
    }

    #[test]
    fn shell_not_loaded_lets_the_rc_scan_pick_the_next_step() {
        // Hook absent: no new shell fixes it — name the file and the fix.
        let absent = ActivationNotice::for_shell_not_loaded(
            Some((HookPresence::Absent, "~/.zshrc".into())),
            "HOOK",
        );
        assert_eq!(absent.state, "shell-not-loaded");
        assert_eq!(absent.severity, "warning");
        assert_eq!(absent.message, "This shell hasn't loaded dodot.");
        let hint = absent.hint.unwrap();
        assert!(hint.contains("~/.zshrc") && hint.contains("dodot install --write"));
        assert!(!hint.contains("new shell"), "{hint}");

        // Hook present (managed or manual): an old shell is the likely
        // story — advise a new one, escalate to `up` after that.
        for presence in [HookPresence::ManagedBlock, HookPresence::Manual] {
            let present =
                ActivationNotice::for_shell_not_loaded(Some((presence, "~/.zshrc".into())), "HOOK");
            assert_eq!(present.severity, "info");
            let hint = present.hint.unwrap();
            assert!(
                hint.contains("new shell") && hint.contains("dodot up"),
                "{hint}"
            );
        }

        // No rc to name (unsupported shell): just the manual line.
        let unknown = ActivationNotice::for_shell_not_loaded(None, "HOOK");
        assert_eq!(unknown.severity, "info");
        assert!(unknown.hint.unwrap().contains("HOOK"));
    }

    #[test]
    fn stale_shell_says_open_a_new_shell() {
        let notice =
            ActivationNotice::for_state(ActivationState::StaleShell, "hook", true).unwrap();
        assert_eq!(notice.severity, "info");
        assert!(notice.hint.unwrap().contains("Open a new shell"));
    }

    #[test]
    fn hook_line_outside_home_stays_absolute() {
        let hook = hook_line(
            Path::new("/opt/dodot/shell/dodot-init.sh"),
            Path::new("/home/u"),
        );
        assert!(hook.contains("/opt/dodot/shell/dodot-init.sh"), "{hook}");
        assert!(!hook.contains("$HOME"), "{hook}");
    }
}

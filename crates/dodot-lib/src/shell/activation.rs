//! Activation evidence — is a shell actually loading dodot?
//!
//! `dodot up` proves the datastore is correct. It proves nothing about
//! whether any shell ever *sources* the generated init script, which is
//! the only layer users experience. This module carries the evidence
//! half of that gap (`docs/proposals/shell-hookup.lex` §2):
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
//! [`evaluate`] folds the pair into one user-facing
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
//! The empirical probe (spec §3) and `dodot install` (spec §4) are not
//! part of this module: the states below are decided on evidence alone,
//! and the never-activated notice points at the manual hook line.

use std::path::Path;

use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::fs::Fs;
use crate::paths::Pather;

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

/// The user-facing activation state (spec §5). `verified broken` is
/// deliberately absent: it requires the probe, which this slice does
/// not ship.
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
}

impl ActivationState {
    /// Stable identifier for the state, used as the serialized
    /// discriminator and in tests.
    pub fn as_str(self) -> &'static str {
        match self {
            ActivationState::Healthy => "healthy",
            ActivationState::StaleShell => "stale-shell",
            ActivationState::NeverActivated => "never-activated",
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

/// Fold the two signals into one state.
///
/// The stamp wins when present: it is direct evidence about the shell
/// the user is typing in, which is the one they can act on. A current
/// stamp is healthy even with no heartbeat (this shell activated; the
/// marker write is best-effort), and a stale stamp is a stale shell
/// even when some *other* shell is current — "open a new shell" is
/// still the fix.
///
/// With no stamp at all, the heartbeat decides: fresh means shells are
/// activating and this process just isn't one of them (a cron job, an
/// editor's task runner); old means nothing has activated since the
/// last regeneration; absent means nothing ever has.
pub fn evaluate(stamp: StampState, heartbeat: HeartbeatState) -> ActivationState {
    match (stamp, heartbeat) {
        (StampState::Current, _) => ActivationState::Healthy,
        (StampState::Stale, _) => ActivationState::StaleShell,
        (StampState::Absent, HeartbeatState::Fresh) => ActivationState::Healthy,
        (StampState::Absent, HeartbeatState::Old) => ActivationState::StaleShell,
        (StampState::Absent, HeartbeatState::Absent) => ActivationState::NeverActivated,
    }
}

/// Read both evidence signals and evaluate them against `reference`.
///
/// `env_stamp` is passed in rather than read here so evaluation stays
/// a function of its inputs: production snapshots it once into
/// [`ExecutionContext`](crate::packs::orchestration::ExecutionContext)
/// via [`read_env_stamp`], tests hand over a value.
pub fn collect_signals(
    fs: &dyn Fs,
    paths: &dyn Pather,
    env_stamp: Option<u64>,
    reference: Option<u64>,
) -> ActivationState {
    let heartbeat = read_heartbeat(fs, paths);
    evaluate(
        classify_stamp(env_stamp, reference),
        classify_heartbeat(heartbeat, reference),
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
/// `quiet_ok` asks for the healthy line; see
/// [`ActivationNotice::for_state`].
pub fn notice_for(
    fs: &dyn Fs,
    paths: &dyn Pather,
    env_stamp: Option<u64>,
    reference: Option<u64>,
    quiet_ok: bool,
) -> Option<ActivationNotice> {
    let init_script = paths.init_script_path();
    if !fs.exists(&init_script) {
        return None;
    }
    let state = collect_signals(fs, paths, env_stamp, reference);
    ActivationNotice::for_state(state, &hook_line(&init_script, paths.home_dir()), quiet_ok)
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
                    "Add this to your shell rc file, then open a new shell: {hook_line}"
                )),
            }),
        }
    }
}

/// The rc-file line that wires a shell up to the generated init
/// script, with the home prefix written back as `$HOME`.
///
/// `$HOME` rather than `~` because the path is quoted: a shell expands
/// `$HOME` inside double quotes but not `~`, so a tilde would make the
/// line source a literal `~` path once pasted. Quoting itself is not
/// optional — it is what keeps a home directory with spaces working.
///
/// This is the manual hook: `dodot install` (spec §4) lands in a later
/// work stream, and until it does, telling the user to run a command
/// that doesn't exist is worse than telling them nothing.
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
    fn evaluation_covers_the_full_stamp_by_heartbeat_matrix() {
        use ActivationState::*;
        use HeartbeatState as H;
        use StampState as S;
        let matrix = [
            (S::Current, H::Fresh, Healthy),
            (S::Current, H::Old, Healthy),
            (S::Current, H::Absent, Healthy),
            (S::Stale, H::Fresh, StaleShell),
            (S::Stale, H::Old, StaleShell),
            (S::Stale, H::Absent, StaleShell),
            (S::Absent, H::Fresh, Healthy),
            (S::Absent, H::Old, StaleShell),
            (S::Absent, H::Absent, NeverActivated),
        ];
        for (stamp, heartbeat, expected) in matrix {
            assert_eq!(
                evaluate(stamp, heartbeat),
                expected,
                "stamp={stamp:?} heartbeat={heartbeat:?}"
            );
        }
    }

    #[test]
    fn end_to_end_generation_matrix_through_collect_signals() {
        // Same matrix, driven by raw generations through the IO entry
        // point: reference 100, stamps/heartbeats above and below it.
        let cases = [
            (Some(100), Some(100), ActivationState::Healthy),
            (Some(99), Some(100), ActivationState::StaleShell),
            (None, Some(100), ActivationState::Healthy),
            (None, Some(99), ActivationState::StaleShell),
            (None, None, ActivationState::NeverActivated),
            (Some(100), None, ActivationState::Healthy),
            (Some(99), None, ActivationState::StaleShell),
        ];
        for (stamp, heartbeat, expected) in cases {
            let env = TempEnvironment::builder().build();
            if let Some(h) = heartbeat {
                env.fs.mkdir_all(&env.paths.probes_hookup_dir()).unwrap();
                env.fs
                    .write_file(&env.paths.hookup_heartbeat_path(), h.to_string().as_bytes())
                    .unwrap();
            }
            assert_eq!(
                collect_signals(env.fs.as_ref(), env.paths.as_ref(), stamp, Some(100)),
                expected,
                "stamp={stamp:?} heartbeat={heartbeat:?}"
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
        // WS02 ships `dodot install`; until then, pointing at it would
        // be pointing at a command that does not exist.
        assert!(!hint.contains("dodot install"), "hint: {hint}");
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

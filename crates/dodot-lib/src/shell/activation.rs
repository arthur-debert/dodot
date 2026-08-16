//! Activation evidence — is a shell actually loading dodot, and which
//! dodot?
//!
//! `dodot up` proves the datastore is correct. It proves nothing about
//! whether any shell ever *sources* the generated init script, which is
//! the only layer users experience. This module carries the evidence
//! half of that gap (`docs/proposals/shipped/shell-hookup.lex` §2,
//! widened by `docs/proposals/shell-hookup-ergonomics.lex` §2):
//!
//! - The **generation stamp** — the init script exports
//!   [`INIT_GEN_ENV`] with the generation it was written at, and
//!   [`INIT_VERSION_ENV`] with the dodot that wrote it. Any dodot
//!   command inherits the calling shell's environment, so it knows with
//!   certainty whether the invoking shell sourced init, whether that
//!   init predates the last regeneration, and whether it came from the
//!   binary now running ([`EnvStamp`]).
//! - The **heartbeat** — the init script truncates
//!   [`Pather::hookup_heartbeat_path`] to the same two fields on every
//!   source. One redirect of static content, so parallel shell startups
//!   can't corrupt it: last writer wins. Its *mtime*, not its contents,
//!   answers "when did a shell last load dodot" ([`Heartbeat`]).
//!
//! All of it is emitted unconditionally by
//! [`crate::shell::generate_init_script`] — unlike the opt-in
//! profiling TSVs — and stays on the hot-path budget: exports and one
//! redirect, no command execution, no `dodot` invocation.
//!
//! # Evaluation
//!
//! [`Evidence`] is the whole input to the answer: both signals, the
//! reference generation, the running version, and the clock. Reading
//! it is the only IO ([`Evidence::collect`]); everything downstream —
//! [`classify_stamp`], [`classify_heartbeat`], [`evaluate`],
//! [`Evidence::state`], [`Evidence::footer`] — is pure, so every state
//! and every rendered string is unit-testable without a filesystem.
//!
//! The reference generation is "the generation a healthy shell would
//! be running", and who supplies it differs by caller:
//!
//! - `dodot status` reads it off the init script on disk.
//! - `dodot up` and `dodot down` capture it *before* regenerating the
//!   script. Comparing against the freshly written generation instead
//!   would mark the invoking shell stale on every single run, which is
//!   noise, not news.
//!
//! # The footer
//!
//! The answer renders as two lines ([`ActivationNotice`]) that every
//! `pack-status` render carries — `up`, `down` and `status` alike.
//! Line one names the state, line two the evidence behind it. The
//! strings are pinned here and asserted verbatim in the tests, because
//! the user documentation quotes them.
//!
//! The empirical probe (spec §3) lives in [`crate::shell::probe`] and
//! the rc-file machinery behind `dodot install` (spec §4) in
//! [`crate::shell::rc`]. Everything below is decided on evidence
//! alone; a caller that is allowed to spawn a shell goes through
//! [`probe::notice_with_probe`](crate::shell::probe::notice_with_probe)
//! instead, which starts here and only measures when these signals
//! come back inconclusive.

use std::fmt;
use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::fs::Fs;
use crate::paths::Pather;
use crate::shell::rc::{self, HookPresence};

/// Environment variable the generated init script exports, carrying
/// the generation it was written at.
pub const INIT_GEN_ENV: &str = "DODOT_INIT_GEN";

/// Environment variable the generated init script exports, carrying
/// the dodot version that generated it.
pub const INIT_VERSION_ENV: &str = "DODOT_INIT_VERSION";

/// The last release whose evidence carried no version.
///
/// One constant, one rule: any stamp or heartbeat without a version
/// field was written by a dodot at or below this release, and renders
/// as `≤5.5.1` rather than as "unknown". It covers both halves of the
/// same situation — a pre-RCS01 binary generating a script right now,
/// and a stale heartbeat one left on disk months ago.
pub const PRE_VERSION_RELEASE: &str = "5.5.1";

/// The version of the running binary — what every piece of evidence is
/// compared against.
pub fn running_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

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

/// The version a stamp or heartbeat reports.
///
/// Absent is not unknown: evidence without a version field can only
/// have been written by a dodot that predates the field, which is a
/// bound, not a mystery — see [`PRE_VERSION_RELEASE`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceVersion {
    /// The version the evidence names.
    Known(String),
    /// No version field: written at or before [`PRE_VERSION_RELEASE`].
    PreVersion,
}

impl EvidenceVersion {
    /// Read a version field, treating an absent or blank one as
    /// [`EvidenceVersion::PreVersion`].
    pub fn from_field(field: Option<&str>) -> EvidenceVersion {
        match field.map(str::trim).filter(|v| !v.is_empty()) {
            Some(v) => EvidenceVersion::Known(v.to_string()),
            None => EvidenceVersion::PreVersion,
        }
    }

    /// Whether this evidence is consistent with having been written by
    /// `running`.
    ///
    /// [`EvidenceVersion::PreVersion`] is a bound rather than a value,
    /// so it matches exactly one running version: the bound itself.
    /// Every later release is unambiguously *not* what wrote it, which
    /// is the version skew this whole state exists to name; a binary
    /// that still *is* [`PRE_VERSION_RELEASE`] cannot tell its own
    /// version-less evidence from someone else's, and must not claim
    /// skew on evidence that ambiguous.
    pub fn is(&self, running: &str) -> bool {
        match self {
            EvidenceVersion::Known(v) => v == running,
            EvidenceVersion::PreVersion => running == PRE_VERSION_RELEASE,
        }
    }
}

impl fmt::Display for EvidenceVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvidenceVersion::Known(v) => f.write_str(v),
            EvidenceVersion::PreVersion => write!(f, "≤{PRE_VERSION_RELEASE}"),
        }
    }
}

/// Signal 1's raw form: what the calling shell's environment reports.
///
/// A generation without a version is the pre-RCS01 shape and stays
/// meaningful; a version without a generation is not evidence of
/// anything, so [`EnvStamp::version`] is only ever consulted when
/// [`EnvStamp::generation`] is set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvStamp {
    /// `DODOT_INIT_GEN` — the generation of the script this shell
    /// sourced.
    pub generation: Option<u64>,
    /// `DODOT_INIT_VERSION` — the dodot that generated it.
    pub version: Option<String>,
}

impl EnvStamp {
    /// Read both fields from the process environment. Unset or
    /// unparseable reads as no signal.
    pub fn read() -> EnvStamp {
        EnvStamp {
            generation: std::env::var(INIT_GEN_ENV)
                .ok()
                .as_deref()
                .and_then(parse_generation),
            version: std::env::var(INIT_VERSION_ENV)
                .ok()
                .filter(|v| !v.trim().is_empty()),
        }
    }

    /// A stamp for `generation` with no version — the pre-RCS01 shape,
    /// and the shorthand tests use when the version is not the subject.
    pub fn at(generation: u64) -> EnvStamp {
        EnvStamp {
            generation: Some(generation),
            version: None,
        }
    }

    /// The version this shell loaded, or `None` when it loaded nothing.
    pub fn evidence_version(&self) -> Option<EvidenceVersion> {
        self.generation
            .map(|_| EvidenceVersion::from_field(self.version.as_deref()))
    }
}

/// Signal 2's raw form: the heartbeat file, contents and mtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heartbeat {
    /// The generation the last shell to source init was running.
    pub generation: Option<u64>,
    /// The dodot that generated that script.
    pub version: Option<String>,
    /// When the file was last written — i.e. when a shell last
    /// *sourced* the script, which is not when the script was
    /// generated. Under the file-source hook the two diverge
    /// permanently, and the mtime is the one that answers "last run".
    /// `None` when the filesystem would not say.
    pub last_run: Option<SystemTime>,
}

impl Heartbeat {
    /// The version the last shell loaded.
    pub fn evidence_version(&self) -> EvidenceVersion {
        EvidenceVersion::from_field(self.version.as_deref())
    }
}

/// Split heartbeat text into its generation and version fields.
///
/// The file holds `<generation> <version>`; a pre-RCS01 one holds just
/// `<generation>`. Anything else yields no generation, so a corrupt
/// file reads as no activation rather than as the oldest possible one.
pub fn parse_heartbeat(raw: &str) -> (Option<u64>, Option<String>) {
    let mut fields = raw.split_whitespace();
    let generation = fields.next().and_then(parse_generation);
    let version = fields.next().map(str::to_string);
    (generation, version)
}

/// Read the heartbeat left by the last shell activation. `None` when no
/// shell has ever sourced the init script on this machine (or the
/// marker is unreadable).
pub fn read_heartbeat(fs: &dyn Fs, paths: &dyn Pather) -> Option<Heartbeat> {
    let path = paths.hookup_heartbeat_path();
    if !fs.exists(&path) {
        return None;
    }
    let raw = fs.read_to_string(&path).ok()?;
    let (generation, version) = parse_heartbeat(&raw);
    Some(Heartbeat {
        generation,
        version,
        last_run: fs.modified(&path).ok(),
    })
}

/// Read the init script currently on disk. `None` when nothing has ever
/// been deployed, or the script is unreadable.
pub fn read_script(fs: &dyn Fs, paths: &dyn Pather) -> Option<String> {
    let path = paths.init_script_path();
    if !fs.exists(&path) {
        return None;
    }
    fs.read_to_string(&path).ok()
}

/// Read the generation stamped into the init script currently on disk
/// — the generation a shell started right now would pick up. `None`
/// when the script is missing (nothing has ever been deployed) or
/// carries no stamp.
pub fn read_script_generation(fs: &dyn Fs, paths: &dyn Pather) -> Option<u64> {
    read_script(fs, paths)
        .as_deref()
        .and_then(parse_script_generation)
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
    /// generation, from the binary now running.
    Healthy,
    /// Shells are loading dodot, but a *different* dodot than the one
    /// running (`shell-hookup-ergonomics.lex` §2.3). The state the
    /// version field exists to make visible: a hookup can be wired,
    /// sourced on every shell start, and still dead, and every other
    /// state would call this one healthy or stale and send the user to
    /// a fix that cannot work.
    VersionSkew,
    /// Shells are loading dodot and the script they load deploys
    /// nothing — after `down`, in a repository where every pack is
    /// ignored, or after a first `up` that deployed nothing. A healthy
    /// hookup line here is technically true and practically
    /// misleading.
    EmptyScript,
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
            ActivationState::VersionSkew => "version-skew",
            ActivationState::EmptyScript => "empty-script",
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

// ── The evidence, gathered ───────────────────────────────────────────

/// Everything the footer is computed from: both raw signals, the
/// generation they are judged against, and the facts about this
/// process they are compared to — its version, its session, its clock.
///
/// Reading this is the only IO on the path ([`Evidence::collect`]);
/// [`Evidence::state`] and [`Evidence::footer`] are pure functions of
/// it, so every state and every rendered string can be tested by
/// building one of these by hand.
#[derive(Debug, Clone)]
pub struct Evidence {
    /// What the calling shell's environment reports.
    pub stamp: EnvStamp,
    /// What the last shell to source init left behind, if any.
    pub heartbeat: Option<Heartbeat>,
    /// The generation a healthy shell would be running — see the
    /// module docs on who supplies it.
    pub reference: Option<u64>,
    /// Whether the generated init script contributes anything at all
    /// (`crate::shell::script_has_contributions`).
    pub script_has_contributions: bool,
    /// Whether this process is attached to a terminal — the session
    /// evidence that breaks the stampless tie (#279).
    pub tty: bool,
    /// The version of the binary rendering this footer.
    pub running_version: String,
    /// "Now", for the relative time on line two. An input so the
    /// rendered strings are assertable.
    pub now: SystemTime,
}

impl Evidence {
    /// Read both signals and the init script off disk.
    ///
    /// `stamp` and `tty` are passed in rather than read here so
    /// evaluation stays a function of its inputs: production snapshots
    /// both once into
    /// [`ExecutionContext`](crate::packs::orchestration::ExecutionContext)
    /// (via [`EnvStamp::read`] and an `isatty` check), tests hand over
    /// values. Callers that judge on the classic two-signal ladder
    /// alone (`up`, `install`) pass `tty: false`; only the callers that
    /// cannot measure supply real session evidence — see
    /// [`notice_for`].
    ///
    /// `None` when nothing has ever been deployed: with no init script
    /// on disk there is no hookup to have.
    pub fn collect(
        fs: &dyn Fs,
        paths: &dyn Pather,
        stamp: EnvStamp,
        reference: Option<u64>,
        tty: bool,
    ) -> Option<Evidence> {
        let script = read_script(fs, paths)?;
        Some(Evidence {
            stamp,
            heartbeat: read_heartbeat(fs, paths),
            reference,
            script_has_contributions: crate::shell::script_has_contributions(&script),
            tty,
            running_version: running_version().to_string(),
            now: SystemTime::now(),
        })
    }

    /// The version the evidence says was loaded, or `None` when it says
    /// nothing was.
    ///
    /// The stamp wins over the heartbeat for the same reason it wins in
    /// [`evaluate`]: it is direct evidence about the shell the user is
    /// typing in, where the heartbeat speaks for some past session.
    pub fn loaded_version(&self) -> Option<EvidenceVersion> {
        self.stamp
            .evidence_version()
            .or_else(|| self.heartbeat.as_ref().map(Heartbeat::evidence_version))
    }

    /// Whether the loaded version differs from the running one.
    ///
    /// Never true on an absent signal: with nothing loaded there is
    /// nothing to differ. Version-less evidence counts as a difference
    /// for every release after [`PRE_VERSION_RELEASE`] — see
    /// [`EvidenceVersion::is`] for why the bound release itself is the
    /// one exception.
    fn skewed(&self) -> bool {
        self.loaded_version()
            .is_some_and(|v| !v.is(&self.running_version))
    }

    /// Fold everything into one state.
    ///
    /// The generation ladder ([`evaluate`]) decides first, then two
    /// rules override it:
    ///
    /// - **Version skew** replaces `Healthy` and `StaleShell`. Both say
    ///   the hookup is working; a version mismatch says the working
    ///   hookup is running the wrong dodot, which is strictly more
    ///   specific and is the one thing "open a new shell" may not fix.
    ///   It never fires on staleness alone — a mismatch is required —
    ///   and it never overrides a state that already says a shell
    ///   loaded nothing, where the missing hookup is the news and the
    ///   version still shows on line two.
    /// - **An empty script** replaces `Healthy` only: the hookup is
    ///   sound and the script it sources deploys nothing, which is the
    ///   one claim a healthy line would get wrong. A hookup that is
    ///   *also* broken has a bigger problem to report first.
    pub fn state(&self) -> ActivationState {
        let ladder = evaluate(
            classify_stamp(self.stamp.generation, self.reference),
            classify_heartbeat(
                self.heartbeat.as_ref().and_then(|h| h.generation),
                self.reference,
            ),
            self.tty,
        );
        match ladder {
            ActivationState::Healthy | ActivationState::StaleShell if self.skewed() => {
                ActivationState::VersionSkew
            }
            ActivationState::Healthy if !self.script_has_contributions => {
                ActivationState::EmptyScript
            }
            other => other,
        }
    }

    /// Line two: when a shell last loaded dodot, and which dodot.
    ///
    /// The time comes from the heartbeat file's mtime — a property of
    /// the last shell that *sourced* the script — never from the
    /// generation written inside it, which is a property of the `up`
    /// that *wrote* it. Under the file-source hook those diverge
    /// permanently (spec §2.2).
    pub fn evidence_line(&self) -> String {
        let Some(version) = self.loaded_version() else {
            return "Never loaded.".into();
        };
        let tail = if self.state() == ActivationState::VersionSkew {
            format!(" — you are running {}.", self.running_version)
        } else {
            ".".into()
        };
        match self.elapsed_since_last_run() {
            Some(elapsed) => format!("Last loaded {} by dodot {version}{tail}", relative(elapsed)),
            None => format!("Last loaded at an unknown time by dodot {version}{tail}"),
        }
    }

    /// How long ago a shell last sourced the init script, from the
    /// heartbeat's mtime. `None` when there is no heartbeat, the
    /// filesystem would not say, or the mtime is in the future (a
    /// clock change, which is not a duration worth rendering).
    fn elapsed_since_last_run(&self) -> Option<Duration> {
        let last_run = self.heartbeat.as_ref()?.last_run?;
        self.now.duration_since(last_run).ok()
    }

    /// Render the two-line footer.
    ///
    /// `scan` is the static rc scan ([`rc::scan_expected_rc`]), which
    /// only [`ActivationState::ShellNotLoaded`] consults for its next
    /// step; `hook_line` the manual line every "wire it up" hint
    /// carries.
    pub fn footer(
        &self,
        hook_line: &str,
        scan: Option<(HookPresence, String)>,
    ) -> ActivationNotice {
        ActivationNotice::for_state(self.state(), hook_line, scan, self.evidence_line())
    }
}

/// Render a duration the way the footer says it: "4 minutes ago".
///
/// `timeago` with default features off, so no date-time crate enters
/// the tree and the edge cases of hand-rolled arithmetic stay someone
/// else's maintained problem. The one override is the below-a-minute
/// case, which reads better as "just now" than as the crate's "now" in
/// the sentence this lands in.
fn relative(elapsed: Duration) -> String {
    let mut formatter = timeago::Formatter::new();
    formatter.too_low("just now");
    formatter.convert(elapsed)
}

/// Evaluate the evidence and render the two-line footer, or `None` when
/// nothing has ever been deployed.
///
/// Silent before a first deploy: with no init script on disk there is
/// no hookup to have, and "no shell has loaded dodot yet" on a machine
/// where `dodot up` has never run is a warning about a non-problem.
///
/// `tty` is the session evidence (see [`evaluate`]); `shell_env` is
/// only consulted when the state is
/// [`ActivationState::ShellNotLoaded`], whose next step comes from the
/// static rc scan ([`rc::scan_expected_rc`]) — file reads, never a
/// shell spawn, so `status` stays inside spec §9.
pub fn notice_for(
    fs: &dyn Fs,
    paths: &dyn Pather,
    stamp: EnvStamp,
    reference: Option<u64>,
    tty: bool,
    shell_env: &rc::ShellEnv,
) -> Option<ActivationNotice> {
    let evidence = Evidence::collect(fs, paths, stamp, reference, tty)?;
    let hook_line = hook_line(&paths.init_script_path(), paths.home_dir());
    let scan = (evidence.state() == ActivationState::ShellNotLoaded)
        .then(|| rc::scan_expected_rc(fs, paths.home_dir(), shell_env, None))
        .flatten();
    Some(evidence.footer(&hook_line, scan))
}

/// The rendered two-line footer every `pack-status` render ends with.
///
/// `severity` names the presentation, not a failure level: `ok` for a
/// working hookup, `info` for one the next shell fixes, `warning` for
/// one no new shell will fix, `error` for one a measurement found
/// broken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActivationNotice {
    /// [`ActivationState::as_str`] — lets JSON consumers switch on the
    /// state without parsing the prose.
    pub state: String,
    /// `"ok"` | `"info"` | `"warning"` | `"error"`.
    pub severity: String,
    /// Line one: whether dodot is sourced in new shells.
    pub message: String,
    /// The fix, when line one reports a hookup that needs one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Line two: when a shell last loaded dodot, and which dodot.
    pub evidence: String,
}

impl ActivationNotice {
    /// Build the footer for a state.
    ///
    /// The line-one strings are pinned here — the user documentation
    /// quotes them verbatim, and the tests assert them as exact
    /// strings for the same reason. `hook_line` is the rc-file line
    /// every "wire it up" hint offers (see [`hook_line`]); `scan` is
    /// the rc scan only [`ActivationState::ShellNotLoaded`] consults;
    /// `evidence` is line two, from [`Evidence::evidence_line`].
    pub fn for_state(
        state: ActivationState,
        hook_line: &str,
        scan: Option<(HookPresence, String)>,
        evidence: String,
    ) -> ActivationNotice {
        let (severity, message, hint): (&str, &str, Option<String>) = match state {
            ActivationState::Healthy => ("ok", HEALTHY_MESSAGE, None),
            ActivationState::VersionSkew => (
                "warning",
                "Shell hookup: your shells load a different dodot.",
                Some(
                    "Open a new shell. If that changes nothing, the `dodot` your shells run is \
                     a different install than the one you just ran — check which one your PATH \
                     finds first."
                        .into(),
                ),
            ),
            ActivationState::EmptyScript => (
                "info",
                "Shell hookup: wired, but the init script is empty — nothing is deployed.",
                Some("Run `dodot up` to deploy your packs.".into()),
            ),
            ActivationState::StaleShell => (
                "info",
                "Shell hookup: this shell predates your last `dodot up`.",
                Some("Open a new shell to pick up the current deployment.".into()),
            ),
            ActivationState::NeverActivated => (
                "warning",
                "Shell hookup: no shell has loaded dodot yet.",
                Some(format!(
                    "Run `dodot install --write` to wire it up, or add this to your shell rc \
                     file yourself: {hook_line}"
                )),
            ),
            // The next step is whatever the rc scan found; the
            // headline states only what the evidence proves.
            ActivationState::ShellNotLoaded => (
                shell_not_loaded_severity(&scan),
                "Shell hookup: this shell did not load dodot.",
                Some(shell_not_loaded_hint(scan, hook_line)),
            ),
            // Only the probe reaches this state, and it renders its own
            // diagnosis (`probe::Verdict::notice`). This arm is the
            // answer for a caller that folds the state through the
            // evidence path anyway: same headline, generic next step.
            ActivationState::VerifiedBroken => (
                "error",
                VERIFIED_BROKEN_MESSAGE,
                Some(format!(
                    "Run `dodot install --write` to wire the hook, or add this to your shell rc \
                     file yourself: {hook_line}"
                )),
            ),
        };
        ActivationNotice {
            state: state.as_str().into(),
            severity: severity.into(),
            message: message.into(),
            hint,
            evidence,
        }
    }
}

/// How loudly to report [`ActivationState::ShellNotLoaded`].
///
/// A tty is not proof of a shell session — an IDE task runner
/// allocates one too, and legitimately reads no rc — so this is a
/// warning only when the rc scan found no hook, which is the one shape
/// no new shell will fix.
fn shell_not_loaded_severity(scan: &Option<(HookPresence, String)>) -> &'static str {
    match scan {
        Some((HookPresence::Absent, _)) => "warning",
        _ => "info",
    }
}

/// The next step for [`ActivationState::ShellNotLoaded`], from the
/// static scan of the rc the user's shell should be reading (`scan`:
/// presence + display path, as returned by [`rc::scan_expected_rc`]):
///
/// - hook absent → the rc has no hook, so no new shell will fix it;
///   name the file and the fix.
/// - hook present → this is most plausibly a shell opened before the
///   hook landed; open a new one, escalate to `dodot up` (which may
///   probe) if that changes nothing.
/// - `None` (unsupported shell, no rc to name) → just the manual line.
fn shell_not_loaded_hint(scan: Option<(HookPresence, String)>, hook_line: &str) -> String {
    match scan {
        Some((HookPresence::Absent, rc_path)) => format!(
            "{rc_path} doesn't have the dodot hook. Run `dodot install --write` to \
             wire it up, or add this line yourself: {hook_line}"
        ),
        Some((_, rc_path)) => format!(
            "The hook is in {rc_path}, so this shell probably predates it — open a \
             new shell. If that changes nothing, run `dodot up` to diagnose."
        ),
        None => format!(
            "dodot could not tell which rc file your shell reads. Make sure it has \
             this line: {hook_line}"
        ),
    }
}

/// Headline for a hookup the probe measured as broken. Shared so the
/// evidence path and [`crate::shell::probe`] can never drift into two
/// different phrasings of the same finding.
pub const VERIFIED_BROKEN_MESSAGE: &str = "Shell hookup: a new shell did not load dodot.";

/// Headline for a hookup the probe measured as working. The same
/// string [`ActivationState::Healthy`] renders: the probe's
/// contribution is that the claim is measured rather than inferred,
/// not a different claim.
pub const HEALTHY_MESSAGE: &str = "Shell hookup: dodot is sourced in new shells.";

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

    /// A fixed "now" for the pure footer tests, so every relative time
    /// they assert is a constant rather than a race with the clock.
    const NOW: SystemTime = SystemTime::UNIX_EPOCH;

    /// A `SystemTime` `seconds` before [`NOW`].
    fn ago(seconds: u64) -> Option<SystemTime> {
        Some(NOW - Duration::from_secs(seconds))
    }

    /// A heartbeat from its raw file contents and mtime.
    fn heartbeat(raw: &str, last_run: Option<SystemTime>) -> Heartbeat {
        let (generation, version) = parse_heartbeat(raw);
        Heartbeat {
            generation,
            version,
            last_run,
        }
    }

    /// The baseline the footer tests vary one field of: a shell running
    /// the current generation and the current version, four minutes
    /// ago, against a script that deploys something. Every state below
    /// is one deliberate deviation from this.
    fn skewless() -> Evidence {
        Evidence {
            stamp: EnvStamp::default(),
            heartbeat: Some(heartbeat("100 5.6.0", ago(4 * 60))),
            reference: Some(100),
            script_has_contributions: true,
            tty: false,
            running_version: "5.6.0".into(),
            now: NOW,
        }
    }

    /// Deploy one shell source so the generated init script carries a
    /// pack contribution — otherwise every script is the empty one and
    /// every healthy reading comes back as
    /// [`ActivationState::EmptyScript`].
    fn deploy_a_shell_source(env: &TempEnvironment) {
        let shell_dir = env.paths.handler_data_dir("vim", "shell");
        env.fs.mkdir_all(&shell_dir).unwrap();
        let target = env.home.join("aliases.sh");
        env.fs.write_file(&target, b"alias v=vim").unwrap();
        env.fs
            .symlink(&target, &shell_dir.join("aliases.sh"))
            .unwrap();
    }

    /// Write heartbeat contents, creating the directory the generated
    /// script's redirect would have relied on.
    fn plant_heartbeat(env: &TempEnvironment, contents: &str) {
        env.fs.mkdir_all(&env.paths.probes_hookup_dir()).unwrap();
        env.fs
            .write_file(&env.paths.hookup_heartbeat_path(), contents.as_bytes())
            .unwrap();
    }

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
    fn end_to_end_generation_matrix_through_collected_evidence() {
        // Same matrix, driven by raw generations through the IO entry
        // point: reference 100, stamps/heartbeats above and below it.
        // Every signal here carries the running version, so the
        // version-skew rule stays out of the way of the ladder.
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
            deploy_a_shell_source(&env);
            crate::shell::write_init_script(env.fs.as_ref(), env.paths.as_ref(), false, None).unwrap();
            if let Some(h) = heartbeat {
                plant_heartbeat(&env, &format!("{h} {}", running_version()));
            }
            let stamp = EnvStamp {
                generation: stamp,
                version: Some(running_version().into()),
            };
            let evidence =
                Evidence::collect(env.fs.as_ref(), env.paths.as_ref(), stamp, Some(100), tty)
                    .expect("a script is on disk");
            assert_eq!(
                evidence.state(),
                expected,
                "stamp={:?} heartbeat={heartbeat:?} tty={tty}",
                evidence.stamp.generation
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
        plant_heartbeat(&env, "garbage");
        let heartbeat = read_heartbeat(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert_eq!(heartbeat.generation, None);
    }

    #[test]
    fn a_heartbeat_splits_into_generation_and_version() {
        assert_eq!(
            parse_heartbeat("1786830419 5.6.0\n"),
            (Some(1_786_830_419), Some("5.6.0".into()))
        );
        // The pre-RCS01 shape: generation only.
        assert_eq!(parse_heartbeat("1786830419"), (Some(1_786_830_419), None));
        assert_eq!(parse_heartbeat(""), (None, None));
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

    // ── Version skew ─────────────────────────────────────────────────

    #[test]
    fn a_version_less_signal_renders_as_the_bound_not_as_unknown() {
        assert_eq!(EvidenceVersion::from_field(None).to_string(), "≤5.5.1");
        assert_eq!(
            EvidenceVersion::from_field(Some("  ")).to_string(),
            "≤5.5.1"
        );
        assert_eq!(
            EvidenceVersion::from_field(Some("5.6.0")).to_string(),
            "5.6.0"
        );
        assert!(!EvidenceVersion::PreVersion.is("5.6.0"));
        assert!(EvidenceVersion::Known("5.6.0".into()).is("5.6.0"));
        // The bound matches only the release it bounds: a binary that
        // is itself 5.5.1 cannot tell its own version-less evidence
        // apart from an older dodot's, so it claims no skew.
        assert!(EvidenceVersion::PreVersion.is(PRE_VERSION_RELEASE));
    }

    /// A pre-RCS01 heartbeat — generation, no version — is the shape
    /// every upgrading user has on disk. It must read as a bounded
    /// version, name both sides, and never panic.
    #[test]
    fn a_version_less_heartbeat_reports_skew_against_the_bound() {
        let evidence = Evidence {
            stamp: EnvStamp::default(),
            heartbeat: Some(heartbeat("1786830419", ago(4 * 60))),
            reference: Some(1_786_830_419),
            script_has_contributions: true,
            tty: false,
            running_version: "5.6.0".into(),
            now: NOW,
        };
        assert_eq!(evidence.state(), ActivationState::VersionSkew);
        assert_eq!(
            evidence.evidence_line(),
            "Last loaded 4 minutes ago by dodot ≤5.5.1 — you are running 5.6.0."
        );
    }

    #[test]
    fn skew_is_a_version_mismatch_never_staleness_alone() {
        // Stale generation, matching version: the existing stale-shell
        // state, whose fix (a new shell) is the right one.
        let stale = Evidence {
            stamp: EnvStamp {
                generation: Some(99),
                version: Some("5.6.0".into()),
            },
            ..skewless()
        };
        assert_eq!(stale.state(), ActivationState::StaleShell);

        // Current generation, different version: the failure the epic
        // exists to catch — wired, sourced, and running the wrong dodot.
        let skewed = Evidence {
            stamp: EnvStamp {
                generation: Some(100),
                version: Some("5.0.0".into()),
            },
            ..skewless()
        };
        assert_eq!(skewed.state(), ActivationState::VersionSkew);
        assert_eq!(
            skewed.evidence_line(),
            "Last loaded 4 minutes ago by dodot 5.0.0 — you are running 5.6.0."
        );

        // Nothing loaded at all: no version to differ from.
        let never = Evidence {
            stamp: EnvStamp::default(),
            heartbeat: None,
            ..skewless()
        };
        assert_eq!(never.state(), ActivationState::NeverActivated);
        assert_eq!(never.evidence_line(), "Never loaded.");
    }

    /// The stamp speaks for the shell the user is typing in, so it
    /// decides the version the same way it decides the generation.
    #[test]
    fn the_stamp_version_outranks_the_heartbeat_version() {
        let evidence = Evidence {
            stamp: EnvStamp {
                generation: Some(100),
                version: Some("5.6.0".into()),
            },
            heartbeat: Some(heartbeat("100 5.0.0", ago(4 * 60))),
            ..skewless()
        };
        assert_eq!(evidence.state(), ActivationState::Healthy);
        assert_eq!(
            evidence.evidence_line(),
            "Last loaded 4 minutes ago by dodot 5.6.0."
        );
    }

    // ── Run time is not generation time ──────────────────────────────

    /// The measured divergence from the spec (§2.2): heartbeat content
    /// written when `up` generated the script, mtime written when a
    /// shell last sourced it. The footer must report the mtime.
    #[test]
    fn last_loaded_comes_from_the_heartbeat_mtime_not_its_contents() {
        let env = TempEnvironment::builder().build();
        deploy_a_shell_source(&env);
        crate::shell::write_init_script(env.fs.as_ref(), env.paths.as_ref(), false, None).unwrap();
        // Content says 18:46 (when `up` wrote the script); the file was
        // last touched 45 minutes later, by the shell that sourced it.
        plant_heartbeat(&env, &format!("1786830419 {}", running_version()));
        let touched = SystemTime::now() - Duration::from_secs(45 * 60);
        env.fs
            .set_modified(&env.paths.hookup_heartbeat_path(), touched)
            .unwrap();

        let evidence = Evidence::collect(
            env.fs.as_ref(),
            env.paths.as_ref(),
            EnvStamp::default(),
            Some(1_786_830_419),
            false,
        )
        .unwrap();

        assert_eq!(
            evidence.evidence_line(),
            format!("Last loaded 45 minutes ago by dodot {}.", running_version()),
            "the generation inside the file is a write time, not a run time"
        );
    }

    #[test]
    fn an_unreadable_mtime_says_so_instead_of_guessing() {
        let evidence = Evidence {
            heartbeat: Some(heartbeat("100 5.6.0", None)),
            ..skewless()
        };
        assert_eq!(
            evidence.evidence_line(),
            "Last loaded at an unknown time by dodot 5.6.0."
        );
    }

    // ── Footer strings, per state ────────────────────────────────────

    /// WS04's documentation quotes these, so they are pinned as exact
    /// strings — a reworded line is a doc bug, not a cosmetic change.
    #[test]
    fn every_state_renders_its_pinned_two_lines() {
        let cases = [
            (
                ActivationState::Healthy,
                "ok",
                "Shell hookup: dodot is sourced in new shells.",
            ),
            (
                ActivationState::VersionSkew,
                "warning",
                "Shell hookup: your shells load a different dodot.",
            ),
            (
                ActivationState::EmptyScript,
                "info",
                "Shell hookup: wired, but the init script is empty — nothing is deployed.",
            ),
            (
                ActivationState::StaleShell,
                "info",
                "Shell hookup: this shell predates your last `dodot up`.",
            ),
            (
                ActivationState::NeverActivated,
                "warning",
                "Shell hookup: no shell has loaded dodot yet.",
            ),
            (
                ActivationState::ShellNotLoaded,
                "info",
                "Shell hookup: this shell did not load dodot.",
            ),
            (
                ActivationState::VerifiedBroken,
                "error",
                "Shell hookup: a new shell did not load dodot.",
            ),
        ];
        for (state, severity, message) in cases {
            let notice = ActivationNotice::for_state(state, "HOOK", None, "Never loaded.".into());
            assert_eq!(notice.state, state.as_str());
            assert_eq!(notice.severity, severity, "{state:?}");
            assert_eq!(notice.message, message, "{state:?}");
            assert_eq!(notice.evidence, "Never loaded.", "{state:?}");
        }
    }

    #[test]
    fn the_healthy_footer_reads_as_the_spec_tabulates_it() {
        let evidence = Evidence {
            stamp: EnvStamp {
                generation: Some(100),
                version: Some("5.6.0".into()),
            },
            ..skewless()
        };
        let notice = evidence.footer("HOOK", None);
        assert_eq!(
            notice.message,
            "Shell hookup: dodot is sourced in new shells."
        );
        assert_eq!(notice.evidence, "Last loaded 4 minutes ago by dodot 5.6.0.");
        assert_eq!(notice.hint, None);
    }

    /// One rule, three occasions: after `down`, with every pack
    /// ignored, and after a first `up` that deployed nothing. All three
    /// reach here as "the script carries no contributions".
    #[test]
    fn a_contribution_less_script_says_so_instead_of_claiming_health() {
        let evidence = Evidence {
            stamp: EnvStamp {
                generation: Some(100),
                version: Some("5.6.0".into()),
            },
            script_has_contributions: false,
            ..skewless()
        };
        assert_eq!(evidence.state(), ActivationState::EmptyScript);

        // A hookup that is *also* broken has a bigger problem to report.
        let broken = Evidence {
            stamp: EnvStamp::default(),
            heartbeat: None,
            script_has_contributions: false,
            ..skewless()
        };
        assert_eq!(broken.state(), ActivationState::NeverActivated);
    }

    // ── Notices ──────────────────────────────────────────────────────

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

        let notice = ActivationNotice::for_state(
            ActivationState::NeverActivated,
            &hook,
            None,
            "Never loaded.".into(),
        );
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
        let not_loaded = |scan| {
            ActivationNotice::for_state(
                ActivationState::ShellNotLoaded,
                "HOOK",
                scan,
                "Never loaded.".into(),
            )
        };

        // Hook absent: no new shell fixes it — name the file and the fix.
        let absent = not_loaded(Some((HookPresence::Absent, "~/.zshrc".into())));
        assert_eq!(absent.state, "shell-not-loaded");
        assert_eq!(absent.severity, "warning");
        assert_eq!(
            absent.message,
            "Shell hookup: this shell did not load dodot."
        );
        let hint = absent.hint.unwrap();
        assert!(hint.contains("~/.zshrc") && hint.contains("dodot install --write"));
        assert!(!hint.contains("new shell"), "{hint}");

        // Hook present (managed or manual): an old shell is the likely
        // story — advise a new one, escalate to `up` after that.
        for presence in [HookPresence::ManagedBlock, HookPresence::Manual] {
            let present = not_loaded(Some((presence, "~/.zshrc".into())));
            assert_eq!(present.severity, "info");
            let hint = present.hint.unwrap();
            assert!(
                hint.contains("new shell") && hint.contains("dodot up"),
                "{hint}"
            );
        }

        // No rc to name (unsupported shell): just the manual line.
        let unknown = not_loaded(None);
        assert_eq!(unknown.severity, "info");
        assert!(unknown.hint.unwrap().contains("HOOK"));
    }

    #[test]
    fn stale_shell_says_open_a_new_shell() {
        let notice = ActivationNotice::for_state(
            ActivationState::StaleShell,
            "hook",
            None,
            "Never loaded.".into(),
        );
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

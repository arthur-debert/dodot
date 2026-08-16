//! The verification probe — measuring whether a *new* shell would
//! activate dodot (`docs/proposals/shipped/shell-hookup.lex` §3).
//!
//! Signals 1 and 2 (the environment stamp and the heartbeat, both in
//! [`crate::shell::activation`]) answer "is this shell live?" and "has
//! any shell activated?". Neither answers the question a fresh install
//! or a freshly broken hookup actually poses: *would the next terminal
//! the user opens load dodot?* Only running one answers that, so when
//! the cheap signals come back inconclusive, dodot spawns the user's
//! shell and reads the stamp back out of it.
//!
//! # Gating
//!
//! [`gate_says_probe`] is the whole cost story. A current stamp or a
//! fresh heartbeat means shells are activating, and the probe never
//! runs. The first real shell activation writes the heartbeat and
//! retires the probe until the hookup actually breaks — so a healthy
//! machine pays nothing, forever. `dodot status` never spawns a shell
//! at all (spec §9); only `up` and `install` may, and only through
//! [`ProbePolicy::Gated`], which every non-production
//! [`ExecutionContext`](crate::packs::ExecutionContext) leaves at
//! [`ProbePolicy::Never`].
//!
//! # Mechanics
//!
//! User rc files prompt, hang, `exec` into multiplexers, and fail in
//! creative ways, so [`run`] is defensive by construction: interactive
//! non-login shell (the mode that reads the file the hook lives in),
//! stdin from `/dev/null`, output captured, a hard timeout that kills
//! the whole process group, and `DODOT_INIT_*` scrubbed from the child
//! environment — an inherited stamp would be a false positive every
//! time the probe runs from an already-live shell.
//!
//! # Verdicts
//!
//! [`Verdict`] keeps three outcomes apart because they demand
//! different action: verified, verified-broken (with the static rc
//! scan splitting *hook absent* from *hook present but never
//! reached*), and couldn't-verify, which degrades to the scan's answer
//! labeled as configuration state. A probe failure never wedges `up`.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::fs::Fs;
use crate::paths::Pather;
use crate::shell::activation::{
    self, ActivationNotice, ActivationState, HeartbeatState, StampState, INIT_GEN_ENV,
};
use crate::shell::rc::{self, HookPresence, ShellEnv};

/// Prefix the probe command prints its stamp behind, so the one bit we
/// read survives arbitrary rc noise on the same stream.
pub const PROBE_MARKER: &str = "dodot-probe-gen:";

/// How long a spawned shell gets before its process group is killed.
/// Spec §3.2 calls for "order of 5 seconds": long enough for a heavy
/// rc file, short enough that a hung one is not a hung `dodot up`.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Environment prefix scrubbed from the probed shell.
const SCRUB_PREFIX: &str = "DODOT_INIT_";

/// How long to wait between liveness checks on the spawned shell.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

// ── Policy ──────────────────────────────────────────────────────

/// Whether a command may spawn a shell while judging activation.
///
/// *Which* shell is not this type's business — that comes from the
/// context's [`ShellEnv`], the same value the rc ladder reads, so a
/// probe can never measure one shell while the diagnosis names another
/// shell's rc file.
///
/// Defaults to [`ProbePolicy::Never`], which is what makes "no test
/// ever spawns the developer's real shell" a property of the type
/// rather than of everyone's discipline: only
/// [`ExecutionContext::production`](crate::packs::ExecutionContext::production)
/// opts in, and probe tests opt in with a fabricated shell.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProbePolicy {
    /// Report from evidence alone. Every test context, and `status`
    /// no matter the context.
    #[default]
    Never,
    /// Spawn the shell when [`gate_says_probe`] says the cheap signals
    /// are inconclusive, giving it `timeout` to finish starting up.
    Gated { timeout: Duration },
}

impl ProbePolicy {
    /// The production policy: gated, with the standard timeout.
    pub fn production() -> Self {
        ProbePolicy::Gated {
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// The timeout when spawning is allowed at all, else `None`.
    pub fn timeout(&self) -> Option<Duration> {
        match self {
            ProbePolicy::Never => None,
            ProbePolicy::Gated { timeout } => Some(*timeout),
        }
    }
}

/// Whether the cheap signals leave anything worth measuring.
///
/// A current stamp means the calling shell is live; a fresh heartbeat
/// means some shell activated since the last regeneration. Either is
/// proof enough, and proof is cheaper than measurement. Only when
/// neither holds — the fresh-install and broken-hook cases, precisely
/// — is a shell spawn justified (spec §3.1).
pub fn gate_says_probe(stamp: StampState, heartbeat: HeartbeatState) -> bool {
    !matches!(stamp, StampState::Current) && !matches!(heartbeat, HeartbeatState::Fresh)
}

// ── Running one ─────────────────────────────────────────────────

/// What one shell spawn reported back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// The shell finished and printed a parseable generation stamp.
    Stamp(u64),
    /// The shell finished and printed no stamp — it did not source
    /// dodot's init script.
    NoStamp,
    /// The shell outlived its timeout; its process group was killed.
    TimedOut,
    /// The shell could not be run at all.
    SpawnFailed(String),
}

/// The line the probe prints before spawning. The probe is announced,
/// never covert (spec §3.2): every terminal open runs the user's rc
/// anyway, and the only unacceptable version of this is a silent one.
pub fn announcement(shell: &Path) -> String {
    let name = shell
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("your shell");
    format!("verifying shell integration ({name})…")
}

/// The command handed to the spawned shell: print the stamp we may or
/// may not have inherited from its rc, behind a marker.
fn probe_command() -> String {
    format!("printf '{PROBE_MARKER}%s\\n' \"${{{INIT_GEN_ENV}-}}\"")
}

/// Extract the stamp from captured probe output.
///
/// Scans every line for the marker and takes the last one: an rc file
/// is free to print whatever it likes before our command runs, and the
/// probe reads exactly one bit out of the noise.
pub fn parse_probe_output(stdout: &str) -> Option<u64> {
    stdout
        .lines()
        .filter_map(|line| line.trim().strip_prefix(PROBE_MARKER))
        .filter_map(activation::parse_generation)
        .next_back()
}

/// Spawn `shell` interactively and read the stamp back.
///
/// Safe against hostile rc files by construction — see the module
/// docstring. Never returns an error: every failure mode is an
/// outcome, because a probe that could not run must degrade, not
/// propagate (spec §3.3).
pub fn run(shell: &Path, timeout: Duration) -> ProbeOutcome {
    let mut command = Command::new(shell);
    command
        // Interactive non-login: the mode that reads the rc file the
        // hook lives in.
        .arg("-ic")
        .arg(probe_command())
        // No tty, nothing to read: an rc file that prompts gets EOF
        // instead of blocking forever.
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for key in scrubbed_keys(std::env::vars().map(|(k, _)| k)) {
        command.env_remove(key);
    }
    // Its own process group, so a timeout can take out everything the
    // rc file spawned, not just the shell we can see.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => return ProbeOutcome::SpawnFailed(format!("{e}")),
    };
    let pid = child.id();

    // Drain both pipes off-thread: a chatty rc file that fills a pipe
    // buffer would otherwise deadlock against our own wait loop.
    let stdout = child.stdout.take().map(drain);
    let stderr = child.stderr.take().map(drain);

    let deadline = Instant::now() + timeout;
    let timed_out = loop {
        match child.try_wait() {
            Ok(Some(_)) => break false,
            Ok(None) => {}
            Err(e) => return ProbeOutcome::SpawnFailed(format!("{e}")),
        }
        if Instant::now() >= deadline {
            kill_process_group(pid);
            // Reap: the group is dead, so this returns promptly and we
            // leave no zombie behind.
            let _ = child.wait();
            break true;
        }
        std::thread::sleep(POLL_INTERVAL);
    };

    let captured = stdout.and_then(|h| h.join().ok()).unwrap_or_default();
    drop(stderr.map(|h| h.join()));

    if timed_out {
        return ProbeOutcome::TimedOut;
    }
    // A nonzero exit is not a failed probe: unrelated rc breakage
    // fails loudly and still activates dodot. The stamp is the bit.
    match parse_probe_output(&captured) {
        Some(generation) => ProbeOutcome::Stamp(generation),
        None => ProbeOutcome::NoStamp,
    }
}

/// Read a child pipe to end on its own thread.
fn drain<R: Read + Send + 'static>(mut pipe: R) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = pipe.read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).into_owned()
    })
}

/// The environment keys the child must not inherit.
///
/// The child inherits our exports, and this process may well have been
/// started *by* an activated shell — an inherited stamp would make
/// every probe from a live shell report success regardless of what the
/// spawned one did (spec §3.2).
pub fn scrubbed_keys(keys: impl Iterator<Item = String>) -> Vec<String> {
    keys.filter(|k| k.starts_with(SCRUB_PREFIX)).collect()
}

/// Kill the probed shell's whole process group.
///
/// The shell was spawned as its own group leader, so its pid is the
/// group id and a negative-pid signal reaches every process the rc
/// file started — the `exec`-into-a-multiplexer case, where killing
/// only the shell we can see would leave the real hang behind.
#[cfg(unix)]
fn kill_process_group(pid: u32) {
    // SAFETY: `kill` is a plain syscall wrapper with no memory
    // effects. A negative pid addresses the process group; the group
    // is one we created via `process_group(0)`, so we are not
    // signalling anything we did not spawn. A failure (the group
    // already exited) is nothing to handle.
    unsafe {
        libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
    }
}

// ── Verdicts ────────────────────────────────────────────────────

/// Why a measured-broken hookup is broken (spec §3.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diagnosis {
    /// The hook is not in the rc file the shell reads.
    HookAbsent { rc: String },
    /// The hook is there, but the shell never got to it — something
    /// earlier in the file is failing.
    HookNotReached { rc: String },
    /// The shell activated, but from an init script older than the one
    /// on disk. The hookup works; what it sources is stale.
    StaleScript { found: u64, expected: u64 },
    /// No rc file could be named (unsupported shell), so the scan has
    /// nothing to say. The hook line is all we can offer.
    Unknown,
}

/// The measured answer to "would a new shell activate dodot?"
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Measured ✓ — the spawned shell reported the current generation.
    Verified { generation: u64 },
    /// The shell ran and did not activate dodot.
    Broken { diagnosis: Diagnosis },
    /// The measurement itself failed. Degrade to configuration state.
    Unverified { reason: String },
}

impl Verdict {
    /// Fold one spawn's outcome into a verdict, using the static rc
    /// scan only where spec §2.2 allows it: to explain a failure.
    pub fn from_outcome(
        outcome: ProbeOutcome,
        reference: Option<u64>,
        hook: Option<(HookPresence, String)>,
    ) -> Verdict {
        match outcome {
            ProbeOutcome::Stamp(found) => {
                match activation::classify_stamp(Some(found), reference) {
                    StampState::Current => Verdict::Verified { generation: found },
                    // Unreachable in practice — a new shell sources the
                    // script we just wrote — but a real answer beats
                    // rounding it up to "verified".
                    _ => Verdict::Broken {
                        diagnosis: Diagnosis::StaleScript {
                            found,
                            expected: reference.unwrap_or(found),
                        },
                    },
                }
            }
            ProbeOutcome::NoStamp => Verdict::Broken {
                diagnosis: match hook {
                    Some((presence, rc)) if presence.is_present() => {
                        Diagnosis::HookNotReached { rc }
                    }
                    Some((_, rc)) => Diagnosis::HookAbsent { rc },
                    None => Diagnosis::Unknown,
                },
            },
            ProbeOutcome::TimedOut => Verdict::Unverified {
                reason: "your shell did not finish starting up in time".into(),
            },
            ProbeOutcome::SpawnFailed(e) => Verdict::Unverified {
                reason: format!("could not run your shell ({e})"),
            },
        }
    }

    /// Render the verdict for `up` / `install` output.
    ///
    /// `evidence` is the footer the signal ladder alone produced,
    /// `evidence_line` the footer's second line re-read *after* the
    /// probe ran (the spawned shell updates the heartbeat, so the
    /// pre-probe reading is stale by the time there is a verdict), and
    /// `hook_line` the manual line to fall back on. A couldn't-verify
    /// verdict degrades to `evidence`, clearly labeled as
    /// configuration state rather than measured activation; the other
    /// two verdicts are measurements and take precedence over it —
    /// including over the stale-shell advice, which is the wrong
    /// answer for a hookup that just measured broken.
    ///
    /// A measured verdict says the same thing the evidence path says
    /// for the same state, in the same words
    /// ([`activation::HEALTHY_MESSAGE`]): what the measurement buys is
    /// that the claim is right, not that it is phrased differently.
    pub fn notice(
        &self,
        evidence: Option<ActivationNotice>,
        evidence_line: &str,
        hook_line: &str,
    ) -> Option<ActivationNotice> {
        match self {
            Verdict::Verified { .. } => Some(ActivationNotice {
                state: ActivationState::Healthy.as_str().into(),
                severity: "ok".into(),
                message: activation::HEALTHY_MESSAGE.into(),
                hint: None,
                evidence: evidence_line.into(),
            }),
            Verdict::Broken { diagnosis } => Some(ActivationNotice {
                state: ActivationState::VerifiedBroken.as_str().into(),
                severity: "error".into(),
                message: activation::VERIFIED_BROKEN_MESSAGE.into(),
                evidence: evidence_line.into(),
                hint: Some(match diagnosis {
                    Diagnosis::HookAbsent { rc } => format!(
                        "The dodot hook is missing from {rc} — run `dodot install --write` to add it."
                    ),
                    Diagnosis::HookNotReached { rc } => format!(
                        "The dodot hook is in {rc} but was never reached — something earlier in \
                         that file is failing before it."
                    ),
                    Diagnosis::StaleScript { found, expected } => format!(
                        "Your shell sourced an older init script (generation {found}, current is \
                         {expected}) — check for a second dodot hook or a stale copy."
                    ),
                    Diagnosis::Unknown => format!(
                        "dodot could not tell which rc file your shell reads. Add this line to it: \
                         {hook_line}"
                    ),
                }),
            }),
            Verdict::Unverified { reason } => {
                let mut notice = evidence?;
                notice.hint = Some(match notice.hint.take() {
                    Some(hint) => format!(
                        "{hint} (dodot could not verify by running your shell — {reason} — so this \
                         reports your configuration, not measured activation.)"
                    ),
                    None => format!(
                        "dodot could not verify by running your shell — {reason} — so this reports \
                         your configuration, not measured activation."
                    ),
                });
                Some(notice)
            }
        }
    }
}

// ── The measured path ───────────────────────────────────────────

/// Run the probe and turn it into a notice, doing the rc scan for the
/// diagnosis.
///
/// `reference` is the generation a shell started now would pick up
/// (the script on disk), and `evidence` the signals-only notice this
/// replaces when the measurement succeeds. `rc_override` names the
/// file the diagnosis should talk about when the caller already knows
/// it (`dodot install --rc`), instead of re-walking the ladder.
pub fn measure(
    fs: &dyn Fs,
    paths: &dyn Pather,
    timeout: Duration,
    shell_env: &ShellEnv,
    rc_override: Option<&Path>,
    reference: Option<u64>,
    evidence: Option<ActivationNotice>,
) -> Option<ActivationNotice> {
    let hook_line = activation::hook_line(&paths.init_script_path(), paths.home_dir());
    let stale_line = evidence
        .as_ref()
        .map(|n| n.evidence.clone())
        .unwrap_or_else(|| "Never loaded.".into());
    let Some(shell) = shell_env.shell.as_deref().map(Path::new) else {
        return Verdict::Unverified {
            reason: "$SHELL is not set".into(),
        }
        .notice(evidence, &stale_line, &hook_line);
    };

    eprintln!("{}", announcement(shell));
    let outcome = run(shell, timeout);
    let hook = rc::scan_expected_rc(fs, paths.home_dir(), shell_env, rc_override);
    // Line two is re-read now, not before the spawn: a shell that
    // activated wrote the heartbeat on its way through, and "last
    // loaded 9 days ago" under a verdict that just watched it load
    // would contradict itself. A shell that did *not* activate left
    // the heartbeat alone, so the same read still reports the last
    // time one did — which is the evidence the broken verdict wants.
    let evidence_line =
        activation::Evidence::collect(fs, paths, activation::EnvStamp::default(), reference, false)
            .map(|e| e.evidence_line())
            .unwrap_or(stale_line);
    Verdict::from_outcome(outcome, reference, hook).notice(evidence, &evidence_line, &hook_line)
}

/// Evaluate shell activation for a command that is allowed to measure.
///
/// Evidence first, always: [`gate_says_probe`] has to agree before a
/// shell is spawned, so the steady-state cost of this call on a
/// healthy machine is the two signal reads it would have done anyway.
///
/// `reference_for_gate` is the generation the *evidence* is judged
/// against (for `up`, the pre-regeneration one — see
/// `commands::shell_hookup_footer`), while the probe is judged against
/// the script on disk, which is what a shell started now would source.
///
/// `tty` is the session evidence the stampless ladder falls back on
/// (#279). A caller that will measure passes `false` — it gets the
/// real answer from the spawn instead of inferring one from the
/// session — so in practice this is only ever `true` alongside
/// [`ProbePolicy::Never`].
pub fn notice_with_probe(
    fs: &dyn Fs,
    paths: &dyn Pather,
    policy: &ProbePolicy,
    shell_env: &ShellEnv,
    env_stamp: &activation::EnvStamp,
    reference_for_gate: Option<u64>,
    tty: bool,
) -> Option<ActivationNotice> {
    let evidence = activation::notice_for(
        fs,
        paths,
        env_stamp.clone(),
        reference_for_gate,
        tty,
        shell_env,
    );
    let Some(timeout) = policy.timeout() else {
        return evidence;
    };
    if !fs.exists(&paths.init_script_path()) {
        // Nothing deployed: there is no hookup to measure yet.
        return evidence;
    }
    let stamp = activation::classify_stamp(env_stamp.generation, reference_for_gate);
    let heartbeat = activation::classify_heartbeat(
        activation::read_heartbeat(fs, paths).and_then(|h| h.generation),
        reference_for_gate,
    );
    if !gate_says_probe(stamp, heartbeat) {
        return evidence;
    }
    let reference = activation::read_script_generation(fs, paths);
    measure(fs, paths, timeout, shell_env, None, reference, evidence)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gate_fires_only_when_both_cheap_signals_are_inconclusive() {
        use HeartbeatState as H;
        use StampState as S;
        let matrix = [
            // A live shell is proof; never probe.
            (S::Current, H::Absent, false),
            (S::Current, H::Fresh, false),
            (S::Current, H::Old, false),
            // Some shell activated since the last regeneration; proof
            // enough, whatever this process inherited.
            (S::Absent, H::Fresh, false),
            (S::Stale, H::Fresh, false),
            // Fresh install: nothing has ever activated.
            (S::Absent, H::Absent, true),
            // Previously working, now nothing since the regeneration —
            // the broken-hook case the probe exists for.
            (S::Absent, H::Old, true),
            (S::Stale, H::Old, true),
            (S::Stale, H::Absent, true),
        ];
        for (stamp, heartbeat, expected) in matrix {
            assert_eq!(
                gate_says_probe(stamp, heartbeat),
                expected,
                "stamp={stamp:?} heartbeat={heartbeat:?}"
            );
        }
    }

    #[test]
    fn the_stamp_is_read_out_of_arbitrary_rc_noise() {
        let noisy = format!(
            "Welcome to your shell!\n[oh-my-zsh] update available\n{PROBE_MARKER}1755200000\n"
        );
        assert_eq!(parse_probe_output(&noisy), Some(1_755_200_000));
        // No stamp exported: the marker is there, the value is empty.
        assert_eq!(parse_probe_output(&format!("{PROBE_MARKER}\n")), None);
        assert_eq!(parse_probe_output("nothing at all\n"), None);
    }

    #[test]
    fn only_the_init_stamp_family_is_scrubbed() {
        let keys = [
            "DODOT_INIT_GEN",
            "DODOT_INIT_ANYTHING",
            "DODOT_DATA_DIR",
            "PATH",
            "HOME",
        ]
        .into_iter()
        .map(String::from);
        assert_eq!(
            scrubbed_keys(keys),
            vec!["DODOT_INIT_GEN".to_string(), "DODOT_INIT_ANYTHING".into()]
        );
    }

    #[test]
    fn the_announcement_names_the_shell_being_run() {
        assert_eq!(
            announcement(Path::new("/bin/zsh")),
            "verifying shell integration (zsh)…"
        );
    }

    // ── Verdicts ────────────────────────────────────────────────

    fn hook(presence: HookPresence) -> Option<(HookPresence, String)> {
        Some((presence, "~/.zshrc".to_string()))
    }

    #[test]
    fn a_current_stamp_is_a_measured_verification() {
        let v = Verdict::from_outcome(
            ProbeOutcome::Stamp(100),
            Some(100),
            hook(HookPresence::ManagedBlock),
        );
        assert_eq!(v, Verdict::Verified { generation: 100 });
        let notice = v
            .notice(None, "Last loaded just now by dodot 5.6.0.", "HOOK")
            .unwrap();
        assert_eq!(notice.state, "healthy");
        assert_eq!(notice.severity, "ok");
        // A measurement reports the healthy state in the state's own
        // words: what the spawn buys is that the claim is right, not a
        // second phrasing of it for the docs to keep in sync.
        assert_eq!(notice.message, activation::HEALTHY_MESSAGE);
        assert_eq!(notice.evidence, "Last loaded just now by dodot 5.6.0.");
    }

    #[test]
    fn no_stamp_plus_no_hook_names_the_file_and_the_command() {
        let v = Verdict::from_outcome(ProbeOutcome::NoStamp, Some(100), hook(HookPresence::Absent));
        assert_eq!(
            v,
            Verdict::Broken {
                diagnosis: Diagnosis::HookAbsent {
                    rc: "~/.zshrc".into()
                }
            }
        );
        let notice = v.notice(None, "Never loaded.", "HOOK").unwrap();
        assert_eq!(notice.state, "verified-broken");
        assert_eq!(notice.severity, "error");
        let hint = notice.hint.unwrap();
        assert!(hint.contains("~/.zshrc"), "{hint}");
        assert!(hint.contains("dodot install --write"), "{hint}");
    }

    #[test]
    fn no_stamp_with_the_hook_present_blames_the_rc_file_instead() {
        for presence in [HookPresence::ManagedBlock, HookPresence::Manual] {
            let v = Verdict::from_outcome(ProbeOutcome::NoStamp, Some(100), hook(presence));
            let hint = v
                .notice(None, "Never loaded.", "HOOK")
                .unwrap()
                .hint
                .unwrap();
            assert!(
                hint.contains("never reached"),
                "{presence:?} should diagnose a broken rc, not a missing hook: {hint}"
            );
            assert!(
                !hint.contains("dodot install --write"),
                "adding the hook again fixes nothing here: {hint}"
            );
        }
    }

    #[test]
    fn an_unknown_shell_falls_back_to_the_hook_line() {
        let v = Verdict::from_outcome(ProbeOutcome::NoStamp, Some(100), None);
        assert_eq!(
            v,
            Verdict::Broken {
                diagnosis: Diagnosis::Unknown
            }
        );
        let hint = v
            .notice(None, "Never loaded.", "THE-HOOK-LINE")
            .unwrap()
            .hint
            .unwrap();
        assert!(hint.contains("THE-HOOK-LINE"), "{hint}");
    }

    #[test]
    fn a_stale_sourced_script_is_reported_as_such() {
        let v = Verdict::from_outcome(
            ProbeOutcome::Stamp(90),
            Some(100),
            hook(HookPresence::ManagedBlock),
        );
        assert_eq!(
            v,
            Verdict::Broken {
                diagnosis: Diagnosis::StaleScript {
                    found: 90,
                    expected: 100
                }
            }
        );
    }

    #[test]
    fn a_failed_measurement_degrades_to_the_evidence_notice() {
        let evidence = ActivationNotice {
            state: "never-activated".into(),
            severity: "warning".into(),
            message: "Shell hookup: no shell has loaded dodot yet.".into(),
            hint: Some("Add this to your rc file: HOOK".into()),
            evidence: "Never loaded.".into(),
        };
        for outcome in [
            ProbeOutcome::TimedOut,
            ProbeOutcome::SpawnFailed("no such file".into()),
        ] {
            let v = Verdict::from_outcome(outcome, Some(100), hook(HookPresence::Absent));
            let notice = v
                .notice(Some(evidence.clone()), "Never loaded.", "HOOK")
                .unwrap();
            // The evidence verdict survives untouched...
            assert_eq!(notice.state, "never-activated");
            assert_eq!(notice.severity, "warning");
            // ...but is labeled as configuration, not measurement.
            let hint = notice.hint.unwrap();
            assert!(hint.starts_with("Add this to your rc file: HOOK"), "{hint}");
            assert!(hint.contains("could not verify"), "{hint}");
            assert!(hint.contains("not measured activation"), "{hint}");
        }
    }

    #[test]
    fn a_failed_measurement_with_nothing_to_degrade_to_stays_silent() {
        // Healthy-and-quiet evidence plus an unrunnable shell is not a
        // reason to invent a warning.
        let v = Verdict::Unverified {
            reason: "boom".into(),
        };
        assert_eq!(v.notice(None, "Never loaded.", "HOOK"), None);
    }

    // ── Spawn mechanics, against fabricated shells ──────────────
    //
    // Every test below runs a shell script this file wrote into a
    // temp dir. None of them can reach the developer's real `$SHELL`
    // or their real rc files — which is the whole point: the probe's
    // job is surviving hostile rc behaviour, and the only way to test
    // that is to fabricate the hostility.

    use crate::testing::TempEnvironment;
    use std::path::PathBuf;

    /// Write an executable fake `$SHELL`.
    ///
    /// It is invoked exactly as the real thing is — `<shell> -ic
    /// '<command>'` — so `$2` is the probe command, and `eval "$2"` is
    /// the fake's stand-in for "run the command after the rc file".
    /// What each fixture puts *before* that line is the rc behaviour
    /// under test.
    fn fake_shell(env: &TempEnvironment, name: &str, rc_behaviour: &str) -> PathBuf {
        let path = env.home.join(name);
        let script = format!("#!/bin/sh\n{rc_behaviour}\neval \"$2\"\n");
        env.fs
            .write_file_with_mode(&path, script.as_bytes(), 0o755)
            .unwrap();
        path
    }

    #[test]
    fn a_shell_that_activates_reports_the_stamp() {
        let env = TempEnvironment::builder().build();
        let shell = fake_shell(
            &env,
            "activating-shell",
            &format!("export {INIT_GEN_ENV}=1755200000"),
        );
        assert_eq!(
            run(&shell, Duration::from_secs(10)),
            ProbeOutcome::Stamp(1_755_200_000)
        );
    }

    #[test]
    fn a_shell_with_no_hook_reports_no_stamp() {
        let env = TempEnvironment::builder().build();
        let shell = fake_shell(&env, "bare-shell", "echo 'welcome to your shell'");
        assert_eq!(run(&shell, Duration::from_secs(10)), ProbeOutcome::NoStamp);
    }

    #[test]
    fn rc_noise_and_a_nonzero_exit_do_not_fail_a_successful_probe() {
        // Unrelated rc breakage is loud and irrelevant: the shell
        // still sourced dodot, so the probe still says verified.
        let env = TempEnvironment::builder().build();
        let path = env.home.join("noisy-shell");
        let script = format!(
            "#!/bin/sh\n\
             echo 'error: some unrelated rc line failed' >&2\n\
             echo 'p10k wants your attention'\n\
             export {INIT_GEN_ENV}=42\n\
             eval \"$2\"\n\
             exit 3\n"
        );
        env.fs
            .write_file_with_mode(&path, script.as_bytes(), 0o755)
            .unwrap();
        assert_eq!(run(&path, Duration::from_secs(10)), ProbeOutcome::Stamp(42));
    }

    #[test]
    fn a_hanging_rc_times_out_and_takes_its_children_with_it() {
        let env = TempEnvironment::builder().build();
        let pidfile = env.home.join("grandchild.pid");
        // A shell that hangs *and* leaves a background process behind
        // — the `exec`-into-a-multiplexer shape. Killing only the
        // shell we can see would leave that process running forever.
        let path = env.home.join("hanging-shell");
        let script = format!(
            "#!/bin/sh\nsh -c 'echo $$ > {pid}; sleep 300' &\nsleep 300\n",
            pid = pidfile.display()
        );
        env.fs
            .write_file_with_mode(&path, script.as_bytes(), 0o755)
            .unwrap();

        let start = Instant::now();
        let outcome = run(&path, Duration::from_millis(500));
        assert_eq!(outcome, ProbeOutcome::TimedOut);
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "the timeout must not wait out the rc file: {:?}",
            start.elapsed()
        );

        let pid: i32 = wait_for_pidfile(&env, &pidfile);
        assert!(
            wait_until_dead(pid),
            "pid {pid} survived the timeout: the process *group* was not killed"
        );
    }

    /// Read the grandchild's pid, giving the fake shell a moment to
    /// have written it.
    fn wait_for_pidfile(env: &TempEnvironment, pidfile: &Path) -> i32 {
        for _ in 0..100 {
            if let Ok(text) = env.fs.read_to_string(pidfile) {
                if let Ok(pid) = text.trim().parse() {
                    return pid;
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("the fake shell never recorded its background child's pid");
    }

    /// Poll `kill(pid, 0)` until the process is gone. Reparenting to
    /// init reaps it, so this converges quickly — but not instantly,
    /// which is why it polls instead of asserting once.
    fn wait_until_dead(pid: i32) -> bool {
        for _ in 0..100 {
            // SAFETY: signal 0 sends nothing; it only asks whether the
            // pid can be signalled, which is the liveness check here.
            let alive = unsafe { libc::kill(pid, 0) } == 0;
            if !alive {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn an_inherited_stamp_is_scrubbed_before_the_child_sees_it() {
        // The probe often runs *from* a live shell, which exports the
        // stamp. Without scrubbing, a totally unhooked shell would
        // still hand it back and every probe would report success.
        let env = TempEnvironment::builder().build();
        let _guard = crate::testing::EnvVarGuard::set(INIT_GEN_ENV, "999999");
        let shell = fake_shell(&env, "inheriting-shell", "# sources nothing");
        assert_eq!(
            run(&shell, Duration::from_secs(10)),
            ProbeOutcome::NoStamp,
            "an inherited stamp must not count as this shell's activation"
        );
    }

    #[test]
    fn a_shell_that_cannot_be_run_is_a_couldnt_verify_not_a_panic() {
        let env = TempEnvironment::builder().build();
        let missing = env.home.join("no-such-shell");
        assert!(matches!(
            run(&missing, Duration::from_secs(5)),
            ProbeOutcome::SpawnFailed(_)
        ));
    }
}

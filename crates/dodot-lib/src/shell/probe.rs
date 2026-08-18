//! The shell verification probe — measuring whether a *new* shell
//! reaches dodot's init hook.
//!
//! Signals 1 and 2 (the environment stamp and the heartbeat, both in
//! [`crate::shell::activation`]) answer "is this shell live?" and "has
//! any shell activated?". Neither answers the question a fresh install
//! or a freshly broken hookup actually poses: *would the next terminal the
//! user opens reach the hook dodot just wrote?* Only running one answers
//! that, so when the cheap signals come back inconclusive, dodot spawns the
//! user's shell and accepts the nonce-bound response emitted at the start of
//! current init scripts. Older hooks fall back to the post-rc stamp command.
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
//! creative ways, so [`run_targeted`] is defensive by construction:
//! interactive non-login shell (the mode that reads the file the hook lives
//! in), stdin from `/dev/null`, output captured, a hard timeout that kills the
//! whole process group, and `DODOT_INIT_*` scrubbed from the child environment
//! — an inherited stamp would be a false positive whenever a legacy fallback
//! probe runs from an already-live shell.
//!
//! That defensive envelope lives in [`spawn_captured`] and its streaming
//! sibling [`spawn_captured_until_stderr`]. Hook-line tracing uses those
//! capture helpers, while targeted verification uses the same process-group
//! and pipe handling in [`spawn_until_targeted_marker`].
//!
//! # Verdicts
//!
//! [`Verdict`] keeps four outcomes apart because they demand
//! different action: verified, version-skew (the shell activated, from
//! a dodot other than the one running), verified-broken (with the
//! static rc scan splitting *hook absent* from *hook present but never
//! reached*), and couldn't-verify, which degrades to the scan's answer
//! labeled as configuration state. A probe failure never wedges `up`.
//!
//! The skew arm is why the probed shell reports *both* halves of the
//! stamp ([`ProbeStamp`]). A generation alone cannot tell a working
//! hookup from one that resolves to the wrong binary — a hand-wired
//! `eval` hook running some other dodot mints a fresh generation just
//! as convincingly as the right one — so reading it alone would report
//! the epic's own failure as health, on the first `up`, which is the
//! run this probe exists for.

use std::io::{BufRead, BufReader, Read};
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::fs::Fs;
use crate::paths::Pather;
use crate::shell::activation::{
    self, ActivationNotice, ActivationState, EvidenceVersion, HeartbeatState, StampState,
    INIT_GEN_ENV, INIT_VERSION_ENV,
};
use crate::shell::rc::{self, HookPresence, ShellEnv};

/// Prefix the probe command prints its stamp behind, so the record we
/// read survives arbitrary rc noise on the same stream.
pub const PROBE_MARKER: &str = "dodot-probe-stamp:";

/// Separates the two fields of a probe record: the generation the
/// spawned shell sourced, and the dodot that wrote it.
pub const PROBE_FIELD_SEP: char = '|';

/// Challenge variable carrying the one-use nonce for targeted
/// shell-init verification.
pub const TARGET_PROBE_ENV: &str = "DODOT_INTERNAL_SHELL_INIT_PROBE";

/// Challenge variable carrying the verifier process ID the directly
/// launched shell must see as `$PPID`.
pub const TARGET_PROBE_PARENT_ENV: &str = "DODOT_INTERNAL_SHELL_INIT_PROBE_PARENT";

/// Prefix for the process-bound response emitted by current init
/// scripts before activation evidence.
pub const TARGET_PROBE_MARKER: &str = "dodot-shell-init-probe:v1|";

/// How long a spawned shell gets before its process group is killed.
/// Spec §3.2 calls for "order of 5 seconds": long enough for a heavy
/// rc file, short enough that a hung one is not a hung `dodot up`.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Environment prefix scrubbed from the probed shell.
const SCRUB_PREFIX: &str = "DODOT_INIT_";

/// How long to wait between liveness checks on the spawned shell.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Extra time after the direct child exits or is killed for the reader
/// thread to deliver already-written output. This stays bounded because
/// rc-started background jobs can inherit stdout.
const POST_EXIT_DRAIN: Duration = Duration::from_millis(200);

/// Preferred lower bound for the descriptor inherited by a report-only hold.
/// The actual descriptor is selected below the process's soft open-file limit.
const PREFERRED_PARENT_LIVENESS_FD: RawFd = 63;

/// Put a probed shell in its own session and process group.
///
/// Besides making whole-group timeout termination possible, dropping the
/// controlling terminal prevents an explicitly interactive shell from
/// stopping itself with `SIGTTIN` or `SIGTTOU` when dodot runs under a PTY.
pub(crate) fn configure_probe_session(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    // SAFETY: `setsid` is an async-signal-safe syscall and touches no Rust
    // state between fork and exec. A failure is returned by `spawn`.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// A close-on-exec socket pair whose child end is inherited by the
/// report-only hold at a dynamically selected descriptor.
pub(crate) struct ParentLiveness {
    child_end: UnixStream,
    parent_end: UnixStream,
}

impl ParentLiveness {
    pub(crate) fn attach(command: &mut Command) -> std::io::Result<Self> {
        use std::os::unix::process::CommandExt;

        let (child_end, parent_end) = UnixStream::pair()?;
        let child_end = duplicate_liveness_descriptor(&child_end)?;
        let child_fd = child_end.as_raw_fd();
        let parent_fd = parent_end.as_raw_fd();
        // SAFETY: the closure uses only async-signal-safe descriptor
        // syscalls between fork and exec. Both source descriptors stay
        // alive in `Self` until `Command::spawn` returns.
        unsafe {
            command.pre_exec(move || {
                libc::close(parent_fd);
                if libc::fcntl(child_fd, libc::F_SETFD, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Ok(Self {
            child_end,
            parent_end,
        })
    }

    pub(crate) fn descriptor(&self) -> RawFd {
        self.child_end.as_raw_fd()
    }

    pub(crate) fn child_spawned(self) -> UnixStream {
        drop(self.child_end);
        self.parent_end
    }
}

fn duplicate_liveness_descriptor(child_end: &UnixStream) -> std::io::Result<UnixStream> {
    let soft_limit = descriptor_soft_limit()?;
    let preferred = PREFERRED_PARENT_LIVENESS_FD.min(soft_limit - 1);
    let duplicated =
        duplicate_from(child_end.as_raw_fd(), preferred).or_else(|preferred_error| {
            if preferred <= libc::STDERR_FILENO + 1 {
                return Err(preferred_error);
            }
            duplicate_from(child_end.as_raw_fd(), libc::STDERR_FILENO + 1)
        })?;

    // SAFETY: F_DUPFD_CLOEXEC returned a new descriptor owned by this process.
    Ok(unsafe { UnixStream::from_raw_fd(duplicated) })
}

fn duplicate_from(source: RawFd, minimum: RawFd) -> std::io::Result<RawFd> {
    // SAFETY: `source` is a live descriptor and F_DUPFD_CLOEXEC takes an integer.
    let duplicated = unsafe { libc::fcntl(source, libc::F_DUPFD_CLOEXEC, minimum) };
    if duplicated == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(duplicated)
    }
}

fn descriptor_soft_limit() -> std::io::Result<RawFd> {
    let mut limit = std::mem::MaybeUninit::<libc::rlimit>::uninit();
    // SAFETY: getrlimit initializes `limit` on success.
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, limit.as_mut_ptr()) } == -1 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: the successful getrlimit call initialized the value.
    let current = unsafe { limit.assume_init() }.rlim_cur;
    let capped = current.min(RawFd::MAX as libc::rlim_t);
    if capped <= (libc::STDERR_FILENO + 1) as libc::rlim_t {
        return Err(std::io::Error::other(
            "open-file limit leaves no descriptor for parent liveness",
        ));
    }
    Ok(capped as RawFd)
}

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
    /// are inconclusive, giving it `timeout` to reach the hook or return
    /// legacy fallback evidence.
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

/// What the spawned shell reported about the init script it sourced.
///
/// Both fields, never just the generation: a hookup can source a
/// current-generation script written by a *different* dodot, and a
/// generation alone reads that as health (`shell-hookup-ergonomics.lex`
/// §2.3). [`EvidenceVersion`] rather than an `Option<String>` so a
/// version-less report carries the same bounded meaning here as it does
/// in the evidence path — one rule, both signals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeStamp {
    /// `DODOT_INIT_GEN` as the spawned shell exported it.
    pub generation: u64,
    /// `DODOT_INIT_VERSION`, or [`EvidenceVersion::PreVersion`] when
    /// the script that ran was too old to export one.
    pub version: EvidenceVersion,
}

/// What one shell spawn reported back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// The shell finished and printed a parseable stamp.
    Stamp(ProbeStamp),
    /// The shell finished and printed no stamp — it did not source
    /// dodot's init script.
    NoStamp,
    /// The shell outlived its timeout; its process group was killed.
    TimedOut,
    /// The shell could not be run at all.
    SpawnFailed(String),
}

/// Result of one targeted verification run, including the wall-clock
/// time until a marker, timeout, spawn failure, or shell completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetedVerification {
    pub outcome: ProbeOutcome,
    pub elapsed_us: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TargetedResponse {
    nonce: String,
    verifier_pid: u32,
    shell_pid: u32,
    generation: u64,
    version: EvidenceVersion,
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

/// The command handed to the spawned shell: print both halves of the
/// stamp it may or may not have inherited from its rc, behind a marker.
///
/// Both are printed unconditionally, empty when unset, so the record
/// always has its two fields and the parser never has to guess which
/// one a lone value was.
fn probe_command() -> String {
    format!(
        "printf '{PROBE_MARKER}%s{PROBE_FIELD_SEP}%s\\n' \
         \"${{{INIT_GEN_ENV}-}}\" \"${{{INIT_VERSION_ENV}-}}\""
    )
}

/// Extract the stamp from captured probe output.
///
/// Scans every line for the marker and takes the last one: an rc file
/// is free to print whatever it likes before our command runs, and the
/// probe reads exactly one record out of the noise. A record whose
/// generation does not parse is not a stamp at all — the same rule
/// [`activation::read_heartbeat`] holds the heartbeat to, so an
/// activation is never inferred from a version field alone.
pub fn parse_probe_output(stdout: &str) -> Option<ProbeStamp> {
    stdout
        .lines()
        .filter_map(|line| line.trim().strip_prefix(PROBE_MARKER))
        .filter_map(|record| record.split_once(PROBE_FIELD_SEP))
        .filter_map(|(generation, version)| {
            Some(ProbeStamp {
                generation: activation::parse_generation(generation)?,
                version: EvidenceVersion::from_field(Some(version)),
            })
        })
        .next_back()
}

/// Extract the process-bound response from one stdout line.
fn parse_targeted_response(line: &str) -> Option<TargetedResponse> {
    let record = line.trim().strip_prefix(TARGET_PROBE_MARKER)?;
    let mut fields = record.split(PROBE_FIELD_SEP);
    let nonce = fields.next()?.to_string();
    let verifier_pid = fields.next()?.parse::<u32>().ok()?;
    let shell_pid = fields.next()?.parse::<u32>().ok()?;
    let generation = activation::parse_generation(fields.next()?)?;
    let version = EvidenceVersion::from_field(Some(fields.next()?));
    fields.next().is_none().then_some(TargetedResponse {
        nonce,
        verifier_pid,
        shell_pid,
        generation,
        version,
    })
}

fn matching_targeted_stamp(
    line: &str,
    nonce: &str,
    verifier_pid: u32,
    child_pid: u32,
) -> Option<ProbeStamp> {
    let response = parse_targeted_response(line)?;
    (response.nonce == nonce
        && response.verifier_pid == verifier_pid
        && response.shell_pid == child_pid)
        .then_some(ProbeStamp {
            generation: response.generation,
            version: response.version,
        })
}

/// Both streams a spawned process wrote before finishing, and how it
/// finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnCapture {
    pub stdout: String,
    pub stderr: String,
    /// The exit code, or `None` when the process was killed by a
    /// signal.
    ///
    /// The activation probe ignores it on purpose — unrelated rc
    /// breakage exits non-zero and still activates dodot, so the stamp
    /// is the bit. [`crate::shell::trace`] needs it for the opposite
    /// kind of question: `<shell> -n` answers *only* through its exit
    /// status, and a syntax check whose answer is discarded is a
    /// syntax check that never ran.
    pub status: Option<i32>,
}

/// What became of one enveloped spawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpawnOutcome {
    /// The process finished (any exit status) within the timeout.
    Finished(SpawnCapture),
    /// The process outlived its timeout; its process group was killed.
    TimedOut,
    /// The process could not be spawned at all.
    SpawnFailed(String),
}

/// Run `command` under the probe envelope and capture both streams.
///
/// The envelope, applied here so every caller gets all of it: stdin
/// from `/dev/null` (an rc file that prompts gets EOF instead of
/// blocking), stdout/stderr captured off-thread (a chatty rc cannot
/// deadlock a full pipe against our wait loop), `DODOT_INIT_*`
/// scrubbed (an inherited stamp must never masquerade as the child's),
/// its own process group, and a hard timeout that kills that whole
/// group. Callers set the program, arguments, and any extra
/// environment before handing the command over.
pub fn spawn_captured(mut command: Command, timeout: Duration) -> SpawnOutcome {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for key in scrubbed_keys(std::env::vars().map(|(k, _)| k)) {
        command.env_remove(key);
    }
    configure_probe_session(&mut command);

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => return SpawnOutcome::SpawnFailed(format!("{e}")),
    };
    let pid = child.id();

    // Drain both pipes off-thread: a chatty rc file that fills a pipe
    // buffer would otherwise deadlock against our own wait loop.
    let stdout = child.stdout.take().map(drain);
    let stderr = child.stderr.take().map(drain);

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // The direct process can exit while rc-started descendants
                // keep running or retain one of the capture pipes. End the
                // whole probe lifetime before joining either reader.
                kill_process_group(pid);
                break Some(status);
            }
            Ok(None) => {}
            Err(e) => {
                kill_process_group(pid);
                let _ = child.wait();
                return SpawnOutcome::SpawnFailed(format!("{e}"));
            }
        }
        if Instant::now() >= deadline {
            kill_process_group(pid);
            // Reap: the group is dead, so this returns promptly and we
            // leave no zombie behind.
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(POLL_INTERVAL);
    };

    let stdout = stdout.and_then(|h| h.join().ok()).unwrap_or_default();
    let stderr = stderr.and_then(|h| h.join().ok()).unwrap_or_default();

    let Some(status) = status else {
        return SpawnOutcome::TimedOut;
    };
    SpawnOutcome::Finished(SpawnCapture {
        stdout,
        stderr,
        status: status.code(),
    })
}

/// Run `command` under the probe envelope and stop its process group as
/// soon as `stop_when` matches one newly captured stderr line.
///
/// This is the streaming counterpart to [`spawn_captured`]. It keeps the
/// same environment scrub, stderr capture, process-group, and
/// timeout guarantees, but lets a protocol response end a shell whose rc
/// deliberately waits after writing that response. Stdout is discarded:
/// the caller's protocol is on stderr, and a descendant retaining stdout
/// must not outlive the hard timeout. A private inherited descriptor is
/// an unwritten liveness pipe: a report-only hold can detect the Rust
/// process disappearing by reading EOF without interfering with an
/// interactive shell's stdin. A response-stopped process returns
/// [`SpawnOutcome::Finished`] with `status: None`.
pub fn spawn_captured_until_stderr(
    mut command: Command,
    timeout: Duration,
    stop_when: impl FnMut(&str) -> bool,
) -> SpawnOutcome {
    let liveness = match ParentLiveness::attach(&mut command) {
        Ok(liveness) => liveness,
        Err(e) => return SpawnOutcome::SpawnFailed(format!("{e}")),
    };
    spawn_captured_until_stderr_with_liveness(command, timeout, liveness, stop_when)
}

pub(crate) fn spawn_captured_until_stderr_with_liveness(
    mut command: Command,
    timeout: Duration,
    liveness: ParentLiveness,
    mut stop_when: impl FnMut(&str) -> bool,
) -> SpawnOutcome {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    for key in scrubbed_keys(std::env::vars().map(|(k, _)| k)) {
        command.env_remove(key);
    }
    configure_probe_session(&mut command);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => return SpawnOutcome::SpawnFailed(format!("{e}")),
    };
    let child_pid = child.id();
    // Keep the parent end alive for exactly as long as this Rust scope.
    let _parent_liveness = liveness.child_spawned();
    let (tx, rx) = mpsc::channel();
    if let Some(stderr) = child.stderr.take() {
        read_lines(stderr, tx);
    }

    let deadline = Instant::now() + timeout;
    let mut stderr = String::new();
    let mut stderr_connected = true;
    let mut stopped_on_output = false;
    let status = 'running: loop {
        loop {
            match rx.try_recv() {
                Ok(line) => {
                    let should_stop = stop_when(&line);
                    stderr.push_str(&line);
                    if should_stop {
                        kill_process_group(child_pid);
                        let _ = child.wait();
                        stopped_on_output = true;
                        break 'running None;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    stderr_connected = false;
                    break;
                }
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                // The direct shell is done, but descendants may still
                // hold its pipes or continue diagnostic side effects.
                kill_process_group(child_pid);
                break Some(status);
            }
            Ok(None) => {}
            Err(e) => {
                kill_process_group(child_pid);
                let _ = child.wait();
                drain_pending_lines(&rx, &mut stderr, POST_EXIT_DRAIN);
                return SpawnOutcome::SpawnFailed(format!("{e}"));
            }
        }
        if Instant::now() >= deadline {
            kill_process_group(child_pid);
            let _ = child.wait();
            break None;
        }
        let wait = POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now()));
        if wait.is_zero() {
            continue;
        }
        if let Some(line) = receive_line_or_wait(&rx, wait, &mut stderr_connected) {
            let should_stop = stop_when(&line);
            stderr.push_str(&line);
            if should_stop {
                kill_process_group(child_pid);
                let _ = child.wait();
                stopped_on_output = true;
                break 'running None;
            }
        }
    };

    drain_pending_lines(&rx, &mut stderr, POST_EXIT_DRAIN);
    if stopped_on_output {
        return SpawnOutcome::Finished(SpawnCapture {
            stdout: String::new(),
            stderr,
            status: None,
        });
    }
    let Some(status) = status else {
        return SpawnOutcome::TimedOut;
    };
    SpawnOutcome::Finished(SpawnCapture {
        stdout: String::new(),
        stderr,
        status: status.code(),
    })
}

/// Receive one stderr line without spinning after the reader exits.
fn receive_line_or_wait(
    rx: &mpsc::Receiver<String>,
    wait: Duration,
    connected: &mut bool,
) -> Option<String> {
    if !*connected {
        std::thread::sleep(wait);
        return None;
    }
    match rx.recv_timeout(wait) {
        Ok(line) => Some(line),
        Err(mpsc::RecvTimeoutError::Timeout) => None,
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            *connected = false;
            std::thread::sleep(wait);
            None
        }
    }
}

/// Spawn `shell` interactively and read the stamp back.
///
/// Safe against hostile rc files by construction through the shared
/// process-group and pipe-handling envelope. Never returns an error:
/// every failure mode is an outcome, because a probe that could not
/// run must degrade, not propagate (spec §3.3).
pub fn run(shell: &Path, timeout: Duration) -> ProbeOutcome {
    run_targeted(shell, timeout).outcome
}

/// Spawn `shell` interactively and verify the hook through the
/// process-bound v1 response branch.
///
/// Current init scripts respond before activation evidence, profiling,
/// PATH setup, Homebrew, and pack contributions, so success is bounded
/// by time-to-hook rather than time-to-finish-rc. The post-rc stamp
/// command remains as a compatibility fallback for legacy hooks that
/// do not understand the targeted challenge.
pub fn run_targeted(shell: &Path, timeout: Duration) -> TargetedVerification {
    let started = Instant::now();
    let mut command = Command::new(shell);
    command.arg("-ic").arg(probe_command());
    let nonce = fresh_nonce();
    let verifier_pid = std::process::id();
    command
        .env(TARGET_PROBE_ENV, &nonce)
        .env(TARGET_PROBE_PARENT_ENV, verifier_pid.to_string());

    let outcome = match spawn_until_targeted_marker(command, timeout, &nonce, verifier_pid) {
        TargetedSpawnOutcome::TargetedStamp(stamp) => ProbeOutcome::Stamp(stamp),
        TargetedSpawnOutcome::Finished(stdout) => match parse_probe_output(&stdout) {
            Some(stamp) => ProbeOutcome::Stamp(stamp),
            None => ProbeOutcome::NoStamp,
        },
        TargetedSpawnOutcome::TimedOut => ProbeOutcome::TimedOut,
        TargetedSpawnOutcome::SpawnFailed(e) => ProbeOutcome::SpawnFailed(e),
    };
    TargetedVerification {
        outcome,
        elapsed_us: elapsed_us(started),
    }
}

enum TargetedSpawnOutcome {
    TargetedStamp(ProbeStamp),
    Finished(String),
    TimedOut,
    SpawnFailed(String),
}

fn spawn_until_targeted_marker(
    mut command: Command,
    timeout: Duration,
    nonce: &str,
    verifier_pid: u32,
) -> TargetedSpawnOutcome {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for key in scrubbed_keys(std::env::vars().map(|(k, _)| k)) {
        command.env_remove(key);
    }
    configure_probe_session(&mut command);

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => return TargetedSpawnOutcome::SpawnFailed(format!("{e}")),
    };
    let child_pid = child.id();
    let (tx, rx) = mpsc::channel();
    if let Some(stdout) = child.stdout.take() {
        read_lines(stdout, tx);
    }
    let _stderr = child.stderr.take().map(drain);

    let deadline = Instant::now() + timeout;
    let mut stdout = String::new();
    loop {
        while let Ok(line) = rx.try_recv() {
            stdout.push_str(&line);
            if let Some(stamp) = matching_targeted_stamp(&line, nonce, verifier_pid, child_pid) {
                kill_process_group(child_pid);
                let _ = child.wait();
                return TargetedSpawnOutcome::TargetedStamp(stamp);
            }
        }
        match child.try_wait() {
            Ok(Some(_)) => {
                // The shell is only the process-group leader. Its exit does
                // not imply that rc-started descendants are gone.
                kill_process_group(child_pid);
                drain_pending_lines(&rx, &mut stdout, POST_EXIT_DRAIN);
                if let Some(stamp) =
                    matching_targeted_stamp_in_output(&stdout, nonce, verifier_pid, child_pid)
                {
                    return TargetedSpawnOutcome::TargetedStamp(stamp);
                }
                return TargetedSpawnOutcome::Finished(stdout);
            }
            Ok(None) => {}
            Err(e) => {
                kill_process_group(child_pid);
                let _ = child.wait();
                drain_pending_lines(&rx, &mut stdout, POST_EXIT_DRAIN);
                return TargetedSpawnOutcome::SpawnFailed(format!("{e}"));
            }
        }
        if Instant::now() >= deadline {
            kill_process_group(child_pid);
            let _ = child.wait();
            drain_pending_lines(&rx, &mut stdout, POST_EXIT_DRAIN);
            if let Some(stamp) =
                matching_targeted_stamp_in_output(&stdout, nonce, verifier_pid, child_pid)
            {
                return TargetedSpawnOutcome::TargetedStamp(stamp);
            }
            return TargetedSpawnOutcome::TimedOut;
        }
        let wait = POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now()));
        if wait.is_zero() {
            continue;
        }
        match rx.recv_timeout(wait) {
            Ok(line) => {
                stdout.push_str(&line);
                if let Some(stamp) = matching_targeted_stamp(&line, nonce, verifier_pid, child_pid)
                {
                    kill_process_group(child_pid);
                    let _ = child.wait();
                    return TargetedSpawnOutcome::TargetedStamp(stamp);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected | mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

fn read_lines<R: Read + Send + 'static>(pipe: R, tx: mpsc::Sender<String>) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(pipe);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
}

fn matching_targeted_stamp_in_output(
    stdout: &str,
    nonce: &str,
    verifier_pid: u32,
    child_pid: u32,
) -> Option<ProbeStamp> {
    stdout
        .lines()
        .find_map(|line| matching_targeted_stamp(line, nonce, verifier_pid, child_pid))
}

fn drain_pending_lines(rx: &mpsc::Receiver<String>, stdout: &mut String, max_wait: Duration) {
    let deadline = Instant::now() + max_wait;
    while Instant::now() < deadline {
        match rx.recv_timeout(POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())))
        {
            Ok(line) => stdout.push_str(&line),
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
        }
    }
    while let Ok(line) = rx.try_recv() {
        stdout.push_str(&line);
    }
}

fn elapsed_us(started: Instant) -> u64 {
    started.elapsed().as_micros().try_into().unwrap_or(u64::MAX)
}

fn fresh_nonce() -> String {
    let mut bytes = [0u8; 16];
    if std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .is_ok()
    {
        return bytes.iter().map(|b| format!("{b:02x}")).collect();
    }
    format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    )
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
/// The shell was spawned as its own group leader, either directly or
/// as a new session leader, so its pid is the group id and a negative-
/// pid signal reaches every process the rc file started — the
/// `exec`-into-a-multiplexer case, where killing only the shell we can
/// see would leave the real hang behind.
#[cfg(unix)]
fn kill_process_group(pid: u32) {
    // SAFETY: `kill` is a plain syscall wrapper with no memory
    // effects. A negative pid addresses the process group; the group
    // is one we created for this child, so we are not signalling
    // anything we did not spawn. A failure (the group already exited)
    // is nothing to handle.
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
    /// Measured ✓ — the spawned shell reported the current generation,
    /// from the dodot now running.
    Verified { generation: u64 },
    /// The shell activated, and from a *different* dodot than the one
    /// running. Measured, so it is the same finding
    /// [`ActivationState::VersionSkew`] names from evidence, arrived at
    /// the one way that cannot be talked out of it.
    VersionSkew {
        generation: u64,
        loaded: EvidenceVersion,
    },
    /// The shell ran and did not activate dodot.
    Broken { diagnosis: Diagnosis },
    /// The measurement itself failed. Degrade to configuration state.
    Unverified { reason: String },
}

impl Verdict {
    /// Fold one spawn's outcome into a verdict, using the static rc
    /// scan only where spec §2.2 allows it: to explain a failure.
    ///
    /// `running` is the version every measured stamp is judged against,
    /// through [`activation::is_skewed`] — the same rule
    /// [`activation::Evidence`] applies to the cheap signals, called
    /// rather than restated, so a probe cannot certify a hookup the
    /// footer would call skewed. Skew outranks the generation ladder
    /// for the same reason it does there: a current generation from
    /// the wrong dodot is not health, and a stale one from the wrong
    /// dodot is not fixed by opening a new shell.
    ///
    /// `known_capable` is the targeted-protocol capability of the hook the
    /// rc file is expected to run. A capable hook that finishes or times
    /// out without a marker is measured broken; an eval or legacy hook
    /// without a marker is only inconclusive unless the static scan shows
    /// the hook is absent.
    pub fn from_outcome(
        outcome: ProbeOutcome,
        reference: Option<u64>,
        running: &str,
        hook: Option<(HookPresence, String)>,
        known_capable: bool,
    ) -> Verdict {
        match outcome {
            ProbeOutcome::Stamp(stamp) if activation::is_skewed(Some(&stamp.version), running) => {
                Verdict::VersionSkew {
                    generation: stamp.generation,
                    loaded: stamp.version,
                }
            }
            ProbeOutcome::Stamp(stamp) => {
                let found = stamp.generation;
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
            ProbeOutcome::NoStamp => match hook {
                Some((HookPresence::Absent, rc)) => Verdict::Broken {
                    diagnosis: Diagnosis::HookAbsent { rc },
                },
                Some((_, rc)) if known_capable => Verdict::Broken {
                    diagnosis: Diagnosis::HookNotReached { rc },
                },
                Some(_) | None => Verdict::Unverified {
                    reason: "verification was inconclusive".into(),
                },
            },
            ProbeOutcome::TimedOut if known_capable => Verdict::Broken {
                diagnosis: match hook {
                    Some((_, rc)) => Diagnosis::HookNotReached { rc },
                    None => Diagnosis::Unknown,
                },
            },
            ProbeOutcome::TimedOut => Verdict::Unverified {
                reason: "verification produced no conclusive evidence before the timeout".into(),
            },
            ProbeOutcome::SpawnFailed(e) => Verdict::Unverified {
                reason: format!("could not run your shell ({e})"),
            },
        }
    }

    /// Render the verdict for `up` / `install` output.
    ///
    /// `evidence` is the footer the signal ladder alone produced,
    /// `evidence_line` is either the measured-success line from the
    /// targeted response or the historical line the signals produced,
    /// and `hook_line` is the manual line to fall back on. A
    /// couldn't-verify verdict degrades to `evidence`, clearly labeled
    /// as configuration state rather than measured activation; the
    /// other two verdicts are measurements and take precedence over it
    /// — including over the stale-shell advice, which is the wrong
    /// answer for a hookup that just measured broken.
    ///
    /// A measured verdict says the same thing the evidence path says
    /// for the same state, in the same words — and reaches the state
    /// the same way. A spawn settles the generation ladder's rung with
    /// certainty and learns nothing about the two rules that override
    /// it, so the ladder's answer goes through the shared
    /// [`activation::refine`] before [`ActivationNotice::for_state`]
    /// renders it. Short-circuiting straight to `Healthy` is how the
    /// measured path used to report "dodot is sourced in new shells"
    /// for a deployment of no packs, contradicting the `status` run
    /// immediately after it.
    ///
    /// `script_has_contributions` is the input that rule needs; it is
    /// a property of the script on disk, which the spawn does not
    /// change. Only [`Verdict::Broken`] writes its own hint, because
    /// only it has something the evidence path cannot know — which of
    /// the [`Diagnosis`] shapes the failure took.
    pub fn notice(
        &self,
        evidence: Option<ActivationNotice>,
        evidence_line: &str,
        hook_line: &str,
        script_has_contributions: bool,
    ) -> Option<ActivationNotice> {
        match self {
            // One arm: a measured activation is the ladder's `Healthy`
            // rung, and which state that *is* depends on the same two
            // overrides the evidence path applies.
            Verdict::Verified { .. } | Verdict::VersionSkew { .. } => {
                let state = activation::refine(
                    ActivationState::Healthy,
                    matches!(self, Verdict::VersionSkew { .. }),
                    script_has_contributions,
                );
                Some(ActivationNotice::for_state(
                    state,
                    hook_line,
                    None,
                    evidence_line.into(),
                ))
            }
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
    // Read once, before the spawn: the probe runs the user's rc, not
    // `dodot up`, so the script it sources is the same file afterwards.
    let has_contributions = activation::read_script(fs, paths)
        .is_some_and(|script| crate::shell::script_has_contributions(&script));
    let Some(shell) = shell_env.shell.as_deref().map(Path::new) else {
        return Verdict::Unverified {
            reason: "$SHELL is not set".into(),
        }
        .notice(evidence, &stale_line, &hook_line, has_contributions);
    };

    eprintln!("{}", announcement(shell));
    let hook = rc::scan_expected_rc(fs, paths.home_dir(), shell_env, rc_override);
    let known_capable =
        hook_verification_capability(fs, paths, shell_env, rc_override, hook.as_ref());
    let outcome = run(shell, timeout);
    // Current targeted verification does not write the heartbeat:
    // success is a measured event owned by this command, while the
    // heartbeat remains evidence from ordinary shell use. Legacy hooks
    // can still update it before their compatibility stamp prints; for
    // broken/unverified outcomes the historical evidence remains the
    // useful line.
    let evidence_line =
        activation::Evidence::collect(fs, paths, activation::EnvStamp::default(), reference, false)
            .map(|e| e.evidence_line())
            .unwrap_or(stale_line);
    let measured_line = measured_evidence_line(&outcome).unwrap_or(evidence_line);
    Verdict::from_outcome(
        outcome,
        reference,
        activation::running_version(),
        hook,
        known_capable,
    )
    .notice(evidence, &measured_line, &hook_line, has_contributions)
}

fn hook_verification_capability(
    fs: &dyn Fs,
    paths: &dyn Pather,
    shell_env: &ShellEnv,
    rc_override: Option<&Path>,
    hook: Option<&(HookPresence, String)>,
) -> bool {
    let Some((presence, _)) = hook else {
        return false;
    };
    match presence {
        HookPresence::ManagedBlock => activation::read_script(fs, paths)
            .is_some_and(|script| crate::shell::script_supports_targeted_probe(&script)),
        HookPresence::Manual => {
            manual_hook_supports_targeted_probe(fs, paths.home_dir(), shell_env, rc_override)
        }
        HookPresence::Absent => false,
    }
}

fn manual_hook_supports_targeted_probe(
    fs: &dyn Fs,
    home: &Path,
    shell_env: &ShellEnv,
    rc_override: Option<&Path>,
) -> bool {
    use crate::shell::trace::{self, HookForm, SourcedScript};

    let path = match rc_override {
        Some(path) => path.to_path_buf(),
        None => {
            let Some(shell) = shell_env.hookup_shell() else {
                return false;
            };
            rc::resolve_rc(fs, home, Some(shell), shell_env, None).path
        }
    };
    let Ok(rc_text) = fs.read_to_string(&path) else {
        return false;
    };
    let Some(hook) = trace::find_hook(&rc_text, home) else {
        return false;
    };
    match hook.form {
        HookForm::FileSource(SourcedScript::Path(script)) => fs
            .read_to_string(&script)
            .is_ok_and(|text| crate::shell::script_supports_targeted_probe(&text)),
        _ => false,
    }
}

fn measured_evidence_line(outcome: &ProbeOutcome) -> Option<String> {
    match outcome {
        ProbeOutcome::Stamp(ProbeStamp {
            version: EvidenceVersion::Known(version),
            ..
        }) => Some(format!("Verified just now by dodot {version}.")),
        ProbeOutcome::Stamp(ProbeStamp {
            version: EvidenceVersion::PreVersion,
            ..
        }) => Some(format!(
            "Verified just now by dodot {} or earlier.",
            activation::PRE_VERSION_RELEASE
        )),
        _ => None,
    }
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
/// (#279), and callers pass it **as it is** — whether it gets used is
/// this function's decision, not theirs.
///
/// It is consulted only when no measurement happens, because a spawn
/// answers the same question better. But "may this caller spawn" and
/// "did a spawn happen" are different facts, and only the second one
/// licences dropping the session signal: [`gate_says_probe`] declines
/// whenever the heartbeat is `Fresh`, which is *precisely* the case
/// #279 introduced the session signal to overrule — some other shell
/// activated, this one demonstrably did not. Deciding from the policy
/// instead let `up` print "dodot is sourced in new shells" in a session
/// that had not loaded dodot, while `status` in the same terminal
/// correctly said it had not, neither of them having spawned anything.
/// So the gate runs first and its answer, not the policy's permission,
/// is what suppresses the signal.
pub fn notice_with_probe(
    fs: &dyn Fs,
    paths: &dyn Pather,
    policy: &ProbePolicy,
    shell_env: &ShellEnv,
    env_stamp: &activation::EnvStamp,
    reference_for_gate: Option<u64>,
    tty: bool,
) -> Option<ActivationNotice> {
    // Nothing deployed means there is no hookup to measure yet, so the
    // script's existence is part of the same question.
    let timeout = policy
        .timeout()
        .filter(|_| fs.exists(&paths.init_script_path()))
        .filter(|_| {
            gate_says_probe(
                activation::classify_stamp(env_stamp.generation, reference_for_gate),
                activation::classify_heartbeat(
                    activation::read_heartbeat(fs, paths).map(|h| h.generation),
                    reference_for_gate,
                ),
            )
        });
    let evidence = activation::notice_for(
        fs,
        paths,
        env_stamp.clone(),
        reference_for_gate,
        tty && timeout.is_none(),
        shell_env,
    );
    let Some(timeout) = timeout else {
        return evidence;
    };
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

    /// A stamp for `generation` from `version`, the shape a probed
    /// shell running the current dodot reports.
    fn stamp(generation: u64, version: &str) -> ProbeStamp {
        ProbeStamp {
            generation,
            version: EvidenceVersion::Known(version.into()),
        }
    }

    #[test]
    fn the_stamp_is_read_out_of_arbitrary_rc_noise() {
        let noisy = format!(
            "Welcome to your shell!\n[oh-my-zsh] update available\n{PROBE_MARKER}1755200000|5.6.0\n"
        );
        assert_eq!(
            parse_probe_output(&noisy),
            Some(stamp(1_755_200_000, "5.6.0"))
        );
        // A shell that sourced a pre-RCS01 script reports a generation
        // and an empty version — bounded, not unknown, exactly as the
        // heartbeat's version-less shape reads.
        assert_eq!(
            parse_probe_output(&format!("{PROBE_MARKER}1755200000|\n")),
            Some(ProbeStamp {
                generation: 1_755_200_000,
                version: EvidenceVersion::PreVersion,
            })
        );
        // No stamp exported: the marker is there, both fields empty.
        assert_eq!(parse_probe_output(&format!("{PROBE_MARKER}|\n")), None);
        // A version with no generation is not evidence a shell loaded
        // anything, the same rule the heartbeat is held to.
        assert_eq!(parse_probe_output(&format!("{PROBE_MARKER}|5.6.0\n")), None);
        assert_eq!(parse_probe_output("nothing at all\n"), None);
    }

    #[test]
    fn targeted_response_requires_nonce_and_direct_child_identity() {
        let line = format!("{TARGET_PROBE_MARKER}abc|10|20|100|5.7.0");
        assert_eq!(
            matching_targeted_stamp(&line, "abc", 10, 20),
            Some(stamp(100, "5.7.0"))
        );
        assert_eq!(matching_targeted_stamp(&line, "wrong", 10, 20), None);
        assert_eq!(matching_targeted_stamp(&line, "abc", 11, 20), None);
        assert_eq!(matching_targeted_stamp(&line, "abc", 10, 21), None);
        assert_eq!(
            parse_targeted_response(&format!(
                "{TARGET_PROBE_MARKER}abc|10|20|not-a-generation|5.7.0"
            )),
            None
        );
        assert_eq!(
            parse_targeted_response(&format!("{TARGET_PROBE_MARKER}abc|10|20|100")),
            None
        );
    }

    #[test]
    fn targeted_response_is_found_after_trailing_output_is_drained() {
        let stdout =
            format!("unterminated rc output\n{TARGET_PROBE_MARKER}abc|10|20|100|5.7.0\nignored\n");

        assert_eq!(
            matching_targeted_stamp_in_output(&stdout, "abc", 10, 20),
            Some(stamp(100, "5.7.0"))
        );
        assert_eq!(
            matching_targeted_stamp_in_output(&stdout, "abc", 10, 21),
            None
        );
    }

    #[test]
    fn targeted_runner_terminates_descendants_when_the_direct_shell_exits() {
        use crate::fs::Fs;

        let shell = Path::new("/bin/sh");
        if !shell.exists() {
            return;
        }
        let env = crate::testing::TempEnvironment::builder().build();
        let started = env.home.join("targeted-descendant-started");
        let leaked = env.home.join("targeted-descendant-survived");
        let mut command = Command::new(shell);
        command
            .env("DODOT_TEST_DESCENDANT_STARTED", &started)
            .env("DODOT_TEST_DESCENDANT_LEAKED", &leaked)
            .args([
                "-c",
                "( : > \"$DODOT_TEST_DESCENDANT_STARTED\"; sleep 1; \
                 : > \"$DODOT_TEST_DESCENDANT_LEAKED\" ) & \
                 while [ ! -e \"$DODOT_TEST_DESCENDANT_STARTED\" ]; do :; done",
            ]);

        let outcome = spawn_until_targeted_marker(
            command,
            Duration::from_secs(2),
            "unused-nonce",
            std::process::id(),
        );

        assert!(
            matches!(outcome, TargetedSpawnOutcome::Finished(_)),
            "the marker-less direct shell should finish"
        );
        assert!(env.fs.exists(&started), "the descendant never started");
        std::thread::sleep(Duration::from_millis(1_200));
        assert!(
            !env.fs.exists(&leaked),
            "a descendant survived the targeted probe's terminal path"
        );
    }

    #[test]
    fn stderr_stop_does_not_wait_for_a_descendant_retaining_stdout() {
        let shell = Path::new("/bin/sh");
        if !shell.exists() {
            return;
        }
        let mut command = Command::new(shell);
        command.args(["-c", "(sleep 5) &"]);

        let started = Instant::now();
        let outcome = spawn_captured_until_stderr(command, Duration::from_secs(2), |_| false);

        assert!(
            matches!(
                outcome,
                SpawnOutcome::Finished(SpawnCapture {
                    status: Some(0),
                    ..
                })
            ),
            "direct child should finish normally: {outcome:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "a descendant retaining stdout must not block completion"
        );
    }

    #[test]
    fn disconnected_stderr_waits_instead_of_spinning() {
        let (tx, rx) = mpsc::channel::<String>();
        drop(tx);
        let mut connected = true;
        let wait = Duration::from_millis(30);

        let started = Instant::now();
        assert_eq!(receive_line_or_wait(&rx, wait, &mut connected), None);

        assert!(!connected);
        assert!(
            started.elapsed() >= Duration::from_millis(25),
            "a disconnected channel must still yield the polling interval"
        );
    }

    #[test]
    fn stderr_stop_matches_each_line_and_retains_complete_diagnostics() {
        let shell = Path::new("/bin/sh");
        if !shell.exists() {
            return;
        }
        let mut command = Command::new(shell);
        command.args([
            "-c",
            "i=0; while [ \"$i\" -lt 2000 ]; do printf 'noise-%s\\n' \"$i\" >&2; \
             i=$((i + 1)); done; printf 'stop\\n' >&2; sleep 5",
        ]);
        let mut inspected = 0;

        let outcome = spawn_captured_until_stderr(command, Duration::from_secs(5), |line| {
            inspected += 1;
            assert_eq!(
                line.lines().count(),
                1,
                "predicate received more than one line"
            );
            line == "stop\n"
        });
        let SpawnOutcome::Finished(capture) = outcome else {
            panic!("expected marker-stopped capture, got {outcome:?}");
        };

        assert_eq!(inspected, 2001);
        assert_eq!(capture.stderr.lines().count(), 2001);
        assert!(capture.stderr.starts_with("noise-0\n"));
        assert!(capture.stderr.ends_with("stop\n"));
        assert_eq!(capture.status, None);
        assert!(capture.stdout.is_empty());
    }

    #[test]
    fn targeted_verification_stops_at_current_file_source_hook() {
        use crate::fs::Fs;
        use crate::paths::Pather;

        let shell = if Path::new("/bin/bash").exists() {
            "/bin/bash"
        } else {
            return;
        };
        let env = crate::testing::TempEnvironment::builder().build();
        let _home = crate::testing::EnvVarGuard::set("HOME", &env.home.display().to_string());
        let script =
            crate::shell::write_init_script(env.fs.as_ref(), env.paths.as_ref(), true, None)
                .unwrap();
        env.fs
            .write_file(
                &env.home.join(".bashrc"),
                format!(
                    "shopt -s expand_aliases\nalias printf='echo alias-hijacked'\n. '{}'\nsleep 5\n",
                    script.display()
                )
                .as_bytes(),
            )
            .unwrap();

        let run = run_targeted(Path::new(shell), Duration::from_millis(700));

        assert!(
            matches!(run.outcome, ProbeOutcome::Stamp(_)),
            "expected marker-driven stamp, got {run:?}"
        );
        assert!(
            run.elapsed_us < 700_000,
            "verification must stop before post-hook rc work: {run:?}"
        );
        assert!(
            !env.fs.exists(&env.paths.hookup_heartbeat_path()),
            "targeted verification must not write activation heartbeat"
        );
        let profile_entries = env
            .fs
            .read_dir(&env.paths.probes_shell_init_dir())
            .unwrap_or_default();
        assert!(
            profile_entries.is_empty(),
            "targeted verification must not create startup profiles: {profile_entries:?}"
        );
    }

    const PTY_DRIVER_ENV: &str = "DODOT_TEST_PROBE_PTY_DRIVER";
    const PTY_SHELL_ENV: &str = "DODOT_TEST_PROBE_PTY_SHELL";

    #[test]
    fn primary_trace_spawn_finishes_under_a_pty() {
        if std::env::var(PTY_DRIVER_ENV).as_deref() == Ok("primary") {
            let shell_path = std::env::var_os(PTY_SHELL_ENV).expect("PTY shell is selected");
            let shell = Path::new(&shell_path);
            let mut command = Command::new(shell);
            command.args(["-ic", "printf 'primary-pty-ok\\n'"]);
            let outcome = spawn_captured(command, Duration::from_secs(2));
            assert!(
                matches!(
                    outcome,
                    SpawnOutcome::Finished(SpawnCapture { ref stdout, .. })
                        if stdout.contains("primary-pty-ok")
                ),
                "primary trace spawn did not finish under a PTY: {outcome:?}"
            );
            eprintln!("pty-driver-primary-complete");
            return;
        }

        let Some(shell) = pty_test_shell() else {
            return;
        };
        run_current_test_under_pty(
            "shell::probe::tests::primary_trace_spawn_finishes_under_a_pty",
            "primary",
            shell,
        );
    }

    #[test]
    fn targeted_marker_spawn_finishes_under_a_pty() {
        if std::env::var(PTY_DRIVER_ENV).as_deref() == Ok("targeted") {
            let shell_path = std::env::var_os(PTY_SHELL_ENV).expect("PTY shell is selected");
            let shell = Path::new(&shell_path);
            let nonce = "pty-nonce";
            let verifier_pid = std::process::id();
            let mut command = Command::new(shell);
            command.args([
                "-ic",
                &format!(
                    "printf '{TARGET_PROBE_MARKER}{nonce}|{verifier_pid}|%s|42|5.7.0\\n' \
                     \"$$\"; sleep 5"
                ),
            ]);

            let outcome =
                spawn_until_targeted_marker(command, Duration::from_secs(2), nonce, verifier_pid);
            assert!(
                matches!(
                    outcome,
                    TargetedSpawnOutcome::TargetedStamp(ProbeStamp {
                        generation: 42,
                        version: EvidenceVersion::Known(ref version),
                    }) if version == "5.7.0"
                ),
                "targeted marker spawn did not finish under a PTY"
            );
            eprintln!("pty-driver-targeted-complete");
            return;
        }

        let Some(shell) = pty_test_shell() else {
            return;
        };
        run_current_test_under_pty(
            "shell::probe::tests::targeted_marker_spawn_finishes_under_a_pty",
            "targeted",
            shell,
        );
    }

    fn pty_test_shell() -> Option<&'static Path> {
        ["/bin/zsh", "/usr/bin/zsh", "/bin/bash", "/usr/bin/bash"]
            .into_iter()
            .map(Path::new)
            .find(|path| path.exists())
    }

    fn run_current_test_under_pty(test_name: &str, driver: &str, shell: &Path) {
        use std::io::Read as _;
        use std::os::fd::{AsRawFd as _, FromRawFd as _, OwnedFd};
        use std::os::unix::process::CommandExt as _;

        let mut master_fd = -1;
        let mut slave_fd = -1;
        // SAFETY: `openpty` initializes both descriptors on success; the
        // null termios and window-size pointers request system defaults.
        let opened = unsafe {
            libc::openpty(
                &mut master_fd,
                &mut slave_fd,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert_eq!(opened, 0, "openpty: {}", std::io::Error::last_os_error());
        // SAFETY: successful `openpty` returned two newly owned descriptors.
        let mut master = unsafe { std::fs::File::from_raw_fd(master_fd) };
        let slave = unsafe { OwnedFd::from_raw_fd(slave_fd) };
        set_close_on_exec(master.as_raw_fd());
        set_close_on_exec(slave.as_raw_fd());

        let slave_fd = slave.as_raw_fd();
        let mut command = Command::new(std::env::current_exe().expect("current test binary"));
        command
            .args(["--exact", test_name, "--nocapture"])
            .env(PTY_DRIVER_ENV, driver)
            .env(PTY_SHELL_ENV, shell)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // SAFETY: the closure uses only async-signal-safe descriptor and
        // session syscalls before exec. `slave` remains alive through spawn.
        unsafe {
            command.pre_exec(move || {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::ioctl(slave_fd, libc::TIOCSCTTY as _, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                for target in [libc::STDIN_FILENO, libc::STDOUT_FILENO, libc::STDERR_FILENO] {
                    if libc::dup2(slave_fd, target) == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                }
                if slave_fd > libc::STDERR_FILENO {
                    libc::close(slave_fd);
                }
                Ok(())
            });
        }

        let mut child = command.spawn().expect("PTY test driver starts");
        drop(slave);
        let output = std::thread::spawn(move || {
            let mut output = Vec::new();
            let _ = master.read_to_end(&mut output);
            output
        });
        let deadline = Instant::now() + Duration::from_secs(8);
        let status = loop {
            if let Some(status) = child.try_wait().expect("PTY driver remains waitable") {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("PTY test driver timed out: {test_name}");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let output = output.join().expect("PTY output reader joins");
        let output = String::from_utf8_lossy(&output);
        assert!(
            status.success(),
            "PTY test driver failed: {test_name}\n{output}"
        );
        assert!(
            output.contains(&format!("pty-driver-{driver}-complete")),
            "PTY test driver did not exercise its inner branch: {test_name}\n{output}"
        );
    }

    fn set_close_on_exec(fd: RawFd) {
        // SAFETY: `fd` is live for both fcntl calls and no pointer is involved.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        assert_ne!(flags, -1, "F_GETFD: {}", std::io::Error::last_os_error());
        assert_ne!(
            unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) },
            -1,
            "F_SETFD: {}",
            std::io::Error::last_os_error()
        );
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

    /// The version every verdict test judges its measured stamp
    /// against — a fixed string rather than [`activation::running_version`],
    /// so the assertions do not move with the crate's own release.
    const RUNNING: &str = "5.6.0";

    /// `script_has_contributions` for a machine with packs deployed —
    /// the ordinary case, and the one the verdict tests below are not
    /// about. The empty case has a test of its own.
    const DEPLOYED: bool = true;

    #[test]
    fn a_current_stamp_is_a_measured_verification() {
        let v = Verdict::from_outcome(
            ProbeOutcome::Stamp(stamp(100, RUNNING)),
            Some(100),
            RUNNING,
            hook(HookPresence::ManagedBlock),
            false,
        );
        assert_eq!(v, Verdict::Verified { generation: 100 });
        let notice = v
            .notice(
                None,
                "Last loaded just now by dodot 5.6.0.",
                "HOOK",
                DEPLOYED,
            )
            .unwrap();
        assert_eq!(notice.state, "healthy");
        assert_eq!(notice.severity, "ok");
        // A measurement reports the healthy state in the state's own
        // words: what the spawn buys is that the claim is right, not a
        // second phrasing of it for the docs to keep in sync.
        assert_eq!(notice.message, activation::HEALTHY_MESSAGE);
        assert_eq!(notice.evidence, "Last loaded just now by dodot 5.6.0.");
    }

    /// The false positive this probe was rebuilt to stop reporting: a
    /// hand-wired hook resolves to some other dodot, that dodot's init
    /// script mints a perfectly current generation, and a
    /// generation-only probe converts it into a green "sourced in new
    /// shells". Both shapes skew — a named older version, and the
    /// version-less script every pre-RCS01 dodot generates.
    #[test]
    fn a_current_generation_from_another_dodot_is_skew_not_health() {
        for loaded in [
            EvidenceVersion::Known("5.0.0".into()),
            EvidenceVersion::PreVersion,
        ] {
            let v = Verdict::from_outcome(
                ProbeOutcome::Stamp(ProbeStamp {
                    generation: 100,
                    version: loaded.clone(),
                }),
                Some(100),
                RUNNING,
                hook(HookPresence::Manual),
                false,
            );
            assert_eq!(
                v,
                Verdict::VersionSkew {
                    generation: 100,
                    loaded: loaded.clone()
                },
                "a current generation from {loaded} is not a verification"
            );
            let notice = v
                .notice(
                    None,
                    "Last loaded just now by dodot 5.0.0.",
                    "HOOK",
                    DEPLOYED,
                )
                .unwrap();
            // The state the evidence path would name, in the evidence
            // path's own words — the measurement changes which state is
            // reported, never how a state reads.
            assert_eq!(notice.state, "version-skew");
            assert_eq!(notice.severity, "warning");
            assert_eq!(
                notice.message,
                "Shell hookup: your shells load a different dodot."
            );
            assert!(notice.hint.unwrap().contains("PATH finds first"));
        }
    }

    /// A measurement settles which rung of the generation ladder the
    /// hookup is on. It says nothing about whether the script that
    /// shell sourced deploys anything, so the measured path has to run
    /// its answer through the same [`activation::refine`] the evidence
    /// path does. Short-circuiting to `Healthy` had `up` say "dodot is
    /// sourced in new shells" on a fresh install with no packs, with
    /// `status` contradicting it a second later.
    #[test]
    fn a_measured_activation_of_an_empty_script_is_not_reported_healthy() {
        let v = Verdict::from_outcome(
            ProbeOutcome::Stamp(stamp(100, RUNNING)),
            Some(100),
            RUNNING,
            hook(HookPresence::ManagedBlock),
            false,
        );
        assert_eq!(v, Verdict::Verified { generation: 100 });

        let deployed = v
            .notice(None, "Last loaded just now.", "HOOK", DEPLOYED)
            .unwrap();
        assert_eq!(deployed.state, "healthy");

        let empty = v
            .notice(None, "Last loaded just now.", "HOOK", false)
            .unwrap();
        assert_eq!(
            empty.state, "empty-script",
            "the spawn proved the hookup fires; it proved nothing about what it deploys"
        );
        // The evidence path's words for the state, not a second set.
        assert_eq!(
            empty.message,
            "Shell hookup: wired, but no packs are deployed."
        );
    }

    /// Skew outranks the empty-script rule for the measured path too —
    /// the order is [`activation::refine`]'s, applied once, not
    /// re-decided here.
    #[test]
    fn a_measured_skew_outranks_an_empty_script() {
        let v = Verdict::VersionSkew {
            generation: 100,
            loaded: EvidenceVersion::Known("5.0.0".into()),
        };
        let notice = v
            .notice(None, "Last loaded just now.", "HOOK", false)
            .unwrap();
        assert_eq!(notice.state, "version-skew");
    }

    /// Skew outranks the generation ladder in both directions, exactly
    /// as [`activation::Evidence::state`] applies it: a *stale*
    /// generation from the wrong dodot is still skew, because "open a
    /// new shell" is not the fix.
    #[test]
    fn skew_outranks_a_stale_generation_the_way_the_evidence_path_does() {
        let v = Verdict::from_outcome(
            ProbeOutcome::Stamp(stamp(90, "5.0.0")),
            Some(100),
            RUNNING,
            hook(HookPresence::ManagedBlock),
            false,
        );
        assert_eq!(
            v,
            Verdict::VersionSkew {
                generation: 90,
                loaded: EvidenceVersion::Known("5.0.0".into())
            }
        );
    }

    /// The bound release cannot tell its own version-less evidence from
    /// an older dodot's, so it claims no skew — one rule
    /// ([`activation::EvidenceVersion::is`]), applied here by calling
    /// it rather than by restating it.
    #[test]
    fn the_bound_release_does_not_claim_skew_on_a_version_less_stamp() {
        let v = Verdict::from_outcome(
            ProbeOutcome::Stamp(ProbeStamp {
                generation: 100,
                version: EvidenceVersion::PreVersion,
            }),
            Some(100),
            activation::PRE_VERSION_RELEASE,
            hook(HookPresence::ManagedBlock),
            false,
        );
        assert_eq!(v, Verdict::Verified { generation: 100 });
    }

    #[test]
    fn no_stamp_plus_no_hook_names_the_file_and_the_command() {
        let v = Verdict::from_outcome(
            ProbeOutcome::NoStamp,
            Some(100),
            RUNNING,
            hook(HookPresence::Absent),
            false,
        );
        assert_eq!(
            v,
            Verdict::Broken {
                diagnosis: Diagnosis::HookAbsent {
                    rc: "~/.zshrc".into()
                }
            }
        );
        let notice = v.notice(None, "Never loaded.", "HOOK", DEPLOYED).unwrap();
        assert_eq!(notice.state, "verified-broken");
        assert_eq!(notice.severity, "error");
        let hint = notice.hint.unwrap();
        assert!(hint.contains("~/.zshrc"), "{hint}");
        assert!(hint.contains("dodot install --write"), "{hint}");
    }

    #[test]
    fn no_stamp_from_a_known_capable_hook_blames_the_rc_file() {
        for presence in [HookPresence::ManagedBlock, HookPresence::Manual] {
            let v = Verdict::from_outcome(
                ProbeOutcome::NoStamp,
                Some(100),
                RUNNING,
                hook(presence),
                true,
            );
            let hint = v
                .notice(None, "Never loaded.", "HOOK", DEPLOYED)
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
    fn no_stamp_from_an_unknown_capability_hook_is_inconclusive() {
        let v = Verdict::from_outcome(
            ProbeOutcome::NoStamp,
            Some(100),
            RUNNING,
            hook(HookPresence::Manual),
            false,
        );

        assert!(matches!(v, Verdict::Unverified { .. }), "{v:?}");
    }

    #[test]
    fn timeout_from_a_known_capable_hook_is_broken() {
        let v = Verdict::from_outcome(
            ProbeOutcome::TimedOut,
            Some(100),
            RUNNING,
            hook(HookPresence::ManagedBlock),
            true,
        );

        assert_eq!(
            v,
            Verdict::Broken {
                diagnosis: Diagnosis::HookNotReached {
                    rc: "~/.zshrc".into()
                }
            }
        );
    }

    #[test]
    fn no_stamp_without_static_hook_context_is_inconclusive() {
        let v = Verdict::from_outcome(ProbeOutcome::NoStamp, Some(100), RUNNING, None, true);

        assert!(matches!(v, Verdict::Unverified { .. }), "{v:?}");
    }

    #[test]
    fn a_stale_sourced_script_is_reported_as_such() {
        let v = Verdict::from_outcome(
            ProbeOutcome::Stamp(stamp(90, RUNNING)),
            Some(100),
            RUNNING,
            hook(HookPresence::ManagedBlock),
            false,
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
            let v = Verdict::from_outcome(
                outcome,
                Some(100),
                RUNNING,
                hook(HookPresence::Absent),
                false,
            );
            let notice = v
                .notice(Some(evidence.clone()), "Never loaded.", "HOOK", DEPLOYED)
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
        assert_eq!(v.notice(None, "Never loaded.", "HOOK", DEPLOYED), None);
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
    fn a_shell_that_activates_reports_both_halves_of_the_stamp() {
        let env = TempEnvironment::builder().build();
        let shell = fake_shell(
            &env,
            "activating-shell",
            &format!("export {INIT_GEN_ENV}=1755200000\nexport {INIT_VERSION_ENV}=5.6.0"),
        );
        assert_eq!(
            run(&shell, Duration::from_secs(10)),
            ProbeOutcome::Stamp(stamp(1_755_200_000, "5.6.0"))
        );
    }

    /// The epic's own failure, measured end to end: the hook resolves
    /// to a different dodot, whose init script exports a *current*
    /// generation — the shape that used to come back as a green
    /// "sourced in new shells". Both flavours of wrong binary: one that
    /// names its version, and a pre-RCS01 one that exports none.
    #[test]
    fn a_shell_activating_another_dodot_measures_as_skew() {
        for (name, exports, loaded) in [
            (
                "older-dodot-shell",
                format!("export {INIT_GEN_ENV}=1755200000\nexport {INIT_VERSION_ENV}=5.0.0"),
                EvidenceVersion::Known("5.0.0".into()),
            ),
            (
                "pre-version-dodot-shell",
                format!("export {INIT_GEN_ENV}=1755200000"),
                EvidenceVersion::PreVersion,
            ),
        ] {
            let env = TempEnvironment::builder().build();
            let shell = fake_shell(&env, name, &exports);
            let outcome = run(&shell, Duration::from_secs(10));
            assert_eq!(
                outcome,
                ProbeOutcome::Stamp(ProbeStamp {
                    generation: 1_755_200_000,
                    version: loaded.clone(),
                }),
                "{name}"
            );
            let verdict = Verdict::from_outcome(
                outcome,
                Some(1_755_200_000),
                RUNNING,
                hook(HookPresence::Manual),
                false,
            );
            assert_eq!(
                verdict,
                Verdict::VersionSkew {
                    generation: 1_755_200_000,
                    loaded
                },
                "{name}: a fresh generation from the wrong dodot is not a verification"
            );
        }
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
             export {INIT_VERSION_ENV}=5.6.0\n\
             eval \"$2\"\n\
             exit 3\n"
        );
        env.fs
            .write_file_with_mode(&path, script.as_bytes(), 0o755)
            .unwrap();
        assert_eq!(
            run(&path, Duration::from_secs(10)),
            ProbeOutcome::Stamp(stamp(42, "5.6.0"))
        );
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

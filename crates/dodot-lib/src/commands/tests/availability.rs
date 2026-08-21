//! What an absent manager costs, and what it must not.
//!
//! A Linux user with a `Brewfile` had `brew bundle` spawned at them
//! anyway; the spawn failed, the error left the intent loop, and the
//! pack lost the symlinks it had already earned. These tests pin the
//! replacement: dodot asks whether the manager is there, an absent one
//! is a skip that names where it looked, and `up`, `status`, and
//! `--dry-run` all say the same thing about the same machine.
//!
//! Every manager here is a fake executable in a temp directory. The
//! probe's candidate list is injected on the context precisely so
//! these tests do not depend on whether the machine running them has
//! Homebrew installed — a laptop and a Linux CI runner have to agree.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::commands::{self, DisplayFile, PackStatusResult};
use crate::datastore::{CommandOutput, CommandRunner, CommandSpec};
use crate::fs::{DirEntry, Fs, FsMetadata, OsFs};
use crate::handlers::HANDLER_HOMEBREW;
use crate::packs::orchestration::ExecutionContext;
use crate::paths::Pather;
use crate::provisioners::availability::ProvisionHost;
use crate::testing::TempEnvironment;
use crate::Result;

/// A pack with a `Brewfile` and a symlink, so every test can also ask
/// what the *rest* of the pack did.
fn env_with_a_brewfile() -> TempEnvironment {
    TempEnvironment::builder()
        .pack("dev")
        .file("Brewfile", "brew \"ripgrep\"\n")
        .file("home.vimrc", "set nocompatible")
        .done()
        .build()
}

/// Where the fake `brew` lives — inside the test's own temp tree,
/// never a real prefix.
fn brew_path(env: &TempEnvironment) -> PathBuf {
    env.home.join("fake-prefix/bin/brew")
}

/// Create the fake `brew`: a regular file with an execute bit, which
/// is the entire question the presence probe asks.
fn install_fake_brew(env: &TempEnvironment) {
    let path = brew_path(env);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// A provisioning-enabled context that looks for brew at exactly one
/// place: the test's fake prefix.
fn ctx_probing_fake_prefix(env: &TempEnvironment) -> ExecutionContext {
    let mut ctx = super::support::make_ctx(env);
    ctx.no_provision = false;
    ctx.provision_host = Arc::new(ProvisionHost::with_candidates(
        HANDLER_HOMEBREW,
        vec![brew_path(env)],
    ));
    ctx
}

fn row<'a>(result: &'a PackStatusResult, name: &str) -> &'a DisplayFile {
    result.packs[0]
        .files
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| {
            panic!(
                "no row for `{name}`; rows: {:?}",
                result.packs[0]
                    .files
                    .iter()
                    .map(|f| (&f.name, &f.status, &f.status_label))
                    .collect::<Vec<_>>()
            )
        })
}

fn note_of(result: &PackStatusResult, file: &DisplayFile) -> String {
    file.note_ref
        .map(|n| result.notes[(n - 1) as usize].body.clone())
        .unwrap_or_else(|| panic!("row `{}` carries no note", file.name))
}

fn brew_sentinel_exists(env: &TempEnvironment) -> bool {
    let dir = env.paths.handler_data_dir("dev", HANDLER_HOMEBREW);
    std::fs::read_dir(&dir)
        .map(|entries| entries.count() > 0)
        .unwrap_or(false)
}

// ── The skip ────────────────────────────────────────────────────

#[test]
fn an_absent_manager_is_a_skip_that_names_where_dodot_looked() {
    let env = env_with_a_brewfile();
    let ctx = ctx_probing_fake_prefix(&env);

    let result = commands::up::up(None, &ctx).unwrap();

    let brewfile = row(&result, "Brewfile");
    assert_eq!(brewfile.status, "skipped");
    assert_eq!(brewfile.status_label, "homebrew not installed");

    let note = note_of(&result, brewfile);
    assert!(
        note.contains(&brew_path(&env).display().to_string()),
        "the note must name the location probed, or \"not installed\" and \
         \"installed somewhere dodot does not look\" are the same row: {note}"
    );
    assert!(note.contains("https://brew.sh"), "note: {note}");
}

#[test]
fn an_absent_manager_costs_the_pack_nothing() {
    let env = env_with_a_brewfile();
    let ctx = ctx_probing_fake_prefix(&env);

    let result = commands::up::up(None, &ctx).unwrap();

    // The symlink phase runs after provisioning, and used to be lost
    // with it.
    env.assert_double_link(
        "dev",
        "symlink",
        "home.vimrc",
        &env.dotfiles_root.join("dev/home.vimrc"),
        &env.home.join(".vimrc"),
    );
    assert_eq!(row(&result, "home.vimrc").status, "deployed");
    assert!(
        !result.failed,
        "an absent manager is not a failure and must not reach the exit code"
    );
    assert!(
        !brew_sentinel_exists(&env),
        "a skipped file must leave no receipt behind"
    );
}

#[test]
fn installing_the_manager_runs_the_file_on_the_next_up() {
    // The point of writing no receipt: no flag, no cleanup, no state
    // to repair — the next `up` simply runs it.
    let env = env_with_a_brewfile();
    let ctx = ctx_probing_fake_prefix(&env);

    commands::up::up(None, &ctx).unwrap();
    assert!(!brew_sentinel_exists(&env));

    install_fake_brew(&env);
    let result = commands::up::up(None, &ctx).unwrap();

    assert_eq!(row(&result, "Brewfile").status, "deployed");
    assert!(
        brew_sentinel_exists(&env),
        "with brew present the Brewfile runs and records a receipt"
    );
}

// ── The agreement matrix ────────────────────────────────────────

/// Answers `stat` for one path with a staged reply, and delegates
/// everything else to the real filesystem.
///
/// Two machine states in this file cannot be staged on a real disk: a
/// directory the test user cannot read is not a property a suite can
/// rely on, and macOS refuses to create a file whose name is not valid
/// UTF-8 at all. Both are one `stat` answer, so both are this.
struct StagedStat {
    inner: OsFs,
    path: PathBuf,
    answer: StagedAnswer,
}

/// What [`StagedStat`] reports for its path.
enum StagedAnswer {
    /// A permission error — the probe cannot examine the candidate.
    Denied,
    /// A regular file carrying an execute bit — a brew, as far as the
    /// probe can tell.
    Executable,
}

impl StagedStat {
    fn staged(&self, path: &Path) -> Result<FsMetadata> {
        match self.answer {
            StagedAnswer::Denied => Err(crate::DodotError::Fs {
                path: path.to_path_buf(),
                source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            }),
            StagedAnswer::Executable => Ok(FsMetadata {
                is_file: true,
                is_dir: false,
                is_symlink: false,
                len: 0,
                mode: 0o755,
            }),
        }
    }
}

impl Fs for StagedStat {
    fn stat(&self, path: &Path) -> Result<FsMetadata> {
        if path == self.path {
            return self.staged(path);
        }
        self.inner.stat(path)
    }
    fn lstat(&self, path: &Path) -> Result<FsMetadata> {
        self.inner.lstat(path)
    }
    fn open_read(&self, path: &Path) -> Result<Box<dyn std::io::Read + Send + Sync>> {
        self.inner.open_read(path)
    }
    fn read_file(&self, path: &Path) -> Result<Vec<u8>> {
        self.inner.read_file(path)
    }
    fn read_to_string(&self, path: &Path) -> Result<String> {
        self.inner.read_to_string(path)
    }
    fn write_file(&self, path: &Path, contents: &[u8]) -> Result<()> {
        self.inner.write_file(path, contents)
    }
    fn set_permissions(&self, path: &Path, mode: u32) -> Result<()> {
        self.inner.set_permissions(path, mode)
    }
    fn mkdir_all(&self, path: &Path) -> Result<()> {
        self.inner.mkdir_all(path)
    }
    fn symlink(&self, original: &Path, link: &Path) -> Result<()> {
        self.inner.symlink(original, link)
    }
    fn readlink(&self, path: &Path) -> Result<PathBuf> {
        self.inner.readlink(path)
    }
    fn remove_file(&self, path: &Path) -> Result<()> {
        self.inner.remove_file(path)
    }
    fn remove_dir_all(&self, path: &Path) -> Result<()> {
        self.inner.remove_dir_all(path)
    }
    fn exists(&self, path: &Path) -> bool {
        self.inner.exists(path)
    }
    fn is_symlink(&self, path: &Path) -> bool {
        self.inner.is_symlink(path)
    }
    fn is_dir(&self, path: &Path) -> bool {
        self.inner.is_dir(path)
    }
    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>> {
        self.inner.read_dir(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        self.inner.rename(from, to)
    }
    fn copy_file(&self, from: &Path, to: &Path) -> Result<()> {
        self.inner.copy_file(from, to)
    }
}

/// The three machine states, staged.
enum Machine {
    Present,
    Absent,
    Unprobeable,
}

fn ctx_for(env: &TempEnvironment, machine: &Machine) -> ExecutionContext {
    let mut ctx = ctx_probing_fake_prefix(env);
    match machine {
        Machine::Present => install_fake_brew(env),
        Machine::Absent => {}
        Machine::Unprobeable => {
            ctx.fs = Arc::new(StagedStat {
                inner: OsFs::new(),
                path: brew_path(env),
                answer: StagedAnswer::Denied,
            });
        }
    }
    ctx
}

/// For each machine state, `up`, `status`, and `up --dry-run` must
/// report the same thing about the same `Brewfile`.
///
/// Agreement is not a coincidence to be spot-checked: `up`'s planner
/// and `status` read one probe, and this is the test that says so out
/// loud. A second implementation of "is brew here?" would fail here
/// before it reached a user.
#[test]
fn up_status_and_dry_run_agree_on_every_machine_state() {
    for (machine, expected_status, expected_label) in [
        (Machine::Present, "pending", "brew packages not installed"),
        (Machine::Absent, "skipped", "homebrew not installed"),
        (Machine::Unprobeable, "broken", "cannot probe homebrew"),
    ] {
        let env = env_with_a_brewfile();

        // `status` and `--dry-run` first: both are passive, so they
        // report the pre-`up` machine, which for a present brew is
        // "never ran".
        let ctx = ctx_for(&env, &machine);
        let status = commands::status::status(None, &ctx).unwrap();
        assert_eq!(
            (
                row(&status, "Brewfile").status.as_str(),
                row(&status, "Brewfile").status_label.as_str()
            ),
            (expected_status, expected_label),
            "status disagrees for {expected_label}"
        );

        let mut dry = ctx_for(&env, &machine);
        dry.dry_run = true;
        let preview = commands::up::up(None, &dry).unwrap();
        let previewed = row(&preview, "Brewfile");
        if matches!(machine, Machine::Present) {
            // A present manager produces a real intent, which the
            // preview renders as the operation it would run.
            assert_eq!(previewed.handler, HANDLER_HOMEBREW);
        } else {
            assert_eq!(
                (previewed.status.as_str(), previewed.status_label.as_str()),
                (expected_status, expected_label),
                "--dry-run disagrees for {expected_label}"
            );
        }

        let up = commands::up::up(None, &ctx_for(&env, &machine)).unwrap();
        let after = row(&up, "Brewfile");
        if matches!(machine, Machine::Present) {
            assert_eq!(after.status, "deployed", "up disagrees for a present brew");
        } else {
            assert_eq!(
                (after.status.as_str(), after.status_label.as_str()),
                (expected_status, expected_label),
                "up disagrees for {expected_label}"
            );
        }
    }
}

#[test]
fn a_probe_failure_is_an_error_not_an_absence() {
    let env = env_with_a_brewfile();
    let ctx = ctx_for(&env, &Machine::Unprobeable);

    let result = commands::status::status(None, &ctx).unwrap();
    let brewfile = row(&result, "Brewfile");

    assert_eq!(brewfile.status, "broken");
    let note = note_of(&result, brewfile);
    assert!(note.contains("permission denied"), "note: {note}");
    assert_eq!(
        result.notes[(brewfile.note_ref.unwrap() - 1) as usize].kind,
        "error",
        "a probe failure must not be softened into a warning"
    );
}

// ── The receipt outranks the manager's departure ────────────────

#[test]
fn a_manager_removed_after_a_successful_run_annotates_without_changing_the_verdict() {
    let env = env_with_a_brewfile();
    let ctx = ctx_probing_fake_prefix(&env);

    install_fake_brew(&env);
    commands::up::up(None, &ctx).unwrap();
    assert!(brew_sentinel_exists(&env));

    std::fs::remove_file(brew_path(&env)).unwrap();
    let result = commands::status::status(None, &ctx).unwrap();
    let brewfile = row(&result, "Brewfile");

    assert_eq!(
        brewfile.status, "deployed",
        "the receipt wins: this Brewfile did run, and brew leaving does not undo it"
    );
    assert_eq!(brewfile.status_label, "installed");
    let note = note_of(&result, brewfile);
    assert!(note.contains("homebrew is not installed"), "note: {note}");
    assert_eq!(
        result.notes[(brewfile.note_ref.unwrap() - 1) as usize].kind,
        "warning",
        "annotating a working row must not present as a current error"
    );
}

// ── Three skips, three stories ──────────────────────────────────

#[test]
fn an_absent_manager_a_gate_and_no_provision_read_differently() {
    // Same output, three reasons a file did not run — each with its
    // own remedy, so none of them may borrow another's words.
    let gate_that_is_false_here = if cfg!(target_os = "macos") {
        "_linux"
    } else {
        "_darwin"
    };
    let env = TempEnvironment::builder()
        .pack("dev")
        .file("Brewfile", "brew \"ripgrep\"\n")
        .file(&format!("install.{gate_that_is_false_here}.sh"), "echo hi")
        .done()
        .build();

    let ctx = ctx_probing_fake_prefix(&env);
    let result = commands::status::status(None, &ctx).unwrap();
    let absent = row(&result, "Brewfile").status_label.clone();
    let gated = result.packs[0]
        .files
        .iter()
        .find(|f| f.name.starts_with("install."))
        .map(|f| f.status_label.clone())
        .expect("the gated file has a row of its own");

    let mut no_provision = ctx_probing_fake_prefix(&env);
    no_provision.no_provision = true;
    let skipped = row(
        &commands::status::status(None, &no_provision).unwrap(),
        "Brewfile",
    )
    .status_label
    .clone();

    assert_eq!(absent, "homebrew not installed");
    assert_eq!(skipped, crate::commands::PROVISION_SKIPPED_LABEL);
    assert!(gated.starts_with("gated out"), "gated label: {gated}");
    assert_ne!(absent, skipped);
    assert_ne!(absent, gated);
}

// ── Status stays passive ────────────────────────────────────────

/// Panics on any spawn. The presence probe reads mode bits, and a
/// `status` that needed a manager to *answer* would trip this.
struct NeverSpawns;

impl CommandRunner for NeverSpawns {
    fn run(&self, command: CommandSpec<'_>) -> Result<CommandOutput> {
        panic!(
            "status spawned {} {:?}",
            command.executable, command.arguments
        );
    }
}

fn snapshot_data_dir(env: &TempEnvironment) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, Option<Vec<u8>>)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.map(|e| e.unwrap().path()).collect();
        entries.sort();
        for path in entries {
            if path.is_dir() && !path.is_symlink() {
                out.push((path.clone(), None));
                walk(&path, out);
            } else {
                out.push((path.clone(), std::fs::read(&path).ok()));
            }
        }
    }
    let mut out = Vec::new();
    walk(env.paths.data_dir(), &mut out);
    out
}

#[test]
fn status_spawns_nothing_and_writes_nothing_whether_the_manager_is_there_or_not() {
    let env = env_with_a_brewfile();

    // Prime the datastore with a real deployment first, so the
    // comparison is against populated state rather than an empty dir.
    install_fake_brew(&env);
    commands::up::up(None, &ctx_probing_fake_prefix(&env)).unwrap();
    let primed = snapshot_data_dir(&env);

    for machine in [Machine::Present, Machine::Absent] {
        if matches!(machine, Machine::Absent) {
            std::fs::remove_file(brew_path(&env)).unwrap();
        }
        let mut ctx = super::support::make_ctx_with_runner(&env, Arc::new(NeverSpawns));
        ctx.no_provision = false;
        ctx.provision_host = Arc::new(ProvisionHost::with_candidates(
            HANDLER_HOMEBREW,
            vec![brew_path(&env)],
        ));

        commands::status::status(None, &ctx).unwrap();
        assert_eq!(
            primed,
            snapshot_data_dir(&env),
            "status must leave the datastore byte-identical"
        );
    }
}

// ── The run spawns the executable the probe found ───────────────

/// Records every spawn so a test can assert on the *command*, not
/// merely on whether a sentinel appeared afterwards.
///
/// [`MockCommandRunner`](super::support::MockCommandRunner) answers
/// success to anything, which makes a sentinel a poor witness: it
/// would be written just as happily for a `brew` the OS could never
/// have resolved.
/// One spawn, as the runner saw it.
struct Spawn {
    executable: String,
    arguments: Vec<String>,
    environment: Vec<(String, String)>,
}

#[derive(Default)]
struct RecordingRunner {
    spawns: std::sync::Mutex<Vec<Spawn>>,
}

impl CommandRunner for RecordingRunner {
    fn run(&self, command: CommandSpec<'_>) -> Result<CommandOutput> {
        self.spawns.lock().unwrap().push(Spawn {
            executable: command.executable.to_string(),
            arguments: command.arguments.to_vec(),
            environment: command.environment.to_vec(),
        });
        Ok(CommandOutput {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

#[test]
fn the_run_spawns_the_absolute_path_the_probe_found() {
    // The probe answers "brew is at <this exact path>". Spawning a
    // bare `brew` afterwards asks the OS a different question — one
    // that a host whose `PATH` omits the prefix answers with "no such
    // command", turning a manager dodot just found into a failed run.
    //
    // The fake brew lives inside the test's temp tree, which no
    // `PATH` entry names, so the argument is not merely equal to the
    // located path: it is the only spelling that could work.
    let env = env_with_a_brewfile();
    install_fake_brew(&env);

    let runner = Arc::new(RecordingRunner::default());
    let mut ctx = super::support::make_ctx_with_runner(&env, runner.clone());
    ctx.no_provision = false;
    ctx.provision_host = Arc::new(ProvisionHost::with_candidates(
        HANDLER_HOMEBREW,
        vec![brew_path(&env)],
    ));

    commands::up::up(None, &ctx).unwrap();

    let spawns = runner.spawns.lock().unwrap();
    let bundle = spawns
        .iter()
        .find(|s| s.arguments.first().map(String::as_str) == Some("bundle"))
        .expect("the Brewfile ran");

    assert_eq!(
        Path::new(&bundle.executable),
        brew_path(&env),
        "the run must spawn the brew the probe found, not a name for the OS to resolve"
    );
    // The arguments are the handler's own, untouched: substituting
    // the program must not shift the manifest position the descriptor
    // in `provisioners::PROVISIONERS` declares.
    let manifest = crate::provisioners::manifest_argument(HANDLER_HOMEBREW, &bundle.arguments);
    assert!(
        manifest.is_some_and(|m| m.ends_with("Brewfile")),
        "the declared manifest position must still find the Brewfile in {:?}",
        bundle.arguments
    );
    // And the descriptor's environment still rides along: the program
    // and the environment come from the same row, so substituting one
    // must not drop the other.
    assert_eq!(
        bundle.environment.as_slice(),
        [("HOMEBREW_NO_AUTO_UPDATE".to_string(), "1".to_string())]
    );
}

#[test]
fn a_handler_dodot_does_not_locate_keeps_its_path_lookup() {
    // `install` resolves its interpreter through `PATH` by design
    // (ADR-0007) and probes present with no path, so nothing is
    // substituted — the spawn stays the bare interpreter name.
    let env = TempEnvironment::builder()
        .pack("dev")
        .file("install.sh", "echo hi\n")
        .done()
        .build();

    let runner = Arc::new(RecordingRunner::default());
    let mut ctx = super::support::make_ctx_with_runner(&env, runner.clone());
    ctx.no_provision = false;

    commands::up::up(None, &ctx).unwrap();

    let spawns = runner.spawns.lock().unwrap();
    let script = spawns
        .iter()
        .find(|s| s.arguments.iter().any(|a| a.ends_with("install.sh")))
        .expect("the install script ran");
    assert!(
        !script.executable.contains('/'),
        "install\u{2019}s interpreter is a PATH lookup, not a located path: {}",
        script.executable
    );
}

#[cfg(unix)]
#[test]
fn a_located_path_dodot_cannot_name_is_never_handed_over_corrupted() {
    // The probe reads `$HOMEBREW_PREFIX` as bytes so a non-UTF-8
    // prefix keeps its candidate. An intent's executable is a
    // `String`, so such a path cannot be carried through to the
    // spawn — and replacing its bytes with U+FFFD would be the one
    // unacceptable answer: dodot would stat a real brew, then spawn a
    // path naming no file and report "not found" about it.
    //
    // What must hold is that nothing corrupted reaches the runner.
    // The row falls back to the handler's own name, which is where
    // every provisioning row was before the probe existed, and says
    // so in a warning.
    //
    // The candidate is staged rather than created: macOS refuses to
    // write a filename that is not valid UTF-8, so a test that made
    // the file would only ever run on Linux.
    use std::os::unix::ffi::OsStrExt;

    let env = env_with_a_brewfile();
    let brew = env
        .home
        .join(std::ffi::OsStr::from_bytes(b"pre\xffix"))
        .join("bin/brew");
    assert!(
        brew.to_str().is_none(),
        "the fixture only tests anything if the path is genuinely unnameable"
    );

    let runner = Arc::new(RecordingRunner::default());
    let mut ctx = super::support::make_ctx_with_runner(&env, runner.clone());
    ctx.no_provision = false;
    ctx.fs = Arc::new(StagedStat {
        inner: OsFs::new(),
        path: brew.clone(),
        answer: StagedAnswer::Executable,
    });
    ctx.provision_host = Arc::new(ProvisionHost::with_candidates(
        HANDLER_HOMEBREW,
        vec![brew.clone()],
    ));

    let result = commands::up::up(None, &ctx).unwrap();

    let spawns = runner.spawns.lock().unwrap();
    let bundle = spawns
        .iter()
        .find(|s| s.arguments.first().map(String::as_str) == Some("bundle"))
        .expect("the Brewfile still runs — an unnameable path is not an absent manager");
    assert!(
        !bundle.executable.contains(char::REPLACEMENT_CHARACTER),
        "a lossy path would name no file at all: {}",
        bundle.executable
    );
    assert_eq!(
        bundle.executable, "brew",
        "what dodot cannot name, it does not pretend to have named"
    );
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("cannot name exactly")),
        "the substitution dodot did not make has to be visible: {:?}",
        result.warnings
    );
}

// ── A probe failure reaches the exit code ───────────────────────

#[test]
fn a_probe_failure_fails_the_run_without_costing_the_pack() {
    // An absent manager is an ordinary machine state and leaves the
    // exit code alone. A manager dodot could not *look for* is the
    // other thing: the question went unanswered, so the run's report
    // about that file is not to be trusted and `up` says so. The rest
    // of the pack still deploys either way (ADR-0008).
    let env = env_with_a_brewfile();
    let ctx = ctx_for(&env, &Machine::Unprobeable);

    let result = commands::up::up(None, &ctx).unwrap();

    assert!(
        result.failed,
        "a probe dodot could not answer is a failure, and the only one that never \
         reaches a pack result — no intent means no operation to carry the verdict"
    );
    assert_eq!(
        result.message.as_deref(),
        Some("Packs deployed with errors."),
        "the message and the exit code must tell the same story"
    );
    assert_eq!(
        row(&result, "home.vimrc").status,
        "deployed",
        "the rest of the pack still deploys"
    );
}

#[test]
fn an_absent_manager_and_a_dry_run_both_stay_out_of_the_exit_code() {
    let env = env_with_a_brewfile();

    assert!(
        !commands::up::up(None, &ctx_for(&env, &Machine::Absent))
            .unwrap()
            .failed,
        "absence is a skip, not a failure"
    );

    let mut dry = ctx_for(&env, &Machine::Unprobeable);
    dry.dry_run = true;
    assert!(
        !commands::up::up(None, &dry).unwrap().failed,
        "a dry run attempted nothing, so it reports the probe failure without being one"
    );
}

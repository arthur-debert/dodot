//! What a manager's *version* costs, and where dodot is allowed to
//! ask for it.
//!
//! A `Brewfile` may declare `go`, `cargo`, `uv`, `npm`, and `krew`
//! entries, and those need Homebrew 5.1.2 or newer. Brew's own
//! auto-update used to carry a stale installation over that line;
//! dodot switched the auto-update off, so it no longer does. These
//! tests pin what dodot does instead: it asks — on `up`, once, and
//! only for a file it is about to run — reports a below-floor brew
//! with `brew update` as the remedy, and runs the file anyway.
//!
//! The other half is the boundary. Asking costs a subprocess, so
//! `dodot status` and `dodot up --dry-run` must not ask at all, and
//! the runner in these tests panics if they do.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::commands::{self, PackStatusResult};
use crate::datastore::{CommandOutput, CommandRunner, CommandSpec};
use crate::handlers::HANDLER_HOMEBREW;
use crate::packs::orchestration::ExecutionContext;
use crate::paths::Pather;
use crate::provisioners::availability::ProvisionHost;
use crate::testing::TempEnvironment;
use crate::Result;

/// A pack with a `Brewfile` and a symlink, so a test can also ask
/// what the rest of the pack did while brew was being judged.
fn env_with_a_brewfile() -> TempEnvironment {
    TempEnvironment::builder()
        .pack("dev")
        .file("Brewfile", "npm \"typescript\"\n")
        .file("home.vimrc", "set nocompatible")
        .done()
        .build()
}

/// Where the fake `brew` lives — inside the test's own temp tree,
/// never a real prefix.
fn brew_path(env: &TempEnvironment) -> PathBuf {
    env.home.join("fake-prefix/bin/brew")
}

/// Create the fake `brew`. Presence is a `stat` question, so a file
/// with an execute bit is a whole brew as far as WS03's probe is
/// concerned; what it *says* comes from the runner below.
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

/// One spawn, as the runner saw it.
#[derive(Clone)]
struct Spawn {
    executable: String,
    arguments: Vec<String>,
    environment: Vec<(String, String)>,
}

/// Answers `brew --version` with a staged reply, succeeds at
/// everything else, and records the lot.
struct BrewSaying {
    version: String,
    spawns: Mutex<Vec<Spawn>>,
}

impl BrewSaying {
    fn new(version: &str) -> Arc<Self> {
        Arc::new(Self {
            version: version.to_string(),
            spawns: Mutex::new(Vec::new()),
        })
    }

    fn version_probes(&self) -> Vec<Spawn> {
        self.spawns
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.arguments == ["--version"])
            .cloned()
            .collect()
    }

    fn ran_the_brewfile(&self) -> bool {
        self.spawns
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.arguments.first().map(String::as_str) == Some("bundle"))
    }
}

impl CommandRunner for BrewSaying {
    fn run(&self, command: CommandSpec<'_>) -> Result<CommandOutput> {
        self.spawns.lock().unwrap().push(Spawn {
            executable: command.executable.to_string(),
            arguments: command.arguments.to_vec(),
            environment: command.environment.to_vec(),
        });
        let stdout = if command.arguments == ["--version"] {
            format!("Homebrew {}\n", self.version)
        } else {
            String::new()
        };
        Ok(CommandOutput {
            exit_code: 0,
            stdout,
            stderr: String::new(),
        })
    }
}

/// What `brew bundle` says on a brew too old for an `npm` line —
/// close enough to the real thing for a test about *when* the user
/// reads it.
const BUNDLE_PARSE_ERROR: &str = "Error: Unknown Brewfile method: npm";

/// A brew below the floor that then fails the bundle, writing to the
/// terminal as it goes.
///
/// The stderr write is the part that matters:
/// [`ShellCommandRunner`](crate::datastore::ShellCommandRunner)
/// forwards a failed provisioning command's stderr to the user's own
/// stderr while the run is still in progress. A fake that only
/// returned the text would make the ordering this test is about
/// unobservable.
struct BrewTooOldForTheBundle;

impl CommandRunner for BrewTooOldForTheBundle {
    fn run(&self, command: CommandSpec<'_>) -> Result<CommandOutput> {
        use std::io::Write;

        if command.arguments == ["--version"] {
            return Ok(CommandOutput {
                exit_code: 0,
                stdout: "Homebrew 5.0.16\n".to_string(),
                stderr: String::new(),
            });
        }
        if command.arguments.first().map(String::as_str) == Some("bundle") {
            // Through `std::io::stderr()`, as `ShellCommandRunner`
            // does — a fake that used `eprintln!` would write
            // somewhere the user's terminal is not.
            let _ = writeln!(std::io::stderr(), "{BUNDLE_PARSE_ERROR}");
            return Err(crate::DodotError::CommandFailed {
                command: format!("{} bundle", command.executable),
                exit_code: 1,
                stderr: format!("{BUNDLE_PARSE_ERROR}\n"),
            });
        }
        Ok(CommandOutput {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

/// Serializes the redirection below — fd 2 is process-wide, and two
/// captures at once would each take half of the other's output.
static STDERR_CAPTURE: Mutex<()> = Mutex::new(());

/// Run `body` with the process's stderr pointed at a temp file, and
/// return its result alongside everything written there.
///
/// The only way to answer the question this module's headline test
/// asks. dodot's warning and brew's parse error are two writers to
/// one terminal, and no return value can say which of them the user
/// reads first; the file can. Output from a test running concurrently
/// may land in the capture too — harmless, since the assertions look
/// for two specific lines and compare their positions.
///
/// `body` runs on a thread of its own because the test harness
/// installs its own stderr sink per test thread, which `eprintln!`
/// finds before fd 2. A fresh thread has no such sink, so its output
/// goes to the file descriptor — which is what the user has.
fn capturing_stderr<T: Send>(body: impl FnOnce() -> T + Send) -> (T, String) {
    use std::io::Write;
    use std::os::unix::io::AsRawFd;

    let _serialized = STDERR_CAPTURE.lock().unwrap_or_else(|e| e.into_inner());
    let file = tempfile::NamedTempFile::new().expect("a file to capture stderr into");
    let saved = unsafe { libc::dup(libc::STDERR_FILENO) };
    assert!(saved >= 0, "could not duplicate stderr");
    assert!(
        unsafe { libc::dup2(file.as_file().as_raw_fd(), libc::STDERR_FILENO) } >= 0,
        "could not redirect stderr"
    );

    // Restore fd 2 even if `body` panics — leaving the process's
    // stderr pointed at a deleted temp file would silence every test
    // that ran afterwards.
    let outcome = std::thread::scope(|scope| scope.spawn(body).join());

    let _ = std::io::stderr().flush();
    unsafe {
        libc::dup2(saved, libc::STDERR_FILENO);
        libc::close(saved);
    }
    let printed = std::fs::read_to_string(file.path()).expect("the capture file reads back");
    match outcome {
        Ok(value) => (value, printed),
        Err(panic) => {
            eprint!("{printed}");
            std::panic::resume_unwind(panic)
        }
    }
}

/// Panics on any `--version` spawn, and succeeds quietly at
/// everything else.
///
/// The fitness probe is the only thing in dodot that asks a manager
/// for its version, so this is the passive-command contract with
/// teeth: a `status` or `--dry-run` that reached it dies here rather
/// than silently costing a user a subprocess.
#[derive(Default)]
struct NeverAsksTheVersion;

impl CommandRunner for NeverAsksTheVersion {
    fn run(&self, command: CommandSpec<'_>) -> Result<CommandOutput> {
        assert_ne!(
            command.arguments,
            ["--version"],
            "a passive command asked {} for its version — the fitness probe spawns, \
             and only `dodot up` may reach it",
            command.executable
        );
        Ok(CommandOutput {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

/// A provisioning-enabled context whose brew is the test's fake one,
/// answering through `runner`.
fn ctx_with(env: &TempEnvironment, runner: Arc<dyn CommandRunner>) -> ExecutionContext {
    let mut ctx = super::support::make_ctx_with_runner(env, runner);
    ctx.no_provision = false;
    ctx.provision_host = Arc::new(ProvisionHost::with_candidates(
        HANDLER_HOMEBREW,
        vec![brew_path(env)],
    ));
    ctx
}

fn warning_about_the_version(result: &PackStatusResult) -> Option<&String> {
    result
        .warnings
        .iter()
        .find(|w| w.contains("5.1.2") || w.contains("did not report its version"))
}

// ── The floor ───────────────────────────────────────────────────

#[test]
fn a_brew_below_the_floor_is_named_with_its_remedy_and_the_file_still_runs() {
    let env = env_with_a_brewfile();
    install_fake_brew(&env);
    let runner = BrewSaying::new("5.0.16");

    let result = commands::up::up(None, &ctx_with(&env, runner.clone())).unwrap();

    let warning = warning_about_the_version(&result)
        .unwrap_or_else(|| panic!("no version warning in {:?}", result.warnings));
    assert!(warning.contains("5.0.16"), "{warning}");
    assert!(
        warning.contains("brew update"),
        "the remedy is what makes this a condition rather than a complaint: {warning}"
    );
    assert!(
        !warning.contains("not installed"),
        "an old brew is not a missing one: {warning}"
    );
    assert!(
        runner.ran_the_brewfile(),
        "dodot has not read the Brewfile, so it does not know the floor matters here — \
         refusing the run would turn an unverified warning into a failed deployment"
    );
    assert!(
        !result.failed,
        "a below-floor manager is a warning, not a failed run"
    );
}

#[test]
fn the_version_question_is_asked_before_the_brewfile_runs() {
    // Spawn order only — that the question precedes the work. What
    // the *user* sees, and in which order, is the sibling test below.
    let env = env_with_a_brewfile();
    install_fake_brew(&env);
    let runner = BrewSaying::new("5.0.16");

    commands::up::up(None, &ctx_with(&env, runner.clone())).unwrap();

    let spawns = runner.spawns.lock().unwrap();
    let asked = spawns
        .iter()
        .position(|s| s.arguments == ["--version"])
        .expect("brew was asked its version");
    let ran = spawns
        .iter()
        .position(|s| s.arguments.first().map(String::as_str) == Some("bundle"))
        .expect("the Brewfile ran");
    assert!(asked < ran, "the version question comes first");
}

#[test]
fn the_warning_reaches_the_terminal_before_the_bundle_does() {
    // The claim the whole feature rests on: when `brew bundle` fails
    // on an `npm` line, the explanation is *already on screen*. That
    // is a claim about output, not about spawn order — the warning
    // also travels in `PackStatusResult::warnings`, which the CLI
    // prints only after `up` has returned, long after brew has
    // written its own diagnostics to the same terminal. So this test
    // reads the terminal.
    let env = env_with_a_brewfile();
    install_fake_brew(&env);
    let ctx = ctx_with(&env, Arc::new(BrewTooOldForTheBundle));

    let (result, printed) = capturing_stderr(|| commands::up::up(None, &ctx).unwrap());

    let warned = printed
        .find("older than 5.1.2")
        .unwrap_or_else(|| panic!("the version warning never reached stderr:\n{printed}"));
    let failed = printed
        .find(BUNDLE_PARSE_ERROR)
        .unwrap_or_else(|| panic!("the bundle's own error never reached stderr:\n{printed}"));
    assert!(
        warned < failed,
        "the warning must explain the parse error, not follow it:\n{printed}"
    );
    assert!(
        printed.contains(&format!(
            "dodot: {}",
            warning_about_the_version(&result).expect("the warning is returned as well")
        )),
        "what is printed is the same warning the result carries:\n{printed}"
    );
    assert!(
        result.failed,
        "the bundle failed on its own terms; the warning does not change that"
    );
}

#[test]
fn a_brew_at_or_above_the_floor_produces_no_extra_output() {
    for version in ["5.1.2", "6.0.18"] {
        let env = env_with_a_brewfile();
        install_fake_brew(&env);
        let runner = BrewSaying::new(version);

        let result = commands::up::up(None, &ctx_with(&env, runner.clone())).unwrap();

        assert_eq!(
            warning_about_the_version(&result),
            None,
            "brew {version} clears the floor and is not news: {:?}",
            result.warnings
        );
        assert!(runner.ran_the_brewfile());
    }
}

#[test]
fn a_brew_that_will_not_say_is_a_probe_failure_not_an_absent_manager() {
    /// Fails `--version` the way a broken install does, and succeeds
    /// at everything else.
    struct BrokenVersion;
    impl CommandRunner for BrokenVersion {
        fn run(&self, command: CommandSpec<'_>) -> Result<CommandOutput> {
            if command.arguments == ["--version"] {
                return Ok(CommandOutput {
                    exit_code: 127,
                    stdout: String::new(),
                    stderr: "dyld: library not loaded\n".to_string(),
                });
            }
            Ok(CommandOutput {
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    let env = env_with_a_brewfile();
    install_fake_brew(&env);

    let result = commands::up::up(None, &ctx_with(&env, Arc::new(BrokenVersion))).unwrap();

    let warning = warning_about_the_version(&result)
        .unwrap_or_else(|| panic!("no version warning in {:?}", result.warnings));
    assert!(warning.contains("dyld"), "brew's own words: {warning}");
    assert!(
        !warning.contains("not installed") && !warning.contains("probed"),
        "the presence probe already answered; this is a different question: {warning}"
    );

    // And the row is still the ordinary one — a brew that would not
    // answer a version question is not a brew that is missing.
    let row = result.packs[0]
        .files
        .iter()
        .find(|f| f.name == "Brewfile")
        .expect("the Brewfile has a row");
    assert_ne!(row.status_label, "homebrew not installed");
}

// ── The probe's cost ────────────────────────────────────────────

#[test]
fn one_spawn_per_manager_however_many_packs_declare_a_brewfile() {
    let env = TempEnvironment::builder()
        .pack("dev")
        .file("Brewfile", "npm \"typescript\"\n")
        .done()
        .pack("ops")
        .file("Brewfile", "brew \"ripgrep\"\n")
        .done()
        .build();
    install_fake_brew(&env);
    let runner = BrewSaying::new("5.0.16");

    let result = commands::up::up(None, &ctx_with(&env, runner.clone())).unwrap();

    assert_eq!(
        runner.version_probes().len(),
        1,
        "the answer is a fact about the machine, not about the pack"
    );
    assert_eq!(
        result
            .warnings
            .iter()
            .filter(|w| w.contains("5.1.2"))
            .count(),
        1,
        "and it is said once: {:?}",
        result.warnings
    );
}

#[test]
fn a_brewfile_that_is_not_going_to_run_costs_no_version_probe() {
    // The second `up` of the day finds the receipt current and spawns
    // nothing. Asking brew its version anyway would buy a subprocess
    // to warn about a run that is not happening.
    let env = env_with_a_brewfile();
    install_fake_brew(&env);

    commands::up::up(None, &ctx_with(&env, BrewSaying::new("6.0.18"))).unwrap();

    let second = BrewSaying::new("6.0.18");
    commands::up::up(None, &ctx_with(&env, second.clone())).unwrap();

    assert!(
        !second.ran_the_brewfile(),
        "precondition: the receipt from the first run holds"
    );
    assert!(
        second.version_probes().is_empty(),
        "nothing was going to be spawned, so there was nothing to ask about"
    );
}

#[test]
fn provision_rerun_asks_again_because_the_file_runs_again() {
    let env = env_with_a_brewfile();
    install_fake_brew(&env);
    commands::up::up(None, &ctx_with(&env, BrewSaying::new("5.0.16"))).unwrap();

    let rerun_runner = BrewSaying::new("5.0.16");
    let mut rerun = ctx_with(&env, rerun_runner.clone());
    rerun.provision_rerun = true;
    let result = commands::up::up(None, &rerun).unwrap();

    assert!(
        rerun_runner.ran_the_brewfile(),
        "precondition: --provision-rerun runs the file despite the receipt"
    );
    assert_eq!(
        rerun_runner.version_probes().len(),
        1,
        "a file that is going to run is a file worth warning about"
    );
    assert!(warning_about_the_version(&result).is_some());
}

#[test]
fn the_version_probe_carries_the_same_environment_the_run_will() {
    // Without `HOMEBREW_NO_AUTO_UPDATE`, asking brew its version is
    // itself a brew command — and brew would update first, so the
    // answer would describe a brew that did not exist a moment ago
    // and the floor check would pass by accident.
    let env = env_with_a_brewfile();
    install_fake_brew(&env);
    let runner = BrewSaying::new("5.0.16");

    commands::up::up(None, &ctx_with(&env, runner.clone())).unwrap();

    let probes = runner.version_probes();
    assert_eq!(probes.len(), 1);
    assert_eq!(PathBuf::from(&probes[0].executable), brew_path(&env));
    assert_eq!(
        probes[0].environment.as_slice(),
        [("HOMEBREW_NO_AUTO_UPDATE".to_string(), "1".to_string())]
    );
}

// ── The boundary: only `up` asks ────────────────────────────────

#[test]
fn status_and_dry_run_never_ask_a_manager_for_its_version() {
    let env = env_with_a_brewfile();
    install_fake_brew(&env);

    let ctx = ctx_with(&env, Arc::new(NeverAsksTheVersion));
    commands::status::status(None, &ctx).unwrap();

    let mut dry = ctx_with(&env, Arc::new(NeverAsksTheVersion));
    dry.dry_run = true;
    commands::up::up(None, &dry).unwrap();

    // And still not after a real run has populated the datastore.
    commands::up::up(None, &ctx_with(&env, BrewSaying::new("6.0.18"))).unwrap();
    commands::status::status(None, &ctx_with(&env, Arc::new(NeverAsksTheVersion))).unwrap();
}

// ── The answer is never persisted ───────────────────────────────

#[test]
fn the_fitness_verdict_leaves_nothing_behind() {
    // A receipt asserts that this exact file content ran. The
    // manager's version at that moment is not part of that claim, so
    // a below-floor run and an above-floor run must leave datastores
    // that are identical entry for entry.
    fn entries(env: &TempEnvironment) -> Vec<String> {
        fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<String>) {
            let Ok(read) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in read {
                let path = entry.unwrap().path();
                out.push(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                );
                if path.is_dir() && !path.is_symlink() {
                    walk(&path, root, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(env.paths.data_dir(), env.paths.data_dir(), &mut out);
        out.sort();
        out
    }

    let below = env_with_a_brewfile();
    install_fake_brew(&below);
    commands::up::up(None, &ctx_with(&below, BrewSaying::new("5.0.16"))).unwrap();

    let above = env_with_a_brewfile();
    install_fake_brew(&above);
    commands::up::up(None, &ctx_with(&above, BrewSaying::new("6.0.18"))).unwrap();

    assert_eq!(
        entries(&below),
        entries(&above),
        "the fitness answer is a fact about this machine right now — \
         nothing about it belongs in the datastore"
    );
    assert!(
        !entries(&below).is_empty(),
        "precondition: the Brewfile ran and left its receipt"
    );
}

//! Safety Lock through the whole CLI: clap, the pre-dispatch hook, the
//! handler, and rendering.
//!
//! The level between the library's unit tests — which prove the decisions —
//! and the Bats/PTY suite, which proves the process behaviour a harness
//! cannot reach (a real terminal, a real signal, a real exit status). What
//! belongs here is the wiring: that the taxonomy sends `up` to the gate and
//! `up --dry-run` past it, that a refusal leaves stdout empty and the trust
//! file untouched, and that `roots list` says the same thing in every output
//! mode.
//!
//! Every test is `#[serial]`: [`TestHarness`] installs the environment, the
//! working directory, and Standout's detectors as process-global state, and
//! restores them when the run's result drops. [`Sandbox::run`] drops that
//! result before returning, so no two runs' restores can interleave.
//!
//! One thing the harness deliberately cannot fake is a terminal. The gate
//! asks `std::io::stdin().is_terminal()` about the real process, so
//! in-process every prompt attempt takes the non-interactive path — which is
//! exactly the behaviour these tests assert. Affirmative interaction lives in
//! `tests/e2e/bats/test_safety_lock.bats`, on a real pseudo-terminal, because
//! piping `yes` into a program is not a test that it required a terminal.

use std::path::{Path, PathBuf};

use standout::cli::ExitStatus;
use standout::OutputMode;
use standout_test::{serial, TestHarness};

use dodot_lib::safety_lock::{
    RootIdentity, SafetyLockConfig, TrustFileTransaction, TrustedRootsSection,
};

/// A dotfiles root, a home to deploy into, and a data directory — all under
/// one tempdir, so no test can touch the developer's real deployment.
///
/// Owns the tempdir rather than borrowing the harness's, because a sandbox
/// outlives any one run: approving a root and then invoking against it are
/// two runs that have to agree on the path.
struct Sandbox {
    temp: tempfile::TempDir,
    explicit_root: Option<PathBuf>,
}

impl Sandbox {
    /// Two packs — one symlinked file, one shell file — so the orientation
    /// inventory has something to count in more than one category.
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let sandbox = Self {
            temp,
            explicit_root: None,
        };
        sandbox.write("dots/vim/vimrc", "set number\n");
        sandbox.write("dots/shell/aliases.sh", "alias ll='ls -l'\n");
        std::fs::create_dir_all(sandbox.home()).unwrap();
        sandbox
    }

    fn write(&self, relative: &str, content: &str) {
        let path = self.temp.path().join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn home(&self) -> PathBuf {
        self.temp.path().join("home")
    }

    fn data_dir(&self) -> PathBuf {
        self.home().join(".local").join("share").join("dodot")
    }

    /// The dotfiles root, canonicalized the way Safety Lock canonicalizes it
    /// — on macOS a tempdir path goes through `/private`, and the spelling
    /// the gate reports is the resolved one.
    fn root(&self) -> PathBuf {
        std::fs::canonicalize(self.temp.path().join("dots")).unwrap()
    }

    /// Select a root explicitly, the way automation is meant to.
    fn with_explicit_root(mut self, value: impl AsRef<Path>) -> Self {
        self.explicit_root = Some(value.as_ref().to_path_buf());
        self
    }

    /// Pre-approve `path`, standing in for a user who already answered the
    /// prompt on an earlier run.
    fn approve(&self, path: &Path) {
        TrustFileTransaction::begin(&self.data_dir())
            .unwrap()
            .persist(&SafetyLockConfig {
                roots: TrustedRootsSection {
                    approved: vec![RootIdentity::new(path).unwrap()],
                },
            })
            .unwrap();
    }

    /// Write the trust file verbatim — for states a valid write cannot
    /// produce.
    fn write_raw_trust_file(&self, content: &str) {
        std::fs::create_dir_all(self.data_dir()).unwrap();
        std::fs::write(SafetyLockConfig::path_in(&self.data_dir()), content).unwrap();
    }

    /// The approved roots as the next invocation would load them.
    fn approved(&self) -> Vec<String> {
        SafetyLockConfig::load_from(&self.data_dir())
            .expect("the trust file must stay loadable")
            .roots
            .approved
            .iter()
            .map(RootIdentity::spelling)
            .collect()
    }

    fn trust_file_exists(&self) -> bool {
        SafetyLockConfig::path_in(&self.data_dir()).exists()
    }

    fn run(&self, args: &[&str]) -> Outcome {
        self.run_as(OutputMode::Text, args)
    }

    fn run_as(&self, mode: OutputMode, args: &[&str]) -> Outcome {
        let home = self.home();
        let mut harness = TestHarness::new()
            .env("HOME", home.display().to_string())
            .env(
                "XDG_DATA_HOME",
                home.join(".local").join("share").display().to_string(),
            )
            .env(
                "XDG_CONFIG_HOME",
                home.join(".config").display().to_string(),
            )
            .env("XDG_CACHE_HOME", home.join(".cache").display().to_string())
            .no_color()
            .terminal_width(100)
            .output_mode(mode)
            .cwd(self.temp.path().join("dots"));

        harness = match &self.explicit_root {
            Some(value) => harness.env("DOTFILES_ROOT", value.display().to_string()),
            None => harness.env_remove("DOTFILES_ROOT"),
        };

        let mut argv = vec!["dodot"];
        argv.extend_from_slice(args);
        let result = harness.run(&crate::build_app(), crate::build_clap_command(), argv);

        // Copy everything out and drop the result here, so its restore of the
        // process-global overrides happens before the caller's next run.
        Outcome {
            stdout: result.stdout().to_string(),
            error: result.error().map(str::to_string),
            exit_status: result.exit_status(),
        }
    }
}

/// One run's observable result, detached from the harness that produced it.
struct Outcome {
    stdout: String,
    error: Option<String>,
    exit_status: Option<ExitStatus>,
}

impl Outcome {
    fn assert_succeeded(&self) {
        assert!(
            self.error.is_none(),
            "expected success, got: {}",
            self.error.as_deref().unwrap_or_default()
        );
    }

    /// Every refusal looks the same from outside: status 1, and nothing on
    /// stdout for a consumer to mistake for a result.
    fn assert_refused(&self) -> &str {
        assert_eq!(self.exit_status, Some(ExitStatus::FAILURE));
        assert_eq!(self.stdout, "", "a refusal wrote to stdout");
        self.error
            .as_deref()
            .expect("a refusal carries a diagnostic")
    }

    fn assert_stdout_contains(&self, needle: &str) {
        assert!(
            self.stdout.contains(needle),
            "`{needle}` missing from stdout:\n{}",
            self.stdout
        );
    }
}

// ── The gate ────────────────────────────────────────────────────

/// The headline: a mutating command on an implicitly discovered, unapproved
/// root does not run, does not approve, and does not deploy.
#[test]
#[serial]
fn a_mutating_command_on_an_unapproved_implicit_root_refuses() {
    let sandbox = Sandbox::new();

    sandbox.run(&["up"]).assert_refused();

    assert!(
        !sandbox.trust_file_exists(),
        "a refusal recorded an approval"
    );
    assert!(
        !sandbox.home().join(".vimrc").exists(),
        "a refused command deployed anyway"
    );
}

/// The refusal has to say which root and what to do instead — it is the only
/// thing a refused user has to work from.
#[test]
#[serial]
fn the_refusal_names_the_root_and_the_deliberate_alternative() {
    let sandbox = Sandbox::new();
    let outcome = sandbox.run(&["up"]);
    let diagnostic = outcome.assert_refused();

    assert!(
        diagnostic.contains(&sandbox.root().display().to_string()),
        "the refusal did not name the root: {diagnostic}"
    );
    assert!(
        diagnostic.contains("DOTFILES_ROOT"),
        "the refusal did not point at the deliberate path: {diagnostic}"
    );
    assert!(
        !diagnostic.starts_with("Hook error:"),
        "the framework's prefix leaked into a user-facing refusal: {diagnostic}"
    );
}

/// A preview is what makes an otherwise root-sensitive invocation safe, so it
/// crosses no gate — and establishes no trust either.
#[test]
#[serial]
fn a_dry_run_neither_prompts_nor_establishes_trust() {
    let sandbox = Sandbox::new();

    sandbox.run(&["up", "--dry-run"]).assert_succeeded();

    assert!(
        !sandbox.trust_file_exists(),
        "a dry run recorded an approval"
    );
}

/// Read-only commands are how a user finds out what Dodot sees before
/// deciding to authorize it, so they must work on an untrusted root.
#[test]
#[serial]
fn read_only_commands_work_on_an_untrusted_root() {
    let sandbox = Sandbox::new();

    sandbox.run(&["status"]).assert_succeeded();

    assert!(
        !sandbox.trust_file_exists(),
        "a read-only command recorded an approval"
    );
}

/// The gate reads the trust file only for the operation whose answer depends
/// on it, so a broken one cannot take down the diagnostics a user needs most
/// when something is wrong.
#[test]
#[serial]
fn an_unusable_trust_file_does_not_break_read_only_commands() {
    let sandbox = Sandbox::new();
    sandbox.write_raw_trust_file("[roots]\napproved = [\"relative/dotfiles\"]\n");

    sandbox.run(&["status"]).assert_succeeded();
}

/// The same broken file must stop a mutation, naming the file rather than
/// reading as "nothing approved" and asking the user to approve over state
/// Dodot never understood.
#[test]
#[serial]
fn an_unusable_trust_file_stops_a_mutation_and_names_itself() {
    let sandbox = Sandbox::new();
    sandbox.write_raw_trust_file("[roots]\napproved = [\"relative/dotfiles\"]\n");

    let outcome = sandbox.run(&["up"]);
    let diagnostic = outcome.assert_refused();

    assert!(
        diagnostic.contains("safety-lock.toml"),
        "the diagnostic did not name the trust file: {diagnostic}"
    );
}

/// `DOTFILES_ROOT` is the deliberate, noninteractive selection: it needs no
/// stored approval, and records none.
#[test]
#[serial]
fn a_valid_explicit_root_runs_without_approval_and_records_none() {
    let sandbox = Sandbox::new();
    let root = sandbox.root();
    let sandbox = sandbox.with_explicit_root(&root);

    sandbox.run(&["up"]).assert_succeeded();

    assert!(
        !sandbox.trust_file_exists(),
        "an explicit root was written into the approved collection"
    );
}

/// A present but unusable explicit value fails where it is read. Falling
/// through to Git or the current directory would turn a configuration error
/// into a different — and unapproved — root.
#[test]
#[serial]
fn an_invalid_explicit_root_fails_without_falling_back() {
    let sandbox = Sandbox::new();
    let missing = sandbox.root().join("no-such-directory");
    let sandbox = sandbox.with_explicit_root(&missing);

    let outcome = sandbox.run(&["status"]);

    let diagnostic = outcome
        .error
        .as_deref()
        .expect("an unusable explicit value must fail");
    assert!(
        diagnostic.contains(&missing.display().to_string()),
        "the failure did not name the invalid value: {diagnostic}"
    );
}

/// Approval does not expire and is not re-asked: a later run of the same
/// command against the same canonical path just proceeds.
#[test]
#[serial]
fn a_previously_approved_root_proceeds_without_being_asked_again() {
    let sandbox = Sandbox::new();
    sandbox.approve(&sandbox.root());

    sandbox.run(&["up"]).assert_succeeded();

    assert_eq!(
        sandbox.approved(),
        vec![sandbox.root().display().to_string()],
        "the approval was rewritten or lost"
    );
}

/// Trust is bound to the canonical path, so approving one root says nothing
/// about another — including a directory inside it.
#[test]
#[serial]
fn an_approval_does_not_extend_to_a_neighbouring_root() {
    let sandbox = Sandbox::new();
    sandbox.approve(&sandbox.root().join("vim"));

    sandbox.run(&["up"]).assert_refused();
}

// ── The whole protected set ─────────────────────────────────────

/// Every dispatched family in ADR-0002's protected set crosses the same
/// gate: deployment (`down`), repository writes (`init`, `fill`, `adopt`,
/// `addignore`), the repository-local installers, and the cache-derived
/// writers (`refresh`, `transform check`). One invocation per family — the
/// wiring is shared, so what this proves is that each family declared
/// itself mutating rather than inheriting "not gated" from a helper.
#[test]
#[serial]
fn every_mutating_dispatch_family_refuses_on_an_untrusted_implicit_root() {
    let sandbox = Sandbox::new();

    let families: &[&[&str]] = &[
        &["down"],
        &["init", "newpack"],
        &["fill", "vim"],
        &["adopt", "--into", "vim", "some-file"],
        &["addignore", "vim"],
        &["git-install-filters"],
        &["template", "install-filter"],
        &["transform", "install-hook"],
        &["refresh"],
        &["transform", "check"],
    ];

    for family in families {
        let outcome = sandbox.run(family);
        let diagnostic = outcome.assert_refused();
        assert!(
            diagnostic.contains("has not been approved"),
            "`{}` failed for a reason other than the gate: {diagnostic}",
            family.join(" ")
        );
    }

    assert!(
        !sandbox.trust_file_exists(),
        "a refused family recorded an approval"
    );
}

/// The documented preview of each previewable family stays available
/// without trust — a preview is how a user decides whether to approve.
#[test]
#[serial]
fn every_documented_preview_bypasses_the_gate_without_establishing_trust() {
    let sandbox = Sandbox::new();

    let previews: &[&[&str]] = &[
        &["down", "--dry-run"],
        &["refresh", "--list-paths"],
        &["transform", "check", "--dry-run"],
    ];

    for preview in previews {
        let outcome = sandbox.run(preview);
        assert!(
            outcome.error.is_none(),
            "`{}` was gated despite being a preview: {}",
            preview.join(" "),
            outcome.error.as_deref().unwrap_or_default()
        );
    }

    assert!(
        !sandbox.trust_file_exists(),
        "a preview recorded an approval"
    );
}

/// Mutations the root did not select stay outside Safety Lock (ADR-0002):
/// the dismissed-prompt registry lives in Dodot's own data directory, so
/// managing it needs no trusted root.
#[test]
#[serial]
fn a_root_independent_mutation_runs_on_an_untrusted_root() {
    let sandbox = Sandbox::new();

    sandbox
        .run(&["prompts", "reset", "--all"])
        .assert_succeeded();

    assert!(
        !sandbox.trust_file_exists(),
        "a root-independent mutation recorded an approval"
    );
}

// ── Root management ─────────────────────────────────────────────

/// The management surface has to be usable in every mode a consumer might
/// ask for, and the structured ones must stay parseable.
#[test]
#[serial]
fn roots_list_renders_in_every_output_mode() {
    let sandbox = Sandbox::new();
    sandbox.approve(&sandbox.root());
    let spelling = sandbox.root().display().to_string();

    let json = sandbox.run_as(OutputMode::Json, &["roots", "list"]);
    json.assert_succeeded();
    let parsed: serde_json::Value =
        serde_json::from_str(&json.stdout).expect("JSON mode must produce a parseable document");
    assert_eq!(parsed["roots"][0]["path"], spelling);
    assert_eq!(parsed["roots"][0]["exists"], true);

    let yaml = sandbox.run_as(OutputMode::Yaml, &["roots", "list"]);
    yaml.assert_succeeded();
    yaml.assert_stdout_contains(&spelling);

    let text = sandbox.run_as(OutputMode::Text, &["roots", "list"]);
    text.assert_succeeded();
    text.assert_stdout_contains(&spelling);

    // Terminal-debug renders style names as tags, which is how the semantic
    // styling gets inspected without a terminal. An undefined style would
    // come back as `[name?]`.
    let debug = sandbox.run_as(OutputMode::TermDebug, &["roots", "list"]);
    debug.assert_succeeded();
    debug.assert_stdout_contains("[header]");
    assert!(
        !debug.stdout.contains("?]"),
        "roots-list used a style the dodot theme does not define:\n{}",
        debug.stdout
    );
}

/// A user with no approvals reads "nothing approved", not an error and not a
/// bare empty list.
#[test]
#[serial]
fn roots_list_reports_an_absent_trust_file_as_no_approvals() {
    let sandbox = Sandbox::new();

    let outcome = sandbox.run(&["roots", "list"]);

    outcome.assert_succeeded();
    outcome.assert_stdout_contains("No root has been approved");
}

/// Managing the trust collection must not depend on resolving a dotfiles
/// root, or a broken `DOTFILES_ROOT` would lock a user out of the surface
/// that exists to inspect and repair approvals.
#[test]
#[serial]
fn roots_list_works_even_when_the_explicit_root_is_unusable() {
    let sandbox = Sandbox::new();
    let root = sandbox.root();
    sandbox.approve(&root);
    let sandbox = sandbox.with_explicit_root(Path::new("/no/such/root"));

    let outcome = sandbox.run(&["roots", "list"]);

    outcome.assert_succeeded();
    outcome.assert_stdout_contains(&root.display().to_string());
}

/// A trust file `roots list` refuses is exactly the one `roots forget` has to
/// be able to repair.
#[test]
#[serial]
fn roots_list_surfaces_an_unusable_trust_file() {
    let sandbox = Sandbox::new();
    sandbox.write_raw_trust_file("[roots]\napproved = [\"relative/dotfiles\"]\n");

    let outcome = sandbox.run(&["roots", "list"]);

    assert!(
        outcome.error.is_some(),
        "a broken trust file listed as if it were readable:\n{}",
        outcome.stdout
    );
}

/// Revocation puts the root back where the gate found it the first time.
#[test]
#[serial]
fn roots_forget_removes_the_approval() {
    let sandbox = Sandbox::new();
    sandbox.approve(&sandbox.root());

    sandbox
        .run(&["roots", "forget", &sandbox.root().display().to_string()])
        .assert_succeeded();

    assert!(
        sandbox.approved().is_empty(),
        "the approval survived revocation"
    );
}

/// "No such approval" is an answer, not a failure — a user has to be able to
/// tell it apart from a trust file Dodot could not read.
#[test]
#[serial]
fn roots_forget_reports_an_argument_that_matches_nothing() {
    let sandbox = Sandbox::new();

    let outcome = sandbox.run(&["roots", "forget", "/nowhere/at/all"]);

    outcome.assert_succeeded();
    outcome.assert_stdout_contains("No approved root matches");
}

/// After revocation the gate asks again, which is the whole point of the
/// command.
#[test]
#[serial]
fn a_forgotten_root_is_gated_again() {
    let sandbox = Sandbox::new();
    sandbox.approve(&sandbox.root());
    sandbox.run(&["up"]).assert_succeeded();

    sandbox
        .run(&["roots", "forget", &sandbox.root().display().to_string()])
        .assert_succeeded();

    sandbox.run(&["up"]).assert_refused();
}

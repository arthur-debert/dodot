//! Safety Lock's process boundary: capture, resolve, gate, inject.
//!
//! [`dodot_lib::safety_lock`] is CLI-free by construction — it reads no
//! environment variable, no current directory, and no Git. Somebody has to,
//! and this is that somebody. One Standout pre-dispatch hook runs before every
//! dispatched command: it captures the process facts once
//! ([`ProcessFacts::capture`]), resolves the invocation's one dotfiles root,
//! puts the command's declared [`RootOperation`] to the gate, and leaves the
//! result in the command context's extensions as an immutable [`SafetyState`].
//!
//! Handlers read that value. They do not call `std::env`, do not shell out to
//! `git`, and do not canonicalize a root of their own — which is what makes
//! "the root the user approved is the root Dodot mutates" a property of the
//! wiring rather than a rule every handler has to remember
//! (`docs/adr/0001-trust-dotfiles-roots-by-canonical-path.md`).
//!
//! # The taxonomy is declared, not inferred
//!
//! [`COMMAND_SENSITIVITY`] is the one place a command says what it does to the
//! root. Naming a context builder "read-only" is not evidence — several
//! commands mutate pack source through exactly that helper — so the
//! declaration lives here as a list that can be reviewed, and a command
//! missing from it gets no root at all rather than silently escaping the gate
//! (`docs/adr/0002-guard-root-derived-mutations.md`).
//!
//! # Refusal
//!
//! Every refusal — declining, an unrecognized answer, EOF, a non-terminal
//! channel, unusable configuration, an approval that could not be persisted —
//! goes out the same door: [`refuse`] returns a Standout [`ExternalFailure`],
//! which is written verbatim to stderr and exits 1 with stdout untouched. The
//! interruption case needs no code at all: Dodot installs no `SIGINT`
//! handler, so Ctrl-C at the prompt kills the process the conventional way
//! and the shell reports 130, with no trust and no deployment written.

use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use standout::cli::{get_deepest_matches, CommandContext, ExternalFailure, HookError, Hooks};
use standout::OutputMode;

use dodot_lib::commands::safety::SafetyPromptView;
use dodot_lib::fs::OsFs;
use dodot_lib::gates::HostFacts;
use dodot_lib::paths::{Pather, XdgPather};
use dodot_lib::render;
use dodot_lib::safety_lock::{
    approve, authorize, resolve_root, GateOutcome, OsPathProbe, ResolvedRoot, RootIdentity,
    RootOperation, RootSelectionInput, SafetyLockConfig, SafetyLockError, TrustFileTransaction,
};

/// How a command relates to the resolved dotfiles root.
///
/// A thin layer over [`RootOperation`], and only because one distinction
/// depends on the invocation rather than the command: `up` is a mutation and
/// `up --dry-run` is a preview, and the two must not be declared separately or
/// they will drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootSensitivity {
    /// A mutation whose targets the root selects. Gated — unless the
    /// invocation asked for a preview, which the named flag reports.
    Mutating {
        /// The flag that turns this command into a preview, or `None` when it
        /// has none.
        preview_flag: Option<&'static str>,
    },
    /// Reads the root, writes nothing derived from it. Stays available on an
    /// unapproved root so a user can find out what Dodot sees before deciding
    /// to authorize it.
    ReadOnly,
    /// Mutates something the root did not select. Keeps its own safeguards;
    /// root trust has nothing to say about it.
    RootIndependent,
    /// Takes no dotfiles root at all.
    ///
    /// `roots list` and `roots forget` manage the trust collection itself, so
    /// resolving a root for them would let a broken `DOTFILES_ROOT` lock the
    /// user out of the surface that exists to inspect and repair approvals.
    Rootless,
}

impl RootSensitivity {
    /// The gate's view of this invocation.
    fn operation(self, matches: &clap::ArgMatches) -> RootOperation {
        match self {
            RootSensitivity::Mutating { preview_flag } => {
                let previewing = preview_flag.is_some_and(|flag| {
                    matches
                        .try_get_one::<bool>(flag)
                        .ok()
                        .flatten()
                        .copied()
                        .unwrap_or(false)
                });
                if previewing {
                    RootOperation::DryRun
                } else {
                    RootOperation::RootSensitiveMutation
                }
            }
            RootSensitivity::ReadOnly => RootOperation::ReadOnly,
            RootSensitivity::RootIndependent | RootSensitivity::Rootless => {
                RootOperation::RootIndependentMutation
            }
        }
    }
}

const MUTATES: RootSensitivity = RootSensitivity::Mutating { preview_flag: None };
const MUTATES_OR_PREVIEWS: RootSensitivity = RootSensitivity::Mutating {
    preview_flag: Some("dry-run"),
};

/// What every dispatched command does to the resolved dotfiles root.
///
/// The reviewable list ADR-0002 asks for. Adding a command means adding a row
/// here: without one the hook never runs, no [`SafetyState`] reaches the
/// handler, and the handler's first attempt to build a context fails — a new
/// mutating command cannot quietly inherit "not gated" from whichever helper
/// it happened to call.
///
/// Keys are Standout's dotted command paths, which is what
/// [`standout::cli::App::hooks`] matches on.
pub const COMMAND_SENSITIVITY: &[(&str, RootSensitivity)] = &[
    // Deployment and repository writes: the root chose every target.
    ("up", MUTATES_OR_PREVIEWS),
    ("down", MUTATES_OR_PREVIEWS),
    ("adopt", MUTATES_OR_PREVIEWS),
    ("init", MUTATES),
    ("fill", MUTATES),
    ("addignore", MUTATES),
    ("git-install-filters", MUTATES),
    ("template.install-filter", MUTATES),
    ("transform.install-hook", MUTATES),
    // Cache-derived writes, scoped back to the authorized root by
    // `scope_to_root` rather than by a per-root cache (ADR-0002).
    // `refresh --list-paths` reports the touches it would make.
    (
        "refresh",
        RootSensitivity::Mutating {
            preview_flag: Some("list-paths"),
        },
    ),
    ("transform.check", MUTATES_OR_PREVIEWS),
    // Inspection: available on any root, trusted or not.
    ("status", RootSensitivity::ReadOnly),
    ("list", RootSensitivity::ReadOnly),
    ("probe", RootSensitivity::ReadOnly),
    ("probe.deployment-map", RootSensitivity::ReadOnly),
    ("probe.show-data-dir", RootSensitivity::ReadOnly),
    ("probe.shell-init", RootSensitivity::ReadOnly),
    ("probe.app", RootSensitivity::ReadOnly),
    ("git-show-filters", RootSensitivity::ReadOnly),
    ("git-show-alias", RootSensitivity::ReadOnly),
    ("transform.status", RootSensitivity::ReadOnly),
    ("secret.probe", RootSensitivity::ReadOnly),
    ("secret.list", RootSensitivity::ReadOnly),
    ("prompts.list", RootSensitivity::ReadOnly),
    // Mutations the root did not select: the user's shell rc, Dodot's own
    // data directory, the dismissed-prompt registry.
    ("install", RootSensitivity::RootIndependent),
    ("git-install-alias", RootSensitivity::RootIndependent),
    ("prompts.reset", RootSensitivity::RootIndependent),
    ("reset", RootSensitivity::RootIndependent),
    // The trust collection's own management surface.
    ("roots.list", RootSensitivity::Rootless),
    ("roots.forget", RootSensitivity::Rootless),
];

/// Everything about the invoking process Safety Lock depends on, read once.
///
/// Captured at the boundary and then carried, so nothing downstream can
/// consult the environment, Git, or the current directory a second time and
/// pick a different root than the one that was authorized.
#[derive(Debug, Clone)]
pub struct ProcessFacts {
    selection: RootSelectionInput,
    data_dir: PathBuf,
}

impl ProcessFacts {
    /// Read the process facts.
    ///
    /// `DOTFILES_ROOT` is taken as [`OsString`], never a lossy `String`: a
    /// non-Unicode explicit value has to keep its native form all the way
    /// through resolution and into any diagnostic that names it.
    ///
    /// The Git top-level comes from `git rev-parse --show-toplevel`. A failed
    /// or empty answer is `None` — "not inside a repository" — which is a
    /// selection input, not an error: the current directory is the next
    /// candidate.
    pub fn capture() -> Result<Self, anyhow::Error> {
        let pather = XdgPather::from_env()?;
        let current_dir = std::env::current_dir()?;

        let mut selection = RootSelectionInput::new(current_dir, pather.home_dir());
        selection.dotfiles_root = std::env::var_os("DOTFILES_ROOT");
        selection.git_top_level = git_top_level();

        Ok(Self {
            selection,
            data_dir: pather.data_dir().to_path_buf(),
        })
    }

    /// The directory Dodot was invoked from.
    pub fn current_dir(&self) -> &Path {
        &self.selection.current_dir
    }

    /// Dodot's data directory — where the trust file lives.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Resolve this invocation's dotfiles root: `DOTFILES_ROOT`, then the Git
    /// top-level, then the current directory.
    pub fn resolve_root(&self) -> Result<ResolvedRoot, anyhow::Error> {
        Ok(resolve_root(&self.selection, &OsPathProbe)?)
    }
}

/// Ask Git for the top-level of the repository enclosing the current
/// directory.
fn git_top_level() -> Option<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let toplevel = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!toplevel.is_empty()).then(|| PathBuf::from(toplevel))
}

/// The authorized root and the facts it was resolved from, as handlers see it.
///
/// Immutable by construction: there is no way to replace the root once the
/// hook has put this in the command context, so the value that crossed the
/// gate is the value the command runs against.
#[derive(Debug, Clone)]
pub struct SafetyState {
    facts: ProcessFacts,
    root: Option<ResolvedRoot>,
}

impl SafetyState {
    /// The captured process facts.
    pub fn facts(&self) -> &ProcessFacts {
        &self.facts
    }

    /// The one dotfiles root this invocation may work under.
    ///
    /// Errors only for a command that declared itself [`Rootless`] and then
    /// asked for a root anyway — a wiring mistake, not a user-facing
    /// condition.
    ///
    /// [`Rootless`]: RootSensitivity::Rootless
    pub fn root(&self) -> Result<&ResolvedRoot, anyhow::Error> {
        self.root
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("this command does not resolve a dotfiles root"))
    }

    /// The authorized root's canonical identity — what `refresh` and
    /// `transform check` scope their cache-derived write set to.
    pub fn root_identity(&self) -> Result<RootIdentity, anyhow::Error> {
        Ok(self.root()?.identity().clone())
    }
}

/// Read the [`SafetyState`] the hook injected.
///
/// A handler reaching this without a hook having run is a registration bug:
/// the command is missing from [`COMMAND_SENSITIVITY`], which is exactly the
/// case that must fail loudly rather than fall back to rediscovering a root.
pub fn state(ctx: &CommandContext) -> Result<&SafetyState, anyhow::Error> {
    ctx.extensions.get::<SafetyState>().ok_or_else(|| {
        anyhow::anyhow!(
            "`{}` is not declared in dodot's root-sensitivity taxonomy, so no \
             dotfiles root was resolved for it",
            ctx.command_path.join(" ")
        )
    })
}

/// The pre-dispatch hook for a command with the given sensitivity.
///
/// One implementation, registered once per command path because Standout
/// looks hooks up by path and has no all-commands seam.
pub fn hook(sensitivity: RootSensitivity) -> Hooks {
    Hooks::new().pre_dispatch(move |matches, ctx| {
        let state = gate(sensitivity, get_deepest_matches(matches))?;
        ctx.extensions.insert(state);
        Ok(())
    })
}

/// Capture, resolve, and authorize — the whole boundary, in order.
fn gate(
    sensitivity: RootSensitivity,
    matches: &clap::ArgMatches,
) -> Result<SafetyState, HookError> {
    let facts = ProcessFacts::capture().map_err(refuse)?;

    if sensitivity == RootSensitivity::Rootless {
        return Ok(SafetyState { facts, root: None });
    }

    let root = facts.resolve_root().map_err(refuse)?;
    let operation = sensitivity.operation(matches);
    tracing::debug!(
        root = %root.identity().spelling(),
        selected_by = %root.source(),
        operation = operation.label(),
        "safety lock: resolved root",
    );

    if operation.requires_trusted_root() {
        // Loaded only for the one kind of operation whose answer depends on
        // it, so an unusable trust file cannot take `status` down with it.
        let config = SafetyLockConfig::load_from(facts.data_dir()).map_err(refuse)?;
        let outcome =
            authorize(operation, &root, &config, &OsFs, &HostFacts::detect()).map_err(refuse)?;

        if let GateOutcome::ConfirmationRequired { inventory } = outcome {
            confirm(&root, &inventory, facts.data_dir())?;
        }
    }

    Ok(SafetyState {
        facts,
        root: Some(root),
    })
}

/// Show the root and its orientation inventory, ask, and record the answer.
///
/// Both channels must be terminal-backed. A terminal stdin whose stderr was
/// redirected is refused rather than prompted: approval must never be accepted
/// for a root and an inventory the user was never shown.
///
/// Approval is written **before** this returns, because it records the user's
/// decision about a path, not the outcome of the mutation that follows — a
/// mutation that partly fails must not un-approve the root, and a mutation
/// that never started because the write failed must not run un-recorded.
fn confirm(
    root: &ResolvedRoot,
    inventory: &dodot_lib::safety_lock::RootInventory,
    data_dir: &Path,
) -> Result<(), HookError> {
    let mut stderr = std::io::stderr().lock();
    if !std::io::stdin().is_terminal() || !stderr.is_terminal() {
        return Err(refusal(format!(
            "dodot has not been approved for {}.\n  \
             Approving needs a terminal on both sides — dodot asks on stderr and reads \
             the answer from stdin.\n  \
             For automation, set DOTFILES_ROOT to the root you mean.",
            root.identity().spelling()
        )));
    }

    let view = SafetyPromptView::new(root, inventory);
    let prompt = render::render_safety_prompt(&view, OutputMode::Auto).map_err(|err| {
        refusal(format!(
            "cannot describe {}: {err}",
            root.identity().spelling()
        ))
    })?;

    let mut ask = || -> std::io::Result<String> {
        write!(stderr, "{prompt}\nApprove this root? [y/N] ")?;
        stderr.flush()?;
        let mut answer = String::new();
        // Zero bytes read is EOF, which stays an empty answer and so refuses.
        std::io::stdin().lock().read_line(&mut answer)?;
        Ok(answer)
    };

    let answer = ask().map_err(|err| {
        refusal(format!(
            "cannot ask about {}: {err}",
            root.identity().spelling()
        ))
    })?;

    if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        return Err(refusal(format!(
            "Not approved — {} was left untrusted and nothing was changed.",
            root.identity().spelling()
        )));
    }

    record_approval(root, data_dir).map_err(refuse)?;
    tracing::debug!(root = %root.identity().spelling(), "safety lock: approval recorded");
    Ok(())
}

/// Record the user's approval of `root` in the trust file, transactionally.
///
/// The state is loaded fresh under the trust file's writer lock rather than
/// reusing what the gate decided from: between the prompt appearing and the
/// user answering, another Dodot process may have approved or forgotten a
/// root, and this approval must compose with those writes — never resurrect a
/// concurrently forgotten root, never overwrite a concurrent approval (Spec,
/// concurrent attempts). [`approve`] is idempotent, so re-applying it to the
/// fresh state is correct whatever happened in between.
fn record_approval(root: &ResolvedRoot, data_dir: &Path) -> Result<(), SafetyLockError> {
    let transaction = TrustFileTransaction::begin(data_dir)?;
    let change = approve(&transaction.load()?, root)?;
    if change.added {
        transaction.persist(&change.config)?;
    }
    Ok(())
}

/// Turn any refusal into the exit-1, verbatim-stderr failure.
fn refuse(error: impl std::fmt::Display) -> HookError {
    refusal(error.to_string())
}

/// The one refusal shape: status 1, the diagnostic exactly as written, and a
/// handler that never ran, so stdout stays empty and nothing was mutated.
fn refusal(diagnostic: String) -> HookError {
    let failure = ExternalFailure::new(1, diagnostic).expect("1 is a nonzero status");
    HookError::pre_dispatch_external(failure)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches_with(flags: &[(&str, bool)]) -> clap::ArgMatches {
        let mut command = clap::Command::new("test");
        for (name, _) in flags {
            command = command.arg(
                clap::Arg::new(name.to_string())
                    .long(name.to_string())
                    .action(clap::ArgAction::SetTrue),
            );
        }
        let mut argv = vec!["test".to_string()];
        for (name, set) in flags {
            if *set {
                argv.push(format!("--{name}"));
            }
        }
        command.get_matches_from(argv)
    }

    #[test]
    fn a_mutating_command_is_gated() {
        assert_eq!(
            MUTATES_OR_PREVIEWS.operation(&matches_with(&[("dry-run", false)])),
            RootOperation::RootSensitiveMutation
        );
    }

    /// The preview flag is what separates `up` from `up --dry-run`; declaring
    /// them apart is what would let one drift from the other.
    #[test]
    fn the_preview_flag_downgrades_the_same_declaration() {
        assert_eq!(
            MUTATES_OR_PREVIEWS.operation(&matches_with(&[("dry-run", true)])),
            RootOperation::DryRun
        );
    }

    #[test]
    fn a_command_without_a_preview_flag_is_always_a_mutation() {
        assert_eq!(
            MUTATES.operation(&matches_with(&[("dry-run", true)])),
            RootOperation::RootSensitiveMutation
        );
    }

    #[test]
    fn only_root_sensitive_mutation_reaches_the_gate() {
        for sensitivity in [
            RootSensitivity::ReadOnly,
            RootSensitivity::RootIndependent,
            RootSensitivity::Rootless,
        ] {
            assert!(
                !sensitivity
                    .operation(&matches_with(&[]))
                    .requires_trusted_root(),
                "{sensitivity:?} was gated"
            );
        }
    }

    /// An implicit root at `dir`, the way the gate resolves one when neither
    /// `DOTFILES_ROOT` nor Git selects a root.
    fn implicit_root(dir: &Path, home: &Path) -> ResolvedRoot {
        resolve_root(&RootSelectionInput::new(dir, home), &OsPathProbe).unwrap()
    }

    fn approved_spellings(data_dir: &Path) -> Vec<String> {
        SafetyLockConfig::load_from(data_dir)
            .unwrap()
            .roots
            .approved
            .iter()
            .map(RootIdentity::spelling)
            .collect()
    }

    /// The interleaving the transaction exists for: the gate reads trust
    /// state, the prompt sits open, and meanwhile another process forgets a
    /// root. Recording the answered approval must apply to the state on disk
    /// — adding the new root, keeping the survivor, and *not* resurrecting
    /// the forgotten one from the gate's stale read.
    #[test]
    fn recording_an_approval_composes_with_a_concurrent_forget() {
        let data = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let kept = tempfile::tempdir().unwrap();
        let forgotten = tempfile::tempdir().unwrap();
        let answered = tempfile::tempdir().unwrap();

        let kept_root = implicit_root(kept.path(), home.path());
        let forgotten_root = implicit_root(forgotten.path(), home.path());
        record_approval(&kept_root, data.path()).unwrap();
        record_approval(&forgotten_root, data.path()).unwrap();

        // The gate has read [kept, forgotten] and is prompting; another
        // process revokes one of them before the user answers.
        dodot_lib::commands::roots::forget(
            data.path(),
            Path::new("/"),
            forgotten_root.identity().as_path().as_os_str(),
            &OsPathProbe,
        )
        .unwrap();

        // The user answers `y` for a third root.
        let answered_root = implicit_root(answered.path(), home.path());
        record_approval(&answered_root, data.path()).unwrap();

        let spellings = approved_spellings(data.path());
        assert!(spellings.contains(&kept_root.identity().spelling()));
        assert!(spellings.contains(&answered_root.identity().spelling()));
        assert!(
            !spellings.contains(&forgotten_root.identity().spelling()),
            "the stale gate read resurrected a forgotten root: {spellings:?}"
        );
    }

    /// Two first-run approvals racing from separate terminals: both must be
    /// recorded — the Spec's concurrent-attempt requirement, through the
    /// same `record_approval` the prompt commits with.
    #[test]
    fn concurrent_approvals_are_both_recorded() {
        let data = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let roots: Vec<_> = (0..4).map(|_| tempfile::tempdir().unwrap()).collect();

        let writers: Vec<_> = roots
            .iter()
            .map(|root| {
                let root = implicit_root(root.path(), home.path());
                let data = data.path().to_path_buf();
                std::thread::spawn(move || record_approval(&root, &data).unwrap())
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }

        let spellings = approved_spellings(data.path());
        assert_eq!(
            spellings.len(),
            roots.len(),
            "an approval was lost: {spellings:?}"
        );
    }

    /// The taxonomy is only reviewable if it is unambiguous: one row per
    /// command path.
    #[test]
    fn every_command_is_declared_exactly_once() {
        let mut paths: Vec<&str> = COMMAND_SENSITIVITY.iter().map(|(path, _)| *path).collect();
        paths.sort_unstable();
        let mut unique = paths.clone();
        unique.dedup();
        assert_eq!(paths, unique, "a command path is declared twice");
    }
}

//! Safety Lock's process boundary: capture, resolve, gate, inject.
//!
//! [`dodot_lib::safety_lock`] is CLI-free by construction — it reads no
//! environment variable, no current directory, and no Git. Somebody has to,
//! and this is that somebody. One Standout pre-dispatch hook runs before every
//! dispatched command: it captures the process facts once
//! ([`ProcessFacts::capture`]), resolves the invocation's one dotfiles root,
//! puts the command's declared [`RootOperation`] to the gate, and leaves the
//! result in the command context's extensions as an immutable [`SafetyState`].
//! The one carve-out is a [`Rootless`](RootSensitivity::Rootless) command,
//! which gets only the facts it uses — no Git, no root resolution — so the
//! trust collection's own management surface stays reachable from the broken
//! states it exists to repair.
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
//! # The routes Standout does not dispatch
//!
//! `main` hands a few commands past Standout — the git-filter passthroughs,
//! `config`, `init-sh`, the tutorial — so no pre-dispatch hook can gate them.
//! Each declares its policy in [`PASSTHROUGH_POLICY`], and the ones that do
//! mutate root-selected state ([`PassthroughPolicy::GatedInRoute`]) cross the
//! same gate: one policy, one door, whichever way a command entered the
//! process (ADR-0002). `config` crosses at [`gate_passthrough`] (capture,
//! resolve, authorize, in one call); the tutorial resolved its root when its
//! environment was built and crosses at [`gate_captured`], which authorizes
//! that captured selection instead of re-reading the process (ADR-0001,
//! within-invocation stability).
//!
//! # Refusal
//!
//! Every refusal — declining, an unrecognized answer, EOF, a non-terminal
//! channel, unusable configuration, an approval that could not be persisted —
//! goes out the same door: a [`Refusal`], written verbatim to stderr, exit 1,
//! stdout untouched. Under Standout dispatch it travels as an
//! [`ExternalFailure`]; the passthrough routes print it themselves
//! (`main::exit_passthrough_failure`). The interruption case needs no code at
//! all: Dodot installs no `SIGINT` handler, so Ctrl-C at the prompt kills the
//! process the conventional way and the shell reports 130, with no trust and
//! no deployment written.

use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use standout::cli::{get_deepest_matches, CommandContext, ExternalFailure, HookError, Hooks};
use standout::OutputMode;

use dodot_lib::commands::safety::SafetyPromptView;
use dodot_lib::fs::OsFs;
use dodot_lib::gates::HostFacts;
use dodot_lib::paths::XdgPather;
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

/// How a route `main` dispatches itself relates to the dotfiles root.
///
/// These commands return before (or instead of) `app.dispatch`, so no
/// pre-dispatch hook can run for them; declaring them "not hooked" would be
/// exactly the silent escape ADR-0002 forbids. Instead each declares its
/// policy in [`PASSTHROUGH_POLICY`], and the matrix test in `main.rs` holds
/// every command the CLI has to one of the two tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassthroughPolicy {
    /// A stdin/stdout git filter. Its write target is whatever stream git
    /// connected, never something the root selected — ADR-0002's explicit
    /// exclusion. Gating it would also corrupt the filtered bytes.
    Filter,
    /// Reads the root, writes nothing derived from it.
    ReadOnly,
    /// The route runs the gate itself, choosing the operation from the
    /// parsed invocation: `config` gates `set`/`unset` (the actions that
    /// persist into the selected root) via [`gate_passthrough`] and leaves
    /// reads open; the tutorial gates its real deployment step via
    /// [`gate_captured`] — authorizing the selection captured when its
    /// environment was built — and leaves the narrated read-only steps
    /// open.
    GatedInRoute,
    /// Never touches a dotfiles root (help text).
    Rootless,
}

/// The reviewable list for the routes Standout does not dispatch — the
/// passthrough half of the taxonomy [`COMMAND_SENSITIVITY`] starts.
///
/// Keys are dotted command paths. A row naming a bare command (`config`,
/// `tutorial`) covers that command's whole subtree; a dotted row (`plist.clean`)
/// covers only that leaf, so `template.clean` being a filter says nothing
/// about `template.install-filter`, which stays in [`COMMAND_SENSITIVITY`].
pub const PASSTHROUGH_POLICY: &[(&str, PassthroughPolicy)] = &[
    ("plist.clean", PassthroughPolicy::Filter),
    ("plist.smudge", PassthroughPolicy::Filter),
    ("template.clean", PassthroughPolicy::Filter),
    ("config", PassthroughPolicy::GatedInRoute),
    ("init-sh", PassthroughPolicy::ReadOnly),
    ("tutorial", PassthroughPolicy::GatedInRoute),
    // The help pre-scan in `main` answers before any dispatch; clap's own
    // synthesized `help` subcommand never runs.
    ("help", PassthroughPolicy::Rootless),
];

/// True when the passthrough table classifies `path` — an exact row, or a
/// bare-command row covering the whole subtree.
#[cfg(test)]
pub fn passthrough_policy_covers(path: &str) -> bool {
    let top = path.split('.').next().unwrap_or(path);
    PASSTHROUGH_POLICY
        .iter()
        .any(|(declared, _)| *declared == path || *declared == top)
}

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
    ///
    /// `HOME` is resolved exactly once; the data directory is derived from
    /// that single reading, so the captured facts cannot disagree about which
    /// home they came from.
    pub fn capture() -> Result<Self, anyhow::Error> {
        let home_dir = XdgPather::home_dir_from_env();
        let data_dir = XdgPather::data_dir_for_home(&home_dir);
        let current_dir = std::env::current_dir()?;

        let mut selection = RootSelectionInput::new(current_dir, home_dir);
        selection.dotfiles_root = std::env::var_os("DOTFILES_ROOT");
        selection.git_top_level = git_top_level();

        Ok(Self {
            selection,
            data_dir,
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
///
/// Kept byte-accurate, like every other path Safety Lock handles: on Unix the
/// stdout bytes, minus Git's record terminator, *are* the native path, so a
/// non-UTF-8 repository path resolves, gates, and prints exactly as Git
/// reported it — a lossy `String` hop here would canonicalize, approve, and
/// diagnose a corrupted spelling.
fn git_top_level() -> Option<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let toplevel = strip_git_terminator(&output.stdout);
    if toplevel.is_empty() {
        return None;
    }
    Some(PathBuf::from(native_os_string(toplevel)?))
}

/// Strip exactly the record terminator Git appends to its stdout — one
/// trailing newline — and nothing else.
///
/// A `trim`-style whitespace strip would eat bytes that belong to the path: a
/// repository at `…/dots ` (trailing space) would resolve as its sibling
/// `…/dots`, so an approval of the sibling would let a mutation proceed in a
/// root the user never approved — breaking Safety Lock's displayed-root =
/// authorized-root = mutated-root invariant.
fn strip_git_terminator(stdout: &[u8]) -> &[u8] {
    let stdout = stdout.strip_suffix(b"\n").unwrap_or(stdout);
    // Off Unix only: there `\r` cannot legally end a path, and toolchains
    // that emit CRLF exist. On Unix a `\r` before the terminator is a path
    // byte and must survive.
    #[cfg(not(unix))]
    let stdout = stdout.strip_suffix(b"\r").unwrap_or(stdout);
    stdout
}

/// Reassemble Git's raw stdout bytes into a native `OsString`.
///
/// On Unix, `OsString` is bytes and the conversion is exact. Elsewhere the
/// bytes must be valid UTF-8 to be representable; a non-UTF-8 answer yields
/// `None` — "not inside a repository" — rather than a corrupted path, letting
/// root selection fall through to the current directory.
#[cfg(unix)]
fn native_os_string(bytes: &[u8]) -> Option<std::ffi::OsString> {
    use std::os::unix::ffi::OsStringExt;
    Some(std::ffi::OsString::from_vec(bytes.to_vec()))
}

#[cfg(not(unix))]
fn native_os_string(bytes: &[u8]) -> Option<std::ffi::OsString> {
    Some(std::str::from_utf8(bytes).ok()?.into())
}

/// The authorized root and the facts it was resolved from, as handlers see it.
///
/// Immutable by construction: there is no way to replace the root once the
/// hook has put this in the command context, so the value that crossed the
/// gate is the value the command runs against.
#[derive(Debug, Clone)]
pub struct SafetyState {
    data_dir: PathBuf,
    invocation_dir: Option<PathBuf>,
    root: Option<ResolvedRoot>,
}

impl SafetyState {
    /// The state for a command that takes no dotfiles root.
    ///
    /// Deliberately less than a full capture: no Git subprocess, no root
    /// resolution, and a current directory deleted underneath the shell is
    /// tolerated rather than fatal — the management surface is exactly what
    /// a user in a broken state needs, and only anchoring a relative
    /// `roots forget` argument ever consults the invocation directory.
    fn rootless() -> Self {
        Self {
            data_dir: XdgPather::data_dir_for_home(&XdgPather::home_dir_from_env()),
            invocation_dir: std::env::current_dir().ok(),
            root: None,
        }
    }

    /// Dodot's data directory — where the trust file lives.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// The directory Dodot was invoked from, when the process still has
    /// one. `None` only for a [`Rootless`] command whose working directory
    /// no longer exists — every gated command captured it as a hard fact.
    ///
    /// [`Rootless`]: RootSensitivity::Rootless
    pub fn invocation_dir(&self) -> Option<&Path> {
        self.invocation_dir.as_deref()
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
        let state = gate(sensitivity, get_deepest_matches(matches)).map_err(HookError::from)?;
        ctx.extensions.insert(state);
        Ok(())
    })
}

/// Run the gate for a route Standout does not dispatch.
///
/// The same boundary as [`hook`] — capture, resolve, authorize, confirm —
/// invoked directly by the passthrough routes that mutate root-selected
/// state: `config set`/`unset` and the tutorial's real deployment step.
/// A [`Refusal`] must reach the user exactly as the gate wrote it
/// (`main::exit_passthrough_failure`); the caller proceeds only on `Ok`.
pub fn gate_passthrough(operation: RootOperation) -> Result<SafetyState, Refusal> {
    let facts = ProcessFacts::capture().map_err(refuse)?;
    gate_operation(facts, operation)
}

/// Run the gate on a selection captured earlier in this process — no second
/// capture, no second resolution.
///
/// The entry point for a route that resolved its root up front and worked
/// against it before reaching its one mutation (the tutorial: the root it
/// shows, previews, and deploys into is fixed when its environment is
/// built). Re-capturing here would consult the environment, cwd, Git
/// top-level, and symlink targets a second time, so the root put to the
/// user for approval could differ from the root the mutation then runs
/// against — exactly the divergence ADR-0001's within-invocation stability
/// guarantee rules out. The caller hands over the facts and root it has
/// been using; only the authorization itself happens here.
pub fn gate_captured(
    facts: ProcessFacts,
    root: ResolvedRoot,
    operation: RootOperation,
) -> Result<SafetyState, Refusal> {
    authorize_resolved(facts, root, operation)
}

/// The state whose gate this process already put trust to, for the
/// post-`up` prompts.
///
/// The install ladder and the cfprefsd prompt fire in `main` after `up`'s
/// dispatch returns, where the command context — and the [`SafetyState`] the
/// hook put in it — is gone. They must reuse the root the user just
/// authorized rather than resolving a second one (ADR-0002: post-`up`
/// installers reuse the root authorization of the protected `up` that
/// reached them), so the gate parks a copy here — but only when the
/// operation was a root-sensitive mutation, the one kind whose gate is an
/// authorization. `None` means nothing was authorized in this process — no
/// gate ran, or only a dry-run/read-only one passed through — and the
/// caller skips rather than resolving, so `up --dry-run` on an untrusted
/// root arms nothing.
pub fn authorized_state() -> Option<&'static SafetyState> {
    AUTHORIZED.get()
}

static AUTHORIZED: OnceLock<SafetyState> = OnceLock::new();

/// Park the authorized state for [`authorized_state`]. One command
/// dispatches per process, so first-write-wins is exact, not a race policy.
fn remember_authorized(state: &SafetyState) {
    let _ = AUTHORIZED.set(state.clone());
}

/// Capture, resolve, and authorize — the whole boundary, in order.
///
/// A [`Rootless`](RootSensitivity::Rootless) command branches before the
/// full capture: it manages the trust collection itself, so it must stay
/// reachable when capture would fail — a deleted working directory, a Git
/// that hangs — and it gets only the facts it actually uses.
fn gate(sensitivity: RootSensitivity, matches: &clap::ArgMatches) -> Result<SafetyState, Refusal> {
    if sensitivity == RootSensitivity::Rootless {
        return Ok(SafetyState::rootless());
    }

    let facts = ProcessFacts::capture().map_err(refuse)?;
    gate_operation(facts, sensitivity.operation(matches))
}

/// Resolve the root and put `operation` to the gate — the shared tail of
/// [`gate`] and [`gate_passthrough`].
fn gate_operation(facts: ProcessFacts, operation: RootOperation) -> Result<SafetyState, Refusal> {
    let root = facts.resolve_root().map_err(refuse)?;
    authorize_resolved(facts, root, operation)
}

/// Authorize an already-resolved root — the gate minus capture and
/// resolution, shared by [`gate_operation`] and [`gate_captured`].
fn authorize_resolved(
    facts: ProcessFacts,
    root: ResolvedRoot,
    operation: RootOperation,
) -> Result<SafetyState, Refusal> {
    tracing::debug!(
        root = %root.identity().spelling(),
        selected_by = %root.source(),
        operation = operation.label(),
        "safety lock: resolved root",
    );

    let requires_trust = operation.requires_trusted_root();
    if requires_trust {
        // Loaded only for the one kind of operation whose answer depends on
        // it, so an unusable trust file cannot take `status` down with it.
        let config = SafetyLockConfig::load_from(facts.data_dir()).map_err(refuse)?;
        let outcome =
            authorize(operation, &root, &config, &OsFs, &HostFacts::detect()).map_err(refuse)?;

        if let GateOutcome::ConfirmationRequired { inventory } = outcome {
            confirm(&root, &inventory, facts.data_dir())?;
        }
    }

    let state = SafetyState {
        data_dir: facts.data_dir().to_path_buf(),
        invocation_dir: Some(facts.current_dir().to_path_buf()),
        root: Some(root),
    };
    // Parked only when the operation actually put trust to the test: the
    // post-`up` installers reuse this as *authorization*, and a dry-run or
    // read-only pass through the gate authorized nothing (ADR-0002 — they
    // reuse the root authorization of the *protected* `up` that reached
    // them). Recording those too would let `up --dry-run` on an untrusted
    // root arm mutating post-`up` prompts the gate never approved.
    if requires_trust {
        remember_authorized(&state);
    }
    Ok(state)
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
) -> Result<(), Refusal> {
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

/// The one refusal shape: status 1, the diagnostic exactly as written to
/// stderr, and a command that never ran, so stdout stays empty and nothing
/// was mutated.
///
/// Carried as an error value so both doors can deliver it: the pre-dispatch
/// hook converts it to Standout's [`ExternalFailure`], and the passthrough
/// routes bubble it (via `anyhow`) to `main`, which prints it verbatim and
/// exits 1 — never behind an `error:` prefix, because the diagnostic is the
/// whole message.
#[derive(Debug)]
pub struct Refusal(String);

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Refusal {}

impl From<Refusal> for HookError {
    fn from(refusal: Refusal) -> Self {
        let failure = ExternalFailure::new(1, refusal.0).expect("1 is a nonzero status");
        HookError::pre_dispatch_external(failure)
    }
}

/// Turn any refusal into the exit-1, verbatim-stderr failure.
fn refuse(error: impl std::fmt::Display) -> Refusal {
    refusal(error.to_string())
}

fn refusal(diagnostic: String) -> Refusal {
    Refusal(diagnostic)
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

    /// Only Git's own terminator comes off; trailing whitespace that is part
    /// of the repository path stays, so `dots ` can never collapse into a
    /// sibling `dots` whose approval it would then inherit.
    #[test]
    fn stripping_gits_terminator_preserves_trailing_whitespace_path_bytes() {
        assert_eq!(strip_git_terminator(b"/srv/dots \n"), b"/srv/dots ");
        assert_eq!(strip_git_terminator(b"/srv/dots\t\n"), b"/srv/dots\t");
        assert_eq!(strip_git_terminator(b"/srv/dots\n"), b"/srv/dots");
        assert_eq!(strip_git_terminator(b"/srv/dots"), b"/srv/dots");
        assert_eq!(strip_git_terminator(b"\n"), b"");
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

    /// The changed-selection case (ADR-0001, within-invocation stability):
    /// the captured gate authorizes exactly the selection it was handed,
    /// never a fresh read of the process. The handed-in facts name an
    /// explicitly selected tempdir root, while this very process (its cwd,
    /// environment, and Git top-level) would resolve something else
    /// entirely — so a re-capturing implementation either refuses (no
    /// terminal to approve its untrusted root) or authorizes a root that
    /// fails the identity assertion. The captured one authorizes the
    /// tempdir root without any recorded trust, because deliberate
    /// selection needs none.
    #[test]
    fn the_captured_gate_authorizes_the_selection_it_was_handed() {
        let temp = tempfile::tempdir().unwrap();
        let root_dir = temp.path().join("dots");
        let home = temp.path().join("home");
        std::fs::create_dir_all(&root_dir).unwrap();
        std::fs::create_dir_all(&home).unwrap();

        let mut selection = RootSelectionInput::new(&home, &home);
        selection.dotfiles_root = Some(root_dir.clone().into_os_string());
        let facts = ProcessFacts {
            selection,
            data_dir: temp.path().join("data"),
        };
        let root = facts.resolve_root().unwrap();

        let state = gate_captured(facts, root.clone(), RootOperation::RootSensitiveMutation)
            .expect("an explicitly selected root must authorize without recorded trust");
        assert_eq!(
            state.root().unwrap().identity().spelling(),
            root.identity().spelling(),
            "the authorized root is the captured selection, not a re-resolved one"
        );
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
            Some(Path::new("/")),
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
    /// command path, across both halves.
    #[test]
    fn every_command_is_declared_exactly_once() {
        let mut paths: Vec<&str> = COMMAND_SENSITIVITY
            .iter()
            .map(|(path, _)| *path)
            .chain(PASSTHROUGH_POLICY.iter().map(|(path, _)| *path))
            .collect();
        paths.sort_unstable();
        let mut unique = paths.clone();
        unique.dedup();
        assert_eq!(paths, unique, "a command path is declared twice");
    }

    /// A bare-command row covers its subtree; a dotted row covers only its
    /// leaf. `template` is the case that keeps this honest: its `clean` leaf
    /// is a filter passthrough while `install-filter` is a gated dispatch,
    /// and a subtree reading of the dotted row would silently exempt the
    /// gated half.
    #[test]
    fn passthrough_coverage_distinguishes_leaf_rows_from_subtree_rows() {
        assert!(passthrough_policy_covers("plist.clean"));
        assert!(passthrough_policy_covers("template.clean"));
        assert!(passthrough_policy_covers("config"));
        assert!(passthrough_policy_covers("config.set"));
        assert!(passthrough_policy_covers("tutorial"));

        assert!(!passthrough_policy_covers("template.install-filter"));
        assert!(!passthrough_policy_covers("template"));
        assert!(!passthrough_policy_covers("plist"));
        assert!(!passthrough_policy_covers("up"));
    }

    /// A refusal must cross `main`'s passthrough error handling as itself —
    /// downcastable from the `anyhow` chain — or the gate's diagnostic would
    /// pick up an `error:` prefix on one door and not the other.
    #[test]
    fn a_refusal_survives_the_anyhow_hop_intact() {
        let err: anyhow::Error = refusal("dodot has not been approved for /x.".into()).into();
        let refused = err
            .downcast_ref::<Refusal>()
            .expect("the refusal type was lost in conversion");
        assert_eq!(refused.to_string(), "dodot has not been approved for /x.");
    }
}

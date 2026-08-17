//! The one seam every command crosses: what kind of operation is this, and
//! may it run against this root?
//!
//! [`check`](super::check) answers what standing a root has. It cannot answer
//! whether that standing matters, because that depends on what the command is
//! about to do — `status` and `up --dry-run` read the same untrusted root
//! `up` may not write to. [`RootOperation`] is that missing half, and
//! [`authorize`] is the two composed.
//!
//! Root sensitivity is *declared*, never inferred. A command does not become
//! gated because it built a root-aware context, and does not escape the gate
//! because its context builder is spelled "read-only" — several commands that
//! mutate pack source do exactly that today (Spec, "Risks"). ADR-0002 puts the
//! declaration at one seam so a new mutating command has to state which kind
//! it is, and so the taxonomy can be reviewed as a list rather than
//! reconstructed by reading every handler.
//!
//! No Clap type appears here, and no command name either. The taxonomy is a
//! property of what an operation does to the root, which is why it can be
//! tested exhaustively without a CLI. The CLI names each command's variant in
//! one table at the process boundary (its `safety::COMMAND_SENSITIVITY`).

use crate::fs::Fs;
use crate::gates::HostFacts;

use super::check::{decide, TrustDecision};
use super::error::Result;
use super::inventory::{build_inventory, RootInventory};
use super::roots::ResolvedRoot;
use super::schema::SafetyLockConfig;

/// What an operation does with respect to the resolved dotfiles root.
///
/// The variants are about the *target* of the work, not its risk: what makes
/// an operation root-sensitive is that the root selected what it writes to, so
/// a wrong root writes to the wrong place (ADR-0002).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RootOperation {
    /// A mutation of repository, deployment, or user-visible state whose
    /// target is selected from the resolved root: `up`, `down`, `adopt`,
    /// `fill`, `init`, `addignore`, and configuration actions persisted into
    /// the root. The only kind the gate stops.
    RootSensitiveMutation,

    /// Inspection: reads the root, writes nothing derived from it. `status`,
    /// `list`, and probes stay available on an unapproved root so a user or
    /// agent can find out what Dodot sees *before* deciding to authorize it
    /// (Spec, story 10).
    ReadOnly,

    /// A mutation the user asked to preview rather than perform. Distinct
    /// from [`ReadOnly`](Self::ReadOnly) because the command is otherwise
    /// root-sensitive — the preview is what makes this invocation safe — and
    /// keeping it distinct is what stops "we already classified `up`" from
    /// quietly gating its dry run.
    DryRun,

    /// A mutation whose target is chosen independently of the root: `install
    /// --write`, `git-install-alias`, dismissed-prompt management, factory
    /// `reset`, stdin/stdout filters. These keep their own safeguards; root
    /// trust has nothing to say about a write the root did not select
    /// (ADR-0002).
    RootIndependentMutation,
}

impl RootOperation {
    /// Whether reaching this operation requires the root to be trusted.
    ///
    /// True for exactly one variant. That asymmetry is the policy: the gate
    /// protects root-derived mutation, not mutation in general, and widening
    /// it would turn Safety Lock into a general confirmation framework
    /// (ADR-0002).
    pub fn requires_trusted_root(self) -> bool {
        matches!(self, RootOperation::RootSensitiveMutation)
    }

    /// Stable, user-facing name of the operation kind, for diagnostics and
    /// debug logging (Spec, "Observability").
    pub fn label(self) -> &'static str {
        match self {
            RootOperation::RootSensitiveMutation => "root-sensitive mutation",
            RootOperation::ReadOnly => "read-only",
            RootOperation::DryRun => "dry run",
            RootOperation::RootIndependentMutation => "root-independent mutation",
        }
    }
}

/// What the safety gate concluded about one operation on one root.
///
/// Refusal is not here. Declining at the prompt is the user's answer to
/// [`ConfirmationRequired`](Self::ConfirmationRequired), not something the
/// gate decides, and a gate that could not answer at all returns an error
/// instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateOutcome {
    /// The operation is not root-sensitive, so it proceeds without the root's
    /// standing being consulted at all. No approval is read, and none is
    /// established — a read-only command on an untrusted root leaves it
    /// untrusted (Spec, story 10).
    NotRootSensitive,

    /// Root-sensitive, and the root's standing already permits it: a valid
    /// `DOTFILES_ROOT`, or an implicit root the user approved before.
    Permitted(TrustDecision),

    /// Root-sensitive, implicitly selected, and not approved. The mutation
    /// may not proceed until the user approves this exact path.
    ///
    /// The inventory the prompt needs is carried rather than fetched
    /// afterwards, which is what makes "invalid configuration prevents an
    /// approvable decision" structural instead of a rule callers must
    /// remember: an inventory that cannot be built is an error, and no value
    /// of this variant exists for a caller to answer (Spec, story 14).
    ConfirmationRequired {
        /// Root-wide orientation for the path being approved — not scoped to
        /// whatever packs the current command filtered to.
        inventory: RootInventory,
    },
}

impl GateOutcome {
    /// Whether the operation may proceed into mutation as-is.
    pub fn permits_operation(&self) -> bool {
        !matches!(self, GateOutcome::ConfirmationRequired { .. })
    }

    /// The orientation inventory, when the user has to be asked.
    pub fn inventory(&self) -> Option<&RootInventory> {
        match self {
            GateOutcome::ConfirmationRequired { inventory } => Some(inventory),
            _ => None,
        }
    }
}

/// Decide whether `operation` may run against `root`.
///
/// The whole policy, in order:
///
/// 1. An operation that is not root-sensitive proceeds immediately. Nothing
///    is read — not the trust state, not the root's contents — so a corrupt
///    trust file cannot take `status` down with it, and a diagnostic run on an
///    unfamiliar repository costs nothing.
/// 2. Otherwise the root's standing decides ([`decide`]): explicit selection
///    and prior approval both permit the mutation, and neither pays for an
///    inventory it will not show.
/// 3. An unapproved implicit root needs the user, so the orientation
///    inventory is built and carried out with the answer.
///
/// Nothing here writes. Approval is [`approve`](super::check::approve)'s job
/// and persistence is the caller's, which is what lets the CLI record the
/// user's answer before starting the mutation it authorizes (Spec, "Risks").
///
/// Fails when the trust state is unusable, or — for the confirmation case
/// only — when the root's configuration or pack layout cannot be read. Both
/// leave the caller with no approvable outcome to act on.
pub fn authorize(
    operation: RootOperation,
    root: &ResolvedRoot,
    config: &SafetyLockConfig,
    fs: &dyn Fs,
    host: &HostFacts,
) -> Result<GateOutcome> {
    if !operation.requires_trusted_root() {
        return Ok(GateOutcome::NotRootSensitive);
    }

    match decide(root, config)? {
        TrustDecision::ApprovalRequired => Ok(GateOutcome::ConfirmationRequired {
            inventory: build_inventory(root, fs, host)?,
        }),
        permitted => Ok(GateOutcome::Permitted(permitted)),
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::fs::OsFs;
    use crate::testing::TempEnvironment;

    use super::super::error::SafetyLockError;
    use super::super::inventory::InventoryCategory;
    use super::super::roots::{RootIdentity, RootSource};
    use super::super::schema::TrustedRootsSection;
    use super::*;

    const IMPLICIT: [RootSource; 2] = [RootSource::Git, RootSource::CurrentDirectory];

    fn host() -> HostFacts {
        HostFacts::for_tests("darwin", "arm64")
    }

    fn root(path: &Path, source: RootSource) -> ResolvedRoot {
        ResolvedRoot::new(
            RootIdentity::new(std::fs::canonicalize(path).unwrap()).unwrap(),
            source,
        )
    }

    fn approved(paths: impl IntoIterator<Item = PathBuf>) -> SafetyLockConfig {
        SafetyLockConfig {
            roots: TrustedRootsSection {
                approved: paths
                    .into_iter()
                    .map(|path| RootIdentity::new(std::fs::canonicalize(path).unwrap()).unwrap())
                    .collect(),
            },
        }
    }

    /// A root that cannot be inventoried at all, used to prove an outcome was
    /// reached without one being built.
    fn absent_root(source: RootSource) -> ResolvedRoot {
        ResolvedRoot::new(
            RootIdentity::new("/nonexistent/dotfiles-root").unwrap(),
            source,
        )
    }

    fn environment() -> TempEnvironment {
        TempEnvironment::builder()
            .pack("zsh")
            .file("aliases.sh", "alias g=git")
            .done()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .done()
            .build()
    }

    /// The decision matrix, whole: four operation kinds against explicit,
    /// approved-implicit, and unapproved-implicit roots.
    #[test]
    fn the_gate_stops_exactly_unapproved_implicit_root_sensitive_mutation() {
        let env = environment();
        let empty = SafetyLockConfig::default();
        let trusted = approved([env.dotfiles_root.clone()]);

        for source in IMPLICIT {
            let implicit = root(&env.dotfiles_root, source);

            assert!(
                matches!(
                    authorize(
                        RootOperation::RootSensitiveMutation,
                        &implicit,
                        &empty,
                        env.fs.as_ref(),
                        &host()
                    )
                    .unwrap(),
                    GateOutcome::ConfirmationRequired { .. }
                ),
                "an unapproved {source} root reached mutation unasked"
            );

            assert_eq!(
                authorize(
                    RootOperation::RootSensitiveMutation,
                    &implicit,
                    &trusted,
                    env.fs.as_ref(),
                    &host()
                )
                .unwrap(),
                GateOutcome::Permitted(TrustDecision::AlreadyApproved),
                "an approved {source} root was asked about again"
            );
        }

        assert_eq!(
            authorize(
                RootOperation::RootSensitiveMutation,
                &root(&env.dotfiles_root, RootSource::Environment),
                &empty,
                env.fs.as_ref(),
                &host()
            )
            .unwrap(),
            GateOutcome::Permitted(TrustDecision::ExplicitlySelected),
        );

        // Every other operation kind passes on every root, approved or not.
        for operation in [
            RootOperation::ReadOnly,
            RootOperation::DryRun,
            RootOperation::RootIndependentMutation,
        ] {
            for source in [
                RootSource::Environment,
                RootSource::Git,
                RootSource::CurrentDirectory,
            ] {
                assert_eq!(
                    authorize(
                        operation,
                        &root(&env.dotfiles_root, source),
                        &empty,
                        env.fs.as_ref(),
                        &host()
                    )
                    .unwrap(),
                    GateOutcome::NotRootSensitive,
                    "{} on a {source} root crossed the gate",
                    operation.label()
                );
            }
        }
    }

    /// Story 10: a read-only command or a dry run must stay usable on a root
    /// nothing is known about — including one whose trust state is broken.
    /// Answering these from the collection at all would let a corrupt file
    /// take diagnosis down with the mutation it legitimately stops.
    #[test]
    fn a_non_root_sensitive_operation_consults_no_trust_state() {
        let env = environment();
        let identity =
            RootIdentity::new(std::fs::canonicalize(&env.dotfiles_root).unwrap()).unwrap();
        let duplicated = SafetyLockConfig {
            roots: TrustedRootsSection {
                approved: vec![identity.clone(), identity],
            },
        };

        for operation in [
            RootOperation::ReadOnly,
            RootOperation::DryRun,
            RootOperation::RootIndependentMutation,
        ] {
            let outcome = authorize(
                operation,
                &root(&env.dotfiles_root, RootSource::Git),
                &duplicated,
                env.fs.as_ref(),
                &host(),
            )
            .unwrap();

            assert_eq!(outcome, GateOutcome::NotRootSensitive);
            assert!(
                outcome.inventory().is_none(),
                "{} built a prompt inventory",
                operation.label()
            );
        }

        // The same state does stop the mutation it is about.
        assert!(matches!(
            authorize(
                RootOperation::RootSensitiveMutation,
                &root(&env.dotfiles_root, RootSource::Git),
                &duplicated,
                env.fs.as_ref(),
                &host(),
            )
            .unwrap_err(),
            SafetyLockError::DuplicateApprovedRoot { .. }
        ));
    }

    /// Neither a bypass nor an approval: passing the gate without trust must
    /// not leave trust behind, or a dry run would silently authorize the real
    /// run that follows it.
    #[test]
    fn passing_the_gate_never_establishes_trust() {
        let env = environment();
        let config = SafetyLockConfig::default();

        for operation in [
            RootOperation::ReadOnly,
            RootOperation::DryRun,
            RootOperation::RootIndependentMutation,
            RootOperation::RootSensitiveMutation,
        ] {
            let _ = authorize(
                operation,
                &root(&env.dotfiles_root, RootSource::Git),
                &config,
                env.fs.as_ref(),
                &host(),
            );
        }

        assert!(
            config.roots.approved.is_empty(),
            "the gate wrote an approval"
        );
        assert!(
            !authorize(
                RootOperation::RootSensitiveMutation,
                &root(&env.dotfiles_root, RootSource::Git),
                &config,
                env.fs.as_ref(),
                &host(),
            )
            .unwrap()
            .permits_operation(),
            "an earlier pass through the gate trusted the root"
        );
    }

    /// Performance posture (Spec, "Performance"): an already-permitted root
    /// pays a trust check and nothing else. Proven by permitting a root that
    /// does not exist — building its inventory would have to fail.
    #[test]
    fn a_permitted_root_is_never_inventoried() {
        let fs = OsFs::new();

        for (root, config) in [
            (
                absent_root(RootSource::Environment),
                SafetyLockConfig::default(),
            ),
            (
                absent_root(RootSource::Git),
                SafetyLockConfig {
                    roots: TrustedRootsSection {
                        approved: vec![RootIdentity::new("/nonexistent/dotfiles-root").unwrap()],
                    },
                },
            ),
        ] {
            let outcome = authorize(
                RootOperation::RootSensitiveMutation,
                &root,
                &config,
                &fs,
                &host(),
            )
            .unwrap();

            assert!(outcome.permits_operation());
            assert!(outcome.inventory().is_none());
        }
    }

    /// Spec, "Risks": the path is approved, not the current command's filter.
    /// The gate takes no pack filter, so the inventory a filtered `up` shows
    /// is the same root-wide one an unfiltered `up` shows.
    #[test]
    fn the_confirmation_inventory_is_root_wide_not_command_scoped() {
        let env = environment();

        let outcome = authorize(
            RootOperation::RootSensitiveMutation,
            &root(&env.dotfiles_root, RootSource::Git),
            &SafetyLockConfig::default(),
            env.fs.as_ref(),
            &host(),
        )
        .unwrap();

        let inventory = outcome.inventory().expect("no inventory to show the user");
        assert_eq!(inventory.total_files(), 2);
        assert_eq!(
            inventory
                .sample
                .iter()
                .map(|entry| (entry.category, entry.relative_path.to_str().unwrap()))
                .collect::<Vec<_>>(),
            [
                (InventoryCategory::Shell, "zsh/aliases.sh"),
                (InventoryCategory::Link, "vim/vimrc"),
            ],
            "the inventory did not cover every pack under the root"
        );
    }

    /// Story 14: configuration Dodot cannot load must stop the command, and
    /// must stop it *before* an approvable outcome exists. There is no
    /// `ConfirmationRequired` to answer here, so no answer can mint approval.
    #[test]
    fn invalid_configuration_yields_no_approvable_outcome() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .config("this is not valid toml")
            .file("vimrc", "set nocompatible")
            .done()
            .build();

        let error = authorize(
            RootOperation::RootSensitiveMutation,
            &root(&env.dotfiles_root, RootSource::Git),
            &SafetyLockConfig::default(),
            env.fs.as_ref(),
            &host(),
        )
        .unwrap_err();

        assert!(
            matches!(
                &error,
                SafetyLockError::DotfilesConfigUnusable { config_file, .. }
                    if config_file == &std::fs::canonicalize(&env.dotfiles_root).unwrap().join("vim").join(".dodot.toml")
            ),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn only_root_sensitive_mutation_requires_trust() {
        assert!(RootOperation::RootSensitiveMutation.requires_trusted_root());

        for operation in [
            RootOperation::ReadOnly,
            RootOperation::DryRun,
            RootOperation::RootIndependentMutation,
        ] {
            assert!(
                !operation.requires_trusted_root(),
                "{} requires trust",
                operation.label()
            );
        }
    }

    #[test]
    fn operation_labels_name_the_kind() {
        assert_eq!(
            [
                RootOperation::RootSensitiveMutation,
                RootOperation::ReadOnly,
                RootOperation::DryRun,
                RootOperation::RootIndependentMutation,
            ]
            .map(RootOperation::label),
            [
                "root-sensitive mutation",
                "read-only",
                "dry run",
                "root-independent mutation",
            ]
        );
    }
}

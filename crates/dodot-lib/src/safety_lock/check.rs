//! The trust decision: may this resolved root drive a root-sensitive
//! mutation?
//!
//! Deciding and *recording* are separate. [`decide`] is a pure question over
//! the resolved root and the loaded trust state; [`approve`] returns the trust
//! state as it would be after approval, leaving the write to the caller. That
//! split is what keeps this API free of persistence I/O and lets the CLI gate
//! (WS07) order the write before the mutation it authorizes — approval records
//! the user's path decision, not the mutation's outcome (Spec, "Risks").
//!
//! Both operations consult the collection only for an *implicit* root.
//! Provenance is what decides that: a valid `DOTFILES_ROOT` is already the
//! deliberate selection, so it neither reads an approval nor writes one
//! (ADR-0003).
//!
//! WS05 layers the operation policy — read-only, dry run, root-sensitive
//! mutation — on top of what is decided here; this module answers only the
//! question the trust collection can answer.

use super::error::{Result, SafetyLockError};
use super::roots::{ResolvedRoot, RootIdentity};
use super::schema::SafetyLockConfig;

/// Whether a resolved root may proceed into root-sensitive mutation.
///
/// The variants are outcomes, not policy: nothing here says how a caller
/// obtains approval, only what standing the root currently has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustDecision {
    /// Selected by a valid `DOTFILES_ROOT`. Deliberate selection needs no
    /// stored approval and records none.
    ExplicitlySelected,
    /// An implicit root already present in the approved collection.
    AlreadyApproved,
    /// An implicit root with no approval. The command may not mutate until
    /// the user approves this exact canonical path.
    ApprovalRequired,
}

impl TrustDecision {
    /// Whether the root-sensitive mutation may proceed as-is.
    pub fn permits_mutation(self) -> bool {
        matches!(
            self,
            TrustDecision::ExplicitlySelected | TrustDecision::AlreadyApproved
        )
    }

    /// Whether the caller must obtain the user's confirmation first.
    pub fn needs_confirmation(self) -> bool {
        matches!(self, TrustDecision::ApprovalRequired)
    }
}

/// The trust state as it would be after approving a root, plus whether that
/// changed anything.
///
/// Returned rather than written: the caller owns persistence.
#[derive(Debug, Clone)]
pub struct ApprovalChange {
    /// The trust state to persist.
    pub config: SafetyLockConfig,
    /// The root that was approved.
    pub identity: RootIdentity,
    /// `false` when the root was already approved, so the caller can skip a
    /// pointless write.
    pub added: bool,
}

/// Decide what standing `root` has under the loaded trust state.
///
/// Reads nothing from disk: both the root and the trust state are resolved
/// once per invocation and passed in, so the path checked here is the path
/// that will be mutated (ADR-0001).
///
/// An environment root is answered from its provenance alone — the collection
/// is not consulted, because nothing in it could authorize or refuse a
/// selection `DOTFILES_ROOT` already made. For an implicit root the lookup is
/// exact on the canonical identity, so a moved root, an alias that normalized
/// elsewhere, and a root whose lossy rendering collides with an approved one
/// all land on [`ApprovalRequired`](TrustDecision::ApprovalRequired) rather
/// than inheriting another root's approval (Spec, story 6).
///
/// Fails when the collection itself is unusable — the same invariants
/// [`SafetyLockConfig::validate`] enforces at load. Only an implicit root
/// reaches that check: an environment root returns on provenance before the
/// collection is consulted at all, so it is decided whatever state the trust
/// file is in, the mirror of [`approve`]'s refusal preceding every other
/// check. For an implicit root — the only case the collection can answer —
/// trust state Dodot does not fully understand must be reported, never read as
/// "nothing approved", which would silently downgrade a decision to
/// `ApprovalRequired` and, once the user confirmed, write approvals back over a
/// file whose contents were never understood (ADR-0001).
pub fn decide(root: &ResolvedRoot, config: &SafetyLockConfig) -> Result<TrustDecision> {
    if !root.requires_approval() {
        return Ok(TrustDecision::ExplicitlySelected);
    }

    config.validate()?;

    Ok(if config.is_approved(root.identity()) {
        TrustDecision::AlreadyApproved
    } else {
        TrustDecision::ApprovalRequired
    })
}

/// Produce the trust state that records approval of `root`.
///
/// Idempotent: approving a root the collection already holds returns it
/// unchanged with [`added`](ApprovalChange::added) `false`, so a caller can
/// skip the write and a repeated confirmation cannot mint a duplicate the next
/// load would refuse.
///
/// Approval is an append. Roots stay independent and unordered — nothing here
/// promotes, replaces, or arbitrates between the roots already held, because
/// the collection has no preferred entry to be (Spec, story 5).
///
/// Fails with
/// [`EnvironmentRootNotApprovable`](super::error::SafetyLockError::EnvironmentRootNotApprovable)
/// for an environment-selected root: `DOTFILES_ROOT` is already the deliberate
/// selection, and adding it to the collection would trust a path the user was
/// never shown (ADR-0003). That refusal precedes every other check, so an
/// environment root cannot be recorded whatever state the collection is in.
pub fn approve(config: &SafetyLockConfig, root: &ResolvedRoot) -> Result<ApprovalChange> {
    if !root.requires_approval() {
        return Err(SafetyLockError::EnvironmentRootNotApprovable {
            spelling: root.identity().spelling(),
        });
    }

    config.validate()?;

    let identity = root.identity().clone();
    let mut config = config.clone();
    let added = !config.is_approved(&identity);
    if added {
        config.roots.approved.push(identity.clone());
    }

    Ok(ApprovalChange {
        config,
        identity,
        added,
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::path::PathBuf;

    use super::super::roots::RootSource;
    use super::super::schema::TrustedRootsSection;
    use super::*;

    fn identity(path: &str) -> RootIdentity {
        RootIdentity::new(path).unwrap()
    }

    /// A root whose native path is not valid UTF-8. `0x80` is a continuation
    /// byte with no lead byte, so std will never hand these bytes back as a
    /// string.
    fn non_unicode_identity(suffix: &[u8]) -> RootIdentity {
        let mut bytes = b"/tmp/".to_vec();
        bytes.extend_from_slice(suffix);
        RootIdentity::new(PathBuf::from(OsString::from_vec(bytes))).unwrap()
    }

    fn approved(roots: impl IntoIterator<Item = RootIdentity>) -> SafetyLockConfig {
        SafetyLockConfig {
            roots: TrustedRootsSection {
                approved: roots.into_iter().collect(),
            },
        }
    }

    fn implicit(identity: RootIdentity) -> ResolvedRoot {
        ResolvedRoot::new(identity, RootSource::Git)
    }

    fn explicit(identity: RootIdentity) -> ResolvedRoot {
        ResolvedRoot::new(identity, RootSource::Environment)
    }

    #[test]
    fn an_implicit_root_is_approved_only_when_the_collection_holds_it() {
        let root = identity("/home/alice/dotfiles");

        assert_eq!(
            decide(&implicit(root.clone()), &SafetyLockConfig::default()).unwrap(),
            TrustDecision::ApprovalRequired
        );
        assert_eq!(
            decide(&implicit(root.clone()), &approved([root.clone()])).unwrap(),
            TrustDecision::AlreadyApproved
        );

        // Both implicit mechanisms are one case: the collection records a
        // path, not the mechanism that found it.
        let from_cwd = ResolvedRoot::new(root.clone(), RootSource::CurrentDirectory);
        assert_eq!(
            decide(&from_cwd, &approved([root])).unwrap(),
            TrustDecision::AlreadyApproved
        );
    }

    /// Deliberate selection is answered from provenance alone: an environment
    /// root neither reads an approval nor needs one (ADR-0003).
    #[test]
    fn an_environment_root_is_deliberate_without_a_trust_record() {
        let root = identity("/home/alice/dotfiles");
        let config = SafetyLockConfig::default();

        assert_eq!(
            decide(&explicit(root.clone()), &config).unwrap(),
            TrustDecision::ExplicitlySelected
        );

        let refusal = approve(&config, &explicit(root.clone())).unwrap_err();
        assert!(
            matches!(
                refusal,
                SafetyLockError::EnvironmentRootNotApprovable { ref spelling }
                    if spelling == "/home/alice/dotfiles"
            ),
            "unexpected error: {refusal}"
        );
        assert!(
            config.roots.approved.is_empty(),
            "the environment root reached the collection"
        );

        // Its standing does not depend on the collection either way: an
        // already-listed path is still explicitly selected, and still not
        // approvable.
        let listed = approved([root.clone()]);
        assert_eq!(
            decide(&explicit(root.clone()), &listed).unwrap(),
            TrustDecision::ExplicitlySelected
        );
        assert!(approve(&listed, &explicit(root)).is_err());
    }

    /// Story 5: roots are independent. Approving a second one neither replaces
    /// the first nor marks either preferred.
    #[test]
    fn independent_roots_are_approved_without_arbitration() {
        let personal = identity("/home/alice/dotfiles");
        let work = identity("/home/alice/work-dotfiles");

        let change = approve(&SafetyLockConfig::default(), &implicit(personal.clone())).unwrap();
        assert!(change.added);
        let change = approve(&change.config, &implicit(work.clone())).unwrap();
        assert!(change.added);

        assert_eq!(
            change.config.roots.approved,
            vec![personal.clone(), work.clone()]
        );
        assert_eq!(
            decide(&implicit(personal), &change.config).unwrap(),
            TrustDecision::AlreadyApproved
        );
        assert_eq!(
            decide(&implicit(work), &change.config).unwrap(),
            TrustDecision::AlreadyApproved
        );
        assert!(change.config.validate().is_ok());
    }

    /// Re-confirming a root must not mint a second record: the duplicate a
    /// non-idempotent append leaves behind is exactly what the next load
    /// refuses to read.
    #[test]
    fn approving_an_approved_root_changes_nothing() {
        let root = identity("/home/alice/dotfiles");
        let first = approve(&SafetyLockConfig::default(), &implicit(root.clone())).unwrap();

        let second = approve(&first.config, &implicit(root.clone())).unwrap();

        assert!(!second.added, "a repeated approval reported a change");
        assert_eq!(second.identity, root);
        assert_eq!(second.config.roots.approved, vec![root]);
        assert!(second.config.validate().is_ok());
    }

    /// Approval is spelling-insensitive in exactly the way identity is: an
    /// alias of an approved root is the same record, not a second one.
    #[test]
    fn an_alias_of_an_approved_root_is_already_approved() {
        let config = approved([identity("/home/alice/dotfiles")]);

        for alias in [
            "/home/alice//dotfiles",
            "/home/alice/dotfiles/",
            "/home/alice/./dotfiles",
        ] {
            let root = implicit(identity(alias));
            assert_eq!(
                decide(&root, &config).unwrap(),
                TrustDecision::AlreadyApproved,
                "`{alias}` was not recognized as the approved root"
            );
            assert!(
                !approve(&config, &root).unwrap().added,
                "`{alias}` was recorded as a second approval"
            );
        }
    }

    /// Story 6: approval belongs to a path, so a root that moved arrives at
    /// the gate unapproved rather than inheriting its old location's standing.
    #[test]
    fn a_moved_root_does_not_inherit_the_old_locations_approval() {
        let config = approved([identity("/home/alice/dotfiles")]);
        let moved = implicit(identity("/home/alice/archive/dotfiles"));

        assert_eq!(
            decide(&moved, &config).unwrap(),
            TrustDecision::ApprovalRequired
        );

        // Nor does a parent or child of the approved root.
        for neighbour in ["/home/alice", "/home/alice/dotfiles/vim"] {
            assert_eq!(
                decide(&implicit(identity(neighbour)), &config).unwrap(),
                TrustDecision::ApprovalRequired,
                "`{neighbour}` inherited the approved root's standing"
            );
        }
    }

    /// Two roots whose lossy renderings collide are two records: approving one
    /// must not authorize the other, which a rendering-based lookup would.
    #[test]
    fn lossy_colliding_roots_are_approved_independently() {
        let one = non_unicode_identity(b"\x80");
        let other = non_unicode_identity(b"\x81");
        assert_eq!(
            one.as_path().to_string_lossy(),
            other.as_path().to_string_lossy(),
            "test premise: these two roots render identically when lossy"
        );

        let change = approve(&SafetyLockConfig::default(), &implicit(one.clone())).unwrap();

        assert_eq!(
            decide(&implicit(one), &change.config).unwrap(),
            TrustDecision::AlreadyApproved
        );
        assert_eq!(
            decide(&implicit(other.clone()), &change.config).unwrap(),
            TrustDecision::ApprovalRequired
        );

        let change = approve(&change.config, &implicit(other)).unwrap();
        assert!(change.added);
        assert_eq!(change.config.roots.approved.len(), 2);
        assert!(change.config.validate().is_ok());
    }

    /// Trust state Dodot cannot fully read is reported, never treated as an
    /// empty registry: reading it as empty would downgrade the decision to
    /// "ask the user" and then write approvals back over a file whose contents
    /// were never understood.
    ///
    /// Only an implicit root reaches that check, because it is the only case
    /// that consults the collection. An environment root is answered from
    /// provenance first and is decided whatever state the file is in — the
    /// mirror of the refusal `approve` gives it, which likewise precedes every
    /// other check.
    #[test]
    fn unusable_trust_state_is_reported_rather_than_read_as_empty() {
        let root = identity("/home/alice/dotfiles");
        let duplicated = approved([root.clone(), root.clone()]);

        for error in [
            decide(&implicit(root.clone()), &duplicated).unwrap_err(),
            approve(&duplicated, &implicit(root.clone())).unwrap_err(),
        ] {
            assert!(
                matches!(error, SafetyLockError::DuplicateApprovedRoot { ref spelling }
                    if spelling == "/home/alice/dotfiles"),
                "unexpected error: {error}"
            );
        }

        assert_eq!(
            decide(&explicit(root.clone()), &duplicated).unwrap(),
            TrustDecision::ExplicitlySelected,
            "an environment root's standing was made to depend on the collection"
        );
        assert!(
            matches!(
                approve(&duplicated, &explicit(root)).unwrap_err(),
                SafetyLockError::EnvironmentRootNotApprovable { .. }
            ),
            "the collection was consulted before refusing an environment root"
        );
    }

    /// The whole lifecycle against the one thing it is for: state the caller
    /// writes and loads back. Approving, listing, and forgetting each hand
    /// back a collection the validated load accepts — which is what keeps a
    /// non-Unicode root and a repeated confirmation from producing a file
    /// Dodot then refuses to read.
    #[test]
    fn the_lifecycle_produces_collections_that_load_back_unchanged() {
        use super::super::forget::{forget_root, ForgetRequest};
        use super::super::list::list_roots;
        use super::super::test_probe::FakeProbe;

        let data_dir = tempfile::tempdir().unwrap();
        let persist = |config: &SafetyLockConfig| {
            std::fs::write(
                SafetyLockConfig::path_in(data_dir.path()),
                toml::to_string(config).unwrap(),
            )
            .unwrap();
            SafetyLockConfig::load_from(data_dir.path()).unwrap()
        };

        let personal = identity("/home/alice/dotfiles");
        let native = non_unicode_identity(b"\x80dots");

        let mut config = SafetyLockConfig::default();
        for root in [&personal, &native] {
            config = persist(&approve(&config, &implicit(root.clone())).unwrap().config);
        }
        // The repeated confirmation a user gives on a second run.
        config = persist(
            &approve(&config, &implicit(personal.clone()))
                .unwrap()
                .config,
        );

        assert_eq!(
            config.roots.approved,
            vec![personal.clone(), native.clone()]
        );

        let listing = list_roots(&config, &FakeProbe::default()).unwrap();
        let spelling = listing.entries[1].spelling();
        let config = persist(
            &forget_root(
                &config,
                &ForgetRequest::new(spelling),
                &FakeProbe::default(),
            )
            .unwrap()
            .config,
        );

        assert_eq!(config.roots.approved, vec![personal.clone()]);
        assert_eq!(
            decide(&implicit(native), &config).unwrap(),
            TrustDecision::ApprovalRequired,
            "the forgotten root survived the round trip as approved"
        );
        assert_eq!(
            decide(&implicit(personal), &config).unwrap(),
            TrustDecision::AlreadyApproved
        );
    }

    #[test]
    fn only_an_unapproved_implicit_root_needs_confirmation() {
        assert!(TrustDecision::ExplicitlySelected.permits_mutation());
        assert!(TrustDecision::AlreadyApproved.permits_mutation());
        assert!(!TrustDecision::ApprovalRequired.permits_mutation());

        assert!(TrustDecision::ApprovalRequired.needs_confirmation());
        assert!(!TrustDecision::AlreadyApproved.needs_confirmation());
        assert!(!TrustDecision::ExplicitlySelected.needs_confirmation());
    }
}

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
//! Behaviour arrives in WS05; this Work Stream fixes the signatures.

use super::error::Result;
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
pub fn decide(root: &ResolvedRoot, config: &SafetyLockConfig) -> Result<TrustDecision> {
    let _ = (root, config);
    todo!("WS05: safety decisions over the loaded trust state")
}

/// Produce the trust state that records approval of `root`.
///
/// Fails with
/// [`EnvironmentRootNotApprovable`](super::error::SafetyLockError::EnvironmentRootNotApprovable)
/// for an environment-selected root: `DOTFILES_ROOT` is already the deliberate
/// selection, and adding it to the collection would trust a path the user was
/// never shown (ADR-0003).
pub fn approve(config: &SafetyLockConfig, root: &ResolvedRoot) -> Result<ApprovalChange> {
    let _ = (config, root);
    todo!("WS04: trusted-root registry lifecycle")
}

#[cfg(test)]
mod tests {
    use super::*;

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

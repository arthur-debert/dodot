//! Listing approved roots — the inspection half of root management
//! (`dodot roots list`).
//!
//! Every listed entry prints its reversible spelling, which is exactly what
//! [`forget_root`](super::forget::forget_root) accepts back. That round trip
//! is the contract: an approval a user can see is an approval they can revoke,
//! including for a root that has since been moved or deleted (ADR-0003).
//!
//! Behaviour arrives in WS05; this Work Stream fixes the signature.

use super::error::Result;
use super::roots::RootIdentity;
use super::schema::SafetyLockConfig;
use super::util::PathProbe;

/// One approved root as presented to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedRootEntry {
    /// The approved canonical root.
    pub identity: RootIdentity,
    /// Whether that path is still a readable directory.
    ///
    /// Purely informational: a missing root keeps its approval — and stays
    /// revocable by its exact spelling — until the user forgets it.
    pub exists: bool,
}

impl TrustedRootEntry {
    /// The spelling to print, and the one `roots forget` accepts back.
    pub fn spelling(&self) -> String {
        self.identity.spelling()
    }
}

/// The full approved-roots listing.
///
/// Ordered as stored: the collection has no preferred root, so nothing here
/// implies precedence between the entries (Spec, story 5).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrustedRootListing {
    pub entries: Vec<TrustedRootEntry>,
}

impl TrustedRootListing {
    /// Whether no root is approved — the state a user with no trust file is
    /// in, which the listing must render as "nothing approved" rather than as
    /// an error.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// List the approved roots in the loaded trust state.
pub fn list_roots(config: &SafetyLockConfig, probe: &dyn PathProbe) -> Result<TrustedRootListing> {
    let _ = (config, probe);
    todo!("WS05: orientation and root listing")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_prints_the_spelling_forget_accepts() {
        let entry = TrustedRootEntry {
            identity: RootIdentity::new("/home/alice/dotfiles").unwrap(),
            exists: false,
        };

        assert_eq!(entry.spelling(), "/home/alice/dotfiles");
        assert_eq!(
            RootIdentity::parse(&entry.spelling()).unwrap(),
            entry.identity
        );
    }

    #[test]
    fn an_empty_listing_is_a_state_not_an_error() {
        assert!(TrustedRootListing::default().is_empty());
    }
}

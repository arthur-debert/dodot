//! Listing approved roots — the inspection half of root management
//! (`dodot roots list`).
//!
//! Every listed entry prints its reversible spelling, which is exactly what
//! [`forget_root`](super::forget::forget_root) accepts back. That round trip
//! is the contract: an approval a user can see is an approval they can revoke,
//! including for a root that has since been moved or deleted (ADR-0003).

use super::error::Result;
use super::roots::RootIdentity;
use super::schema::SafetyLockConfig;
use super::util::PathProbe;

/// One approved root as presented to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedRootEntry {
    /// The approved canonical root.
    pub identity: RootIdentity,
    /// Whether that path is still a directory Dodot could use as a root.
    ///
    /// Purely informational: a root that has been moved, deleted, or made
    /// unreadable keeps its approval — and stays revocable by its exact
    /// spelling — until the user forgets it.
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
///
/// Deterministic in both order and spelling: entries come back in stored
/// order, each named by its own reversible spelling. Nothing is sorted by a
/// rendering, de-duplicated, or folded — two roots whose lossy renderings
/// collide list as two entries a user can tell apart and revoke separately
/// (Spec, "Proposed Shape").
///
/// The probe answers only [`exists`](TrustedRootEntry::exists), which no other
/// field depends on: listing an approval never re-canonicalizes it, so a root
/// that has been moved or replaced is still shown at the spelling it was
/// approved at — the one `roots forget` needs back.
///
/// Fails when the collection is unusable, so `roots list` surfaces a broken
/// trust file rather than showing it as empty (Spec, "Proposed Shape").
pub fn list_roots(config: &SafetyLockConfig, probe: &dyn PathProbe) -> Result<TrustedRootListing> {
    config.validate()?;

    Ok(TrustedRootListing {
        entries: config
            .roots
            .approved
            .iter()
            .map(|identity| TrustedRootEntry {
                identity: identity.clone(),
                exists: probe.is_readable_dir(identity.as_path()),
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::path::PathBuf;

    use super::super::error::SafetyLockError;
    use super::super::schema::TrustedRootsSection;
    use super::super::test_probe::{Entry, FakeProbe};
    use super::super::util::OsPathProbe;
    use super::*;

    fn identity(path: &str) -> RootIdentity {
        RootIdentity::new(path).unwrap()
    }

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

    /// Order is the stored order, unchanged: the collection has no preferred
    /// root, so re-ordering the listing would invent a precedence the model
    /// does not have.
    #[test]
    fn every_approved_root_lists_once_in_stored_order() {
        let config = approved([
            identity("/home/alice/work-dotfiles"),
            identity("/home/alice/dotfiles"),
            identity("/home/alice/archive/dotfiles"),
        ]);
        let probe = FakeProbe::dir("/home/alice/dotfiles")
            .add("/home/alice/work-dotfiles", Entry::UnreadableDir);

        let listing = list_roots(&config, &probe).unwrap();

        assert!(!listing.is_empty());
        assert_eq!(
            listing
                .entries
                .iter()
                .map(|entry| (entry.spelling(), entry.exists))
                .collect::<Vec<_>>(),
            vec![
                ("/home/alice/work-dotfiles".to_owned(), false),
                ("/home/alice/dotfiles".to_owned(), true),
                ("/home/alice/archive/dotfiles".to_owned(), false),
            ]
        );
    }

    /// A user who has approved nothing is in a state, not in trouble.
    #[test]
    fn nothing_approved_lists_nothing() {
        let listing = list_roots(&SafetyLockConfig::default(), &FakeProbe::default()).unwrap();
        assert!(listing.is_empty());
    }

    /// The round trip the management surface is built on: what listing prints
    /// is what revocation accepts back, for every entry — including one whose
    /// path is gone and one whose path is not valid UTF-8 (ADR-0003).
    #[test]
    fn every_listed_spelling_reads_back_as_the_root_it_names() {
        let config = approved([
            identity("/home/alice/dotfiles"),
            non_unicode_identity(b"\x80dots"),
        ]);

        let listing = list_roots(&config, &FakeProbe::default()).unwrap();

        for entry in &listing.entries {
            assert_eq!(
                RootIdentity::parse(&entry.spelling()).unwrap(),
                entry.identity,
                "`{}` did not read back as the root it names",
                entry.spelling()
            );
        }
    }

    /// Two roots that render identically must stay two lines a user can tell
    /// apart, or revoking one of them is guesswork.
    #[test]
    fn lossy_colliding_roots_list_as_two_distinguishable_entries() {
        let one = non_unicode_identity(b"\x80");
        let other = non_unicode_identity(b"\x81");
        assert_eq!(
            one.as_path().to_string_lossy(),
            other.as_path().to_string_lossy(),
            "test premise: these two roots render identically when lossy"
        );

        let listing = list_roots(&approved([one, other]), &FakeProbe::default()).unwrap();

        let spellings: Vec<String> = listing
            .entries
            .iter()
            .map(TrustedRootEntry::spelling)
            .collect();
        assert_eq!(
            spellings,
            vec!["os-bytes:2f746d702f80", "os-bytes:2f746d702f81"]
        );
    }

    /// `roots list` is the surface that has to *report* a broken trust file:
    /// showing it as "nothing approved" would tell a user their roots were
    /// forgotten when they were merely unreadable.
    #[test]
    fn an_unusable_collection_is_reported_rather_than_listed_empty() {
        let root = identity("/home/alice/dotfiles");

        let error = list_roots(&approved([root.clone(), root]), &FakeProbe::default()).unwrap_err();

        assert!(
            matches!(error, SafetyLockError::DuplicateApprovedRoot { .. }),
            "unexpected error: {error}"
        );
    }

    /// Existence against the real filesystem, which is where the flag is
    /// actually read: an approved root that is still there, and one whose
    /// directory has since been removed.
    #[test]
    fn existence_reflects_the_real_filesystem() {
        let home = tempfile::tempdir().unwrap();
        let present = std::fs::canonicalize(home.path()).unwrap().join("dotfiles");
        std::fs::create_dir(&present).unwrap();
        let removed = present.parent().unwrap().join("gone");

        let config = approved([
            RootIdentity::new(&present).unwrap(),
            RootIdentity::new(&removed).unwrap(),
        ]);

        let listing = list_roots(&config, &OsPathProbe).unwrap();

        assert!(listing.entries[0].exists);
        assert!(!listing.entries[1].exists);
        assert_eq!(listing.entries[1].identity.as_path(), removed);
    }

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

//! Revoking an approval — the reversing half of root management
//! (`dodot roots forget <path>`).
//!
//! Two matching rules, in order, so moving or deleting a root cannot strand
//! its record (ADR-0003):
//!
//! 1. an argument that still exists is canonicalized first, so any alias of
//!    the root selects the same approval;
//! 2. otherwise the argument names the approval by its own spelling — the one
//!    `roots list` printed.
//!
//! Rule 2 is the fallback for *every* way rule 1 can come up empty, not only
//! for a deleted path. A root that was replaced at the same spelling — the
//! directory removed and a symlink to somewhere else put in its place —
//! canonicalizes to a path the collection never held, and if that were the
//! whole rule its record would be unreachable by any argument the user could
//! type. Falling back cannot over-authorize either: revocation only ever
//! removes trust, so the worst a match too many costs is one more
//! confirmation.
//!
//! Like approval, revocation returns the trust state to persist rather than
//! writing it.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use super::error::{Result, SafetyLockError};
use super::roots::RootIdentity;
use super::schema::SafetyLockConfig;
use super::util::{decode_native_path, encode_native_path, PathProbe};

/// What the user asked to forget, exactly as they spelled it.
///
/// An [`OsString`] because the argument may be a non-Unicode path or the
/// `os-bytes:` spelling of one; both must survive to the matching rules
/// untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgetRequest {
    pub argument: OsString,
}

impl ForgetRequest {
    pub fn new(argument: impl Into<OsString>) -> Self {
        Self {
            argument: argument.into(),
        }
    }
}

/// The trust state as it would be after revocation, plus what was removed.
#[derive(Debug, Clone)]
pub struct ForgetChange {
    /// The trust state to persist.
    pub config: SafetyLockConfig,
    /// The approval that was removed, or `None` when the argument matched no
    /// approved root — a distinction the caller reports rather than treating
    /// as failure.
    pub removed: Option<RootIdentity>,
}

impl ForgetChange {
    /// Whether anything was actually removed.
    pub fn changed(&self) -> bool {
        self.removed.is_some()
    }
}

/// Remove the approval `request` names, if the trust state holds it.
///
/// The argument is read as either spelling [`roots list`](super::list) prints
/// — a plain path, or a tagged `os-bytes:` one — and then matched by the two
/// rules this module documents: the canonical identity when the path resolves,
/// the argument's own identity otherwise.
///
/// An argument that matches nothing is not a failure: `removed` is `None` and
/// the returned state is the one passed in, so a caller can tell "no such
/// approval" from "the trust file is broken" and say so.
///
/// Fails when the collection is unusable, and when the argument cannot name a
/// root at all: a malformed `os-bytes:` spelling, a relative path, or one
/// carrying `..`. A relative argument is refused rather than resolved, because
/// resolving it would mean reading the process working directory — the ambient
/// state Safety Lock captures once at the boundary and never re-reads
/// (ADR-0001). Callers anchor a relative argument to their captured invocation
/// directory before asking.
pub fn forget_root(
    config: &SafetyLockConfig,
    request: &ForgetRequest,
    probe: &dyn PathProbe,
) -> Result<ForgetChange> {
    config.validate()?;

    let candidate = requested_path(&request.argument)?;
    if !candidate.is_absolute() {
        return Err(SafetyLockError::RelativeRootIdentity {
            spelling: encode_native_path(&candidate),
        });
    }

    // Rule 1: an argument the filesystem can resolve names the root it
    // resolves to, so any alias of a live root selects its approval.
    let resolved = match probe.canonicalize(&candidate) {
        Ok(canonical) => Some(RootIdentity::new(canonical)?),
        Err(_) => None,
    };

    // Rule 2: otherwise — the path is gone, or resolves somewhere the
    // collection never held — the argument names the approval by its own
    // spelling.
    let target = match resolved {
        Some(identity) if config.is_approved(&identity) => identity,
        _ => RootIdentity::new(candidate)?,
    };

    let mut config = config.clone();
    let removed = if config.is_approved(&target) {
        config.roots.approved.retain(|held| *held != target);
        Some(target)
    } else {
        None
    };

    Ok(ForgetChange { config, removed })
}

/// Read the argument as the path it names: a tagged spelling decodes back to
/// its native bytes, anything else is the path itself.
///
/// A non-Unicode argument cannot carry the tag — the tag is ASCII — so it is
/// taken verbatim, which is how a root typed at the shell and the same root
/// copied out of `roots list` reach the matching rules as one path.
fn requested_path(argument: &OsStr) -> Result<PathBuf> {
    match argument.to_str() {
        Some(text) => decode_native_path(text),
        None => Ok(PathBuf::from(argument)),
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::ffi::OsStringExt;

    use super::super::schema::TrustedRootsSection;
    use super::super::test_probe::{Entry, FakeProbe};
    use super::super::util::OsPathProbe;
    use super::*;

    fn identity(path: &str) -> RootIdentity {
        RootIdentity::new(path).unwrap()
    }

    fn non_unicode_path(suffix: &[u8]) -> PathBuf {
        let mut bytes = b"/tmp/".to_vec();
        bytes.extend_from_slice(suffix);
        PathBuf::from(OsString::from_vec(bytes))
    }

    fn non_unicode_identity(suffix: &[u8]) -> RootIdentity {
        RootIdentity::new(non_unicode_path(suffix)).unwrap()
    }

    fn approved(roots: impl IntoIterator<Item = RootIdentity>) -> SafetyLockConfig {
        SafetyLockConfig {
            roots: TrustedRootsSection {
                approved: roots.into_iter().collect(),
            },
        }
    }

    fn forget(
        config: &SafetyLockConfig,
        argument: impl Into<OsString>,
        probe: &dyn PathProbe,
    ) -> Result<ForgetChange> {
        forget_root(config, &ForgetRequest::new(argument), probe)
    }

    /// Rule 1: a live root is canonicalized first, so every alias of it — a
    /// spelling variant or a symlink — selects the one approval it has.
    #[test]
    fn an_alias_of_a_live_root_selects_its_approval() {
        let config = approved([identity("/srv/dots"), identity("/srv/other")]);

        for (argument, probe) in [
            ("/srv/dots", FakeProbe::dir("/srv/dots")),
            ("/srv/dots/", FakeProbe::dir("/srv/dots")),
            ("/srv//dots", FakeProbe::dir("/srv/dots")),
            ("/srv/./dots", FakeProbe::dir("/srv/dots")),
            (
                "/srv/link",
                FakeProbe::default().link("/srv/link", "/srv/dots"),
            ),
        ] {
            let change = forget(&config, argument, &probe).unwrap();

            assert!(change.changed(), "`{argument}` matched no approval");
            assert_eq!(change.removed.unwrap(), identity("/srv/dots"));
            assert_eq!(
                change.config.roots.approved,
                vec![identity("/srv/other")],
                "`{argument}` disturbed an unrelated approval"
            );
            assert!(change.config.validate().is_ok());
        }
    }

    /// Rule 2, the deleted case: what `roots list` printed is what revocation
    /// accepts back, for a plain path and for a tagged non-Unicode one alike
    /// (ADR-0003).
    #[test]
    fn a_deleted_root_is_forgotten_by_its_exact_stored_spelling() {
        let gone = identity("/srv/deleted");
        let gone_non_unicode = non_unicode_identity(b"\x80dots");
        let config = approved([gone.clone(), gone_non_unicode.clone()]);
        let probe = FakeProbe::default();

        for expected in [gone, gone_non_unicode.clone()] {
            let change = forget(&config, expected.spelling(), &probe).unwrap();

            assert_eq!(change.removed.as_ref(), Some(&expected));
            assert_eq!(change.config.roots.approved.len(), 1);
        }

        // The same root typed as its native bytes rather than as the tagged
        // spelling: one path, one approval, either way in.
        let change = forget(&config, non_unicode_path(b"\x80dots"), &probe).unwrap();
        assert_eq!(change.removed, Some(gone_non_unicode));
    }

    /// Story 6 from the revocation side: a moved root leaves a record at the
    /// old path, and that record is exactly what the old spelling removes —
    /// the new location's approval is untouched.
    #[test]
    fn moving_a_root_leaves_the_old_approval_revocable() {
        let old = identity("/srv/dots");
        let new = identity("/srv/moved/dots");
        let config = approved([old.clone(), new.clone()]);
        let probe = FakeProbe::dir("/srv/moved/dots");

        let change = forget(&config, old.spelling(), &probe).unwrap();

        assert_eq!(change.removed, Some(old));
        assert_eq!(change.config.roots.approved, vec![new]);
    }

    /// The case rule 2 exists for beyond deletion: the approved directory was
    /// replaced at the same spelling, so canonicalization now lands on a path
    /// the collection never held. Without the fallback that record would be
    /// unreachable by any argument the user could type.
    #[test]
    fn a_root_replaced_at_its_own_spelling_is_still_revocable() {
        let replaced = identity("/srv/dots");
        let config = approved([replaced.clone()]);
        let probe = FakeProbe::default().link("/srv/dots", "/srv/elsewhere");

        let change = forget(&config, "/srv/dots", &probe).unwrap();

        assert_eq!(change.removed, Some(replaced));
        assert!(change.config.roots.approved.is_empty());
    }

    /// Rule 1 wins while it matches: when the replacement's own target is
    /// approved too, the live root the argument resolves to is the one
    /// revoked, because that is the root the argument names *now*.
    #[test]
    fn the_canonical_rule_is_tried_before_the_stored_spelling() {
        let config = approved([identity("/srv/dots"), identity("/srv/elsewhere")]);
        let probe = FakeProbe::default().link("/srv/dots", "/srv/elsewhere");

        let change = forget(&config, "/srv/dots", &probe).unwrap();

        assert_eq!(change.removed, Some(identity("/srv/elsewhere")));
        assert_eq!(change.config.roots.approved, vec![identity("/srv/dots")]);
    }

    /// Two roots whose lossy renderings collide are revoked one at a time: a
    /// match key built from a rendering would remove whichever came first.
    #[test]
    fn forgetting_one_lossy_colliding_root_leaves_the_other() {
        let one = non_unicode_identity(b"\x80");
        let other = non_unicode_identity(b"\x81");
        assert_eq!(
            one.as_path().to_string_lossy(),
            other.as_path().to_string_lossy(),
            "test premise: these two roots render identically when lossy"
        );

        let change = forget(
            &approved([one.clone(), other.clone()]),
            one.spelling(),
            &FakeProbe::default(),
        )
        .unwrap();

        assert_eq!(change.removed, Some(one));
        assert_eq!(change.config.roots.approved, vec![other]);
    }

    /// Revocation is idempotent in the only way it can be: forgetting an
    /// approval that is not held is reported, not an error, and forgetting the
    /// same root twice reports the second attempt as a no-op.
    #[test]
    fn an_argument_that_matches_nothing_is_reported_not_failed() {
        let config = approved([identity("/srv/dots")]);
        let probe = FakeProbe::dir("/srv/unapproved");

        let change = forget(&config, "/srv/unapproved", &probe).unwrap();

        assert!(!change.changed());
        assert_eq!(change.config.roots.approved, config.roots.approved);

        let removed = forget(&config, "/srv/dots", &FakeProbe::default()).unwrap();
        let again = forget(&removed.config, "/srv/dots", &FakeProbe::default()).unwrap();
        assert!(!again.changed());
    }

    /// An argument that cannot name a root is a mistake worth naming: a
    /// relative path has no fixed meaning, a `..` spelling is an alias the
    /// collection never stores, and a truncated tagged spelling is not a path
    /// at all. None of them may silently read as "no such approval".
    #[test]
    fn an_argument_that_cannot_name_a_root_says_so() {
        let config = approved([identity("/srv/dots")]);
        let probe = FakeProbe::dir("/srv/dots");

        let error = forget(&config, "dots", &probe).unwrap_err();
        assert!(
            matches!(error, SafetyLockError::RelativeRootIdentity { ref spelling } if spelling == "dots"),
            "unexpected error: {error}"
        );
        assert!(
            probe.canonicalized().is_empty(),
            "a relative argument reached the filesystem probe"
        );

        let error = forget(&config, "/srv/dots/../other", &probe).unwrap_err();
        assert!(
            matches!(error, SafetyLockError::NonCanonicalRootIdentity { .. }),
            "unexpected error: {error}"
        );

        let error = forget(&config, "os-bytes:2f7", &probe).unwrap_err();
        assert!(
            matches!(error, SafetyLockError::UnreadableSpelling { .. }),
            "unexpected error: {error}"
        );
    }

    /// Revocation rewrites the collection, so it must refuse to start from one
    /// it does not understand: writing back a state derived from a file Dodot
    /// could not read would drop approvals it never saw.
    #[test]
    fn an_unusable_collection_is_reported_rather_than_rewritten() {
        let root = identity("/srv/dots");

        let error = forget(
            &approved([root.clone(), root]),
            "/srv/dots",
            &FakeProbe::default(),
        )
        .unwrap_err();

        assert!(
            matches!(error, SafetyLockError::DuplicateApprovedRoot { .. }),
            "unexpected error: {error}"
        );
    }

    /// A directory that exists but cannot be read is still a live root: rule 1
    /// only needs the path to resolve, so an unreadable root is revocable
    /// through its aliases like any other.
    #[test]
    fn an_unreadable_but_present_root_still_canonicalizes() {
        let config = approved([identity("/srv/dots")]);
        let probe = FakeProbe::default()
            .link("/srv/link", "/srv/dots")
            .add("/srv/dots", Entry::UnreadableDir);

        let change = forget(&config, "/srv/link", &probe).unwrap();

        assert_eq!(change.removed, Some(identity("/srv/dots")));
    }

    /// The alias rule against a real filesystem: a symlink and a `..`-free
    /// spelling variant both revoke the approval recorded at the canonical
    /// path.
    #[test]
    fn aliases_resolve_against_a_real_filesystem() {
        let home = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(home.path()).unwrap();
        let root = home.join("dotfiles");
        std::fs::create_dir(&root).unwrap();
        let link = home.join("link-to-dotfiles");
        std::os::unix::fs::symlink(&root, &link).unwrap();

        let config = approved([RootIdentity::new(&root).unwrap()]);

        for argument in [link.clone(), root.join("")] {
            let change = forget(&config, argument.clone(), &OsPathProbe).unwrap();
            assert_eq!(
                change.removed.as_ref().map(RootIdentity::as_path),
                Some(root.as_path()),
                "`{}` did not select the approval at the canonical path",
                argument.display()
            );
        }

        // And the deleted case, on the same filesystem: once the directory is
        // gone, the stored spelling is what removes the record.
        std::fs::remove_dir(&root).unwrap();
        let change = forget(&config, root.as_os_str(), &OsPathProbe).unwrap();
        assert_eq!(
            change.removed.map(|identity| identity.as_path().to_owned()),
            Some(root)
        );
    }

    #[test]
    fn a_request_keeps_the_argument_bytes_it_was_given() {
        let raw = OsString::from_vec(b"/tmp/\x80dots".to_vec());
        assert_eq!(ForgetRequest::new(raw.clone()).argument, raw);
        assert_eq!(
            ForgetRequest::new("os-bytes:2f746d702f80646f7473").argument,
            OsString::from("os-bytes:2f746d702f80646f7473")
        );
    }

    #[test]
    fn a_change_reports_whether_it_removed_anything() {
        let unchanged = ForgetChange {
            config: SafetyLockConfig::default(),
            removed: None,
        };
        let removed = ForgetChange {
            config: SafetyLockConfig::default(),
            removed: Some(RootIdentity::new("/home/alice/dotfiles").unwrap()),
        };

        assert!(!unchanged.changed());
        assert!(removed.changed());
    }
}

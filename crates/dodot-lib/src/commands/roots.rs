//! `dodot roots` — inspect and revoke approved dotfiles roots.
//!
//! The management surface Safety Lock exposes instead of a trust verb: a user
//! approves an implicit root only through the confirmation gate, and
//! automation expresses deliberate selection with `DOTFILES_ROOT`
//! (`docs/adr/0003-inspect-and-revoke-roots-without-a-trust-command.md`).
//! What is left is seeing what was approved and taking it back.
//!
//! This module is the presentation seam and nothing else. The decisions live
//! in [`list_roots`] and [`forget_root`]; here they become the flat,
//! `Serialize` view DTOs the renderer and the JSON/YAML modes share, so the
//! domain types never grow a rendering (`docs/dev/cli-output.lex`).
//!
//! Both entry points take the data directory rather than reading one, and
//! neither resolves a dotfiles root: revoking an approval is not a
//! root-sensitive operation, and a root that no longer resolves is exactly
//! the case `roots forget` exists for.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::commands::MessageResult;
use crate::safety_lock::{
    encode_native_path, forget_root, list_roots, ForgetRequest, PathProbe, SafetyLockConfig,
    SafetyLockError, SafetyLockResult, TrustFileTransaction, NATIVE_BYTES_TAG,
};

/// One approved root as `dodot roots list` presents it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrustedRootRow {
    /// The reversible spelling of the approved root — the exact string
    /// `dodot roots forget` accepts back, including for a root that has since
    /// been moved or deleted.
    pub path: String,
    /// Whether that path is still a directory Dodot could use as a root.
    ///
    /// Informational only: an approval outlives the directory it names, and
    /// is removed by `roots forget` or `dodot reset`, never by a disappearing
    /// root.
    pub exists: bool,
}

/// The `dodot roots list` view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RootsListResult {
    /// The approved roots, in stored order. Nothing here implies precedence:
    /// the collection has no preferred entry.
    pub roots: Vec<TrustedRootRow>,
    /// The trust file these came from, so a user reading a surprising listing
    /// knows which file to look at.
    pub state_path: String,
}

/// `dodot roots list` — show every approved root.
///
/// Fails when the trust file is unusable, rather than showing it as empty:
/// surfacing a broken trust file is this command's half of the Spec's
/// recovery story, and an empty listing would read as "nothing approved"
/// (Spec, "Proposed Shape").
pub fn list(data_dir: &Path, probe: &dyn PathProbe) -> SafetyLockResult<RootsListResult> {
    let config = SafetyLockConfig::load_from(data_dir)?;
    let listing = list_roots(&config, probe)?;

    Ok(RootsListResult {
        roots: listing
            .entries
            .iter()
            .map(|entry| TrustedRootRow {
                path: entry.spelling(),
                exists: entry.exists,
            })
            .collect(),
        state_path: SafetyLockConfig::path_in(data_dir).display().to_string(),
    })
}

/// `dodot roots forget <path>` — revoke one approval.
///
/// `argument` is taken as the user spelled it, so a non-Unicode path and the
/// `os-bytes:` spelling `roots list` prints both survive to the matching
/// rules. A relative argument is anchored to `invocation_dir` here — the
/// invocation directory captured once at the process boundary — because
/// [`forget_root`] refuses to read ambient process state of its own. The
/// directory is optional because the process may no longer have one (deleted
/// underneath the shell); only a relative argument needs it, so absolute and
/// tagged spellings keep working from exactly that broken state.
///
/// An argument that matches no approval is reported, not failed: "no such
/// approval" and "the trust file is broken" are different answers and the
/// user needs to be able to tell them apart.
///
/// Loads through the transaction's
/// [`load_for_revocation`](TrustFileTransaction::load_for_revocation), which
/// is what makes this the narrow recovery route for a trust file listing one
/// root twice — the one invariant violation a readable file can carry, and
/// one whose only other exit would be the factory reset that drops every
/// *other* approval with it. The whole read-remove-write runs inside one
/// [`TrustFileTransaction`], so a revocation racing a concurrent approval
/// composes with it instead of overwriting it.
pub fn forget(
    data_dir: &Path,
    invocation_dir: Option<&Path>,
    argument: &OsStr,
    probe: &dyn PathProbe,
) -> SafetyLockResult<MessageResult> {
    let anchored = anchor(invocation_dir, argument)?;
    let transaction = TrustFileTransaction::begin(data_dir)?;
    let config = transaction.load_for_revocation()?;
    let change = forget_root(&config, &ForgetRequest::new(anchored), probe)?;

    let Some(removed) = change.removed.as_ref() else {
        return Ok(MessageResult {
            message: format!("No approved root matches `{}`.", argument.to_string_lossy()),
            details: vec!["Run `dodot roots list` to see the approved roots.".into()],
        });
    };

    let spelling = removed.spelling();
    transaction.persist(&change.config)?;

    Ok(MessageResult {
        message: format!("Forgot {spelling}."),
        details: vec![
            "The next root-sensitive command run from that root asks for confirmation again."
                .into(),
        ],
    })
}

/// Resolve `argument` against the captured invocation directory when it is a
/// relative path.
///
/// Two spellings are left alone. An absolute path is already anchored, and a
/// tagged `os-bytes:` spelling is not a path at all — it is the encoding
/// `roots list` prints for a non-Unicode root, and joining it onto a directory
/// would turn the one string a user can copy back into a name matching
/// nothing. Neither consults `invocation_dir`, so `None` — a working
/// directory that no longer exists — fails only the one spelling that
/// genuinely cannot be resolved without it.
///
/// Joining is all that happens to what remains — no canonicalization, no
/// symlink resolution. Those belong to [`forget_root`]'s first matching rule,
/// which consults the filesystem itself; doing them here would decide the
/// match before the rules that own it ran.
fn anchor(invocation_dir: Option<&Path>, argument: &OsStr) -> SafetyLockResult<OsString> {
    if argument
        .as_encoded_bytes()
        .starts_with(NATIVE_BYTES_TAG.as_bytes())
    {
        return Ok(argument.to_os_string());
    }

    let candidate = PathBuf::from(argument);
    if candidate.is_absolute() {
        Ok(candidate.into_os_string())
    } else {
        let anchor_dir =
            invocation_dir.ok_or_else(|| SafetyLockError::RelativeArgumentUnanchorable {
                spelling: encode_native_path(&candidate),
            })?;
        Ok(anchor_dir.join(candidate).into_os_string())
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::ffi::OsStrExt;

    use crate::safety_lock::{OsPathProbe, RootIdentity, SafetyLockError, TrustedRootsSection};

    use super::*;

    fn approved(data_dir: &Path, paths: &[&Path]) {
        TrustFileTransaction::begin(data_dir)
            .unwrap()
            .persist(&SafetyLockConfig {
                roots: TrustedRootsSection {
                    approved: paths
                        .iter()
                        .map(|path| RootIdentity::new(*path).unwrap())
                        .collect(),
                },
            })
            .unwrap();
    }

    #[test]
    fn an_absent_trust_file_lists_as_no_approvals() {
        let data_dir = tempfile::tempdir().unwrap();

        let result = list(data_dir.path(), &OsPathProbe).unwrap();

        assert!(result.roots.is_empty());
        assert!(result.state_path.ends_with("safety-lock.toml"));
    }

    #[test]
    fn listing_reports_whether_each_approved_root_still_exists() {
        let data_dir = tempfile::tempdir().unwrap();
        let live = tempfile::tempdir().unwrap();
        let live_path = std::fs::canonicalize(live.path()).unwrap();
        let gone = live_path.join("moved-away");
        approved(data_dir.path(), &[&live_path, &gone]);

        let result = list(data_dir.path(), &OsPathProbe).unwrap();

        assert_eq!(
            result.roots,
            vec![
                TrustedRootRow {
                    path: live_path.display().to_string(),
                    exists: true,
                },
                TrustedRootRow {
                    path: gone.display().to_string(),
                    exists: false,
                },
            ]
        );
    }

    /// A broken trust file has to be visible somewhere, and `roots list` is
    /// the surface the Spec names — showing it as empty would read as "you
    /// approved nothing".
    #[test]
    fn listing_surfaces_an_unusable_trust_file_instead_of_showing_it_empty() {
        let data_dir = tempfile::tempdir().unwrap();
        std::fs::write(
            SafetyLockConfig::path_in(data_dir.path()),
            "[roots]\napproved = [\"relative/dotfiles\"]\n",
        )
        .unwrap();

        let err = list(data_dir.path(), &OsPathProbe).unwrap_err();

        assert!(
            matches!(err, SafetyLockError::TrustStateUnusable { .. }),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn forgetting_removes_the_approval_and_leaves_the_others() {
        let data_dir = tempfile::tempdir().unwrap();
        let one = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let one_path = std::fs::canonicalize(one.path()).unwrap();
        let other_path = std::fs::canonicalize(other.path()).unwrap();
        approved(data_dir.path(), &[&one_path, &other_path]);

        let result = forget(
            data_dir.path(),
            Some(Path::new("/")),
            one_path.as_os_str(),
            &OsPathProbe,
        )
        .unwrap();

        assert!(result.message.contains(&one_path.display().to_string()));
        assert_eq!(
            list(data_dir.path(), &OsPathProbe).unwrap().roots,
            vec![TrustedRootRow {
                path: other_path.display().to_string(),
                exists: true,
            }]
        );
    }

    /// The revocation the ADR promises for a root that has been deleted:
    /// pass back exactly what `roots list` printed.
    #[test]
    fn a_root_that_no_longer_exists_is_still_revocable_by_its_printed_spelling() {
        let data_dir = tempfile::tempdir().unwrap();
        let gone = PathBuf::from("/nonexistent/dotfiles");
        approved(data_dir.path(), &[&gone]);
        let printed = list(data_dir.path(), &OsPathProbe).unwrap().roots[0]
            .path
            .clone();

        forget(
            data_dir.path(),
            Some(Path::new("/")),
            OsStr::new(&printed),
            &OsPathProbe,
        )
        .unwrap();

        assert!(list(data_dir.path(), &OsPathProbe)
            .unwrap()
            .roots
            .is_empty());
    }

    /// A relative argument is anchored to the *captured* invocation
    /// directory, not to whatever the process's working directory happens to
    /// be when the revocation runs.
    #[test]
    fn a_relative_argument_is_anchored_to_the_captured_invocation_directory() {
        let data_dir = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(home.path()).unwrap().join("dotfiles");
        std::fs::create_dir(&root).unwrap();
        approved(data_dir.path(), &[&root]);

        let invoked_from = std::fs::canonicalize(home.path()).unwrap();
        forget(
            data_dir.path(),
            Some(invoked_from.as_path()),
            OsStr::new("dotfiles"),
            &OsPathProbe,
        )
        .unwrap();

        assert!(list(data_dir.path(), &OsPathProbe)
            .unwrap()
            .roots
            .is_empty());
    }

    /// The recovery surface must survive the broken state it exists for: a
    /// working directory deleted underneath the shell yields no invocation
    /// directory, and an absolute argument never needs one.
    #[test]
    fn an_absolute_argument_revokes_without_an_invocation_directory() {
        let data_dir = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let root_path = std::fs::canonicalize(root.path()).unwrap();
        approved(data_dir.path(), &[&root_path]);

        forget(data_dir.path(), None, root_path.as_os_str(), &OsPathProbe).unwrap();

        assert!(list(data_dir.path(), &OsPathProbe)
            .unwrap()
            .roots
            .is_empty());
    }

    /// A relative argument is the one spelling that genuinely cannot be
    /// resolved without an invocation directory; the error must say so and
    /// name the spellings that still work.
    #[test]
    fn a_relative_argument_without_an_invocation_directory_fails_with_the_recovery_route() {
        let data_dir = tempfile::tempdir().unwrap();

        let err = forget(data_dir.path(), None, OsStr::new("dotfiles"), &OsPathProbe).unwrap_err();

        assert!(
            matches!(err, SafetyLockError::RelativeArgumentUnanchorable { .. }),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn an_argument_matching_nothing_is_reported_rather_than_failed() {
        let data_dir = tempfile::tempdir().unwrap();
        let held = tempfile::tempdir().unwrap();
        let held_path = std::fs::canonicalize(held.path()).unwrap();
        approved(data_dir.path(), &[&held_path]);

        let result = forget(
            data_dir.path(),
            Some(Path::new("/")),
            OsStr::new("/nowhere/at/all"),
            &OsPathProbe,
        )
        .unwrap();

        assert!(result.message.contains("No approved root matches"));
        assert_eq!(list(data_dir.path(), &OsPathProbe).unwrap().roots.len(), 1);
    }

    /// The Spec's narrow recovery: a trust file listing one root twice is
    /// refused by every other consumer, and revocation is the operation that
    /// repairs it.
    #[test]
    fn forgetting_repairs_a_duplicated_approval_that_every_other_route_refuses() {
        let data_dir = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let root_path = std::fs::canonicalize(root.path()).unwrap();
        std::fs::write(
            SafetyLockConfig::path_in(data_dir.path()),
            format!(
                "[roots]\napproved = [\"{0}\", \"{0}\"]\n",
                root_path.display()
            ),
        )
        .unwrap();
        assert!(list(data_dir.path(), &OsPathProbe).is_err());

        forget(
            data_dir.path(),
            Some(Path::new("/")),
            root_path.as_os_str(),
            &OsPathProbe,
        )
        .unwrap();

        assert!(list(data_dir.path(), &OsPathProbe)
            .unwrap()
            .roots
            .is_empty());
    }

    /// Two roots whose lossy renderings collide are two records the user can
    /// tell apart and revoke separately — the whole point of storing the
    /// native spelling.
    #[test]
    fn lossy_colliding_roots_list_and_revoke_as_two_records() {
        let data_dir = tempfile::tempdir().unwrap();
        let one = PathBuf::from(OsStr::from_bytes(b"/nonexistent/\x80"));
        let other = PathBuf::from(OsStr::from_bytes(b"/nonexistent/\x81"));
        approved(data_dir.path(), &[&one, &other]);

        let listed = list(data_dir.path(), &OsPathProbe).unwrap().roots;
        assert_eq!(listed.len(), 2);
        assert_ne!(listed[0].path, listed[1].path);

        forget(
            data_dir.path(),
            Some(Path::new("/")),
            OsStr::new(&listed[0].path),
            &OsPathProbe,
        )
        .unwrap();

        let remaining = list(data_dir.path(), &OsPathProbe).unwrap().roots;
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].path, listed[1].path);
    }
}

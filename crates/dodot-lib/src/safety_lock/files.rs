//! Implicit file-root selection: the Git top-level, then the current
//! directory.
//!
//! This is the convenient discovery path Safety Lock keeps rather than
//! removes (Spec, "Goals") — and precisely the path whose result needs
//! approval before it can drive a root-sensitive mutation, because filesystem
//! shape is not user intent.
//!
//! Like environment selection, the inputs are injected: the current directory
//! and the enclosing Git top-level are captured by the caller, so discovery
//! and provenance are testable without a real repository or a process-wide
//! working directory. The result is the same [`ResolvedRoot`] the environment
//! path produces — only its [`RootSource`] differs.
//!
//! # Two candidates, in order, and no third
//!
//! The Git top-level is preferred because it is the answer that survives
//! navigation: a user in `dots/vim/colors` means the repository, not the
//! directory they happen to stand in. The current directory applies when Git
//! reported no top-level at all.
//!
//! There is no `$HOME/dotfiles` fallback. Dodot's pre-Safety-Lock path
//! resolution ends there when Git finds nothing, which invents a root out of a
//! naming convention: it names a path the user did not select, from an
//! invocation that says nothing about it. Git then the current directory are
//! the two mechanisms the Spec approves, and each produces a path the user was
//! standing in or inside of — a path the confirmation prompt can meaningfully
//! ask them to recognize.
//!
//! A chosen candidate is also never retried against the other one. When Git
//! reported a top-level that turns out to be unusable, selection fails naming
//! it; falling through to the current directory would quietly select a
//! *different* root — a descendant of the one that failed — which is the
//! substitution ADR-0002 rules out for explicit values, for the same reason.

use std::path::{Path, PathBuf};

use super::error::{Result, SafetyLockError};
use super::roots::{ResolvedRoot, RootSource};
use super::util::{canonical_root_identity, encode_native_path, PathProbe, UnusableRoot};

/// The filesystem inputs implicit selection depends on, captured once per
/// invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRootInput {
    /// The directory Dodot was invoked from.
    ///
    /// Expected to be absolute — the caller captures it from the process. A
    /// relative candidate is refused rather than handed to the probe, which
    /// would resolve it against the process working directory this module
    /// deliberately does not read.
    pub current_dir: PathBuf,

    /// The Git top-level enclosing `current_dir`, or `None` when it is not
    /// inside a repository.
    ///
    /// Discovered by the caller, which is what keeps `git` out of this
    /// module and lets tests state "inside a repo" as data. Taken as given:
    /// with nested repositories, the top-level Git reports for `current_dir`
    /// is the innermost one, and selection uses that rather than searching for
    /// an outer repository of its own.
    pub git_top_level: Option<PathBuf>,
}

impl FileRootInput {
    /// An input for a current directory outside any Git repository.
    pub fn outside_repository(current_dir: impl Into<PathBuf>) -> Self {
        Self {
            current_dir: current_dir.into(),
            git_top_level: None,
        }
    }

    /// An input for a current directory inside a Git repository.
    pub fn inside_repository(
        current_dir: impl Into<PathBuf>,
        git_top_level: impl Into<PathBuf>,
    ) -> Self {
        Self {
            current_dir: current_dir.into(),
            git_top_level: Some(git_top_level.into()),
        }
    }

    /// The candidate this input selects, and the mechanism that selected it.
    fn candidate(&self) -> (&Path, RootSource) {
        match self.git_top_level.as_deref() {
            Some(top_level) => (top_level, RootSource::Git),
            None => (self.current_dir.as_path(), RootSource::CurrentDirectory),
        }
    }
}

/// Resolve the implicit root: the Git top-level when there is one, otherwise
/// the current directory, canonicalized once.
///
/// The returned root always carries an implicit [`RootSource`] — `Git` or
/// `CurrentDirectory` — so the safety gate can tell the user which mechanism
/// selected the path it is asking them to approve (Spec, story 2).
///
/// Canonicalization happens here and only here, which is what gives the root
/// its identity: symlink aliases of one directory resolve to a single
/// identity, so approving a root through one spelling covers every other
/// spelling of it. It is also why a *moved* root is reassessed rather than
/// inheriting the old approval (Spec, story 6) — the path it canonicalizes to
/// is a different identity — while a directory replaced at the same canonical
/// path keeps the identity the user approved, which is the path-based trust
/// model ADR-0001 chose deliberately.
///
/// Fails when the selected candidate is not a usable root
/// ([`SafetyLockError::ImplicitRootUnusable`]). Unlike a failing
/// `DOTFILES_ROOT`, this is not a configuration mistake to correct — it is
/// usually a `cd` into a directory that has since gone away — but it fails the
/// same way, without substituting another root.
pub fn resolve_file_root(input: &FileRootInput, probe: &dyn PathProbe) -> Result<ResolvedRoot> {
    let (candidate, selected_by) = input.candidate();

    let unusable = |reason: String| SafetyLockError::ImplicitRootUnusable {
        spelling: encode_native_path(candidate),
        selected_by,
        reason,
    };

    if !candidate.is_absolute() {
        return Err(unusable(
            "it is relative, so anchoring it would depend on the process working \
             directory rather than the captured invocation directory"
                .to_owned(),
        ));
    }

    let identity = canonical_root_identity(candidate, probe).map_err(|failure| match failure {
        UnusableRoot::Missing => unusable("it does not exist".to_owned()),
        UnusableRoot::Unresolvable(err) => unusable(format!("it could not be resolved: {err}")),
        UnusableRoot::NotADirectory => unusable("it is not a directory".to_owned()),
        UnusableRoot::Unreadable => {
            unusable("it is a directory whose contents cannot be read".to_owned())
        }
        UnusableRoot::NotAnIdentity(err) => {
            unusable(format!("it did not resolve to a usable root: {err}"))
        }
    })?;

    Ok(ResolvedRoot::new(identity, selected_by))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::ffi::OsString;
    use std::io;
    use std::os::unix::ffi::OsStringExt;

    use super::super::roots::RootIdentity;
    use super::super::test_probe::{Entry, FakeProbe};
    use super::super::util::OsPathProbe;
    use super::*;

    fn unusable_reason(error: SafetyLockError) -> String {
        match error {
            SafetyLockError::ImplicitRootUnusable { reason, .. } => reason,
            other => panic!("expected an unusable-candidate error, got: {other}"),
        }
    }

    fn selecting_mechanism(error: SafetyLockError) -> RootSource {
        match error {
            SafetyLockError::ImplicitRootUnusable { selected_by, .. } => selected_by,
            other => panic!("expected an unusable-candidate error, got: {other}"),
        }
    }

    #[test]
    fn repository_membership_is_stated_as_data() {
        assert_eq!(
            FileRootInput::outside_repository("/tmp/scratch").git_top_level,
            None
        );
        assert_eq!(
            FileRootInput::inside_repository("/srv/dots/vim", "/srv/dots"),
            FileRootInput {
                current_dir: PathBuf::from("/srv/dots/vim"),
                git_top_level: Some(PathBuf::from("/srv/dots")),
            }
        );
    }

    /// Spec story 2: from a subdirectory, the root is the repository — the
    /// path the user must be shown, precisely because it is *not* the
    /// directory their prompt shows.
    #[test]
    fn a_git_top_level_wins_over_the_directory_the_user_stands_in() {
        let probe = FakeProbe::dir("/srv/dots").and_dir("/srv/dots/vim/colors");
        let input = FileRootInput::inside_repository("/srv/dots/vim/colors", "/srv/dots");

        let root = resolve_file_root(&input, &probe).unwrap();

        assert_eq!(root.as_path(), Path::new("/srv/dots"));
        assert_eq!(root.source(), RootSource::Git);
        assert_eq!(
            probe.canonicalized(),
            vec![PathBuf::from("/srv/dots")],
            "the current directory was probed even though Git had an answer"
        );
    }

    /// Nested repositories: the top-level belongs to the innermost repository
    /// containing the current directory, and selection takes what the caller
    /// captured rather than looking for an outer repository.
    #[test]
    fn a_nested_repository_selects_its_own_top_level() {
        let probe = FakeProbe::dir("/srv/dots").and_dir("/srv/dots/vendor/plugin");
        let input = FileRootInput::inside_repository(
            "/srv/dots/vendor/plugin/lua",
            "/srv/dots/vendor/plugin",
        );

        let root = resolve_file_root(&input, &probe).unwrap();

        assert_eq!(root.as_path(), Path::new("/srv/dots/vendor/plugin"));
        assert_eq!(root.source(), RootSource::Git);
    }

    #[test]
    fn without_a_git_top_level_the_current_directory_is_the_root() {
        let probe = FakeProbe::dir("/tmp/scratch");
        let input = FileRootInput::outside_repository("/tmp/scratch");

        let root = resolve_file_root(&input, &probe).unwrap();

        assert_eq!(root.as_path(), Path::new("/tmp/scratch"));
        assert_eq!(root.source(), RootSource::CurrentDirectory);
    }

    /// The fallback Safety Lock removes. `$HOME/dotfiles` is a naming
    /// convention, not a selection: outside a repository the root is the
    /// directory the user actually invoked Dodot from, even when the
    /// conventional path exists and is a perfectly good dotfiles repository.
    #[test]
    fn a_conventional_home_dotfiles_directory_is_not_an_implicit_fallback() {
        let probe = FakeProbe::dir("/home/alice").and_dir("/home/alice/dotfiles");
        let input = FileRootInput::outside_repository("/home/alice");

        let root = resolve_file_root(&input, &probe).unwrap();

        assert_eq!(root.as_path(), Path::new("/home/alice"));
        assert_eq!(root.source(), RootSource::CurrentDirectory);
        assert_eq!(
            probe.canonicalized(),
            vec![PathBuf::from("/home/alice")],
            "a conventional path was probed as a candidate"
        );
    }

    /// Selection canonicalizes once, and that result is the identity: every
    /// alias of one directory therefore approves, lists, and revokes as one
    /// record (ADR-0001).
    #[test]
    fn aliases_of_one_directory_resolve_to_one_identity() {
        let through_link = resolve_file_root(
            &FileRootInput::inside_repository("/srv/link/vim", "/srv/link"),
            &FakeProbe::default().link("/srv/link", "/srv/dots"),
        )
        .unwrap();
        let direct = resolve_file_root(
            &FileRootInput::inside_repository("/srv/dots/vim", "/srv/dots"),
            &FakeProbe::dir("/srv/dots"),
        )
        .unwrap();

        assert_eq!(through_link.identity(), direct.identity());
        assert_eq!(through_link, direct);
        let identities: HashSet<RootIdentity> = [through_link, direct]
            .into_iter()
            .map(ResolvedRoot::into_identity)
            .collect();
        assert_eq!(identities.len(), 1);
    }

    /// Spec story 6: approval follows the canonical path, so a root that moved
    /// resolves to a different identity and is reassessed, while a directory
    /// replaced at the same canonical path keeps the identity that was
    /// approved (ADR-0001 — path-based trust, deliberately).
    #[test]
    fn moving_a_root_changes_its_identity_and_replacing_it_in_place_does_not() {
        let before = resolve_file_root(
            &FileRootInput::outside_repository("/srv/dots"),
            &FakeProbe::dir("/srv/dots"),
        )
        .unwrap();

        let moved = resolve_file_root(
            &FileRootInput::outside_repository("/srv/moved/dots"),
            &FakeProbe::dir("/srv/moved/dots"),
        )
        .unwrap();
        assert_ne!(before.identity(), moved.identity());

        // Same path, different directory behind it: one identity, so trust
        // established before the replacement still matches.
        let replaced = resolve_file_root(
            &FileRootInput::outside_repository("/srv/dots"),
            &FakeProbe::dir("/srv/dots"),
        )
        .unwrap();
        assert_eq!(before.identity(), replaced.identity());
    }

    /// A symlinked root that is *moved* is reassessed by where it now points,
    /// not by the unchanged spelling that reaches selection.
    #[test]
    fn a_relocated_symlink_target_is_a_new_identity() {
        let before = resolve_file_root(
            &FileRootInput::outside_repository("/srv/link"),
            &FakeProbe::default().link("/srv/link", "/srv/dots"),
        )
        .unwrap();
        let after = resolve_file_root(
            &FileRootInput::outside_repository("/srv/link"),
            &FakeProbe::default().link("/srv/link", "/srv/moved/dots"),
        )
        .unwrap();

        assert_ne!(before.identity(), after.identity());
        assert_eq!(after.as_path(), Path::new("/srv/moved/dots"));
    }

    /// Handing the probe a relative path would resolve it against the process
    /// working directory — the one piece of state selection refuses to read,
    /// because it is resolved once per invocation and injected (ADR-0001).
    #[test]
    fn a_relative_candidate_is_refused_before_it_reaches_the_filesystem() {
        for (input, expected) in [
            (
                FileRootInput::outside_repository("dots"),
                RootSource::CurrentDirectory,
            ),
            (
                FileRootInput::inside_repository("/srv/dots/vim", "../dots"),
                RootSource::Git,
            ),
        ] {
            let probe = FakeProbe::dir("dots").and_dir("../dots");

            let error = resolve_file_root(&input, &probe).unwrap_err();

            let SafetyLockError::ImplicitRootUnusable {
                selected_by,
                reason,
                ..
            } = error
            else {
                panic!("expected an unusable-candidate error");
            };
            assert_eq!(selected_by, expected);
            assert!(reason.contains("it is relative"), "{reason}");
            assert!(
                probe.canonicalized().is_empty(),
                "a relative path reached the filesystem probe"
            );
        }
    }

    /// An absolute current directory still selects normally when the Git
    /// top-level is absent — the relative refusal is about the candidate
    /// actually used, not about every captured value.
    #[test]
    fn a_relative_current_directory_does_not_spoil_a_usable_git_top_level() {
        let probe = FakeProbe::dir("/srv/dots");
        let input = FileRootInput::inside_repository("vim", "/srv/dots");

        let root = resolve_file_root(&input, &probe).unwrap();

        assert_eq!(root.as_path(), Path::new("/srv/dots"));
    }

    /// Each unusable class names what is wrong: a root that vanished, a file,
    /// a permission problem, and an unresolvable path are four situations.
    #[test]
    fn each_unusable_class_produces_its_own_diagnostic() {
        let cases = [
            ("does not exist", FakeProbe::default()),
            (
                "is not a directory",
                FakeProbe::with("/srv/dots", Entry::NotADirectory),
            ),
            (
                "cannot be read",
                FakeProbe::with("/srv/dots", Entry::UnreadableDir),
            ),
            (
                "could not be resolved",
                FakeProbe::with(
                    "/srv/dots",
                    Entry::Unresolvable(io::ErrorKind::PermissionDenied),
                ),
            ),
        ];

        let mut reasons = HashSet::new();
        for (expected, probe) in cases {
            let error = resolve_file_root(&FileRootInput::outside_repository("/srv/dots"), &probe)
                .unwrap_err();
            let reason = unusable_reason(error);
            assert!(reason.contains(expected), "{reason}");
            reasons.insert(reason);
        }
        assert_eq!(reasons.len(), 4, "two classes share one diagnostic");
    }

    /// A Git top-level Dodot cannot use is a failure, not a hint to try the
    /// directory below it: selecting the current directory here would deploy
    /// from a *different* root than the mechanism chose.
    #[test]
    fn an_unusable_git_top_level_never_falls_through_to_the_current_directory() {
        let probe = FakeProbe::dir("/srv/dots/vim");
        let input = FileRootInput::inside_repository("/srv/dots/vim", "/srv/dots");

        let error = resolve_file_root(&input, &probe).unwrap_err();

        assert_eq!(selecting_mechanism(error), RootSource::Git);
        assert_eq!(
            probe.canonicalized(),
            vec![PathBuf::from("/srv/dots")],
            "the current directory was probed as a second candidate"
        );
    }

    /// The diagnostic names the mechanism as well as the path, because the
    /// user's next action differs: a stale Git top-level is a repository
    /// problem, a missing current directory is a shell problem.
    #[test]
    fn the_diagnostic_names_the_mechanism_that_chose_the_path() {
        let git = resolve_file_root(
            &FileRootInput::inside_repository("/srv/dots/vim", "/srv/dots"),
            &FakeProbe::default(),
        )
        .unwrap_err();
        let cwd = resolve_file_root(
            &FileRootInput::outside_repository("/srv/dots"),
            &FakeProbe::default(),
        )
        .unwrap_err();

        assert_eq!(
            git.to_string(),
            "the git top-level `/srv/dots` cannot be used as a dotfiles root: \
             it does not exist"
        );
        assert_eq!(
            cwd.to_string(),
            "the current directory `/srv/dots` cannot be used as a dotfiles root: \
             it does not exist"
        );
    }

    /// A non-Unicode root keeps its native bytes into the identity, and is
    /// named the same reversible way trust state names it (ADR-0001).
    #[test]
    fn a_non_unicode_root_keeps_its_native_form() {
        let native = PathBuf::from(OsString::from_vec(b"/srv/\x80dots".to_vec()));
        let probe = FakeProbe::dir(native.clone());

        let root = resolve_file_root(
            &FileRootInput::inside_repository(native.join("vim"), native.clone()),
            &probe,
        )
        .unwrap();

        assert_eq!(root.as_path(), native);
        assert_eq!(root.identity().spelling(), "os-bytes:2f7372762f80646f7473");
        assert_eq!(
            RootIdentity::parse(&root.identity().spelling()).unwrap(),
            *root.identity()
        );
    }

    /// A non-Unicode candidate that fails is named reversibly too: a lossy
    /// rendering would print a path the user cannot act on.
    #[test]
    fn a_non_unicode_failure_is_named_reversibly() {
        let native = PathBuf::from(OsString::from_vec(b"/srv/\x80dots".to_vec()));

        let error = resolve_file_root(
            &FileRootInput::outside_repository(native),
            &FakeProbe::default(),
        )
        .unwrap_err();

        let SafetyLockError::ImplicitRootUnusable { spelling, .. } = error else {
            panic!("expected an unusable-candidate error");
        };
        assert_eq!(spelling, "os-bytes:2f7372762f80646f7473");
    }

    /// Both implicit mechanisms produce a root that needs approval — that is
    /// the whole point of separating them from `DOTFILES_ROOT`.
    #[test]
    fn every_implicit_root_requires_approval() {
        for input in [
            FileRootInput::inside_repository("/srv/dots/vim", "/srv/dots"),
            FileRootInput::outside_repository("/srv/dots"),
        ] {
            let root = resolve_file_root(&input, &FakeProbe::dir("/srv/dots")).unwrap();

            assert!(root.requires_approval());
            assert!(root.source().is_implicit());
        }
    }

    /// The whole path against a real filesystem, so the injected probe is not
    /// the only thing the behaviour has ever been proven against. A temporary
    /// directory is itself reached through a symlink on macOS (`/var` →
    /// `/private/var`), which makes the canonicalization real rather than
    /// arranged.
    #[test]
    fn a_real_repository_resolves_through_the_os_probe() {
        let home = tempfile::tempdir().unwrap();
        let repository = home.path().join("dotfiles");
        std::fs::create_dir_all(repository.join("vim/colors")).unwrap();
        let canonical = std::fs::canonicalize(&repository).unwrap();

        let from_subdirectory = resolve_file_root(
            &FileRootInput::inside_repository(repository.join("vim/colors"), &repository),
            &OsPathProbe,
        )
        .unwrap();
        assert_eq!(from_subdirectory.as_path(), canonical);
        assert_eq!(from_subdirectory.source(), RootSource::Git);

        // The same repository reached through a symlink is one identity.
        let alias = home.path().join("dots-link");
        std::os::unix::fs::symlink(&repository, &alias).unwrap();
        let through_alias =
            resolve_file_root(&FileRootInput::outside_repository(&alias), &OsPathProbe).unwrap();
        assert_eq!(through_alias.identity(), from_subdirectory.identity());
        assert_eq!(through_alias.source(), RootSource::CurrentDirectory);

        // A file at the same coordinate is refused rather than selected.
        let file = home.path().join("not-a-root");
        std::fs::write(&file, b"").unwrap();
        let reason = unusable_reason(
            resolve_file_root(&FileRootInput::outside_repository(&file), &OsPathProbe).unwrap_err(),
        );
        assert!(reason.contains("is not a directory"), "{reason}");

        // And a root that went away since the shell entered it.
        let removed = home.path().join("gone");
        let reason = unusable_reason(
            resolve_file_root(&FileRootInput::outside_repository(&removed), &OsPathProbe)
                .unwrap_err(),
        );
        assert!(reason.contains("does not exist"), "{reason}");
    }
}

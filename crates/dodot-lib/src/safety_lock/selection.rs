//! One resolution per invocation: the single entry point every root consumer
//! shares.
//!
//! Root selection used to live in more than one place — the library's path
//! resolution, the CLI's own discovery, the tutorial's — each re-reading
//! `DOTFILES_ROOT`, re-running `git rev-parse`, and disagreeing about what
//! happens when none of it works (Spec, "Problem"). [`resolve_root`] is the
//! one that answers the question: the invocation's process facts go in
//! ([`RootSelectionInput`]), one immutable [`ResolvedRoot`] comes out, and
//! every consumer carries that value instead of asking again (ADR-0001).
//!
//! The order is the whole policy, and it is short:
//!
//! 1. a present `DOTFILES_ROOT` — authoritative, so a broken value fails here
//!    rather than selecting something else ([`resolve_environment_root`]);
//! 2. the enclosing Git top-level;
//! 3. the current directory ([`resolve_file_root`]).
//!
//! There is no fourth step. What makes "carried, not rediscovered" real is
//! that this function is a pure function of injected input: it holds no
//! process state to re-read, so a second consultation is not merely
//! discouraged, it is not expressible without capturing the process again —
//! which happens once, at the boundary (WS07).

use std::ffi::OsString;
use std::path::PathBuf;

use super::environment::{resolve_environment_root, EnvironmentRootInput};
use super::error::Result;
use super::files::{resolve_file_root, FileRootInput};
use super::roots::ResolvedRoot;
use super::util::PathProbe;

/// Everything about the invoking process that root selection depends on,
/// captured once.
///
/// The current directory is held once rather than once per selection path, so
/// a relative `DOTFILES_ROOT` and the implicit candidate can never be anchored
/// to two different directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootSelectionInput {
    /// `DOTFILES_ROOT` exactly as the process environment carried it, or
    /// `None` when it is unset.
    pub dotfiles_root: Option<OsString>,

    /// The directory Dodot was invoked from. Expected to be absolute.
    pub current_dir: PathBuf,

    /// The home directory, used to expand a leading `~`.
    pub home_dir: PathBuf,

    /// The Git top-level enclosing `current_dir`, or `None` when it is not
    /// inside a repository.
    pub git_top_level: Option<PathBuf>,
}

impl RootSelectionInput {
    /// An invocation with no `DOTFILES_ROOT` and no enclosing repository —
    /// the case where the current directory is the root.
    pub fn new(current_dir: impl Into<PathBuf>, home_dir: impl Into<PathBuf>) -> Self {
        Self {
            dotfiles_root: None,
            current_dir: current_dir.into(),
            home_dir: home_dir.into(),
            git_top_level: None,
        }
    }

    /// The same invocation with `DOTFILES_ROOT` set to `value`.
    pub fn with_dotfiles_root(mut self, value: impl Into<OsString>) -> Self {
        self.dotfiles_root = Some(value.into());
        self
    }

    /// The same invocation from inside a repository whose top-level is
    /// `git_top_level`.
    pub fn with_git_top_level(mut self, git_top_level: impl Into<PathBuf>) -> Self {
        self.git_top_level = Some(git_top_level.into());
        self
    }

    /// The environment-selection view of this invocation.
    pub fn environment(&self) -> EnvironmentRootInput {
        EnvironmentRootInput {
            raw_value: self.dotfiles_root.clone(),
            current_dir: self.current_dir.clone(),
            home_dir: self.home_dir.clone(),
        }
    }

    /// The implicit-selection view of this invocation.
    pub fn files(&self) -> FileRootInput {
        FileRootInput {
            current_dir: self.current_dir.clone(),
            git_top_level: self.git_top_level.clone(),
        }
    }
}

/// Resolve this invocation's dotfiles root: `DOTFILES_ROOT`, then the Git
/// top-level, then the current directory.
///
/// The result is immutable and self-contained — a canonical
/// [`RootIdentity`](super::roots::RootIdentity) plus the
/// [`RootSource`](super::roots::RootSource) that selected it — so trust
/// lookup, the confirmation prompt, and execution can all carry the same value
/// instead of consulting the environment, Git, or the current directory again
/// (ADR-0001).
///
/// Provenance is the only thing that varies with which mechanism won:
/// `DOTFILES_ROOT` yields a root that needs no approval, while both implicit
/// mechanisms yield one that does.
///
/// Fails when a present `DOTFILES_ROOT` is unusable — which never falls
/// through to implicit selection — or when the implicit candidate is unusable.
pub fn resolve_root(input: &RootSelectionInput, probe: &dyn PathProbe) -> Result<ResolvedRoot> {
    match resolve_environment_root(&input.environment(), probe)? {
        Some(root) => Ok(root),
        None => resolve_file_root(&input.files(), probe),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::error::SafetyLockError;
    use super::super::roots::RootSource;
    use super::super::test_probe::{Entry, FakeProbe};
    use super::super::util::OsPathProbe;
    use super::*;

    const CWD: &str = "/srv/dots/vim";
    const HOME: &str = "/home/alice";

    fn invocation() -> RootSelectionInput {
        RootSelectionInput::new(CWD, HOME)
    }

    #[test]
    fn an_explicit_root_outranks_both_implicit_mechanisms() {
        let probe = FakeProbe::dir("/srv/explicit").and_dir("/srv/dots");
        let input = invocation()
            .with_git_top_level("/srv/dots")
            .with_dotfiles_root("/srv/explicit");

        let root = resolve_root(&input, &probe).unwrap();

        assert_eq!(root.as_path(), Path::new("/srv/explicit"));
        assert_eq!(root.source(), RootSource::Environment);
        assert!(!root.requires_approval());
    }

    #[test]
    fn without_an_explicit_root_the_git_top_level_wins() {
        let probe = FakeProbe::dir("/srv/dots");
        let root = resolve_root(&invocation().with_git_top_level("/srv/dots"), &probe).unwrap();

        assert_eq!(root.as_path(), Path::new("/srv/dots"));
        assert_eq!(root.source(), RootSource::Git);
        assert!(root.requires_approval());
    }

    #[test]
    fn without_git_the_current_directory_is_the_root() {
        let probe = FakeProbe::dir(CWD).and_dir("/home/alice/dotfiles");

        let root = resolve_root(&invocation(), &probe).unwrap();

        assert_eq!(root.as_path(), Path::new(CWD));
        assert_eq!(root.source(), RootSource::CurrentDirectory);
        assert_eq!(
            probe.canonicalized(),
            vec![PathBuf::from(CWD)],
            "the conventional home path was probed as a candidate"
        );
    }

    /// The rule the ordering exists to enforce (Spec story 9, ADR-0002): a
    /// present value is authoritative, so nothing about it being broken may
    /// reach the mechanisms below it.
    #[test]
    fn a_broken_explicit_root_never_reaches_implicit_selection() {
        let probe = FakeProbe::dir("/srv/dots");
        let input = invocation()
            .with_git_top_level("/srv/dots")
            .with_dotfiles_root("/srv/missing");

        let error = resolve_root(&input, &probe).unwrap_err();

        assert!(
            matches!(error, SafetyLockError::EnvironmentRootUnusable { .. }),
            "unexpected error: {error}"
        );
        assert_eq!(
            probe.canonicalized(),
            vec![PathBuf::from("/srv/missing")],
            "an implicit candidate was probed after an explicit value failed"
        );
    }

    /// One invocation, one canonicalization: the value that comes out is what
    /// every later consumer uses, so selection must not leave a second
    /// resolution for anyone to perform (ADR-0001).
    #[test]
    fn one_invocation_canonicalizes_exactly_once() {
        for (input, probe) in [
            (
                invocation().with_dotfiles_root("~/dots"),
                FakeProbe::dir("/home/alice/dots"),
            ),
            (
                invocation().with_git_top_level("/srv/link"),
                FakeProbe::default().link("/srv/link", "/srv/dots"),
            ),
            (invocation(), FakeProbe::dir(CWD)),
        ] {
            resolve_root(&input, &probe).unwrap();

            assert_eq!(
                probe.canonicalized().len(),
                1,
                "resolution asked the filesystem more than once: {:?}",
                probe.canonicalized()
            );
        }
    }

    /// A relative `DOTFILES_ROOT` and an implicit candidate are anchored to
    /// the same captured directory, because there is only one to anchor to.
    #[test]
    fn both_selection_paths_see_one_captured_invocation_directory() {
        let input = invocation().with_dotfiles_root("dots");

        assert_eq!(input.environment().current_dir, input.files().current_dir);

        let probe = FakeProbe::dir("/srv/dots/vim/dots");
        let root = resolve_root(&input, &probe).unwrap();
        assert_eq!(root.as_path(), Path::new("/srv/dots/vim/dots"));
    }

    /// The resolved root is a value: consumers hold it, clone it, and compare
    /// it without any of them being able to reach back at the process.
    #[test]
    fn the_resolved_root_is_carried_rather_than_rediscovered() {
        let probe = FakeProbe::dir("/srv/dots");
        let root = resolve_root(&invocation().with_git_top_level("/srv/dots"), &probe).unwrap();

        let carried: Vec<ResolvedRoot> = (0..3).map(|_| root.clone()).collect();

        assert!(carried.iter().all(|held| *held == root));
        assert_eq!(
            probe.canonicalized().len(),
            1,
            "carrying the root re-consulted the filesystem"
        );
    }

    /// The composition against a real filesystem: an explicit value, a
    /// repository, and a bare directory, all through the same entry point.
    #[test]
    fn the_whole_order_holds_against_a_real_filesystem() {
        let home = tempfile::tempdir().unwrap();
        let repository = home.path().join("dotfiles");
        std::fs::create_dir_all(repository.join("vim")).unwrap();
        let explicit = home.path().join("explicit");
        std::fs::create_dir(&explicit).unwrap();
        let canonical_repository = std::fs::canonicalize(&repository).unwrap();
        let canonical_explicit = std::fs::canonicalize(&explicit).unwrap();

        let inside = RootSelectionInput::new(repository.join("vim"), home.path())
            .with_git_top_level(&repository);

        let root = resolve_root(&inside, &OsPathProbe).unwrap();
        assert_eq!(root.as_path(), canonical_repository);
        assert_eq!(root.source(), RootSource::Git);

        let root = resolve_root(
            &inside.clone().with_dotfiles_root("~/explicit"),
            &OsPathProbe,
        )
        .unwrap();
        assert_eq!(root.as_path(), canonical_explicit);
        assert_eq!(root.source(), RootSource::Environment);

        let outside = RootSelectionInput::new(repository.join("vim"), home.path());
        let root = resolve_root(&outside, &OsPathProbe).unwrap();
        assert_eq!(root.as_path(), canonical_repository.join("vim"));
        assert_eq!(root.source(), RootSource::CurrentDirectory);
    }

    /// Selection reads no process state of its own: every fact it needs is a
    /// field of its input, which is what makes the decision tests above
    /// independent of where the test binary runs.
    #[test]
    fn selection_never_reads_the_process_environment() {
        for source in [
            include_str!("selection.rs"),
            include_str!("files.rs"),
            include_str!("environment.rs"),
        ] {
            // The implementation only: this test names the forbidden calls, so
            // scanning itself would always find them.
            let implementation = source
                .split_once("#[cfg(test)]")
                .expect("this file carries a test module")
                .0;
            let code = implementation
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");

            for forbidden in ["std::env", "env::var", "current_dir()", "Command::new"] {
                assert!(
                    !code.contains(forbidden),
                    "root selection reads process state through `{forbidden}`"
                );
            }
        }
    }

    /// `Entry` exists so unusable candidates are stateable; the ordering tests
    /// above rely on that as much as the failure tests do.
    #[test]
    fn an_unusable_implicit_candidate_fails_the_whole_selection() {
        let probe = FakeProbe::with("/srv/dots", Entry::NotADirectory);
        let error =
            resolve_root(&invocation().with_git_top_level("/srv/dots"), &probe).unwrap_err();

        assert!(
            matches!(error, SafetyLockError::ImplicitRootUnusable { .. }),
            "unexpected error: {error}"
        );
    }
}

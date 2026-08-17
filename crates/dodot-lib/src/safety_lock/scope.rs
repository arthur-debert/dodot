//! Scoping a cache-derived mutation set to the authorized root.
//!
//! `refresh` and `transform check` take their write targets from the shared
//! preprocessor baseline cache, whose stored `source_path` may name a file
//! under a *different* root. Per-root cache namespaces are out of scope, so
//! the gate's "the target is the selected root" invariant is met by scoping
//! the mutation set instead: only baselines whose canonical source path lies
//! inside the authorized root are written, and every other baseline is
//! reported as out-of-root (ADR-0002).
//!
//! Trusting one root therefore cannot authorize a write into another root's
//! user-authored source.
//!
//! # What "inside the root" means
//!
//! [`scope_to_root`] answers it in one pass per candidate: resolve the stored
//! path through the injected [`PathProbe`] — which is where a symlink pointing
//! out of the root stops being invisible — and ask the authorized
//! [`RootIdentity`] whether it contains the *resolved* path. Nothing else is
//! trusted: a candidate is authorized because Dodot placed it inside the root,
//! never because it was in the cache.
//!
//! Every way that can fail is an outcome, not an error ([`OutOfRootReason`]).
//! A source that is missing, stale, or unresolvable cannot be placed inside
//! the root, and a mutation set is exactly the set Dodot could place — so
//! these are reported and skipped rather than aborting a command that has real
//! in-root work to do. That is also the fail-closed direction: every
//! uncertainty ends outside the mutation set.
//!
//! # Order is the caller's index
//!
//! Callers hold per-candidate context the scope knows nothing about — a
//! baseline's pack, handler, and cache filename. [`MutationScope`] therefore
//! keeps one [`ScopeOutcome`] per candidate *in the order the candidates were
//! given*, so a caller zips its own rows back onto the verdicts instead of
//! matching them up by path (two baselines may legitimately share one source
//! path, so a path is not a key).

use std::path::{Component, Path, PathBuf};

use super::roots::RootIdentity;
use super::util::PathProbe;

/// Why a candidate was excluded from the mutation set.
///
/// Every variant is reported, never written: a source path Dodot cannot place
/// inside the authorized root is not authorized by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutOfRootReason {
    /// The canonical source path lies outside the authorized root.
    OutsideRoot,
    /// The source path no longer exists.
    Missing,
    /// The cache entry does not name an absolute source path at all — it is
    /// empty, or relative.
    ///
    /// Distinct from [`Missing`](Self::Missing) because nothing was looked
    /// for: a baseline written before the cache recorded source paths has no
    /// path to resolve, and a relative one has no meaning independent of the
    /// process working directory — the ambient state Safety Lock resolves once
    /// per invocation and never re-reads. The next `dodot up` rewrites such an
    /// entry; until then it is reportable, not actionable.
    Stale,
    /// The source path exists but could not be canonicalized, so it cannot be
    /// placed relative to the root.
    Uncanonicalizable,
}

/// A candidate excluded from the mutation set, with the reason to report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutOfRootTarget {
    /// The candidate's source path as the cache stored it.
    ///
    /// The stored spelling, not a resolved one: it is what the user sees in
    /// their own cache and the only coordinate they can act on. A resolved
    /// path exists only for candidates that resolved at all.
    pub source_path: PathBuf,
    pub reason: OutOfRootReason,
}

/// One candidate's verdict against the authorized root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeOutcome {
    /// Authorized: the candidate's canonical path, which lies inside the
    /// root.
    ///
    /// This is the path to write — not the candidate, which may have reached
    /// it through a symlink or an alias. Writing what was checked is what
    /// keeps the authorized path and the mutated path the same one.
    InRoot(PathBuf),
    /// Excluded: reportable, never written.
    OutOfRoot(OutOfRootTarget),
}

/// The mutation set split against the authorized root: one outcome per
/// candidate, in the order the candidates were given.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MutationScope {
    outcomes: Vec<ScopeOutcome>,
}

impl MutationScope {
    /// Build a scope from per-candidate outcomes already decided.
    pub fn new(outcomes: Vec<ScopeOutcome>) -> Self {
        Self { outcomes }
    }

    /// Every verdict, positionally aligned with the candidates that produced
    /// them — what a caller zips its own rows onto.
    pub fn outcomes(&self) -> &[ScopeOutcome] {
        &self.outcomes
    }

    /// The canonical paths inside the authorized root — the only ones that
    /// may be written.
    pub fn in_root(&self) -> impl Iterator<Item = &Path> {
        self.outcomes.iter().filter_map(|outcome| match outcome {
            ScopeOutcome::InRoot(canonical) => Some(canonical.as_path()),
            ScopeOutcome::OutOfRoot(_) => None,
        })
    }

    /// Everything else, with its reason.
    pub fn out_of_root(&self) -> impl Iterator<Item = &OutOfRootTarget> {
        self.outcomes.iter().filter_map(|outcome| match outcome {
            ScopeOutcome::OutOfRoot(target) => Some(target),
            ScopeOutcome::InRoot(_) => None,
        })
    }

    /// Whether any candidate was excluded, i.e. whether the command has an
    /// out-of-root report to make.
    pub fn has_out_of_root(&self) -> bool {
        self.out_of_root().next().is_some()
    }
}

/// Split `candidates` into the paths `root` authorizes and the ones it does
/// not.
///
/// Infallible by design. Every way a candidate can fail to be placed inside
/// the root is a reportable [`OutOfRootReason`], because a command holding a
/// cache full of other roots' baselines still has its own root's work to do —
/// and because an error return would leave the caller deciding whether to
/// proceed, which is the decision this function exists to make.
///
/// The probe is asked once per candidate, and only for candidates that name
/// an absolute path: a relative one would be resolved against the process
/// working directory, which Safety Lock does not read.
pub fn scope_to_root(
    root: &RootIdentity,
    candidates: &[PathBuf],
    probe: &dyn PathProbe,
) -> MutationScope {
    MutationScope::new(
        candidates
            .iter()
            .map(|candidate| classify(root, candidate, probe))
            .collect(),
    )
}

/// Place one candidate relative to `root`, naming what stopped it when it
/// cannot be placed.
fn classify(root: &RootIdentity, candidate: &Path, probe: &dyn PathProbe) -> ScopeOutcome {
    let excluded = |reason| {
        ScopeOutcome::OutOfRoot(OutOfRootTarget {
            source_path: candidate.to_path_buf(),
            reason,
        })
    };

    // Checked before the probe, so a path with no fixed meaning never reaches
    // the filesystem.
    if !candidate.is_absolute() {
        return excluded(OutOfRootReason::Stale);
    }

    let canonical = match probe.canonicalize(candidate) {
        Ok(canonical) => canonical,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return excluded(OutOfRootReason::Missing)
        }
        Err(_) => return excluded(OutOfRootReason::Uncanonicalizable),
    };

    // `RootIdentity::contains` answers `false` for a non-canonical candidate
    // rather than trusting it, so containment alone would already fail
    // closed. The shape is checked separately anyway so the report says which
    // thing went wrong: "the probe gave back something unplaceable" and "this
    // file belongs to another root" are different facts about the user's
    // machine, and only the second one is about roots.
    if !is_placeable(&canonical) {
        return excluded(OutOfRootReason::Uncanonicalizable);
    }

    if root.contains(&canonical) {
        ScopeOutcome::InRoot(canonical)
    } else {
        excluded(OutOfRootReason::OutsideRoot)
    }
}

/// Whether a resolved path can be placed relative to a root at all: absolute,
/// and free of the `..` components that would let a component-wise prefix
/// test walk back out of the root it appears to be under.
fn is_placeable(canonical: &Path) -> bool {
    canonical.is_absolute()
        && !canonical
            .components()
            .any(|component| component == Component::ParentDir)
}

#[cfg(test)]
mod tests {
    use super::super::test_probe::{Entry, FakeProbe};
    use super::*;

    fn root() -> RootIdentity {
        RootIdentity::new("/srv/dots").unwrap()
    }

    fn scope(candidates: &[&str], probe: &dyn PathProbe) -> MutationScope {
        let candidates: Vec<PathBuf> = candidates.iter().map(PathBuf::from).collect();
        scope_to_root(&root(), &candidates, probe)
    }

    fn reasons(scope: &MutationScope) -> Vec<OutOfRootReason> {
        scope.out_of_root().map(|target| target.reason).collect()
    }

    fn in_root(scope: &MutationScope) -> Vec<PathBuf> {
        scope.in_root().map(Path::to_path_buf).collect()
    }

    #[test]
    fn an_empty_scope_authorizes_and_reports_nothing() {
        let scope = MutationScope::default();

        assert!(scope.in_root().next().is_none());
        assert!(!scope.has_out_of_root());
    }

    #[test]
    fn sources_inside_the_root_are_the_mutation_set() {
        let probe = FakeProbe::default()
            .add("/srv/dots/vim/vimrc", Entry::NotADirectory)
            .add("/srv/dots", Entry::ReadableDir);

        let scope = scope(&["/srv/dots/vim/vimrc", "/srv/dots"], &probe);

        assert_eq!(
            in_root(&scope),
            vec![
                PathBuf::from("/srv/dots/vim/vimrc"),
                PathBuf::from("/srv/dots"),
            ]
        );
        assert!(!scope.has_out_of_root());
    }

    /// The whole point of the exception (ADR-0002): a second root's baseline
    /// sits in the same shared cache, and approving this root must not
    /// authorize a write into that one.
    #[test]
    fn another_roots_source_is_reported_not_written() {
        let probe = FakeProbe::default()
            .add("/srv/dots/vim/vimrc", Entry::NotADirectory)
            .add("/other/dots/vim/vimrc", Entry::NotADirectory);

        let scope = scope(&["/srv/dots/vim/vimrc", "/other/dots/vim/vimrc"], &probe);

        assert_eq!(in_root(&scope), vec![PathBuf::from("/srv/dots/vim/vimrc")]);
        assert_eq!(reasons(&scope), vec![OutOfRootReason::OutsideRoot]);
        assert_eq!(
            scope.out_of_root().next().unwrap().source_path,
            PathBuf::from("/other/dots/vim/vimrc"),
            "the report names the path the cache stored"
        );
    }

    /// A source that *lives* in the root but *resolves* elsewhere is the
    /// escape a path-only check misses: the cache stored an in-root spelling,
    /// and writing it would write through the link into another tree.
    #[test]
    fn a_symlink_out_of_the_root_escapes_the_mutation_set() {
        let probe = FakeProbe::default().link("/srv/dots/vim/vimrc", "/other/dots/vim/vimrc");

        let scope = scope(&["/srv/dots/vim/vimrc"], &probe);

        assert!(in_root(&scope).is_empty());
        assert_eq!(reasons(&scope), vec![OutOfRootReason::OutsideRoot]);
    }

    /// The mutation set is the resolved path, not the stored one — so the
    /// write lands where containment was actually decided.
    #[test]
    fn an_in_root_symlink_is_authorized_at_its_resolved_path() {
        let probe = FakeProbe::default().link("/srv/dots/vim/vimrc", "/srv/dots/shared/vimrc");

        let scope = scope(&["/srv/dots/vim/vimrc"], &probe);

        assert_eq!(
            in_root(&scope),
            vec![PathBuf::from("/srv/dots/shared/vimrc")]
        );
    }

    /// A sibling whose name merely starts with the root's is not a
    /// descendant. A string-prefix test says it is, and would authorize
    /// writes into a neighbouring repository.
    #[test]
    fn prefix_confusable_siblings_are_not_descendants() {
        let probe = FakeProbe::default()
            .add("/srv/dots-backup/vim/vimrc", Entry::NotADirectory)
            .add("/srv/dotsomething", Entry::NotADirectory);

        let scope = scope(&["/srv/dots-backup/vim/vimrc", "/srv/dotsomething"], &probe);

        assert!(in_root(&scope).is_empty());
        assert_eq!(
            reasons(&scope),
            vec![OutOfRootReason::OutsideRoot, OutOfRootReason::OutsideRoot]
        );
    }

    #[test]
    fn a_missing_source_is_reported_as_missing() {
        let probe = FakeProbe::default();

        let scope = scope(&["/srv/dots/vim/vimrc"], &probe);

        assert_eq!(reasons(&scope), vec![OutOfRootReason::Missing]);
    }

    /// A baseline written before the cache recorded source paths stores an
    /// empty one. There is nothing to resolve, so it is stale rather than
    /// missing — and the filesystem is never asked about it.
    #[test]
    fn a_baseline_with_no_source_path_is_stale_and_never_probed() {
        let probe = FakeProbe::default();

        let scope = scope(&[""], &probe);

        assert_eq!(reasons(&scope), vec![OutOfRootReason::Stale]);
        assert!(
            probe.canonicalized().is_empty(),
            "a path with no fixed meaning reached the filesystem probe"
        );
    }

    /// A relative stored path would be resolved against the process working
    /// directory — the ambient state Safety Lock refuses to read — so it is
    /// refused before the probe, like an empty one.
    #[test]
    fn a_relative_source_path_is_stale_and_never_probed() {
        let probe = FakeProbe::default();

        let scope = scope(&["vim/vimrc"], &probe);

        assert_eq!(reasons(&scope), vec![OutOfRootReason::Stale]);
        assert!(probe.canonicalized().is_empty());
    }

    #[test]
    fn a_source_that_cannot_be_resolved_is_reported_as_uncanonicalizable() {
        let probe = FakeProbe::default().add(
            "/srv/dots/vim/vimrc",
            Entry::Unresolvable(std::io::ErrorKind::PermissionDenied),
        );

        let scope = scope(&["/srv/dots/vim/vimrc"], &probe);

        assert_eq!(reasons(&scope), vec![OutOfRootReason::Uncanonicalizable]);
    }

    /// A resolved path that still carries `..` cannot be placed: it walks
    /// back out of whatever it appears to be under. It is refused as
    /// unresolvable rather than silently accepted by a prefix test.
    #[test]
    fn a_resolution_that_is_not_placeable_is_refused() {
        let probe = FakeProbe::default().link("/srv/dots/vim/vimrc", "/srv/dots/../etc/passwd");

        let scope = scope(&["/srv/dots/vim/vimrc"], &probe);

        assert!(in_root(&scope).is_empty());
        assert_eq!(reasons(&scope), vec![OutOfRootReason::Uncanonicalizable]);
    }

    /// The caller's index: verdicts stay positionally aligned with the
    /// candidates, so a baseline's pack and handler can be zipped back on.
    #[test]
    fn outcomes_stay_aligned_with_the_candidates() {
        let probe = FakeProbe::default()
            .add("/srv/dots/a", Entry::NotADirectory)
            .add("/other/dots/b", Entry::NotADirectory)
            .add("/srv/dots/c", Entry::NotADirectory);

        let scope = scope(
            &["/srv/dots/a", "/other/dots/b", "/srv/dots/c", "/gone/d"],
            &probe,
        );

        assert_eq!(
            scope.outcomes(),
            &[
                ScopeOutcome::InRoot(PathBuf::from("/srv/dots/a")),
                ScopeOutcome::OutOfRoot(OutOfRootTarget {
                    source_path: PathBuf::from("/other/dots/b"),
                    reason: OutOfRootReason::OutsideRoot,
                }),
                ScopeOutcome::InRoot(PathBuf::from("/srv/dots/c")),
                ScopeOutcome::OutOfRoot(OutOfRootTarget {
                    source_path: PathBuf::from("/gone/d"),
                    reason: OutOfRootReason::Missing,
                }),
            ]
        );
        assert!(scope.has_out_of_root());
    }

    /// Two roots, one shared cache: whichever root is authorized, only its
    /// own sources enter the mutation set. Neither approval reaches the
    /// other's tree.
    #[test]
    fn authorizing_either_root_never_reaches_the_other() {
        let probe = FakeProbe::default()
            .add("/srv/dots/vim/vimrc", Entry::NotADirectory)
            .add("/other/dots/vim/vimrc", Entry::NotADirectory);
        let cache = vec![
            PathBuf::from("/srv/dots/vim/vimrc"),
            PathBuf::from("/other/dots/vim/vimrc"),
        ];

        let here = scope_to_root(&RootIdentity::new("/srv/dots").unwrap(), &cache, &probe);
        let there = scope_to_root(&RootIdentity::new("/other/dots").unwrap(), &cache, &probe);

        assert_eq!(in_root(&here), vec![PathBuf::from("/srv/dots/vim/vimrc")]);
        assert_eq!(
            in_root(&there),
            vec![PathBuf::from("/other/dots/vim/vimrc")]
        );
    }

    /// The real filesystem, not only the stated one: containment and symlink
    /// escape must hold against `std::fs::canonicalize` too.
    #[test]
    fn the_os_probe_scopes_a_real_directory() {
        use super::super::util::OsPathProbe;

        let temp = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(temp.path()).unwrap();
        let root = base.join("dots");
        let other = base.join("other");
        std::fs::create_dir_all(root.join("vim")).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(root.join("vim/vimrc"), b"set nocompatible").unwrap();
        std::fs::write(other.join("vimrc"), b"elsewhere").unwrap();
        std::os::unix::fs::symlink(other.join("vimrc"), root.join("vim/escape")).unwrap();

        let candidates = vec![
            root.join("vim/vimrc"),
            root.join("vim/escape"),
            other.join("vimrc"),
            root.join("vim/gone"),
        ];
        let scope = scope_to_root(
            &RootIdentity::new(&root).unwrap(),
            &candidates,
            &OsPathProbe,
        );

        assert_eq!(in_root(&scope), vec![root.join("vim/vimrc")]);
        assert_eq!(
            reasons(&scope),
            vec![
                OutOfRootReason::OutsideRoot,
                OutOfRootReason::OutsideRoot,
                OutOfRootReason::Missing,
            ]
        );
    }
}

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
//! Behaviour arrives in WS06; this Work Stream fixes the shape.

use std::path::PathBuf;

use super::error::Result;
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
    /// The source path exists but could not be canonicalized, so it cannot be
    /// placed relative to the root.
    Uncanonicalizable,
}

/// A candidate excluded from the mutation set, with the reason to report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutOfRootTarget {
    /// The candidate's source path as the cache stored it.
    pub source_path: PathBuf,
    pub reason: OutOfRootReason,
}

/// The mutation set split against the authorized root.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MutationScope {
    /// Canonical source paths inside the authorized root — the only ones that
    /// may be written.
    pub in_root: Vec<PathBuf>,
    /// Everything else, with its reason.
    pub out_of_root: Vec<OutOfRootTarget>,
}

impl MutationScope {
    /// Whether any candidate was excluded, i.e. whether the command has an
    /// out-of-root report to make.
    pub fn has_out_of_root(&self) -> bool {
        !self.out_of_root.is_empty()
    }
}

/// Split `candidates` into the paths `root` authorizes and the ones it does
/// not.
pub fn scope_to_root(
    root: &RootIdentity,
    candidates: &[PathBuf],
    probe: &dyn PathProbe,
) -> Result<MutationScope> {
    let _ = (root, candidates, probe);
    todo!("WS06: scope shared-cache mutations to one root")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_scope_authorizes_and_reports_nothing() {
        let scope = MutationScope::default();

        assert!(scope.in_root.is_empty());
        assert!(!scope.has_out_of_root());
    }

    #[test]
    fn excluded_candidates_are_reported_not_written() {
        let scope = MutationScope {
            in_root: vec![PathBuf::from("/srv/dots/vim/vimrc")],
            out_of_root: vec![OutOfRootTarget {
                source_path: PathBuf::from("/other/dots/vim/vimrc"),
                reason: OutOfRootReason::OutsideRoot,
            }],
        };

        assert!(scope.has_out_of_root());
        assert!(!scope
            .in_root
            .contains(&PathBuf::from("/other/dots/vim/vimrc")));
    }
}

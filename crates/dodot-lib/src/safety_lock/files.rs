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
//! path produces — only its [`RootSource`](super::roots::RootSource) differs.
//!
//! Behaviour arrives in WS03; this Work Stream fixes the signature.

use std::path::PathBuf;

use super::error::Result;
use super::roots::ResolvedRoot;
use super::util::PathProbe;

/// The filesystem inputs implicit selection depends on, captured once per
/// invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRootInput {
    /// The directory Dodot was invoked from.
    pub current_dir: PathBuf,

    /// The Git top-level enclosing `current_dir`, or `None` when it is not
    /// inside a repository.
    ///
    /// Discovered by the caller, which is what keeps `git` out of this
    /// module and lets tests state "inside a repo" as data.
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
}

/// Resolve the implicit root: the Git top-level when there is one, otherwise
/// the current directory, canonicalized once.
///
/// The returned root always carries an implicit
/// [`RootSource`](super::roots::RootSource) — `Git` or `CurrentDirectory` —
/// so the safety gate can tell the user which mechanism selected the path it
/// is asking them to approve (Spec, story 2).
pub fn resolve_file_root(input: &FileRootInput, probe: &dyn PathProbe) -> Result<ResolvedRoot> {
    let _ = (input, probe);
    todo!("WS03: implicit file-root resolution and path identity")
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

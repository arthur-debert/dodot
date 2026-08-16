//! Authoritative environment-root selection (`DOTFILES_ROOT`).
//!
//! A present `DOTFILES_ROOT` is deliberate selection and is authoritative: an
//! unusable value is a hard error, never a fall-through to Git or the current
//! directory, so an explicit configuration mistake cannot quietly become a
//! different root (ADR-0002, Spec story 9).
//!
//! The variable is *injected*, not read here. The capture belongs to the
//! process boundary (WS07); this module is a pure function of its input so
//! every explicit-value case — empty, relative, non-Unicode, missing,
//! non-directory, unreadable — is testable without mutating a shared
//! environment.
//!
//! Behaviour arrives in WS02; this Work Stream fixes the signature.

use std::ffi::OsString;
use std::path::PathBuf;

use super::error::Result;
use super::roots::ResolvedRoot;
use super::util::PathProbe;

/// The environment inputs root selection depends on, captured once per
/// invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentRootInput {
    /// `DOTFILES_ROOT` exactly as the process environment carried it, or
    /// `None` when it is unset.
    ///
    /// An [`OsString`] rather than a `String`: a non-Unicode value is
    /// accepted when it resolves to a readable directory, so the raw bytes
    /// must survive capture.
    pub raw_value: Option<OsString>,

    /// The home directory used to expand a leading `~`.
    pub home_dir: PathBuf,
}

impl EnvironmentRootInput {
    /// An input with `DOTFILES_ROOT` unset.
    pub fn unset(home_dir: impl Into<PathBuf>) -> Self {
        Self {
            raw_value: None,
            home_dir: home_dir.into(),
        }
    }

    /// An input carrying a captured `DOTFILES_ROOT` value.
    pub fn set(home_dir: impl Into<PathBuf>, raw_value: impl Into<OsString>) -> Self {
        Self {
            raw_value: Some(raw_value.into()),
            home_dir: home_dir.into(),
        }
    }

    /// Whether `DOTFILES_ROOT` was present at all — the question that decides
    /// whether implicit selection runs, independent of whether the value is
    /// usable.
    pub fn is_set(&self) -> bool {
        self.raw_value.is_some()
    }
}

/// Resolve the environment root, if `DOTFILES_ROOT` selected one.
///
/// Returns:
///
/// - `Ok(Some(root))` — a valid value, canonicalized, with source
///   [`RootSource::Environment`](super::roots::RootSource::Environment).
///   Selection produces at most this one invocation-local root; it is never
///   written to the approved-roots collection.
/// - `Ok(None)` — `DOTFILES_ROOT` is unset, so implicit selection applies.
/// - `Err(..)` — the value is present but unusable
///   ([`SafetyLockError::EnvironmentRootEmpty`](super::error::SafetyLockError::EnvironmentRootEmpty)
///   or
///   [`EnvironmentRootUnusable`](super::error::SafetyLockError::EnvironmentRootUnusable)).
///   The caller must fail rather than resolve another root.
pub fn resolve_environment_root(
    input: &EnvironmentRootInput,
    probe: &dyn PathProbe,
) -> Result<Option<ResolvedRoot>> {
    let _ = (input, probe);
    todo!("WS02: authoritative environment-root resolution")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_is_independent_of_usability() {
        assert!(!EnvironmentRootInput::unset("/home/alice").is_set());
        assert!(EnvironmentRootInput::set("/home/alice", "").is_set());
        assert!(EnvironmentRootInput::set("/home/alice", "/srv/dots").is_set());
    }

    #[test]
    fn a_captured_value_keeps_its_native_bytes() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let raw = OsString::from_vec(b"/tmp/\x80dots".to_vec());
        let input = EnvironmentRootInput::set("/home/alice", raw.clone());

        assert_eq!(input.raw_value.as_deref(), Some(raw.as_os_str()));
    }
}

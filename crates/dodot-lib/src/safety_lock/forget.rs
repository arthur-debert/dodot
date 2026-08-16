//! Revoking an approval — the reversing half of root management
//! (`dodot roots forget <path>`).
//!
//! Two matching rules, in order, so moving or deleting a root cannot strand
//! its record (ADR-0003):
//!
//! 1. an argument that still exists is canonicalized first, so any alias of
//!    the root selects the same approval;
//! 2. an argument that does not exist is matched against the exact stored
//!    spelling — the one `roots list` printed.
//!
//! Like approval, revocation returns the trust state to persist rather than
//! writing it.
//!
//! Behaviour arrives in WS04; this Work Stream fixes the signature.

use std::ffi::OsString;

use super::error::Result;
use super::roots::RootIdentity;
use super::schema::SafetyLockConfig;
use super::util::PathProbe;

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
pub fn forget_root(
    config: &SafetyLockConfig,
    request: &ForgetRequest,
    probe: &dyn PathProbe,
) -> Result<ForgetChange> {
    let _ = (config, request, probe);
    todo!("WS04: trusted-root registry lifecycle")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_keeps_the_argument_bytes_it_was_given() {
        use std::os::unix::ffi::OsStringExt;

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

//! Errors raised by Safety Lock.
//!
//! The variants are deliberately narrow: each one names the coordinate the
//! user has to act on (the offending path, the trust file, the spelling that
//! could not be read back) because Safety Lock's diagnostics are the only
//! thing a refused user has to work from. See `docs/spec/safety-lock.md`
//! ("Observability").

use std::path::PathBuf;

use thiserror::Error;

/// Every way Safety Lock can refuse to produce a root, a trust decision, or a
/// scoped mutation set.
///
/// Nothing here describes *user refusal* at the confirmation prompt: declining
/// an implicit root is a [`TrustDecision`](super::check::TrustDecision), not an
/// error. These variants are the cases where Dodot cannot answer the question
/// at all and must fail closed.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SafetyLockError {
    /// `DOTFILES_ROOT` was set to an empty value. Per ADR-0002 a present
    /// explicit value is authoritative, so this fails instead of falling
    /// through to Git or the current directory.
    #[error(
        "DOTFILES_ROOT is set but empty — unset it to let dodot discover the \
         root, or point it at your dotfiles directory"
    )]
    EnvironmentRootEmpty,

    /// `DOTFILES_ROOT` was set to a value that is not a usable directory
    /// (missing, not a directory, unreadable, or uncanonicalizable). Never
    /// falls back to implicit selection.
    #[error("DOTFILES_ROOT is set to `{spelling}` but {reason}")]
    EnvironmentRootUnusable {
        /// The value in the reversible spelling of
        /// [`super::util::encode_native_path`], so a non-Unicode value is
        /// named the same way trust state and `roots list` name it.
        spelling: String,
        /// What made the value unusable, phrased to complete the sentence
        /// "DOTFILES_ROOT is set to `…` but …".
        reason: String,
    },

    /// Implicit selection found neither a Git top-level nor a usable current
    /// directory.
    #[error("could not determine a dotfiles root from `{}`", current_dir.display())]
    NoImplicitRoot { current_dir: PathBuf },

    /// A path that must be canonical and absolute was neither.
    ///
    /// Root identity is the canonical absolute path (ADR-0001); a relative
    /// spelling has no stable identity to approve or revoke.
    #[error("`{spelling}` is not an absolute path, so it cannot identify a dotfiles root")]
    RelativeRootIdentity { spelling: String },

    /// A stored spelling could not be read back into a native path — the
    /// tagged encoding was truncated or contained non-hex characters.
    #[error("`{spelling}` is not a valid dotfiles-root spelling: {reason}")]
    UnreadableSpelling { spelling: String, reason: String },

    /// The trust file exists but cannot be used. Fails closed: an unreadable
    /// or invalid trust file never reads as "no roots approved".
    #[error("cannot read the trusted-roots file at {}: {reason}", path.display())]
    TrustStateUnusable { path: PathBuf, reason: String },

    /// The trust file listed the same canonical root twice, or listed an
    /// entry that is not a usable root identity.
    #[error("the trusted-roots file lists `{spelling}` more than once")]
    DuplicateApprovedRoot { spelling: String },

    /// An environment-selected root was passed to approval. `DOTFILES_ROOT`
    /// *is* the deliberate selection (ADR-0003); writing it into the approved
    /// collection would silently trust a path the user never confirmed.
    #[error(
        "`{spelling}` was selected by DOTFILES_ROOT, which is already deliberate \
         selection — environment roots are never added to the approved roots"
    )]
    EnvironmentRootNotApprovable { spelling: String },
}

/// Result alias for Safety Lock APIs.
///
/// Distinct from [`crate::Result`]: Safety Lock is a self-contained boundary
/// whose callers (the CLI gate, later Work Streams) decide how a refusal maps
/// onto Dodot's command-level error type.
pub type Result<T> = std::result::Result<T, SafetyLockError>;

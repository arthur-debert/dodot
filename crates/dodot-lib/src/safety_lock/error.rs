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

    /// Implicit selection chose a candidate — the Git top-level, or the
    /// current directory — that is not a usable root.
    ///
    /// Naming the mechanism matters as much as naming the path: the Git
    /// top-level is frequently not the directory the user's shell prompt shows
    /// (Spec, story 2), so "the git top-level `/srv/dots` …" tells them where
    /// to look while a bare path does not. The selected mechanism is never
    /// retried against the other one: a Git top-level that cannot be used is
    /// not evidence that the current directory was meant instead.
    #[error("the {selected_by} `{spelling}` cannot be used as a dotfiles root: {reason}")]
    ImplicitRootUnusable {
        /// The candidate in the reversible spelling of
        /// [`super::util::encode_native_path`].
        spelling: String,
        /// Which implicit mechanism chose it.
        selected_by: super::roots::RootSource,
        /// What made it unusable, phrased to complete the sentence "the … `…`
        /// cannot be used as a dotfiles root: …".
        reason: String,
    },

    /// A path that must be canonical and absolute was not absolute.
    ///
    /// Root identity is the canonical absolute path (ADR-0001); a relative
    /// spelling has no stable identity to approve or revoke.
    #[error("`{spelling}` is not an absolute path, so it cannot identify a dotfiles root")]
    RelativeRootIdentity { spelling: String },

    /// An absolute path carried a `..` component, so it is an *alias* of a
    /// root rather than the root itself.
    ///
    /// `..` cannot be resolved without consulting the filesystem — under a
    /// symlink `/a/b/..` is not `/a` — so an identity is refused rather than
    /// lexically rewritten. Two spellings of one directory would otherwise
    /// become two identities and slip past duplicate detection and approval
    /// lookup alike (ADR-0001). Resolve the path first: selection
    /// canonicalizes through [`PathProbe`](super::util::PathProbe).
    #[error(
        "`{spelling}` contains `..`, so it names an alias rather than a \
         dotfiles root — pass the resolved path instead"
    )]
    NonCanonicalRootIdentity { spelling: String },

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

    /// A `.dodot.toml` under the root could not be loaded, parsed, or applied.
    ///
    /// "Applied" covers configuration that reads fine and then fails in use —
    /// an unresolvable gate label, an invalid `[mappings.gates]` glob, a file
    /// gated two ways at once. Those are the user's configuration just as much
    /// as a syntax error is, and they name the same file.
    ///
    /// Raised while building the orientation inventory, which is the only
    /// thing standing between an untrusted root and the confirmation prompt.
    /// Failing here is therefore what keeps Dodot from asking a user to
    /// approve a root it cannot describe, and from operating on configuration
    /// it could not load (Spec, story 14). The offending file is named
    /// because it is the only thing the user can act on.
    #[error("cannot load the dotfiles configuration at {}: {reason}", config_file.display())]
    DotfilesConfigUnusable {
        /// The `.dodot.toml` that failed — the root's or a pack's.
        config_file: PathBuf,
        /// What was wrong with it.
        reason: String,
    },

    /// The pack layout under the root could not be read, or its files could
    /// not be routed to handlers.
    ///
    /// Distinct from [`DotfilesConfigUnusable`](Self::DotfilesConfigUnusable):
    /// the configuration loaded, but the directory it describes cannot be
    /// walked or classified — an unreadable root, a pack name Dodot refuses,
    /// two packs colliding on one display name. Fails the same way and for the
    /// same reason: an inventory that silently omitted what it could not read
    /// would understate what approving the root allows.
    #[error("cannot inspect the packs under {}: {reason}", directory.display())]
    PackRoutingUnusable {
        /// The directory whose contents could not be read or routed — the
        /// root itself, or one pack under it.
        directory: PathBuf,
        /// What went wrong.
        reason: String,
    },
}

/// Result alias for Safety Lock APIs.
///
/// Distinct from [`crate::Result`]: Safety Lock is a self-contained boundary
/// whose callers (the CLI gate, later Work Streams) decide how a refusal maps
/// onto Dodot's command-level error type.
pub type Result<T> = std::result::Result<T, SafetyLockError>;

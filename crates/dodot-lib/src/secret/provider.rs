//! `SecretProvider` trait — the seam between dodot's secret machinery
//! and the per-provider subprocess work.
//!
//! Concrete providers (pass, op, sops, etc.) implement this trait.
//! The `secret()` MiniJinja function dispatches by scheme; the
//! preflight error UX (secrets.lex §5.4) calls [`SecretProvider::probe`]
//! before anything else; resolution is [`SecretProvider::resolve`].
//!
//! See `docs/proposals/secrets.lex` §5.1 for the design rationale and
//! `docs/proposals/secrets-testing.lex` §2 for the testing-seam role.

use crate::secret::secret_string::SecretString;
use crate::Result;

/// Outcome of [`SecretProvider::probe`] — describes whether the
/// provider can be used right now, and if not, why not. Each variant
/// maps to a specific user-facing error message in `secrets.lex` §5.4.
///
/// `Ok` is the "go ahead and call `resolve`" signal; everything else
/// is a fail-fast opportunity that lets us produce an actionable
/// error before subprocess machinery would have produced an opaque
/// one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeResult {
    /// The provider's CLI / library is installed, the auth state
    /// (session, biometric unlock, etc.) is good, ready to resolve.
    Ok,

    /// The provider's CLI is not on PATH (for shell-out providers)
    /// or the required library is missing (for in-process providers).
    /// User remediation: install the tool, or disable the provider in
    /// `[secret.providers.<scheme>] enabled = false`.
    NotInstalled {
        /// Short hint string the error renderer uses — e.g. an
        /// install URL or a package name. Provider-specific.
        hint: String,
    },

    /// CLI is installed but auth isn't established. User
    /// remediation: run the provider's signin / unlock command
    /// (e.g. `op signin`, `bw unlock`) or set the relevant env var
    /// (`OP_SERVICE_ACCOUNT_TOKEN`, `BW_SESSION`).
    NotAuthenticated {
        /// Short hint — e.g. `"run \`op signin\`"` or
        /// `"set OP_SERVICE_ACCOUNT_TOKEN"`.
        hint: String,
    },

    /// The probe found something else wrong — usually a configuration
    /// error specific to the provider (e.g. `pass` initialised but
    /// the gpg key isn't accessible). User remediation lives in the
    /// hint.
    Misconfigured { hint: String },

    /// The probe itself failed (subprocess crashed, IO error). Used
    /// sparingly; most failures should map to one of the cases above
    /// so the error UX stays predictable.
    ProbeFailed { details: String },
}

impl ProbeResult {
    /// True iff the provider is ready to resolve.
    pub fn is_ok(&self) -> bool {
        matches!(self, ProbeResult::Ok)
    }
}

/// A provider knows how to turn a reference like
/// `op://Personal/GitHub/token` into a [`SecretString`].
///
/// Implementations should:
///
/// - Be cheap to construct (no IO in the constructor; defer to
///   [`SecretProvider::probe`]).
/// - Make `probe` cheap and side-effect-free where possible. It runs
///   on every dodot invocation that touches a templated file with
///   `secret()` calls, so it can't be slow.
/// - Resolve via the provider's *non-interactive* path. Any provider
///   that can only be unlocked interactively (e.g. requires a
///   biometric prompt at resolve time) MUST surface that as
///   `ProbeResult::NotAuthenticated` first, so the user sees the
///   actionable hint rather than a hung subprocess.
/// - Never log the resolved value. The `tracing` macros expose the
///   message through the global subscriber, which may go to stdout,
///   stderr, journald, or a CI log — none of which are appropriate
///   destinations for a secret. `SecretString`'s `Debug` impl already
///   redacts on accidental capture; provider code should not unwrap
///   the bytes for logging at all.
pub trait SecretProvider: Send + Sync {
    /// The URI scheme this provider claims, without the colon.
    /// `"op"` for 1Password (`op://...`), `"pass"` for password-store
    /// (`pass:path/to/secret`), `"sops"` for SOPS, etc. The scheme
    /// registry uses this to dispatch references to the right
    /// provider.
    fn scheme(&self) -> &str;

    /// Cheap, side-effect-free check: can this provider service
    /// `resolve()` calls right now? Returns the actionable outcome;
    /// see [`ProbeResult`] variants.
    ///
    /// Called before resolution as a preflight by the `secret()`
    /// function (the error UX path), and on demand by
    /// `dodot secret probe` (the diagnostics command).
    fn probe(&self) -> ProbeResult;

    /// Resolve a reference to its secret value.
    ///
    /// `reference` is the string that came after the scheme prefix —
    /// for `op://Vault/Item/Field` the provider sees
    /// `"//Vault/Item/Field"`, for `pass:path/to/x` it sees
    /// `"path/to/x"`. (Schemes are dispatched by the registry; the
    /// provider doesn't need to re-parse the prefix.)
    ///
    /// Errors when the reference is malformed, the secret doesn't
    /// exist, or the provider's tool returns a non-zero exit. The
    /// error message must NOT contain the secret value — provider
    /// implementations are responsible for stripping value bytes
    /// from any subprocess stderr they propagate.
    fn resolve(&self, reference: &str) -> Result<SecretString>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_result_is_ok_only_for_ok_variant() {
        assert!(ProbeResult::Ok.is_ok());
        assert!(!ProbeResult::NotInstalled { hint: "x".into() }.is_ok());
        assert!(!ProbeResult::NotAuthenticated { hint: "x".into() }.is_ok());
        assert!(!ProbeResult::Misconfigured { hint: "x".into() }.is_ok());
        assert!(!ProbeResult::ProbeFailed {
            details: "x".into()
        }
        .is_ok());
    }
}

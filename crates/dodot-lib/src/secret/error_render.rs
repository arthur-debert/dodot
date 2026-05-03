//! Render `ProbeResult`s into the user-facing strings spec'd in
//! `secrets.lex` §5.4.
//!
//! Lives separate from `provider.rs` so the trait surface stays
//! minimal — provider impls return raw `ProbeResult` variants and
//! the messaging policy (which hint goes where, what cross-references
//! to include, what tone) lives in one place.

use crate::secret::provider::ProbeResult;
use crate::secret::registry::SecretRegistry;
use crate::DodotError;

/// Render a single `(scheme, ProbeResult)` pair as a multi-line user-
/// facing message. `Ok` produces an empty string — callers gate on
/// emptiness to decide whether to surface the line at all.
///
/// The shapes match `secrets.lex` §5.4:
///
/// - **NotInstalled**: tells the user the CLI is missing and provides
///   the provider-supplied install hint plus the disable-in-config
///   escape hatch.
/// - **NotAuthenticated**: tells the user auth is missing and provides
///   the provider-supplied signin / env-var hint.
/// - **Misconfigured**: surfaces the provider's specific
///   configuration problem verbatim (e.g. "password store not
///   initialised at /home/x/.password-store").
/// - **ProbeFailed**: surfaces the diagnostic verbatim — these are
///   the cases where probe() couldn't reach a clean conclusion.
pub fn render_probe_outcome(scheme: &str, outcome: &ProbeResult) -> String {
    let config_key = crate::secret::registry::scheme_to_config_key(scheme);
    match outcome {
        ProbeResult::Ok => String::new(),
        ProbeResult::NotInstalled { hint } => format!(
            "secret provider `{scheme}` is not installed\n  \
             {hint}\n  \
             or disable the provider: [secret.providers.{config_key}] enabled = false"
        ),
        ProbeResult::NotAuthenticated { hint } => format!(
            "secret provider `{scheme}` is not authenticated\n  \
             {hint}"
        ),
        ProbeResult::Misconfigured { hint } => {
            format!("secret provider `{scheme}` is misconfigured\n  {hint}")
        }
        ProbeResult::ProbeFailed { details } => format!(
            "secret provider `{scheme}` probe failed\n  \
             {details}\n  \
             this is unusual; check the dodot debug log (`dodot --debug ...`) for more"
        ),
    }
}

/// Run [`SecretRegistry::probe_all`] and collapse any non-Ok
/// outcomes into a single [`DodotError::Other`] suitable for
/// surfacing to the user before resolution begins.
///
/// Returns `Ok(())` when every provider probes Ok. Otherwise the
/// error is a multi-line message listing each failing provider with
/// its rendered outcome, in scheme-sorted order — same order as
/// `probe_all` returns. The flow is: `dodot up` calls this once per
/// run; on Err it surfaces the message and aborts before any
/// `secret(...)` resolution would be attempted (saving the user from
/// a partial-run state where some templates rendered and some
/// didn't).
///
/// `Ok` providers are not mentioned — silence is the success
/// signal; the message stays focused on what the user has to fix.
pub fn preflight(registry: &SecretRegistry) -> crate::Result<()> {
    let outcomes = registry.probe_all();
    let mut failing: Vec<String> = Vec::new();
    for (scheme, outcome) in &outcomes {
        if !outcome.is_ok() {
            failing.push(render_probe_outcome(scheme, outcome));
        }
    }
    if failing.is_empty() {
        return Ok(());
    }
    Err(DodotError::Other(format!(
        "{} secret provider(s) need attention before `dodot up` can resolve secrets:\n\n{}",
        failing.len(),
        failing.join("\n\n")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::test_support::MockSecretProvider;
    use std::sync::Arc;

    #[test]
    fn render_ok_returns_empty_string() {
        assert_eq!(render_probe_outcome("op", &ProbeResult::Ok), "");
    }

    #[test]
    fn render_not_installed_includes_install_hint_and_disable_pointer() {
        let outcome = ProbeResult::NotInstalled {
            hint: "install 1Password CLI: https://1password.com/downloads/command-line".into(),
        };
        let msg = render_probe_outcome("op", &outcome);
        assert!(msg.contains("`op` is not installed"));
        assert!(msg.contains("1password.com"));
        assert!(msg.contains("[secret.providers.op] enabled = false"));
    }

    #[test]
    fn render_not_installed_uses_underscore_key_for_secret_tool() {
        // Same hyphen-vs-underscore concern as the registry's
        // missing-provider error. The disable-provider hint
        // must point at the actual TOML key that maps to a
        // struct field.
        let outcome = ProbeResult::NotInstalled {
            hint: "install secret-tool".into(),
        };
        let msg = render_probe_outcome("secret-tool", &outcome);
        assert!(msg.contains("`secret-tool` is not installed"));
        assert!(
            msg.contains("[secret.providers.secret_tool] enabled = false"),
            "expected underscore TOML key in disable hint, got: {msg}"
        );
        assert!(
            !msg.contains("[secret.providers.secret-tool]"),
            "hyphen form must not leak: {msg}"
        );
    }

    #[test]
    fn render_not_authenticated_surfaces_provider_hint() {
        let outcome = ProbeResult::NotAuthenticated {
            hint: "set OP_SERVICE_ACCOUNT_TOKEN".into(),
        };
        let msg = render_probe_outcome("op", &outcome);
        assert!(msg.contains("`op` is not authenticated"));
        assert!(msg.contains("OP_SERVICE_ACCOUNT_TOKEN"));
    }

    #[test]
    fn render_misconfigured_surfaces_provider_hint_verbatim() {
        let outcome = ProbeResult::Misconfigured {
            hint: "password store not initialised at /tmp/store".into(),
        };
        let msg = render_probe_outcome("pass", &outcome);
        assert!(msg.contains("`pass` is misconfigured"));
        assert!(msg.contains("/tmp/store"));
    }

    #[test]
    fn render_probe_failed_includes_debug_pointer() {
        let outcome = ProbeResult::ProbeFailed {
            details: "subprocess crashed mid-probe".into(),
        };
        let msg = render_probe_outcome("op", &outcome);
        assert!(msg.contains("probe failed"));
        assert!(msg.contains("subprocess crashed"));
        assert!(msg.contains("`dodot --debug"));
    }

    #[test]
    fn preflight_succeeds_when_all_providers_ok() {
        let mut reg = SecretRegistry::new();
        reg.register(Arc::new(MockSecretProvider::new("pass")));
        reg.register(Arc::new(MockSecretProvider::new("op")));
        assert!(preflight(&reg).is_ok());
    }

    #[test]
    fn preflight_fails_with_aggregated_message_when_one_provider_fails() {
        let mut reg = SecretRegistry::new();
        reg.register(Arc::new(MockSecretProvider::new("pass"))); // Ok by default
        reg.register(Arc::new(MockSecretProvider::new("op").with_probe(
            ProbeResult::NotAuthenticated {
                hint: "set OP_SERVICE_ACCOUNT_TOKEN".into(),
            },
        )));
        let err = preflight(&reg).unwrap_err().to_string();
        assert!(err.contains("1 secret provider(s) need attention"));
        // The Ok one isn't mentioned — silence is success.
        assert!(!err.contains("`pass`"));
        // The failing one is.
        assert!(err.contains("`op` is not authenticated"));
        assert!(err.contains("OP_SERVICE_ACCOUNT_TOKEN"));
    }

    #[test]
    fn preflight_aggregates_multiple_failures_into_one_message() {
        let mut reg = SecretRegistry::new();
        reg.register(Arc::new(MockSecretProvider::new("op").with_probe(
            ProbeResult::NotAuthenticated {
                hint: "set OP_SERVICE_ACCOUNT_TOKEN".into(),
            },
        )));
        reg.register(Arc::new(MockSecretProvider::new("pass").with_probe(
            ProbeResult::Misconfigured {
                hint: "store not initialised".into(),
            },
        )));
        let err = preflight(&reg).unwrap_err().to_string();
        assert!(err.contains("2 secret provider(s) need attention"));
        assert!(err.contains("`op` is not authenticated"));
        assert!(err.contains("`pass` is misconfigured"));
    }

    #[test]
    fn preflight_succeeds_on_empty_registry() {
        // No providers registered — preflight has nothing to check.
        // Templates that call `secret(...)` against an empty
        // registry will fail at resolution time with a different
        // (more specific) error. preflight is concerned with
        // already-configured providers only.
        let reg = SecretRegistry::new();
        assert!(preflight(&reg).is_ok());
    }
}

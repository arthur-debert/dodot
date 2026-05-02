//! Scheme registry — dispatches a secret reference to the right
//! [`SecretProvider`] based on the URI scheme prefix.
//!
//! Reference shapes the registry recognises:
//!
//! - `op://Vault/Item/Field` — full URI form, `://` separator
//! - `pass:path/to/secret`   — single colon, no slashes
//! - `sops:file.yaml#k.path` — single colon, fragment optional
//! - `bw:Folder/Item`        — single colon
//!
//! Rule: split at the first colon, take everything to its left as the
//! scheme. The provider's `resolve()` sees what's after the colon
//! verbatim — it doesn't need to re-parse the scheme prefix.
//!
//! See `secrets.lex` §3.1 for the user-facing reference syntax and
//! §5.5 for the registry's role in adding new providers (no central
//! plumbing change required).

use std::collections::HashMap;
use std::sync::Arc;

use crate::secret::provider::{ProbeResult, SecretProvider};
use crate::secret::secret_string::SecretString;
use crate::{DodotError, Result};

/// Owned set of registered providers, keyed by scheme.
///
/// Constructed once per `dodot up` invocation by the secrets layer in
/// `commands::up`, populated from the `[secret.providers.*]` config,
/// and threaded into the `secret()` MiniJinja function. `Arc<dyn>`
/// because providers are held behind a trait object and the registry
/// is shared with the template engine across rendering passes.
#[derive(Default, Clone)]
pub struct SecretRegistry {
    providers: HashMap<String, Arc<dyn SecretProvider>>,
}

impl SecretRegistry {
    /// Empty registry — no providers registered yet. The `secret()`
    /// function against an empty registry always errors with
    /// "unknown scheme"; callers that want a user-friendly
    /// "secrets disabled" message should check the registry shape
    /// before rendering.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a provider. Replacing an existing entry for the same
    /// scheme is intentionally allowed (it's how tests substitute
    /// the mock for the real provider) but is logged at `debug` to
    /// catch unintentional overrides during normal config loading.
    pub fn register(&mut self, provider: Arc<dyn SecretProvider>) {
        let scheme = provider.scheme().to_string();
        if self.providers.contains_key(&scheme) {
            tracing::debug!(scheme = %scheme, "secret provider replaced");
        }
        self.providers.insert(scheme, provider);
    }

    /// True iff a provider is registered for the given scheme.
    pub fn has(&self, scheme: &str) -> bool {
        self.providers.contains_key(scheme)
    }

    /// All registered scheme names. Stable iteration order is NOT
    /// guaranteed; callers that want a deterministic listing should
    /// `collect::<Vec<_>>()` and sort.
    pub fn schemes(&self) -> impl Iterator<Item = &str> {
        self.providers.keys().map(String::as_str)
    }

    /// Borrow the provider for a scheme, if any.
    pub fn get(&self, scheme: &str) -> Option<&Arc<dyn SecretProvider>> {
        self.providers.get(scheme)
    }

    /// Resolve a full reference (with scheme prefix) by dispatching
    /// to the right provider.
    ///
    /// Returns `DodotError::Other` with an actionable message when:
    ///
    /// - The reference doesn't contain `:` (malformed input).
    /// - No provider is registered for the parsed scheme.
    /// - The provider's `resolve()` itself fails (error propagated
    ///   verbatim — provider impls are responsible for stripping
    ///   secret bytes from any error string).
    ///
    /// Mode gating is NOT this function's job. The `secret()`
    /// MiniJinja function checks `PreprocessMode` before calling
    /// `resolve()` per the §7.4 contract; the registry just routes.
    pub fn resolve(&self, full_reference: &str) -> Result<SecretString> {
        let (scheme, suffix) = split_scheme(full_reference)?;
        let provider = self.get(scheme).ok_or_else(|| {
            DodotError::Other(format!(
                "no secret provider registered for scheme `{scheme}`. \
                 Configured schemes: [{}]. \
                 Add a `[secret.providers.{scheme}] enabled = true` block \
                 to your config, or check the reference for typos.",
                self.sorted_schemes_for_display()
            ))
        })?;
        provider.resolve(suffix)
    }

    /// Probe every registered provider. Used by `dodot secret probe`
    /// to give the user a one-shot view of what's working.
    pub fn probe_all(&self) -> Vec<(String, ProbeResult)> {
        let mut out: Vec<(String, ProbeResult)> = self
            .providers
            .iter()
            .map(|(scheme, p)| (scheme.clone(), p.probe()))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    fn sorted_schemes_for_display(&self) -> String {
        let mut s: Vec<&str> = self.providers.keys().map(String::as_str).collect();
        s.sort_unstable();
        s.join(", ")
    }
}

/// Split a full reference into `(scheme, suffix)` at the first `:`.
///
/// `op://Vault/Item/Field` → `("op", "//Vault/Item/Field")`
/// `pass:path/to/x`         → `("pass", "path/to/x")`
/// `sops:f.yaml#k.path`     → `("sops", "f.yaml#k.path")`
///
/// Empty schemes (`":foo"`) and missing colons (`"plain-string"`)
/// produce an actionable error pointing the user at the reference
/// syntax.
pub fn split_scheme(reference: &str) -> Result<(&str, &str)> {
    let (scheme, suffix) = reference.split_once(':').ok_or_else(|| {
        DodotError::Other(format!(
            "secret reference `{reference}` is missing a scheme prefix. \
             Expected `<scheme>:<provider-specific-reference>` — for example \
             `op://Vault/Item/Field` or `pass:path/to/secret`."
        ))
    })?;
    if scheme.is_empty() {
        return Err(DodotError::Other(format!(
            "secret reference `{reference}` has an empty scheme prefix. \
             Expected `<scheme>:<provider-specific-reference>`."
        )));
    }
    Ok((scheme, suffix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::test_support::MockSecretProvider;

    #[test]
    fn split_scheme_handles_op_uri_form() {
        let (scheme, suffix) = split_scheme("op://Vault/Item/Field").unwrap();
        assert_eq!(scheme, "op");
        // Note: the `//` stays — providers see exactly what came
        // after the first colon. The `op` provider re-parses
        // `//Vault/Item/Field` itself.
        assert_eq!(suffix, "//Vault/Item/Field");
    }

    #[test]
    fn split_scheme_handles_pass_single_colon() {
        let (scheme, suffix) = split_scheme("pass:path/to/secret").unwrap();
        assert_eq!(scheme, "pass");
        assert_eq!(suffix, "path/to/secret");
    }

    #[test]
    fn split_scheme_handles_sops_with_fragment() {
        let (scheme, suffix) = split_scheme("sops:secrets.yaml#database.password").unwrap();
        assert_eq!(scheme, "sops");
        assert_eq!(suffix, "secrets.yaml#database.password");
    }

    #[test]
    fn split_scheme_rejects_no_colon_with_actionable_message() {
        let err = split_scheme("plain-string").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("missing a scheme prefix"));
        assert!(msg.contains("`<scheme>:<provider-specific-reference>`"));
        // Examples in the message help the user reach for the right shape.
        assert!(msg.contains("op://"));
        assert!(msg.contains("pass:"));
    }

    #[test]
    fn split_scheme_rejects_empty_scheme() {
        let err = split_scheme(":nothing").unwrap_err();
        assert!(err.to_string().contains("empty scheme prefix"));
    }

    #[test]
    fn registry_dispatches_to_correct_provider() {
        let mut reg = SecretRegistry::new();
        reg.register(Arc::new(
            MockSecretProvider::new("pass")
                .with("path/to/db", "hunter2")
                .with("path/to/api", "tok-abc"),
        ));
        reg.register(Arc::new(
            MockSecretProvider::new("op").with("//V/Item/password", "op-value"),
        ));

        let v = reg.resolve("pass:path/to/db").unwrap();
        assert_eq!(v.expose().unwrap(), "hunter2");

        let v = reg.resolve("op://V/Item/password").unwrap();
        assert_eq!(v.expose().unwrap(), "op-value");
    }

    #[test]
    fn registry_unknown_scheme_lists_configured_schemes_in_error() {
        let mut reg = SecretRegistry::new();
        reg.register(Arc::new(MockSecretProvider::new("pass")));
        reg.register(Arc::new(MockSecretProvider::new("op")));

        let err = reg.resolve("sops:foo.yaml#x").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("no secret provider registered for scheme `sops`"));
        // Configured schemes appear sorted so the message is stable.
        assert!(msg.contains("op, pass"));
    }

    #[test]
    fn registry_register_replaces_same_scheme() {
        let mut reg = SecretRegistry::new();
        reg.register(Arc::new(MockSecretProvider::new("pass").with("k", "first")));
        // Replace with a fresh provider for the same scheme.
        reg.register(Arc::new(
            MockSecretProvider::new("pass").with("k", "second"),
        ));
        assert_eq!(reg.resolve("pass:k").unwrap().expose().unwrap(), "second");
    }

    #[test]
    fn registry_has_and_schemes_reflect_registered_providers() {
        let mut reg = SecretRegistry::new();
        assert!(!reg.has("pass"));
        reg.register(Arc::new(MockSecretProvider::new("pass")));
        reg.register(Arc::new(MockSecretProvider::new("op")));
        assert!(reg.has("pass"));
        assert!(reg.has("op"));
        assert!(!reg.has("sops"));

        let mut schemes: Vec<&str> = reg.schemes().collect();
        schemes.sort_unstable();
        assert_eq!(schemes, vec!["op", "pass"]);
    }
}

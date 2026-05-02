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
use std::sync::{Arc, Mutex};

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
///
/// Resolved values are cached in `cache` so a reference that appears
/// in N templates only fires the underlying provider once. The cache
/// is shared between [`Clone`]s of the registry (it lives behind an
/// `Arc<Mutex>`); two registries built from the same config but via
/// independent constructor calls have independent caches. This
/// satisfies `secrets.lex` §7.4's "user authenticates once per run"
/// for the common case of a single registry threaded through every
/// pack rendered in one `dodot up` invocation.
#[derive(Default, Clone)]
pub struct SecretRegistry {
    providers: HashMap<String, Arc<dyn SecretProvider>>,
    cache: Arc<Mutex<HashMap<String, String>>>,
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
    /// to the right provider. **Bypasses the within-run cache** —
    /// every call shells out to the provider. Most callers want
    /// [`Self::resolve_cached`] instead; this entry point exists for
    /// tests that want to count provider invocations and for `dodot
    /// secret probe` paths that intentionally hit the wire.
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

    /// Look up a previously-resolved reference in the within-run
    /// cache. Returns `None` on cache miss; the caller (the
    /// `secret()` MiniJinja function) is expected to call
    /// [`Self::resolve`] and then [`Self::cache_put`] to populate
    /// the cache for future calls.
    ///
    /// Splitting cache access from resolution lets the caller
    /// validate values (multi-line refusal, UTF-8) with rich error
    /// messages co-located with the rendering surface, while still
    /// avoiding repeat shell-outs for the cache-hit path.
    pub fn cache_get(&self, full_reference: &str) -> Option<String> {
        self.cache.lock().unwrap().get(full_reference).cloned()
    }

    /// Store a resolved (and validated) value in the within-run
    /// cache. The caller is responsible for ensuring `value` is the
    /// genuine resolved string (no markers, no UTF-8 violations,
    /// not a multi-line value); the cache is dumb storage.
    pub fn cache_put(&self, full_reference: &str, value: &str) {
        self.cache
            .lock()
            .unwrap()
            .insert(full_reference.to_string(), value.to_string());
    }

    /// Number of entries currently held in the within-run cache.
    /// Useful for batching assertions in tests; not a public surface
    /// for production code, which has no reason to inspect cache
    /// size at runtime.
    #[cfg(test)]
    pub fn cache_len(&self) -> usize {
        self.cache.lock().unwrap().len()
    }

    /// Drop every entry from the within-run cache. Tests use this to
    /// re-exercise the provider path; production code should never
    /// need to call it (the cache is per-registry-instance and the
    /// instance is per-run).
    #[cfg(test)]
    pub fn clear_cache(&self) {
        self.cache.lock().unwrap().clear();
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
    fn cache_get_returns_none_until_cache_put_populates_it() {
        let reg = SecretRegistry::new();
        assert!(reg.cache_get("op://V/I/F").is_none());
        reg.cache_put("op://V/I/F", "secret-value");
        assert_eq!(reg.cache_get("op://V/I/F").as_deref(), Some("secret-value"));
    }

    #[test]
    fn cache_is_shared_between_clones_of_the_same_registry() {
        // Clone semantics: the cache lives behind an Arc<Mutex>, so
        // two clones of one registry observe the same cache. This
        // is what lets `commands::up` build the registry once and
        // pass it to N pack-rendering passes that all share auth.
        let reg = SecretRegistry::new();
        let clone = reg.clone();
        clone.cache_put("pass:k", "v");
        assert_eq!(reg.cache_get("pass:k").as_deref(), Some("v"));
    }

    #[test]
    fn cache_is_independent_between_separate_registry_constructions() {
        // Two `SecretRegistry::new()` calls produce independent
        // caches even when the same providers are registered. This
        // pins the "per-instance, not process-global" contract — a
        // later refactor that changes the cache to a static
        // singleton would silently break the test isolation
        // contract every other test in this module relies on.
        let a = SecretRegistry::new();
        let b = SecretRegistry::new();
        a.cache_put("pass:k", "from-a");
        assert!(b.cache_get("pass:k").is_none());
    }

    #[test]
    fn registry_resolve_does_not_consult_or_populate_cache() {
        // resolve() bypasses the cache by design — it's the
        // wire-hitting entry point that callers use to count
        // provider invocations. cache_get and cache_put are the
        // cache-aware surface. Pin the contract.
        let mut reg = SecretRegistry::new();
        reg.register(Arc::new(MockSecretProvider::new("pass").with("k", "v")));
        let _ = reg.resolve("pass:k").unwrap();
        assert_eq!(reg.cache_len(), 0, "resolve() must not populate the cache");
        assert!(
            reg.cache_get("pass:k").is_none(),
            "cache_get must miss when only resolve() ran"
        );
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

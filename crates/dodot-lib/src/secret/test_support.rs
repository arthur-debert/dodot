//! Test doubles for the secrets layer — `MockSecretProvider` and
//! `PanickingProvider`.
//!
//! `MockSecretProvider` is the workhorse: tier-0 unit tests register
//! it with a canned `reference -> value` map, then exercise everything
//! above the trait (the registry, the `secret()` MiniJinja function,
//! the AST pre-walk, the sidecar) without spawning a single
//! subprocess.
//!
//! `PanickingProvider` is the §7.4 contract pin: a provider whose
//! `resolve()` calls `panic!()`. Tests that exercise Passive-mode
//! flows (`dodot status`, `dodot up --dry-run`) register it; if the
//! flow accidentally invokes a provider, the panic surfaces and the
//! test fails loudly. Same shape as the
//! `up_dry_run_does_not_write_to_datastore` pattern from Wave 4.
//!
//! See `secrets-testing.lex` §6.1 / §6.2 for the doc on each.
//!
//! Available under `#[cfg(test)]` only — these are not for production
//! use and should not appear in the public API surface.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::secret::provider::{ProbeResult, SecretProvider};
use crate::secret::secret_string::SecretString;
use crate::{DodotError, Result};

/// In-memory `SecretProvider` for tier-0 unit tests.
///
/// Constructed with a scheme name; populated via [`MockSecretProvider::with`]
/// (chainable). Tracks `resolve()` invocation count so batching /
/// caching tests can assert the provider was hit the right number of
/// times.
pub struct MockSecretProvider {
    scheme: String,
    /// Maps `reference -> value`. The `reference` here is what the
    /// registry passes through to `resolve()` (i.e. post-scheme
    /// suffix) — for `op://V/I/F` that's `//V/I/F`.
    values: HashMap<String, String>,
    /// `probe()` return value. Defaults to `Ok`. Tests that exercise
    /// the error UX path use [`MockSecretProvider::with_probe`].
    probe_result: Mutex<ProbeResult>,
    /// Number of times `resolve()` has been called.
    resolve_calls: AtomicUsize,
}

impl MockSecretProvider {
    /// New mock provider for the given scheme. By default, every
    /// reference resolves to `Err("not found")` and `probe()` returns
    /// `Ok`. Add canned values via [`Self::with`].
    pub fn new(scheme: impl Into<String>) -> Self {
        Self {
            scheme: scheme.into(),
            values: HashMap::new(),
            probe_result: Mutex::new(ProbeResult::Ok),
            resolve_calls: AtomicUsize::new(0),
        }
    }

    /// Add a canned `reference -> value` mapping. Chainable.
    pub fn with(mut self, reference: impl Into<String>, value: impl Into<String>) -> Self {
        self.values.insert(reference.into(), value.into());
        self
    }

    /// Override the `probe()` return value. Used by tests that
    /// exercise the error UX (NotInstalled, NotAuthenticated, etc.).
    pub fn with_probe(self, result: ProbeResult) -> Self {
        *self.probe_result.lock().unwrap() = result;
        self
    }

    /// How many times `resolve()` has been called on this instance.
    /// Used by batching / caching tests.
    pub fn resolve_call_count(&self) -> usize {
        self.resolve_calls.load(Ordering::SeqCst)
    }
}

impl SecretProvider for MockSecretProvider {
    fn scheme(&self) -> &str {
        &self.scheme
    }

    fn probe(&self) -> ProbeResult {
        self.probe_result.lock().unwrap().clone()
    }

    fn resolve(&self, reference: &str) -> Result<SecretString> {
        self.resolve_calls.fetch_add(1, Ordering::SeqCst);
        match self.values.get(reference) {
            Some(v) => Ok(SecretString::new(v.clone())),
            None => Err(DodotError::Other(format!(
                "MockSecretProvider({}): no canned value for reference `{}`",
                self.scheme, reference
            ))),
        }
    }
}

/// `SecretProvider` whose `resolve()` panics. Use when the test's
/// goal is to prove a code path does NOT touch the provider —
/// `dodot status` / `dodot up --dry-run` against a templated pack
/// (Passive mode, §7.4 contract). Probe still returns `Ok` so the
/// preflight doesn't short-circuit before the test reaches the
/// resolve path it's gating on.
pub struct PanickingProvider {
    scheme: String,
}

impl PanickingProvider {
    pub fn new(scheme: impl Into<String>) -> Self {
        Self {
            scheme: scheme.into(),
        }
    }
}

impl SecretProvider for PanickingProvider {
    fn scheme(&self) -> &str {
        &self.scheme
    }

    fn probe(&self) -> ProbeResult {
        ProbeResult::Ok
    }

    fn resolve(&self, reference: &str) -> Result<SecretString> {
        panic!(
            "PanickingProvider({}): resolve(`{}`) was called, \
             but the test contract says no provider should be invoked \
             on this code path (§7.4 Passive contract violated?)",
            self.scheme, reference
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_returns_canned_value() {
        let p = MockSecretProvider::new("pass").with("k", "v");
        let s = p.resolve("k").unwrap();
        assert_eq!(s.expose().unwrap(), "v");
    }

    #[test]
    fn mock_unknown_reference_errors_clearly() {
        let p = MockSecretProvider::new("pass");
        let err = p.resolve("missing-key").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("MockSecretProvider(pass)"));
        assert!(msg.contains("`missing-key`"));
    }

    #[test]
    fn mock_counts_resolve_invocations() {
        let p = MockSecretProvider::new("pass").with("k", "v");
        assert_eq!(p.resolve_call_count(), 0);
        let _ = p.resolve("k");
        let _ = p.resolve("k");
        let _ = p.resolve("missing");
        assert_eq!(p.resolve_call_count(), 3);
    }

    #[test]
    fn mock_with_probe_overrides_default_ok() {
        let p = MockSecretProvider::new("op").with_probe(ProbeResult::NotAuthenticated {
            hint: "set OP_SERVICE_ACCOUNT_TOKEN".into(),
        });
        match p.probe() {
            ProbeResult::NotAuthenticated { hint } => {
                assert_eq!(hint, "set OP_SERVICE_ACCOUNT_TOKEN")
            }
            other => panic!("unexpected probe result: {other:?}"),
        }
    }

    #[test]
    #[should_panic(expected = "Passive contract violated")]
    fn panicking_provider_panics_on_resolve() {
        let p = PanickingProvider::new("op");
        let _ = p.resolve("anything");
    }

    #[test]
    fn panicking_provider_probe_is_ok() {
        // Probe must NOT panic — only resolve should. Otherwise the
        // preflight check would short-circuit before the test
        // reaches the path it's trying to gate.
        let p = PanickingProvider::new("op");
        assert!(p.probe().is_ok());
    }
}

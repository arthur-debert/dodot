//! Test doubles for the secrets layer.
//!
//! `MockSecretProvider` supplies canned values without spawning a subprocess.
//!
//! `PanickingProvider` proves that Passive-mode flows do not resolve secrets.
//!
//! See `secrets-testing.lex` §6.1 / §6.2 for the doc on each.
//!

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::secret::provider::{ProbeResult, SecretProvider};
use crate::secret::secret_string::SecretString;
use crate::{DodotError, Result};

/// In-memory `SecretProvider` with canned values and call tracking.
pub struct MockSecretProvider {
    scheme: String,
    /// Maps `reference -> value`. The `reference` here is what the
    /// registry passes through to `resolve()` (i.e. post-scheme
    /// suffix) — for `op://V/I/F` that's `//V/I/F`.
    values: HashMap<String, String>,
    /// `probe()` return value. Defaults to `Ok`.
    probe_result: Mutex<ProbeResult>,
    resolve_calls: AtomicUsize,
}

impl MockSecretProvider {
    /// By default, references resolve to `Err("not found")` and probing succeeds.
    pub fn new(scheme: impl Into<String>) -> Self {
        Self {
            scheme: scheme.into(),
            values: HashMap::new(),
            probe_result: Mutex::new(ProbeResult::Ok),
            resolve_calls: AtomicUsize::new(0),
        }
    }

    pub fn with(mut self, reference: impl Into<String>, value: impl Into<String>) -> Self {
        self.values.insert(reference.into(), value.into());
        self
    }

    pub fn with_probe(self, result: ProbeResult) -> Self {
        *self.probe_result.lock().unwrap() = result;
        self
    }

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

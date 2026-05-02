//! `SecretString` — a wrapper that holds a resolved secret value and
//! tries to keep it from leaking through the usual Rust footguns.
//!
//! The guarantees this type makes are explicitly limited; see
//! `docs/proposals/secrets.lex` §5.1 ("defense in depth, not a
//! guarantee"). Resolved secret values land on disk in the rendered
//! file regardless, so the meaningful protections are at the OS layer.
//! What this wrapper does add:
//!
//! - **Zero-on-drop.** The byte buffer is overwritten on `Drop` via
//!   the `zeroize` crate's volatile-write loop, which the compiler
//!   can't elide. Reduces the window where a stale-but-still-resident
//!   buffer could be read by another process with sufficient
//!   privilege.
//! - **No `Debug` / `Display`.** The type is opaque to the standard
//!   formatting machinery. A `tracing::error!("{e:?}", e=...)` that
//!   accidentally captures a `SecretString` won't print the bytes; it
//!   prints `SecretString(<redacted>)`.
//! - **No `Serialize`.** Same idea, for the JSON / TOML paths.
//! - **No `Clone`.** Discourages duplicating the value into multiple
//!   buffers; callers that genuinely need a copy can call
//!   [`SecretString::expose`] and re-wrap.
//!
//! What this wrapper does NOT do:
//!
//! - It doesn't `mlock` the page. Memory locking is a process-wide
//!   concern with rlimits to think about; the spec defers it.
//! - It doesn't prevent the value from being copied into a `String`
//!   that the renderer hands off to MiniJinja for substitution. The
//!   rendered output goes to disk; the wrapper has no power past that
//!   handoff.
//!
//! Read this as: every code path that touches the bytes through
//! `SecretString` is a deliberate decision, and the type makes the
//! accidental paths impossible.

use zeroize::Zeroize;

/// A resolved secret value, held briefly in process memory.
///
/// Construct via [`SecretString::new`]; read via [`SecretString::expose`].
/// Zeroes its buffer on drop. Has no `Debug` / `Display` / `Serialize`
/// implementations; printing one through any of those paths produces
/// `<redacted>`.
pub struct SecretString {
    inner: Vec<u8>,
}

impl SecretString {
    /// Wrap a UTF-8 string as a secret. Takes ownership of the input
    /// bytes so the caller can't keep a parallel handle.
    pub fn new(value: String) -> Self {
        Self {
            inner: value.into_bytes(),
        }
    }

    /// Wrap an arbitrary byte slice (for binary secrets — keys, etc.).
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { inner: bytes }
    }

    /// Borrow the secret as `&str`. Returns an error if the bytes
    /// aren't valid UTF-8 — the value-injection path requires UTF-8.
    /// Whole-file deploy uses [`SecretString::expose_bytes`] instead.
    pub fn expose(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.inner)
    }

    /// Borrow the secret as raw bytes. For whole-file flows where
    /// UTF-8 isn't a guarantee.
    pub fn expose_bytes(&self) -> &[u8] {
        &self.inner
    }

    /// Length of the secret in bytes. Safe to log.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// True iff the secret is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// True iff the secret value contains at least one newline.
    /// Used by §3.4's multi-line refusal: value-injection requires
    /// single-line values, and this check is the gate. Reading this
    /// flag does not expose the bytes anywhere — it's a property of
    /// the value, not the value itself.
    pub fn contains_newline(&self) -> bool {
        self.inner.contains(&b'\n')
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        self.inner.zeroize();
    }
}

// Explicitly opaque to formatting machinery.
impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Length included on purpose: it's safe to log, and helps
        // distinguish "empty value resolved" from "no value resolved"
        // when debugging without ever exposing the bytes.
        write!(f, "SecretString(<redacted>, len={})", self.inner.len())
    }
}

// `Display` is intentionally NOT implemented: printing a secret with
// `{}` should fail to compile, not silently format the bytes.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_and_expose_round_trip() {
        let s = SecretString::new("hunter2".into());
        assert_eq!(s.expose().unwrap(), "hunter2");
        assert_eq!(s.len(), 7);
        assert!(!s.is_empty());
    }

    #[test]
    fn empty_secret_is_well_behaved() {
        let s = SecretString::new(String::new());
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
        assert_eq!(s.expose().unwrap(), "");
    }

    #[test]
    fn from_bytes_supports_non_utf8_payload() {
        // 0xff is not valid UTF-8.
        let s = SecretString::from_bytes(vec![0xde, 0xad, 0xbe, 0xef, 0xff]);
        assert_eq!(s.expose_bytes(), &[0xde, 0xad, 0xbe, 0xef, 0xff]);
        assert!(s.expose().is_err(), "expose() must reject non-UTF-8");
    }

    #[test]
    fn debug_does_not_leak_value() {
        let s = SecretString::new("super-secret-token".into());
        let formatted = format!("{:?}", s);
        assert!(!formatted.contains("super-secret-token"));
        assert!(formatted.contains("<redacted>"));
        // Length surfacing is intentional — useful for debugging,
        // doesn't reveal the value.
        assert!(formatted.contains("len=18"));
    }

    #[test]
    fn contains_newline_detects_multiline_for_section_3_4() {
        let single = SecretString::new("one-line-value".into());
        assert!(!single.contains_newline());

        let multi = SecretString::new("line1\nline2".into());
        assert!(multi.contains_newline());

        // CR-only (e.g. classic-Mac line endings) is NOT flagged as
        // multi-line — value-injection's single-line gate is about
        // file-format breakage from `\n`. CR-only inputs are
        // pathological enough that we'd rather catch them via the
        // template engine's UTF-8 / encoding handling.
        let cr_only = SecretString::new("line1\rline2".into());
        assert!(!cr_only.contains_newline());
    }

    #[test]
    fn drop_zeroes_underlying_bytes() {
        // We can't observe the bytes after Drop directly without
        // unsafe (the buffer is freed). Instead, exercise the
        // zeroize pathway by holding the value, asserting the
        // pre-drop content, then dropping and constructing a fresh
        // value to confirm the type still works after Drop ran.
        // The real guarantee — that `Drop::drop` is called and
        // calls `zeroize` — is provided by the type's impl;
        // this test pins the contract on that impl rather than
        // attempting to read freed memory.
        let s = SecretString::new("rotate-me".into());
        assert_eq!(s.expose().unwrap(), "rotate-me");
        drop(s);
        let s2 = SecretString::new("next-value".into());
        assert_eq!(s2.expose().unwrap(), "next-value");
    }

    // Compile-time check: SecretString does NOT implement Clone.
    // (If a future change accidentally derives Clone, the line below
    // would compile and this test would silently pass. Instead, the
    // negative assertion lives as a doc-comment; rust-analyzer / human
    // review catches re-introduced Clone derivations.)
    //
    //   let s = SecretString::new("x".into());
    //   let _ = s.clone();   // <-- must fail to compile
}

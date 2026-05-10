//! `secret(...)` plumbing for the template preprocessor.
//!
//! The MiniJinja `secret()` function returns a unique private-use
//! sentinel rather than the resolved value. After rendering completes,
//! [`finalize_secrets`] walks the output, locates each sentinel, records
//! its line position into a [`SecretLineRange`], and substitutes the
//! sentinel back to the real value. That two-step approach avoids the
//! substring-collision failure mode where a secret value happens to
//! also appear elsewhere in the rendered text. See
//! `docs/proposals/secrets.lex` §3.4 / §7.4.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::preprocessing::SecretLineRange;

/// Per-call accumulator entry for `secret(...)` resolutions. Carries
/// both the unique private-use sentinel that the MiniJinja function
/// emitted and the real resolved value, so [`finalize_secrets`] can
/// compute line ranges from the sentinel positions and then swap
/// sentinels for values in the rendered + tracked outputs.
pub(super) struct SecretCallEntry {
    pub(super) sentinel: String,
    pub(super) reference: String,
    pub(super) value: String,
}

/// Process-wide monotonic counter used to make sentinels unique
/// across concurrent renders. Each `expand()` call gets a fresh id
/// before installing the `secret()` function.
static RENDER_COUNTER: AtomicU64 = AtomicU64::new(1);

pub(super) fn next_render_id() -> u64 {
    RENDER_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Sentinel format: `\u{E000}DSEC.<render_id>.<call_idx>\u{E001}`.
///
/// Both bracket characters live in the Unicode Private Use Area
/// (U+E000–U+F8FF), which by definition has no assigned meaning and
/// does not appear in normal dotfile content. Combined with the
/// per-render id, the resulting string is unique within and across
/// renders, eliminating the substring-collision failure mode of the
/// previous "search for the resolved value" approach.
pub(super) fn make_secret_sentinel(render_id: u64, call_idx: usize) -> String {
    let mut s = String::with_capacity(20);
    s.push('\u{E000}');
    s.push_str("DSEC.");
    s.push_str(&render_id.to_string());
    s.push('.');
    s.push_str(&call_idx.to_string());
    s.push('\u{E001}');
    s
}

/// Walk `rendered` to convert each sentinel into a [`SecretLineRange`]
/// (single-line per Phase S1 / §3.4), then substitute every sentinel
/// back to its real value in both `rendered` and `tracked` and return
/// all three.
///
/// Sentinels that don't appear in the output are dropped from the
/// range list — the `secret()` was evaluated (for resolution side
/// effects) but the value never reached the visible output, e.g. a
/// call inside a false `{% if %}` branch. We still substitute (a
/// no-op in that case) so callers can rely on the post-call output
/// containing zero sentinel characters.
pub(super) fn finalize_secrets(
    rendered: String,
    tracked: String,
    entries: &[SecretCallEntry],
) -> (String, String, Vec<SecretLineRange>) {
    let mut ranges = Vec::with_capacity(entries.len());
    if !entries.is_empty() {
        let line_starts = build_line_starts(&rendered);
        for entry in entries {
            if let Some(byte_off) = rendered.find(entry.sentinel.as_str()) {
                let line = byte_offset_to_line(&line_starts, byte_off);
                ranges.push(SecretLineRange {
                    start: line,
                    end: line + 1,
                    reference: entry.reference.clone(),
                });
            }
        }
    }

    let mut final_rendered = rendered;
    let mut final_tracked = tracked;
    for entry in entries {
        final_rendered = final_rendered.replace(entry.sentinel.as_str(), &entry.value);
        final_tracked = final_tracked.replace(entry.sentinel.as_str(), &entry.value);
    }

    (final_rendered, final_tracked, ranges)
}

/// Byte offsets where each line begins in `s`. `line_starts[0] == 0`;
/// `line_starts[i]` for i > 0 is the byte index just past the i-1th
/// `\n`. Used by [`byte_offset_to_line`] for the sentinel→line lookup.
fn build_line_starts(s: &str) -> Vec<usize> {
    let mut v = Vec::with_capacity(s.len() / 32 + 1);
    v.push(0);
    for (i, b) in s.bytes().enumerate() {
        if b == b'\n' {
            v.push(i + 1);
        }
    }
    v
}

/// Map a byte offset within the source string to its 0-indexed line
/// number. Binary search over `line_starts`.
fn byte_offset_to_line(line_starts: &[usize], offset: usize) -> usize {
    match line_starts.binary_search(&offset) {
        Ok(line) => line,
        Err(insert_pos) => insert_pos.saturating_sub(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `SecretCallEntry` and the rendered text it would
    /// produce when MiniJinja substitutes the sentinel for the value.
    /// Tests construct the rendered text with the sentinel in place
    /// (mimicking `secret()`'s return value) so `finalize_secrets`
    /// has something to find.
    fn entry(idx: usize, reference: &str, value: &str) -> (SecretCallEntry, String) {
        let sentinel = make_secret_sentinel(0, idx);
        let entry = SecretCallEntry {
            sentinel: sentinel.clone(),
            reference: reference.to_string(),
            value: value.to_string(),
        };
        (entry, sentinel)
    }

    #[test]
    fn finalize_secrets_substitutes_sentinels_and_records_line_ranges() {
        let (e, sentinel) = entry(0, "pass:k", "hunter2");
        let rendered = format!("header\nuser = alice\npassword = {sentinel}\nfooter\n");
        let (final_rendered, _, ranges) = finalize_secrets(rendered, String::new(), &[e]);
        assert_eq!(ranges.len(), 1);
        assert_eq!((ranges[0].start, ranges[0].end), (2, 3));
        assert_eq!(ranges[0].reference, "pass:k");
        assert_eq!(
            final_rendered,
            "header\nuser = alice\npassword = hunter2\nfooter\n"
        );
        assert!(!final_rendered.contains('\u{E000}'));
    }

    #[test]
    fn finalize_secrets_does_not_match_value_substring_outside_sentinel() {
        // The substring-based predecessor would mark line 0 (the
        // greeting also contains "hunter2"); the sentinel approach
        // only matches the exact secret slot.
        let (e, sentinel) = entry(0, "pass:k", "hunter2");
        let rendered = format!("greeting = hunter2 hi\npassword = {sentinel}\n");
        let (final_rendered, _, ranges) = finalize_secrets(rendered, String::new(), &[e]);
        assert_eq!(ranges.len(), 1);
        assert_eq!((ranges[0].start, ranges[0].end), (1, 2));
        assert_eq!(
            final_rendered,
            "greeting = hunter2 hi\npassword = hunter2\n"
        );
    }

    #[test]
    fn finalize_secrets_handles_two_calls_resolving_to_same_value() {
        // Two distinct sentinels even when the values are identical;
        // both lines are masked.
        let (e1, s1) = entry(0, "pass:a", "shared");
        let (e2, s2) = entry(1, "pass:b", "shared");
        let rendered = format!("a = {s1}\nb = {s2}\n");
        let (final_rendered, _, ranges) = finalize_secrets(rendered, String::new(), &[e1, e2]);
        assert_eq!(ranges.len(), 2);
        assert_eq!((ranges[0].start, ranges[0].end), (0, 1));
        assert_eq!((ranges[1].start, ranges[1].end), (1, 2));
        assert_eq!(final_rendered, "a = shared\nb = shared\n");
    }

    #[test]
    fn finalize_secrets_drops_entries_whose_sentinel_was_not_emitted() {
        // `secret()` was evaluated (e.g. inside a false `{% if %}`)
        // but the sentinel never reached the visible output. We
        // don't synthesise a fake range; we still substitute (a
        // no-op here) so callers can rely on the post-call output
        // being sentinel-free.
        let (e, _sentinel) = entry(0, "pass:hidden", "never-emitted");
        let rendered = "clean output\n".to_string();
        let (final_rendered, _, ranges) = finalize_secrets(rendered, String::new(), &[e]);
        assert!(ranges.is_empty());
        assert_eq!(final_rendered, "clean output\n");
    }

    #[test]
    fn finalize_secrets_substitutes_sentinels_in_tracked_render_too() {
        // Sentinels must not leak into the baseline cache via the
        // tracked stream.
        let (e, sentinel) = entry(0, "pass:k", "hunter2");
        let tracked = format!("preamble {sentinel} epilogue");
        let (_, final_tracked, _) = finalize_secrets(String::new(), tracked, &[e]);
        assert_eq!(final_tracked, "preamble hunter2 epilogue");
    }
}

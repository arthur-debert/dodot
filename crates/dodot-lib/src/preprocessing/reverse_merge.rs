//! Reverse-merge engine — propagates deployed-file edits back to the
//! template source.
//!
//! Wraps [`burgertocow::generate_diff_with_markers`] and
//! [`diffy::Patch`] into a single function that takes a template and
//! the cached marker-annotated render (from the baseline cache) plus
//! the current deployed text, and produces one of three outcomes:
//!
//! - [`ReverseMergeOutcome::Unchanged`] — pure data edit (only
//!   variable values changed). The template is correct as-is.
//! - [`ReverseMergeOutcome::Patched`] — burgertocow produced a clean
//!   unified diff; the patched template content is returned for the
//!   caller to write back to the source file.
//! - [`ReverseMergeOutcome::Conflict`] — burgertocow couldn't safely
//!   attribute every edit to a static template line (typically because
//!   the edit overlaps a `{{ var }}` region). The conflict block
//!   string is returned for the caller to surface to the user; the
//!   template source is left alone.
//!
//! # Why we don't re-render here
//!
//! The whole point of caching `tracked_render` in the baseline is that
//! `dodot transform check` can compute reverse-diffs without invoking
//! the template engine again. Re-rendering would re-trigger any
//! secret-provider auth prompts in the variable context — auth fatigue
//! that the magic.lex design specifically rules out. We rehydrate the
//! cached tracked string via
//! [`burgertocow::TrackedRender::from_tracked_string`] (added in
//! burgertocow 0.3) and feed it into `generate_diff_with_markers`
//! directly.

use std::ops::Range;

use burgertocow::{generate_diff_with_markers_opts, ConflictMarkers, DiffOptions, TrackedRender};
use diffy::Patch;

use crate::preprocessing::conflict::{MARKER_END, MARKER_MID, MARKER_START};
use crate::preprocessing::SecretLineRange;
use crate::{DodotError, Result};

/// Result of a reverse-merge attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReverseMergeOutcome {
    /// No template change is needed. The deployed-file edit was
    /// confined to variable values.
    Unchanged,
    /// burgertocow produced a clean unified diff; the field carries
    /// the patched template content. Callers write this back to the
    /// source file.
    Patched(String),
    /// burgertocow could not safely auto-merge. The field carries the
    /// conflict block (as emitted by burgertocow with our markers) so
    /// the caller can surface it to the user; the source file is not
    /// modified by `transform check` in this case — the user resolves
    /// it manually with their editor and `git diff`.
    Conflict(String),
}

impl ReverseMergeOutcome {
    /// True iff this outcome represents a template-space change that
    /// the caller should record. `Unchanged` is "no work"; `Patched`
    /// and `Conflict` are both "something happened".
    pub fn is_actionable(&self) -> bool {
        !matches!(self, ReverseMergeOutcome::Unchanged)
    }
}

/// Compute a reverse-merge for one processed file.
///
/// Returns [`ReverseMergeOutcome::Conflict`] when burgertocow flags an
/// ambiguous edit, [`ReverseMergeOutcome::Patched`] when it produces a
/// clean unified diff that diffy successfully applies, and
/// [`ReverseMergeOutcome::Unchanged`] when there's no template-space
/// change to make.
///
/// `secret_ranges` is the per-render secrets sidecar
/// (`<baseline>.secret.json`) for this file, if one was loaded.
/// Burgertocow's mask treats each listed range as if its deployed
/// content always matched the cached render, regardless of actual
/// bytes — so a rotated `{{ secret(...) }}` value (or a hand-edit
/// to a secret line) does not propagate into a template-space diff
/// that would replace the `secret(...)` expression with the literal
/// rotated value. See `secrets.lex` §3.3 / burgertocow#13.
///
/// The legacy three-arg shape is preserved as
/// [`reverse_merge_no_mask`] for callers that don't have a sidecar
/// loaded yet (every existing call site keeps the same behavior).
pub fn reverse_merge(
    template_src: &str,
    cached_tracked: &str,
    deployed: &str,
    secret_ranges: &[SecretLineRange],
) -> Result<ReverseMergeOutcome> {
    if cached_tracked.is_empty() {
        // No tracked render in the baseline (e.g. a v1 baseline with
        // serde-defaulted empty tracked_render, or a non-template
        // preprocessor). We can't drive burgertocow without the
        // marker stream — surface as Unchanged so the caller's loop
        // moves on. The classifier already flagged the divergence;
        // dropping in here just declines to auto-merge.
        return Ok(ReverseMergeOutcome::Unchanged);
    }

    let tracked = TrackedRender::from_tracked_string(cached_tracked.to_string());
    // Each marker line ends in `\n` so the conflict block sits cleanly
    // on its own lines when burgertocow joins them. Bound to locals
    // because `ConflictMarkers` borrows from these strings.
    let start = format!("{MARKER_START}\n");
    let mid = format!("\n{MARKER_MID}\n");
    let end = format!("\n{MARKER_END}\n");
    let markers = ConflictMarkers::new(&start, &mid, &end);
    // Convert `SecretLineRange { start, end, .. }` to the
    // `Range<usize>` shape `DiffOptions` consumes. Bound to a local
    // `Vec` because `with_mask` takes a borrowed slice.
    let mask: Vec<Range<usize>> = secret_ranges.iter().map(|r| r.start..r.end).collect();
    let opts = DiffOptions::new(&markers).with_mask(&mask);
    let diff = generate_diff_with_markers_opts(template_src, &tracked, deployed, &opts);

    if diff.is_empty() {
        return Ok(ReverseMergeOutcome::Unchanged);
    }

    // burgertocow returns *either* a unified diff *or* a conflict-only
    // string. We distinguish by looking at how the result starts: a
    // unified diff begins with `--- header` (the headers we set are
    // "template" / "modified"); a conflict block begins with our
    // MARKER_START line.
    if diff.starts_with(MARKER_START) {
        return Ok(ReverseMergeOutcome::Conflict(diff));
    }

    // Unified diff path: parse and apply via diffy.
    //
    // Error messages deliberately do NOT include the diff body. The
    // diff is built from the deployed file, which can carry secret
    // values that were resolved at render time. Spilling that into
    // stderr / CI logs would leak credentials. Callers needing to
    // debug a parse/apply failure can grep the deployed file or the
    // baseline cache directly — the metadata in the error (the
    // burgertocow error string and a short fingerprint) is enough to
    // locate the offending entry without surfacing the bytes.
    let patch = Patch::from_str(&diff).map_err(|e| {
        DodotError::Other(format!(
            "reverse-merge produced an invalid unified diff: {e} \
             ({} chars, sha-256 prefix {})",
            diff.len(),
            short_diff_fingerprint(&diff),
        ))
    })?;
    let patched = diffy::apply(template_src, &patch).map_err(|e| {
        DodotError::Other(format!(
            "failed to apply reverse-merge diff to template: {e} \
             ({} chars, sha-256 prefix {})",
            diff.len(),
            short_diff_fingerprint(&diff),
        ))
    })?;
    Ok(ReverseMergeOutcome::Patched(patched))
}

/// Convenience for callers that haven't loaded a secrets sidecar
/// for this file (or are computing reverse-merge for a file that
/// has no `secret(...)` calls). Equivalent to
/// [`reverse_merge`] with an empty mask — burgertocow then behaves
/// byte-identically to the pre-0.4 single-mask-less entry point.
pub fn reverse_merge_no_mask(
    template_src: &str,
    cached_tracked: &str,
    deployed: &str,
) -> Result<ReverseMergeOutcome> {
    reverse_merge(template_src, cached_tracked, deployed, &[])
}

/// Hash the diff and return the first 16 hex chars — enough to tell
/// two failure reports apart without leaking the diff body. Used by
/// the error paths in [`reverse_merge`].
fn short_diff_fingerprint(diff: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(diff.as_bytes());
    let mut out = String::with_capacity(16);
    for b in digest.iter().take(8) {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use burgertocow::Tracker;

    /// Render a template through burgertocow, returning the visible
    /// output and the cached tracked string the way the baseline
    /// cache would store them.
    fn render(src: &str, ctx: serde_json::Value) -> (String, String) {
        let mut tracker = Tracker::new();
        tracker.add_template("t", src).unwrap();
        let tracked = tracker.render("t", &ctx).unwrap();
        (tracked.output().to_string(), tracked.tracked().to_string())
    }

    #[test]
    fn unchanged_when_only_variable_values_changed() {
        // The user didn't touch any static template content — they
        // changed a variable's value. Reverse-merge sees this as a
        // pure-data edit and recommends no template change.
        let template = "name = {{ name }}\nport = 5432\n";
        let (rendered, tracked) = render(template, serde_json::json!({"name": "Alice"}));
        // Re-render with a different value to simulate the deployed
        // file as it would be after the next `dodot up` (or after
        // the user manually edited the value).
        let _ = rendered;
        let deployed = "name = Bob\nport = 5432\n";
        let outcome = reverse_merge(template, &tracked, deployed, &[]).unwrap();
        assert_eq!(outcome, ReverseMergeOutcome::Unchanged);
    }

    #[test]
    fn patches_static_text_edit_outside_variables() {
        // The user changed a static line that has no template
        // expression. Reverse-merge should produce a Patched outcome
        // whose content reflects the edit applied to the template
        // source.
        let template = "name = {{ name }}\nport = 5432\n";
        let (_, tracked) = render(template, serde_json::json!({"name": "Alice"}));
        let deployed = "name = Alice\nport = 9999\n";
        let outcome = reverse_merge(template, &tracked, deployed, &[]).unwrap();
        match outcome {
            ReverseMergeOutcome::Patched(patched) => {
                // The static-line edit propagates back to the
                // template, but the variable-bearing line stays as
                // `{{ name }}` (so future renders still pick up the
                // current value).
                assert!(patched.contains("port = 9999"), "patched: {patched:?}");
                assert!(
                    patched.contains("name = {{ name }}"),
                    "patched: {patched:?}"
                );
            }
            other => panic!("expected Patched, got: {other:?}"),
        }
    }

    #[test]
    fn flags_conflict_for_inconsistent_per_iteration_edits() {
        // The textbook conflict case from burgertocow's README:
        // different static edits across loop iterations. Iteration 1
        // changes `-` to `*`; iteration 2 changes `-` to `+`.
        // burgertocow can't pick a single template-space replacement,
        // so it emits a conflict block. Our wrapper surfaces that as
        // Conflict and leaves the source untouched.
        let template = "{% for i in items %}- {{ i }}\n{% endfor %}";
        let (_, tracked) = render(template, serde_json::json!({"items": ["a", "b", "c"]}));
        // Inconsistent prefix edits per iteration:
        let deployed = "* a\n+ b\n- c\n";
        let outcome = reverse_merge(template, &tracked, deployed, &[]).unwrap();
        assert!(
            matches!(outcome, ReverseMergeOutcome::Conflict(_)),
            "expected Conflict for inconsistent loop-iteration edits, got: {outcome:?}"
        );
        if let ReverseMergeOutcome::Conflict(block) = outcome {
            assert!(block.starts_with(MARKER_START), "block: {block:?}");
            assert!(block.contains(MARKER_MID), "block: {block:?}");
            assert!(block.contains(MARKER_END), "block: {block:?}");
        }
    }

    #[test]
    fn auto_merges_consistent_edit_across_loop_iterations() {
        // The companion case: the user changed `-` to `*` in *every*
        // iteration. burgertocow's loop-iteration fallback consolidates
        // those into a single template-space replacement, so the
        // outcome is Patched, not Conflict. This pins that we don't
        // pessimistically surface every loop edit as a conflict.
        let template = "{% for i in items %}- {{ i }}\n{% endfor %}";
        let (_, tracked) = render(template, serde_json::json!({"items": ["a", "b", "c"]}));
        let deployed = "* a\n* b\n* c\n";
        let outcome = reverse_merge(template, &tracked, deployed, &[]).unwrap();
        match outcome {
            ReverseMergeOutcome::Patched(patched) => {
                // Template's loop body now uses `*` instead of `-`.
                assert!(patched.contains("* {{ i }}"), "patched: {patched:?}");
            }
            other => panic!("expected Patched for consistent loop edit, got: {other:?}"),
        }
    }

    #[test]
    fn unchanged_when_cached_tracked_is_empty() {
        // Forward-compat with v1 baselines that were serde-defaulted
        // to an empty tracked_render. Without the marker stream we
        // can't drive burgertocow — return Unchanged so the caller's
        // loop simply moves on.
        let outcome = reverse_merge("name = {{ name }}\n", "", "name = Alice\n", &[]).unwrap();
        assert_eq!(outcome, ReverseMergeOutcome::Unchanged);
    }

    #[test]
    fn patched_outcome_is_byte_stable_across_runs() {
        // Determinism: identical inputs produce identical patched
        // output. This guards against any non-determinism leaking in
        // through diffy or burgertocow's diff machinery.
        let template = "alpha = {{ a }}\nbeta = static\ngamma = {{ g }}\n";
        let (_, tracked) = render(template, serde_json::json!({"a": "1", "g": "2"}));
        let deployed = "alpha = 1\nbeta = changed\ngamma = 2\n";
        let r1 = reverse_merge(template, &tracked, deployed, &[]).unwrap();
        let r2 = reverse_merge(template, &tracked, deployed, &[]).unwrap();
        assert_eq!(r1, r2);
    }

    #[test]
    fn is_actionable_distinguishes_outcomes() {
        assert!(!ReverseMergeOutcome::Unchanged.is_actionable());
        assert!(ReverseMergeOutcome::Patched(String::new()).is_actionable());
        assert!(ReverseMergeOutcome::Conflict(String::new()).is_actionable());
    }

    /// Helper for the masking tests below — build a single-line
    /// `SecretLineRange` covering the given 0-indexed line.
    fn mask_line(line: usize, reference: &str) -> SecretLineRange {
        SecretLineRange {
            start: line,
            end: line + 1,
            reference: reference.to_string(),
        }
    }

    #[test]
    fn mask_makes_secret_line_rotation_invisible_to_reverse_diff() {
        // The motivating case from `secrets.lex` §3.3: the deployed
        // file's secret value rotated (the user re-`up`'d after
        // their vault changed), but the *template* still says
        // `{{ secret("...") }}` and the source-of-truth is the
        // vault, not the deployed bytes. Without the mask, the
        // reverse-merge would propose rewriting the secret() call
        // to the literal new value. With the mask, the change is
        // invisible — Unchanged.
        //
        // The bare burgertocow `Tracker` doesn't have `secret()`
        // installed (that's TemplatePreprocessor's job), so we
        // simulate by rendering a template whose line 1 is a static
        // "password = OLD" baseline; the deployed file shows the
        // rotated "password = NEW_ROTATED". With line 1 masked, the
        // change is invisible.
        let template = "user = {{ name }}\npassword = OLD\n";
        let (rendered, tracked) = render(template, serde_json::json!({"name": "Ada"}));
        assert_eq!(rendered, "user = Ada\npassword = OLD\n");
        let deployed = "user = Ada\npassword = NEW_ROTATED\n";

        // Without mask: a unified diff propagates the rotated value
        // back to the template (or surfaces a conflict — either way,
        // *not* Unchanged).
        let unmasked = reverse_merge(template, &tracked, deployed, &[]).unwrap();
        assert_ne!(unmasked, ReverseMergeOutcome::Unchanged);

        // With the line-1 mask: byte change on the masked line is
        // invisible → Unchanged.
        let masked =
            reverse_merge(template, &tracked, deployed, &[mask_line(1, "pass:db")]).unwrap();
        assert_eq!(masked, ReverseMergeOutcome::Unchanged);
    }

    #[test]
    fn mask_does_not_hide_unmasked_static_line_edits() {
        // Mask one line; edit a different (unmasked) line. The
        // unmasked edit must still surface as Patched — masking is
        // a per-line opt-out, not a "disable reverse-merge" switch.
        let template = "user = {{ name }}\nport = 5432\nsecret_line = OLD\n";
        let (_, tracked) = render(template, serde_json::json!({"name": "Ada"}));
        let deployed = "user = Ada\nport = 9999\nsecret_line = NEW\n";

        let outcome = reverse_merge(
            template,
            &tracked,
            deployed,
            &[mask_line(2, "pass:secret_line")],
        )
        .unwrap();
        match outcome {
            ReverseMergeOutcome::Patched(patched) => {
                assert!(patched.contains("port = 9999"), "patched: {patched:?}");
                // The masked line stays at "OLD" in the source.
                assert!(
                    patched.contains("secret_line = OLD"),
                    "masked line must not propagate: {patched:?}"
                );
            }
            other => panic!("expected Patched, got: {other:?}"),
        }
    }

    #[test]
    fn empty_mask_is_byte_equivalent_to_unmasked() {
        // Pin the back-compat property burgertocow 0.4 documents:
        // an empty mask makes the call behave byte-identical to the
        // pre-mask entry point. `reverse_merge_no_mask` is the wrapper.
        let template = "name = {{ name }}\nport = 5432\n";
        let (_, tracked) = render(template, serde_json::json!({"name": "Ada"}));
        let deployed = "name = Ada\nport = 9999\n";

        let with_empty = reverse_merge(template, &tracked, deployed, &[]).unwrap();
        let no_mask = reverse_merge_no_mask(template, &tracked, deployed).unwrap();
        assert_eq!(with_empty, no_mask);
    }
}

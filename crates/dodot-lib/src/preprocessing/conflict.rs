//! Dodot conflict markers — used by reverse-merge to flag ambiguous edits.
//!
//! When `dodot transform check` (R3) or the template clean filter (R6)
//! cannot reliably attribute an edit to a static line in the template,
//! it splices a conflict block into the source file. The block frames
//! the original template line(s) and the user's deployed-side edit so
//! the user can pick the right side, the same way they would resolve a
//! `git merge` conflict.
//!
//! # Marker shape
//!
//! ```text
//! >>>>>> dodot-conflict (template)
//! host = "{{ env.DB_HOST }}"
//! ====== dodot-conflict (deployed)
//! host = "production.db.internal"
//! <<<<<< dodot-conflict
//! ```
//!
//! Three lines, structurally analogous to `git`'s `<<<<<<< HEAD` /
//! `=======` / `>>>>>>>` markers but with the inverted angle direction
//! so a file with both a real git conflict and a dodot conflict can be
//! pattern-matched independently. The literal `dodot-conflict` token
//! makes them unambiguously machine-detectable, format-agnostic (every
//! config language treats them as syntax errors, which is the desired
//! safety property at deploy time), and resolvable with any text editor
//! that knows how to grep.
//!
//! # Why we refuse to expand a source containing markers
//!
//! Once `dodot transform check` writes a conflict block into the
//! template, the template no longer renders cleanly: MiniJinja sees
//! the marker lines as plain text, the rendered output deploys with
//! the marker lines verbatim, and the user's app reads garbage. The
//! pipeline's safety gate ([`ensure_no_unresolved_markers`]) catches
//! this at the start of `dodot up` and surfaces a pointer to `git diff`
//! and a manual-resolution hint, rather than silently deploying broken
//! configs. See `docs/proposals/preprocessing-pipeline.lex` §6.3.

use std::path::Path;

use crate::{DodotError, Result};

/// Opens a conflict block. Followed by the original template line(s).
pub const MARKER_START: &str = ">>>>>> dodot-conflict (template)";

/// Separates the original template content from the user's edit.
pub const MARKER_MID: &str = "====== dodot-conflict (deployed)";

/// Closes a conflict block.
pub const MARKER_END: &str = "<<<<<< dodot-conflict";

/// All three marker prefixes — anything starting with one of these on
/// a line indicates an unresolved conflict block.
///
/// We match by *prefix*, not exact equality, so that variants like
/// `>>>>>> dodot-conflict (template, line 12)` (which a future
/// reverse-merge pass might emit for richer diagnostics) are still
/// caught by the safety gate. The `dodot-conflict` token is the
/// load-bearing distinguisher; no normal config content begins with it.
const MARKER_PREFIXES: &[&str] = &[
    ">>>>>> dodot-conflict",
    "====== dodot-conflict",
    "<<<<<< dodot-conflict",
];

/// Locate every line in `content` that begins with a dodot-conflict
/// marker prefix. Returns a vector of `(1-based line number, line
/// text)` pairs in source order. The line text is trimmed of trailing
/// whitespace (CR / spaces) so the error renderer can quote each line
/// without mojibake from line endings.
///
/// Returns an empty vector if the content has no markers.
pub fn find_unresolved_marker_lines(content: &str) -> Vec<(usize, String)> {
    content
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let trimmed = line.trim_end();
            if MARKER_PREFIXES.iter().any(|p| trimmed.starts_with(p)) {
                Some((idx + 1, trimmed.to_string()))
            } else {
                None
            }
        })
        .collect()
}

/// True if `content` contains at least one dodot-conflict marker line.
/// Cheaper than [`find_unresolved_marker_lines`] when the caller only
/// needs the boolean — short-circuits on the first hit.
pub fn contains_unresolved_markers(content: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = line.trim_end();
        MARKER_PREFIXES.iter().any(|p| trimmed.starts_with(p))
    })
}

/// Pipeline-level safety gate: refuse to expand a source file whose
/// contents carry unresolved dodot-conflict markers.
///
/// On detection, returns a [`DodotError::UnresolvedConflictMarker`]
/// carrying the source path and the matched line numbers, so the
/// rendering pipeline reports a clean diagnostic instead of silently
/// committing a broken config to the deployed location.
///
/// `content` must be valid UTF-8 (it's `&str`). Callers reading from
/// disk should run the bytes through [`String::from_utf8_lossy`] first
/// — that's how [`crate::preprocessing::pipeline::preprocess_pack`]
/// invokes this helper, so a non-UTF-8 source for a reverse-merge-
/// capable preprocessor still gets a clean scan rather than crashing
/// the gate with a UTF-8 decode error. The marker token is ASCII, so
/// detection works correctly under lossy decode.
pub fn ensure_no_unresolved_markers(content: &str, source_file: &Path) -> Result<()> {
    let lines = find_unresolved_marker_lines(content);
    if lines.is_empty() {
        return Ok(());
    }
    let line_numbers = lines.iter().map(|(n, _)| *n).collect();
    Err(DodotError::UnresolvedConflictMarker {
        source_file: source_file.to_path_buf(),
        line_numbers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_no_markers_in_clean_content() {
        let content = "name = Alice\nport = 5432\nhost = localhost\n";
        assert!(!contains_unresolved_markers(content));
        assert!(find_unresolved_marker_lines(content).is_empty());
    }

    #[test]
    fn detects_a_full_three_line_block() {
        // The canonical case — start, mid, end on three separate lines.
        let content = format!(
            "name = Alice\n{}\nhost = \"{{{{ env.DB_HOST }}}}\"\n{}\nhost = \"prod.db\"\n{}\nport = 5432\n",
            MARKER_START, MARKER_MID, MARKER_END
        );
        assert!(contains_unresolved_markers(&content));
        let lines = find_unresolved_marker_lines(&content);
        assert_eq!(lines.len(), 3, "got: {lines:?}");
        // Line numbers are 1-based.
        assert_eq!(lines[0].0, 2);
        assert_eq!(lines[1].0, 4);
        assert_eq!(lines[2].0, 6);
    }

    #[test]
    fn detects_partial_block() {
        // Even an isolated marker line (e.g. a sloppy git rebase that
        // dropped the matching close marker) is enough to refuse
        // expansion. We don't try to validate that markers are paired
        // — refusing on any marker is the conservative posture.
        let only_start = format!("name = Alice\n{}\nrest\n", MARKER_START);
        assert!(contains_unresolved_markers(&only_start));

        let only_end = format!("rest\n{}\nname = Alice\n", MARKER_END);
        assert!(contains_unresolved_markers(&only_end));
    }

    #[test]
    fn detects_marker_with_trailing_annotation() {
        // Future reverse-merge passes may append richer context after
        // the canonical prefix (e.g. " (template, line 12)"). The
        // detector matches by prefix so these still trip the gate.
        let content = ">>>>>> dodot-conflict (template, source line 12)\nstuff\n";
        assert!(contains_unresolved_markers(content));
        let lines = find_unresolved_marker_lines(content);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].1.contains("(template, source line 12)"));
    }

    #[test]
    fn detects_marker_with_crlf_line_ending() {
        // dotfile repos commonly mix CRLF (Windows-authored configs).
        // `lines()` strips the \n but leaves the \r; the trim_end in
        // the matcher removes it before the prefix comparison.
        let content = format!("host = \"prod\"\r\n{}\r\nrest\r\n", MARKER_START);
        assert!(contains_unresolved_markers(&content));
        let lines = find_unresolved_marker_lines(&content);
        assert_eq!(lines.len(), 1);
        // Trailing \r must not survive into the reported line text.
        assert!(!lines[0].1.contains('\r'));
    }

    #[test]
    fn skips_markers_in_the_middle_of_a_line() {
        // The token must appear at the start of a line. A line like
        // `# example: >>>>>> dodot-conflict ...` in a doc snippet does
        // not trip the gate — only line-leading markers do, matching
        // how `git` itself recognises its own markers.
        let content = "comment about >>>>>> dodot-conflict in docs\n";
        assert!(!contains_unresolved_markers(content));
        assert!(find_unresolved_marker_lines(content).is_empty());
    }

    #[test]
    fn skips_markers_with_leading_whitespace() {
        // Indented marker lines also don't trip the gate. Real reverse-
        // merge output always emits markers flush-left, so an indented
        // mention is a quotation, not a real conflict block.
        let content = format!("  {}\n", MARKER_START);
        assert!(!contains_unresolved_markers(&content));
    }

    #[test]
    fn ensure_no_unresolved_markers_passes_clean_content() {
        let p = Path::new("/tmp/clean.tmpl");
        ensure_no_unresolved_markers("name = Alice\n", p).expect("clean content must pass");
    }

    #[test]
    fn ensure_no_unresolved_markers_returns_descriptive_error() {
        let p = Path::new("/tmp/dirty.tmpl");
        let content = format!("name = Alice\n{}\nbody\n{}\n", MARKER_START, MARKER_END);
        let err = ensure_no_unresolved_markers(&content, p).unwrap_err();
        match err {
            DodotError::UnresolvedConflictMarker {
                source_file,
                line_numbers,
            } => {
                assert_eq!(source_file, p);
                assert_eq!(line_numbers, vec![2, 4]);
            }
            other => panic!("wrong error variant: {other}"),
        }
    }

    #[test]
    fn ensure_no_unresolved_markers_error_renders_actionable_message() {
        // The Display impl is what users see at the CLI; pin the parts
        // that should be there: source path, line numbers, and the
        // recovery hint with the shell-robust form (-- separator and
        // single-quoted path).
        let p = Path::new("app/config.toml.tmpl");
        let content = format!("first\n{}\nsecond\n", MARKER_START);
        let err = ensure_no_unresolved_markers(&content, p).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("app/config.toml.tmpl"), "msg: {msg}");
        assert!(msg.contains("line 2") || msg.contains("2"), "msg: {msg}");
        assert!(
            msg.contains("git diff -- 'app/config.toml.tmpl'"),
            "msg: {msg}"
        );
    }

    #[test]
    fn error_message_quotes_paths_with_spaces() {
        // The recovery hint should render the path quoted so the user
        // can copy-paste a working command for files in directories
        // with spaces in their names. The quoting is single-quote
        // based: shell-safe except for paths containing literal single
        // quotes (which are pathological in dotfile repos).
        let p = Path::new("My Configs/app.tmpl");
        let content = format!("{}\nbody\n", MARKER_START);
        let err = ensure_no_unresolved_markers(&content, p).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("'My Configs/app.tmpl'"),
            "expected single-quoted path in hint, got: {msg}"
        );
    }

    #[test]
    fn error_message_defangs_leading_dash_paths() {
        // A path that starts with `-` (e.g. accidental `--cool.tmpl`)
        // would be interpreted as a flag by `git diff` without the
        // `--` separator. Pin that the hint includes the separator.
        let p = Path::new("-weird-name.tmpl");
        let content = format!("{}\nbody\n", MARKER_START);
        let err = ensure_no_unresolved_markers(&content, p).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("git diff -- "),
            "expected `--` separator in hint, got: {msg}"
        );
    }

    #[test]
    fn marker_constants_use_distinct_directional_chars() {
        // Sanity test: the three markers must start with different
        // characters so a one-line scan can tell them apart, and so
        // they don't collide with git's own `<<<<<<<` / `>>>>>>>` /
        // `=======` markers (we use 6 chars, git uses 7).
        assert!(MARKER_START.starts_with('>'));
        assert!(MARKER_MID.starts_with('='));
        assert!(MARKER_END.starts_with('<'));
        // 6-char prefixes, not 7.
        assert!(MARKER_START.starts_with(">>>>>> "));
        assert!(MARKER_MID.starts_with("====== "));
        assert!(MARKER_END.starts_with("<<<<<< "));
        assert!(!MARKER_START.starts_with(">>>>>>>"));
    }

    #[test]
    fn empty_content_has_no_markers() {
        assert!(!contains_unresolved_markers(""));
        assert!(find_unresolved_marker_lines("").is_empty());
    }

    #[test]
    fn finds_multiple_independent_blocks() {
        // A source can carry more than one resolved-or-unresolved
        // block. We report all of them so the user fixes them in one
        // pass.
        let content = format!(
            "header\n{}\nA\n{}\nmiddle\n{}\nB\n{}\nfooter\n",
            MARKER_START, MARKER_END, MARKER_START, MARKER_END
        );
        let lines = find_unresolved_marker_lines(&content);
        assert_eq!(lines.len(), 4);
        assert_eq!(
            lines.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
            vec![2, 4, 6, 8]
        );
    }
}

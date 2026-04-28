//! Hand-written `--help` text for every dodot command.
//!
//! Each command has a corresponding `.txt` file under `src/help/` with
//! [styling] BBCode tags rendered through the dodot theme. We intercept
//! `--help` / `-h` / the `help` subcommand in `main.rs` before standout's
//! own help dispatch so the user sees the rich text we wrote rather than
//! the auto-generated layout.
//!
//! Why this layer exists: standout's built-in help renderer reads
//! `cmd.get_about()` (the one-liner) and lays out subcommands / options
//! generically. That's fine for the top-level menu, but for individual
//! commands we want prose, examples, and cross-references — so we ship
//! our own text and skip standout's data extraction step.
//!
//! The text files include their own USAGE / OPTIONS / EXAMPLES sections,
//! so when adding or changing a CLI flag in `main.rs`, also update the
//! corresponding `src/help/<cmd>.txt`.

use standout::{render_with_output, OutputMode};

use dodot_lib::render::create_theme;

/// Embedded help texts, keyed by command path (`""` for top-level,
/// `"up"` for `dodot up`, `"probe.shell-init"` for `dodot probe shell-init`).
///
/// Order matters only for `match_command_path` below — longer keys are
/// checked first so `probe.shell-init` wins over `probe`.
const HELP_TEXTS: &[(&str, &str)] = &[
    ("", include_str!("help/dodot.txt")),
    ("up", include_str!("help/up.txt")),
    ("down", include_str!("help/down.txt")),
    ("status", include_str!("help/status.txt")),
    ("list", include_str!("help/list.txt")),
    ("init", include_str!("help/init.txt")),
    ("fill", include_str!("help/fill.txt")),
    ("adopt", include_str!("help/adopt.txt")),
    ("addignore", include_str!("help/addignore.txt")),
    ("tutorial", include_str!("help/tutorial.txt")),
    ("init-sh", include_str!("help/init-sh.txt")),
    ("config", include_str!("help/config.txt")),
    (
        "probe.deployment-map",
        include_str!("help/probe-deployment-map.txt"),
    ),
    (
        "probe.show-data-dir",
        include_str!("help/probe-show-data-dir.txt"),
    ),
    (
        "probe.shell-init",
        include_str!("help/probe-shell-init.txt"),
    ),
    ("probe", include_str!("help/probe.txt")),
];

/// Walk `argv` (skipping the program name) to determine which command
/// the user is asking for help on, then return whether `--help` / `-h`
/// or the bare `help` subcommand was requested.
///
/// Returns `Some(command_path)` if a help request was detected, where
/// `command_path` is the dotted path matching `HELP_TEXTS` keys. If no
/// help marker is present, returns `None` so the caller falls through
/// to normal dispatch.
///
/// Recognized forms:
///   `dodot --help`              -> Some("")
///   `dodot -h`                  -> Some("")
///   `dodot help`                -> Some("")
///   `dodot up --help`           -> Some("up")
///   `dodot probe shell-init -h` -> Some("probe.shell-init")
///   `dodot help probe shell-init` -> Some("probe.shell-init")
pub fn detect_help_request<I, T>(argv: I) -> Option<String>
where
    I: IntoIterator<Item = T>,
    T: AsRef<str>,
{
    let args: Vec<String> = argv
        .into_iter()
        .skip(1) // program name
        .map(|s| s.as_ref().to_string())
        .collect();

    // Scan for the help marker. Keep collecting subcommand-like tokens
    // (non-flag, non-empty) before the marker as the command path.
    // For `dodot help foo bar`, the marker is the bare `help` token and
    // everything after it is the command path.
    let mut path: Vec<String> = Vec::new();
    let mut found_marker = false;
    let mut consume_rest_as_path = false;

    for arg in &args {
        if consume_rest_as_path {
            if arg.starts_with('-') {
                continue;
            }
            path.push(arg.clone());
            continue;
        }
        if arg == "--help" || arg == "-h" {
            found_marker = true;
            break;
        }
        if arg == "help" && path.is_empty() {
            found_marker = true;
            consume_rest_as_path = true;
            continue;
        }
        if arg.starts_with('-') {
            // skip flags / values (we don't care about flag values for
            // path detection — they can't precede the help marker in a
            // meaningful way for our command set)
            continue;
        }
        path.push(arg.clone());
    }

    if !found_marker {
        return None;
    }

    Some(path.join("."))
}

/// Look up the embedded help text for a command path. Falls back from
/// `probe.shell-init` -> `probe` -> top-level if a longer match isn't
/// present, so unknown subcommands at least show their parent's help.
pub fn lookup(path: &str) -> &'static str {
    if let Some(text) = HELP_TEXTS
        .iter()
        .find_map(|(k, v)| (*k == path).then_some(*v))
    {
        return text;
    }
    // Try shorter prefixes
    if let Some((parent, _)) = path.rsplit_once('.') {
        return lookup(parent);
    }
    // Fall back to the top-level help
    HELP_TEXTS
        .iter()
        .find_map(|(k, v)| k.is_empty().then_some(*v))
        .expect("top-level help must be embedded")
}

/// Render an embedded help text to a string using the dodot theme.
pub fn render(text: &str, mode: OutputMode) -> String {
    let theme = create_theme();
    // `render_with_output` requires a `Serialize` data type; we don't
    // template the help text, but we still need to pass something —
    // an empty struct is the conventional zero-data argument.
    #[derive(serde::Serialize)]
    struct NoData;
    render_with_output(text, &NoData, &theme, mode)
        .unwrap_or_else(|e| format!("(help render failed: {e})\n\n{text}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_top_level_help() {
        assert_eq!(
            detect_help_request(["dodot", "--help"]),
            Some(String::new())
        );
        assert_eq!(detect_help_request(["dodot", "-h"]), Some(String::new()));
        assert_eq!(detect_help_request(["dodot", "help"]), Some(String::new()));
    }

    #[test]
    fn detects_subcommand_help() {
        assert_eq!(
            detect_help_request(["dodot", "up", "--help"]),
            Some("up".into())
        );
        assert_eq!(
            detect_help_request(["dodot", "help", "up"]),
            Some("up".into())
        );
        assert_eq!(
            detect_help_request(["dodot", "probe", "shell-init", "--help"]),
            Some("probe.shell-init".into())
        );
        assert_eq!(
            detect_help_request(["dodot", "help", "probe", "shell-init"]),
            Some("probe.shell-init".into())
        );
    }

    #[test]
    fn no_help_request_returns_none() {
        assert_eq!(detect_help_request(["dodot", "up"]), None);
        assert_eq!(detect_help_request(["dodot", "status", "git"]), None);
    }

    #[test]
    fn lookup_falls_back_to_parent() {
        // Unknown probe subcommand -> falls back to "probe"
        let got = lookup("probe.unknown-thing");
        let probe_text = HELP_TEXTS
            .iter()
            .find_map(|(k, v)| (*k == "probe").then_some(*v))
            .unwrap();
        assert_eq!(got, probe_text);
    }

    #[test]
    fn lookup_top_level_for_empty_path() {
        let got = lookup("");
        assert!(got.contains("dodot"));
        assert!(got.contains("tutorial"));
    }

    #[test]
    fn every_registered_command_has_help() {
        // Sanity: each known command path resolves to its own text,
        // not a parent fallback.
        for (path, expected) in HELP_TEXTS {
            assert_eq!(lookup(path), *expected, "path {path:?} should self-match");
        }
    }

    /// Every styling tag used in the help texts must be defined in the
    /// dodot theme. In `TermDebug` mode, an unknown tag renders as
    /// `[name?]` — we render every help text in that mode and assert no
    /// such marker appears, so help authors can never silently ship a
    /// typo'd tag.
    #[test]
    fn all_help_tags_are_recognized_by_theme() {
        for (name, body) in HELP_TEXTS {
            let display_name = if name.is_empty() { "<top-level>" } else { name };
            let rendered = render(body, OutputMode::TermDebug);
            // Any `[foo?]` indicates a tag the theme doesn't know.
            for (lineno, line) in rendered.lines().enumerate() {
                assert!(
                    !line.contains("?]"),
                    "help/{display_name}: unknown tag at line {}: {line}",
                    lineno + 1
                );
            }
        }
    }
}

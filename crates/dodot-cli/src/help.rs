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
/// Order matters: longer, more specific keys must come before shorter
/// prefixes so `probe.shell-init` wins over `probe` in `lookup`.
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
    ("install", include_str!("help/install.txt")),
    ("plist", include_str!("help/plist.txt")),
    (
        "git-install-filters",
        include_str!("help/git-install-filters.txt"),
    ),
    (
        "git-show-filters",
        include_str!("help/git-show-filters.txt"),
    ),
    ("prompts", include_str!("help/prompts.txt")),
    ("roots", include_str!("help/roots.txt")),
    ("reset", include_str!("help/reset.txt")),
    ("config", include_str!("help/config.txt")),
    ("refresh", include_str!("help/refresh.txt")),
    ("transform", include_str!("help/transform.txt")),
    ("secret", include_str!("help/secret.txt")),
    ("template", include_str!("help/template.txt")),
    ("git-show-alias", include_str!("help/git-show-alias.txt")),
    (
        "git-install-alias",
        include_str!("help/git-install-alias.txt"),
    ),
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
    ("probe.app", include_str!("help/probe-app.txt")),
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
/// Takes native `OsStr` arguments, because this scan runs over the raw
/// process argv before Clap does: a non-Unicode argument — a native
/// path headed for `roots forget`'s `OsString` parser — must pass
/// through untouched, not panic the pre-scan. The markers recognized
/// here (`--help`, `-h`, `help`, `--`) are pure ASCII, so they are
/// matched on exact bytes; an argument that is not valid Unicode can
/// never be one of them and stays an ordinary positional token. Such a
/// token only ever influences the *command path* used to pick a help
/// text — and only when a help marker was actually present — where a
/// lossy rendering is fine: it matches no registered command and
/// `lookup` falls back to the nearest parent's help.
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
    T: AsRef<std::ffi::OsStr>,
{
    let args: Vec<std::ffi::OsString> = argv
        .into_iter()
        .skip(1) // program name
        .map(|s| s.as_ref().to_os_string())
        .collect();

    // Scan for the help marker. Keep collecting subcommand-like tokens
    // (non-flag, non-empty) before the marker as the command path.
    // For `dodot help foo bar`, the marker is the bare `help` token and
    // everything after it is the command path.
    //
    // Honor `--` as end-of-options: after it appears, tokens that look
    // like flags (including `--help` / `-h`) are treated as positional
    // arguments and must not trigger handwritten help — matching clap's
    // behavior so e.g. `dodot adopt pack -- --help` adopts a file
    // literally named `--help` instead of showing help.
    let mut path: Vec<String> = Vec::new();
    let mut found_marker = false;
    let mut consume_rest_as_path = false;
    let mut options_terminated = false;

    for arg in &args {
        // Exact-bytes marker matching: `to_str` yields the argument only
        // when it is valid Unicode, so a non-Unicode argument compares
        // equal to none of the ASCII markers below.
        let unicode = arg.to_str();
        let flag_like = arg.as_encoded_bytes().starts_with(b"-");
        if !options_terminated && unicode == Some("--") {
            options_terminated = true;
            continue;
        }
        if consume_rest_as_path {
            if !options_terminated && flag_like {
                continue;
            }
            path.push(arg.to_string_lossy().into_owned());
            continue;
        }
        if !options_terminated && matches!(unicode, Some("--help") | Some("-h")) {
            found_marker = true;
            break;
        }
        if !options_terminated && unicode == Some("help") && path.is_empty() {
            found_marker = true;
            consume_rest_as_path = true;
            continue;
        }
        if !options_terminated && flag_like {
            // skip flags / values (we don't care about flag values for
            // path detection — they can't precede the help marker in a
            // meaningful way for our command set)
            continue;
        }
        path.push(arg.to_string_lossy().into_owned());
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
    if let Some((parent, _)) = path.rsplit_once('.') {
        return lookup(parent);
    }
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
    fn double_dash_terminates_options() {
        // After `--`, `--help` is a positional arg, not a help marker.
        assert_eq!(
            detect_help_request(["dodot", "adopt", "pack", "--", "--help"]),
            None
        );
        assert_eq!(detect_help_request(["dodot", "--", "-h"]), None);
        // But `--help` before `--` still triggers help.
        assert_eq!(
            detect_help_request(["dodot", "up", "--help", "--", "x"]),
            Some("up".into())
        );
    }

    /// The pre-scan runs over raw process argv, before Clap: a native
    /// non-Unicode argument (a path headed for `roots forget`'s
    /// `OsString` parser) must neither panic the scan nor read as a help
    /// marker.
    #[test]
    fn non_unicode_arguments_pass_through_the_scan() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let native = OsString::from_vec(b"/tmp/\x80dots".to_vec());

        // No help marker: the scan falls through to normal dispatch,
        // leaving the argument for Clap.
        assert_eq!(
            detect_help_request([
                OsString::from("dodot"),
                OsString::from("roots"),
                OsString::from("forget"),
                native.clone(),
            ]),
            None
        );

        // A non-Unicode token that merely *starts* like a flag is not a
        // help marker either.
        let flag_like = OsString::from_vec(b"-\x80h".to_vec());
        assert_eq!(
            detect_help_request([OsString::from("dodot"), flag_like]),
            None
        );

        // With a real marker present, the scan still detects help; the
        // non-Unicode token only shapes the lookup path, where the
        // parent fallback absorbs it.
        let path = detect_help_request([
            OsString::from("dodot"),
            OsString::from("help"),
            OsString::from("roots"),
            native,
        ])
        .expect("an explicit `help` word must be detected");
        assert_eq!(lookup(&path), lookup("roots"));
    }

    #[test]
    fn lookup_falls_back_to_parent() {
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
        for (path, expected) in HELP_TEXTS {
            assert_eq!(lookup(path), *expected, "path {path:?} should self-match");
        }
    }

    /// Safety Lock sits before dispatch, so a command's help is the last
    /// read-only surface available to someone whose implicit root is not yet
    /// approved. Keep every mixed or mutating command family explicit about
    /// the gate instead of relying on the separate `roots` page being found.
    ///
    /// The roster is derived from the authoritative policy tables —
    /// [`COMMAND_SENSITIVITY`]'s mutating rows plus [`PASSTHROUGH_POLICY`]'s
    /// route-gated rows — so a newly classified mutating command fails here
    /// until its help family names the gate, instead of staying green behind
    /// a second hand-maintained list.
    #[test]
    fn root_sensitive_command_help_names_safety_lock() {
        use crate::safety::{
            PassthroughPolicy, RootSensitivity, COMMAND_SENSITIVITY, PASSTHROUGH_POLICY,
        };

        let mut families: Vec<&str> = COMMAND_SENSITIVITY
            .iter()
            .filter(|(_, sensitivity)| matches!(sensitivity, RootSensitivity::Mutating { .. }))
            .map(|(path, _)| *path)
            .chain(
                PASSTHROUGH_POLICY
                    .iter()
                    .filter(|(_, policy)| *policy == PassthroughPolicy::GatedInRoute)
                    .map(|(path, _)| *path),
            )
            // Dotted paths (`template.install-filter`) document the gate on
            // their registered parent help page.
            .map(|path| path.split('.').next().unwrap_or(path))
            .collect();
        families.sort_unstable();
        families.dedup();
        assert!(
            !families.is_empty(),
            "the safety policy tables must yield at least one gated help family"
        );

        for path in families {
            let body = lookup(path);
            assert!(
                body.contains("SAFETY LOCK") && body.contains("DOTFILES_ROOT"),
                "help/{path}.txt must explain Safety Lock and explicit-root automation"
            );
            assert!(
                body.contains("non-interactive"),
                "help/{path}.txt must explain non-interactive refusal"
            );
            assert!(
                body.contains("dodot roots"),
                "help/{path}.txt must point to approval recovery"
            );
        }
    }

    #[test]
    fn top_level_help_summarizes_safety_lock_contract() {
        let body = lookup("");
        for required in [
            "Safety Lock",
            "DOTFILES_ROOT",
            "first root-sensitive mutation",
            "non-interactive invocation refuses",
            "Read-only commands",
            "documented previews",
            "dodot roots list",
            "dodot roots forget",
            "dodot reset",
        ] {
            assert!(
                body.contains(required),
                "top-level help is missing Safety Lock contract text {required:?}"
            );
        }
    }

    /// Every declared preview flag must appear in its family's help, derived
    /// from [`COMMAND_SENSITIVITY`] so a new previewable command cannot ship
    /// help that omits its bypass; the audited exact phrasings below then pin
    /// how each bypass is worded, including the route-gated ones the table
    /// carries no flag for.
    #[test]
    fn mixed_command_help_names_its_read_only_bypass() {
        use crate::safety::{RootSensitivity, COMMAND_SENSITIVITY};

        for (path, sensitivity) in COMMAND_SENSITIVITY {
            if let RootSensitivity::Mutating {
                preview_flag: Some(flag),
            } = sensitivity
            {
                let family = path.split('.').next().unwrap_or(path);
                assert!(
                    lookup(family).contains(&format!("--{flag}")),
                    "help/{family}.txt must name the declared preview flag --{flag}"
                );
            }
        }

        for (path, bypass) in [
            ("up", "up --dry-run"),
            ("down", "down --dry-run"),
            ("adopt", "adopt --dry-run"),
            ("config", "list"),
            ("refresh", "refresh --list-paths"),
            ("transform", "check --dry-run"),
            ("template", "template clean"),
            ("tutorial", "dry-run preview"),
        ] {
            assert!(
                lookup(path).contains(bypass),
                "help/{path}.txt must name its Safety Lock bypass {bypass:?}"
            );
        }
    }

    #[test]
    fn audited_help_corrections_stay_pinned() {
        let config = lookup("config");
        assert!(config.contains("local[/item] is the only supported"));
        assert!(
            !config.contains("[item]global[/item]"),
            "config help must not advertise clapfig's nonexistent global persist scope"
        );

        let tutorial = lookup("tutorial");
        assert!(tutorial.contains("save progress"));
        assert!(tutorial.contains("install --write"));
        assert!(tutorial.contains("first root-sensitive mutation"));

        let reset = lookup("reset");
        assert!(reset.contains("safety-lock.toml"));
        assert!(reset.contains("asks again"));

        let roots = lookup("roots");
        assert!(roots.contains("next root-sensitive mutation that implicitly"));
        assert!(roots.contains("discovers that root asks again"));
        assert!(
            !roots.contains("next deploying command"),
            "roots help must cover every gated mutation after forget, not only deployment"
        );
        assert!(
            !roots.contains("mutation run from that root"),
            "forget's promise is scoped to implicit discovery: explicit \
             DOTFILES_ROOT selection never asks"
        );
    }

    /// Every subcommand clap knows about must have its own help text —
    /// `lookup` falling back to the top-level menu for a real command
    /// means `dodot <cmd> --help` silently shows the wrong screen
    /// (issue #233 item 3: refresh/transform/secret/git-install-alias
    /// shipped without per-command help).
    #[test]
    fn every_clap_subcommand_has_dedicated_help() {
        let top_level = lookup("");
        for cmd in crate::build_clap_command().get_subcommands() {
            let name = cmd.get_name();
            assert_ne!(
                lookup(name),
                top_level,
                "`dodot {name} --help` falls back to top-level help; \
                 add src/help/{name}.txt and register it in HELP_TEXTS"
            );
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

//! Rendering infrastructure for dodot output.
//!
//! Wraps standout-render to provide a consistent rendering pipeline
//! across all commands. The theme and templates are defined here;
//! the CLI layer just picks an [`OutputMode`].

use standout_render::{render_with_output, OutputMode, Renderer, Theme};

use crate::Result;

/// The dodot colour theme, defined in YAML for readability.
///
/// Style names are semantic — templates reference them by name,
/// and the theme adapts to terminal capabilities automatically.
const THEME_YAML: &str = r#"
pack-name:
  bold: true
  fg: blue

filename:
  fg: white

handler-symbol:
  bold: true
  fg: yellow

description:
  dim: true

deployed:
  fg: green

pending:
  fg: magenta

error:
  fg: red
  bold: true

broken:
  fg: red

stale:
  fg: yellow

warning:
  fg: yellow

message:
  fg: cyan

dim:
  dim: true

header:
  bold: true

dry-run:
  fg: yellow
  italic: true

conflict-banner:
  fg: white
  bg: red
  bold: true

conflict-header:
  fg: white
  bg: red
  bold: true

conflict-target:
  fg: red
  bold: true

conflict-pack:
  fg: red

conflict-hint:
  dim: true

ignored-pack:
  dim: true
  italic: true

group-banner-deployed:
  fg: green
  bold: true

group-banner-pending:
  fg: yellow
  bold: true

group-banner-error:
  fg: red
  bold: true

group-banner-ignored:
  dim: true
  bold: true

# Tutorial prompt question text. The interactive `dodot tutorial`
# uses inquire for the prompt UI; this style is mirrored by hand into
# its `RenderConfig` (see `tutorial.rs::tutorial_render_config`). Keep
# attributes here in sync with that function so users have one place
# to change the look.
tutorial-prompt:
  italic: true

# CLI help tags. The hand-written --help text in `dodot-cli/src/help/`
# uses these alongside the semantic tags above. Mirror standout's
# default help theme so the look matches the rest of dodot's output:
#   item    — bold (command names, option flags)
#   desc    — plain (descriptions next to items)
#   usage   — plain (the usage line)
#   example — plain (example blocks)
#   about   — plain (intro / about text)
item:
  bold: true
desc: {}
usage: {}
example: {}
about: {}
"#;

// ── Templates ───────────────────────────────────────────────────

/// Status / up / down — pack-level output with file listings.
///
/// Per-item errors are surfaced as `[N]` markers next to the status label;
/// their bodies render in a dedicated `Errors:` section at the bottom so
/// the per-file columns stay single-line and aligned regardless of how
/// long an individual error message is.
pub const TEMPLATE_PACK_STATUS: &str = include_str!("../templates/pack-status.jinja");

/// List — just pack names.
pub const TEMPLATE_LIST: &str = include_str!("../templates/list.jinja");

/// Simple message output (init, fill, adopt, addignore).
pub const TEMPLATE_MESSAGE: &str = include_str!("../templates/message.jinja");

/// Probe — deployment map, data-dir tree, summary. Branches on the
/// `kind` field of the serialized result.
pub const TEMPLATE_PROBE: &str = include_str!("../templates/probe.jinja");

/// Git filter installation snippets (`dodot git-show-filters`).
pub const TEMPLATE_GIT_FILTERS: &str = include_str!("../templates/git-filters.jinja");

/// Dismissed-prompt registry listing (`dodot prompts list`).
pub const TEMPLATE_PROMPTS_LIST: &str = include_str!("../templates/prompts-list.jinja");

/// `dodot transform check` per-file action list + optional unresolved-
/// marker section. See `commands::transform`.
pub const TEMPLATE_TRANSFORM_CHECK: &str = include_str!("../templates/transform-check.jinja");

/// `dodot transform install-hook` outcome message (created /
/// appended / already_installed).
pub const TEMPLATE_TRANSFORM_INSTALL_HOOK: &str =
    include_str!("../templates/transform-install-hook.jinja");

/// `dodot refresh` per-mode output (default report / quiet / list-paths).
pub const TEMPLATE_REFRESH: &str = include_str!("../templates/refresh.jinja");

/// `dodot template install-filter` outcome message.
pub const TEMPLATE_TEMPLATE_INSTALL_FILTER: &str =
    include_str!("../templates/template-install-filter.jinja");

/// `dodot transform status` per-file state list.
pub const TEMPLATE_TRANSFORM_STATUS: &str = include_str!("../templates/transform-status.jinja");

/// `dodot git-show-alias` print-for-paste output.
pub const TEMPLATE_GIT_SHOW_ALIAS: &str = include_str!("../templates/git-show-alias.jinja");

/// `dodot git-install-alias` outcome message.
pub const TEMPLATE_GIT_INSTALL_ALIAS: &str = include_str!("../templates/git-install-alias.jinja");

/// `dodot secret probe` per-provider state list. Surfaces each
/// configured provider's `probe()` outcome with the rendered
/// hint; treats "no providers configured" / "secrets disabled"
/// as a separate render branch.
pub const TEMPLATE_SECRET_PROBE: &str = include_str!("../templates/secret-probe.jinja");

// ── Tutorial step templates ─────────────────────────────────────
//
// One per step of the interactive tutorial. The CLI driver renders
// the appropriate template before each prompt.

pub const TEMPLATE_TUTORIAL_INTRO: &str = include_str!("../templates/tutorial/intro.jinja");
pub const TEMPLATE_TUTORIAL_CHECK_ROOT: &str =
    include_str!("../templates/tutorial/check_root.jinja");
pub const TEMPLATE_TUTORIAL_PICK_PACK: &str = include_str!("../templates/tutorial/pick_pack.jinja");
pub const TEMPLATE_TUTORIAL_NO_PACKS: &str = include_str!("../templates/tutorial/no_packs.jinja");
pub const TEMPLATE_TUTORIAL_SHOW_STATUS: &str =
    include_str!("../templates/tutorial/show_status.jinja");
pub const TEMPLATE_TUTORIAL_ANNOTATE_STATUS: &str =
    include_str!("../templates/tutorial/annotate_status.jinja");
pub const TEMPLATE_TUTORIAL_CONCEPT_TARGETS: &str =
    include_str!("../templates/tutorial/concept_targets.jinja");
pub const TEMPLATE_TUTORIAL_CONCEPT_SHELL: &str =
    include_str!("../templates/tutorial/concept_shell.jinja");
pub const TEMPLATE_TUTORIAL_DRY_RUN: &str = include_str!("../templates/tutorial/dry_run.jinja");
pub const TEMPLATE_TUTORIAL_REAL_UP: &str = include_str!("../templates/tutorial/real_up.jinja");
pub const TEMPLATE_TUTORIAL_OUTRO: &str = include_str!("../templates/tutorial/outro.jinja");

/// Pairs of `(name, body)` for every tutorial step template.
///
/// `render_tutorial_step` looks up the body by name and renders
/// against a fresh theme each call — no shared `Renderer` is
/// retained, since each tutorial run renders fewer than a dozen
/// templates and the per-call cost is negligible.
pub const TUTORIAL_STEP_TEMPLATES: &[(&str, &str)] = &[
    ("tutorial.intro", TEMPLATE_TUTORIAL_INTRO),
    ("tutorial.check_root", TEMPLATE_TUTORIAL_CHECK_ROOT),
    ("tutorial.pick_pack", TEMPLATE_TUTORIAL_PICK_PACK),
    ("tutorial.no_packs", TEMPLATE_TUTORIAL_NO_PACKS),
    ("tutorial.show_status", TEMPLATE_TUTORIAL_SHOW_STATUS),
    (
        "tutorial.annotate_status",
        TEMPLATE_TUTORIAL_ANNOTATE_STATUS,
    ),
    (
        "tutorial.concept_targets",
        TEMPLATE_TUTORIAL_CONCEPT_TARGETS,
    ),
    ("tutorial.concept_shell", TEMPLATE_TUTORIAL_CONCEPT_SHELL),
    ("tutorial.dry_run", TEMPLATE_TUTORIAL_DRY_RUN),
    ("tutorial.real_up", TEMPLATE_TUTORIAL_REAL_UP),
    ("tutorial.outro", TEMPLATE_TUTORIAL_OUTRO),
];

/// Render a tutorial step template with the dodot theme.
///
/// `mode` controls colour output: `OutputMode::Term` for ANSI in a
/// real terminal, `OutputMode::Text` for tests / non-TTY.
pub fn render_tutorial_step<T: serde::Serialize>(
    step: &str,
    data: &T,
    mode: OutputMode,
) -> Result<String> {
    let body = TUTORIAL_STEP_TEMPLATES
        .iter()
        .find_map(|(name, body)| (*name == step).then_some(*body))
        .ok_or_else(|| crate::DodotError::Other(format!("unknown tutorial template: {step}")))?;

    let theme = create_theme();
    render_with_output(body, data, &theme, mode)
        .map_err(|e| crate::DodotError::Other(format!("tutorial render: {e}")))
}

// ── Renderer ────────────────────────────────────────────────────

/// Create the dodot theme from the embedded YAML definition.
pub fn create_theme() -> Theme {
    Theme::from_yaml(THEME_YAML).expect("built-in theme YAML must be valid")
}

/// Create a pre-compiled renderer with all dodot templates registered.
pub fn create_renderer() -> Renderer {
    let theme = create_theme();
    let mut renderer = Renderer::new(theme).expect("renderer creation must succeed");
    renderer
        .add_template("pack-status", TEMPLATE_PACK_STATUS)
        .unwrap();
    renderer.add_template("list", TEMPLATE_LIST).unwrap();
    renderer.add_template("message", TEMPLATE_MESSAGE).unwrap();
    renderer.add_template("probe", TEMPLATE_PROBE).unwrap();
    renderer
        .add_template("git-filters", TEMPLATE_GIT_FILTERS)
        .unwrap();
    renderer
        .add_template("prompts-list", TEMPLATE_PROMPTS_LIST)
        .unwrap();
    renderer
}

/// Render a template with the given data and output mode.
///
/// For JSON mode, serializes the data directly (not through the
/// template) to produce machine-readable output.
pub fn render<T: serde::Serialize>(
    template_name: &str,
    data: &T,
    mode: OutputMode,
) -> Result<String> {
    if matches!(mode, OutputMode::Json) {
        return serde_json::to_string_pretty(data)
            .map_err(|e| crate::DodotError::Other(format!("JSON serialization failed: {e}")));
    }

    let theme = create_theme();
    let template = match template_name {
        "pack-status" => TEMPLATE_PACK_STATUS,
        "list" => TEMPLATE_LIST,
        "message" => TEMPLATE_MESSAGE,
        "probe" => TEMPLATE_PROBE,
        "git-filters" => TEMPLATE_GIT_FILTERS,
        "prompts-list" => TEMPLATE_PROMPTS_LIST,
        other => {
            return Err(crate::DodotError::Other(format!(
                "unknown template: {other}"
            )))
        }
    };

    render_with_output(template, data, &theme, mode)
        .map_err(|e| crate::DodotError::Other(format!("render failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_parses_without_error() {
        let _theme = create_theme();
    }

    #[test]
    fn renderer_creates_with_all_templates() {
        let _renderer = create_renderer();
    }

    #[test]
    fn render_pack_status_text_mode() {
        use serde::Serialize;

        #[derive(Serialize)]
        struct Data {
            message: Option<String>,
            dry_run: bool,
            packs: Vec<Pack>,
        }
        #[derive(Serialize)]
        struct Pack {
            name: String,
            files: Vec<File>,
        }
        #[derive(Serialize)]
        struct File {
            name: String,
            symbol: String,
            description: String,
            status: String,
            status_label: String,
        }

        let data = Data {
            message: None,
            dry_run: false,
            packs: vec![Pack {
                name: "vim".into(),
                files: vec![File {
                    name: "vimrc".into(),
                    symbol: "➞".into(),
                    description: "~/.vimrc".into(),
                    status: "deployed".into(),
                    status_label: "deployed".into(),
                }],
            }],
        };

        let output = render("pack-status", &data, OutputMode::Text).unwrap();
        assert!(output.contains("vim"));
        assert!(output.contains("vimrc"));
        assert!(output.contains("deployed"));
    }

    #[test]
    fn all_tutorial_templates_render_in_text_mode() {
        // Every tutorial step template must parse and render with a
        // populated context — this catches Jinja-syntax mistakes at
        // build time rather than mid-tutorial.
        use crate::commands::tutorial::{TutorialCtx, TutorialPack};

        let ctx = TutorialCtx {
            dotfiles_root: "/home/example/dotfiles".into(),
            via: "DOTFILES_ROOT env var".into(),
            packs: vec![
                TutorialPack {
                    name: "vim".into(),
                    kind: "config only".into(),
                    recommended: true,
                },
                TutorialPack {
                    name: "zsh".into(),
                    kind: "config + shell".into(),
                    recommended: false,
                },
            ],
            chosen_pack: Some("vim".into()),
            chosen_pack_kind: Some("config only".into()),
            status_output: Some("(rendered status would go here)".into()),
            dry_run_output: Some("(dry-run output)".into()),
            up_output: Some("(up output)".into()),
            shell_integration: Some(crate::commands::tutorial::ShellIntegration {
                shell_kind: "zsh".into(),
                rc_path: "~/.zshrc".into(),
                rc_path_abs: std::path::PathBuf::new(),
                line_present: false,
                eval_line: r#"eval "$(dodot init-sh)""#.into(),
            }),
            eval_line: r#"eval "$(dodot init-sh)""#.into(),
            ..Default::default()
        };

        for (name, _) in TUTORIAL_STEP_TEMPLATES {
            let out = render_tutorial_step(name, &ctx, OutputMode::Text)
                .unwrap_or_else(|e| panic!("template {name} failed: {e}"));
            assert!(!out.is_empty(), "template {name} produced empty output");
        }
    }

    #[test]
    fn json_mode_produces_json() {
        use serde::Serialize;

        #[derive(Serialize)]
        struct Data {
            name: String,
        }

        let data = Data {
            name: "test".into(),
        };

        let output = render("list", &data, OutputMode::Json).unwrap();
        assert!(output.contains("\"name\""));
        assert!(output.contains("\"test\""));
    }
}

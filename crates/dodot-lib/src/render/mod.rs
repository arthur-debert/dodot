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
"#;

// ── Templates ───────────────────────────────────────────────────

/// Status / up / down — pack-level output with file listings.
pub const TEMPLATE_PACK_STATUS: &str = r#"{% if message %}[message]{{ message }}[/message]
{% endif %}{% if dry_run %}[dry-run]  (dry run — no changes made)[/dry-run]
{% endif %}{% for pack in packs %}[pack-name]{{ pack.name }}[/pack-name]
{% for file in pack.files %}  {{ file.name | col(24) }} [handler-symbol]{{ file.symbol }}[/handler-symbol] [description]{{ file.description | col(30) }}[/description]  [{{ file.status }}]{{ file.status_label }}[/{{ file.status }}]
{% endfor %}
{% endfor %}"#;

/// List — just pack names.
pub const TEMPLATE_LIST: &str = r#"{% for pack in packs %}{{ pack.name }}{% if pack.ignored %} [dim](ignored)[/dim]{% endif %}
{% endfor %}"#;

/// Simple message output (init, fill, adopt, addignore).
pub const TEMPLATE_MESSAGE: &str = r#"{% if message %}[message]{{ message }}[/message]
{% endif %}{% for line in details %}  {{ line }}
{% endfor %}"#;

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

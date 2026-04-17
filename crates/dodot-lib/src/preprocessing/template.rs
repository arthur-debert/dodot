//! Template preprocessor — renders Jinja2-style templates via MiniJinja.
//!
//! Matches files with configurable extensions (default: `.tmpl`,
//! `.template`), renders them against a variable context with three
//! namespaces:
//!
//! - `dodot.*` — built-in values (os, arch, hostname, username, home,
//!   dotfiles_root), computed once at preprocessor construction.
//! - `env.*` — dynamic lookup of process environment variables.
//! - bare names — user-defined variables from
//!   `[preprocessor.template.vars]` in `.dodot.toml`.
//!
//! Uses MiniJinja strict undefined-behaviour: references to missing vars
//! raise a render error rather than silently producing empty strings.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use minijinja::value::{Enumerator, Object, ObjectRepr, Value};
use minijinja::{Environment, UndefinedBehavior};

use crate::fs::Fs;
use crate::paths::Pather;
use crate::preprocessing::{ExpandedFile, Preprocessor, TransformType};
use crate::{DodotError, Result};

/// Reserved top-level variable names.
const RESERVED_VARS: &[&str] = &["dodot", "env"];

/// MiniJinja object that looks up process environment variables on
/// attribute access. `{{ env.SHELL }}` becomes `std::env::var("SHELL")`.
/// Missing env vars return `None` from `get_value`, which MiniJinja
/// treats as an undefined attribute (a render error under strict mode).
/// For optional variables, use `{{ env.NAME | default("...") }}`.
#[derive(Debug)]
struct EnvLookup;

impl Object for EnvLookup {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        let name = key.as_str()?;
        std::env::var(name).ok().map(Value::from)
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        // Don't enumerate environment variables — printing `{{ env }}`
        // as a whole shouldn't dump the whole process environment.
        Enumerator::NonEnumerable
    }
}

/// Template rendering preprocessor. Generative (one-way) transform.
pub struct TemplatePreprocessor {
    extensions: Vec<String>,
    env: Environment<'static>,
}

impl std::fmt::Debug for TemplatePreprocessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TemplatePreprocessor")
            .field("extensions", &self.extensions)
            .finish_non_exhaustive()
    }
}

impl TemplatePreprocessor {
    /// Construct a new template preprocessor.
    ///
    /// Validates that no user-defined variable uses a reserved name
    /// (`dodot` or `env`). Populates the MiniJinja environment with
    /// the `dodot.*` builtins from `pather` + system info, an `env.*`
    /// dynamic lookup, and each user variable as a bare global.
    ///
    /// Extensions are normalized at construction: a leading dot (e.g.
    /// `".tmpl"`) is stripped so both `"tmpl"` and `".tmpl"` work.
    pub fn new(
        extensions: Vec<String>,
        user_vars: HashMap<String, String>,
        pather: &dyn Pather,
    ) -> Result<Self> {
        for name in user_vars.keys() {
            if RESERVED_VARS.contains(&name.as_str()) {
                return Err(DodotError::TemplateReservedVar { name: name.clone() });
            }
        }

        let extensions: Vec<String> = extensions
            .into_iter()
            .map(|e| e.trim_start_matches('.').to_string())
            .collect();

        let mut env = Environment::new();
        env.set_undefined_behavior(UndefinedBehavior::Strict);

        env.add_global("dodot", Value::from(build_dodot_context(pather)));
        env.add_global("env", Value::from_object(EnvLookup));

        for (name, val) in user_vars {
            env.add_global(name, Value::from(val));
        }

        Ok(Self { extensions, env })
    }
}

impl Preprocessor for TemplatePreprocessor {
    fn name(&self) -> &str {
        "template"
    }

    fn transform_type(&self) -> TransformType {
        TransformType::Generative
    }

    fn matches_extension(&self, filename: &str) -> bool {
        // Extensions are normalized (no leading dot) at construction.
        // We require a literal "." before the extension to avoid e.g.
        // "mpl" matching "foo.tmpl". No per-call allocation.
        self.extensions.iter().any(|ext| {
            filename
                .strip_suffix(ext.as_str())
                .is_some_and(|prefix| prefix.ends_with('.'))
        })
    }

    fn stripped_name(&self, filename: &str) -> String {
        // If multiple configured extensions match (e.g. "tmpl" and
        // "j2.tmpl" both suffixes of the same filename), prefer the
        // longest so behaviour is deterministic and independent of
        // config ordering.
        self.extensions
            .iter()
            .filter_map(|ext| {
                filename
                    .strip_suffix(ext.as_str())
                    .and_then(|prefix| prefix.strip_suffix('.'))
                    .map(|stripped| (ext.len(), stripped))
            })
            .max_by_key(|(len, _)| *len)
            .map(|(_, stripped)| stripped.to_string())
            .unwrap_or_else(|| filename.to_string())
    }

    fn expand(&self, source: &Path, fs: &dyn Fs) -> Result<Vec<ExpandedFile>> {
        let template_str = fs.read_to_string(source)?;

        let rendered =
            self.env
                .render_str(&template_str, ())
                .map_err(|e| DodotError::TemplateRender {
                    source_file: source.to_path_buf(),
                    message: format_minijinja_error(&e),
                })?;

        let filename = source
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let stripped = self.stripped_name(&filename);

        Ok(vec![ExpandedFile {
            relative_path: PathBuf::from(stripped),
            content: rendered.into_bytes(),
            is_dir: false,
        }])
    }
}

/// Build the `dodot.*` namespace map.
///
/// Keys we can always resolve (os, arch, home, dotfiles_root) are
/// always inserted. Keys that depend on environment detection
/// (hostname, username) are inserted only when a non-empty value is
/// found — otherwise they are omitted so that template access via
/// `{{ dodot.hostname }}` triggers a strict-undefined render error,
/// rather than silently injecting an empty string. Users who want a
/// fallback can write `{{ dodot.hostname | default("unknown") }}`.
///
/// Hostname and username detection is cached process-wide via
/// [`OnceLock`] so that building a template preprocessor for each pack
/// does not respawn `hostname(1)` every time.
fn build_dodot_context(pather: &dyn Pather) -> BTreeMap<String, String> {
    let mut ctx = BTreeMap::new();
    ctx.insert("os".into(), std::env::consts::OS.into());
    ctx.insert("arch".into(), std::env::consts::ARCH.into());
    if let Some(h) = cached_hostname() {
        ctx.insert("hostname".into(), h.clone());
    }
    if let Some(u) = cached_username() {
        ctx.insert("username".into(), u.clone());
    }
    ctx.insert("home".into(), pather.home_dir().display().to_string());
    ctx.insert(
        "dotfiles_root".into(),
        pather.dotfiles_root().display().to_string(),
    );
    ctx
}

/// Process-wide cached hostname. First call resolves and pins the
/// result for the lifetime of the process.
fn cached_hostname() -> Option<&'static String> {
    static CACHE: OnceLock<Option<String>> = OnceLock::new();
    CACHE.get_or_init(detect_hostname).as_ref()
}

/// Process-wide cached username. Same caching semantics as
/// [`cached_hostname`].
fn cached_username() -> Option<&'static String> {
    static CACHE: OnceLock<Option<String>> = OnceLock::new();
    CACHE.get_or_init(detect_username).as_ref()
}

fn detect_hostname() -> Option<String> {
    if let Ok(h) = std::env::var("HOSTNAME") {
        if !h.is_empty() {
            return Some(h);
        }
    }
    // Fallback: shell out. Ignore errors.
    let output = std::process::Command::new("hostname").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn detect_username() -> Option<String> {
    for var in ["USER", "USERNAME", "LOGNAME"] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

/// Compact a MiniJinja error into a single human-readable string with
/// a suggestion for the common "undefined variable" case.
fn format_minijinja_error(err: &minijinja::Error) -> String {
    use minijinja::ErrorKind;

    let base = match err.kind() {
        ErrorKind::UndefinedError => {
            // Best-effort: MiniJinja's error message already says
            // "undefined value" but doesn't always name the variable.
            // The Display impl includes line info.
            let mut msg = err.to_string();
            msg.push_str(
                "\n  hint: define the variable in [preprocessor.template.vars] in .dodot.toml,\n  or reference an environment variable with {{ env.NAME }} (with a default filter if optional)",
            );
            msg
        }
        ErrorKind::SyntaxError => err.to_string(),
        _ => err.to_string(),
    };

    // MiniJinja sometimes appends "referenced from" traces; strip them
    // to keep the error message compact.
    base.lines().take(10).collect::<Vec<_>>().join("\n  ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::XdgPather;

    fn make_pather() -> XdgPather {
        XdgPather::builder()
            .home("/home/alice")
            .dotfiles_root("/home/alice/dotfiles")
            .xdg_config_home("/home/alice/.config")
            .data_dir("/home/alice/.local/share/dodot")
            .build()
            .unwrap()
    }

    fn new_pp(vars: HashMap<String, String>) -> TemplatePreprocessor {
        TemplatePreprocessor::new(vec!["tmpl".into(), "template".into()], vars, &make_pather())
            .unwrap()
    }

    // ── Trait basics ────────────────────────────────────────────

    #[test]
    fn trait_properties() {
        let pp = new_pp(HashMap::new());
        assert_eq!(pp.name(), "template");
        assert_eq!(pp.transform_type(), TransformType::Generative);
    }

    #[test]
    fn matches_default_extensions() {
        let pp = new_pp(HashMap::new());
        assert!(pp.matches_extension("config.toml.tmpl"));
        assert!(pp.matches_extension("config.toml.template"));
        assert!(!pp.matches_extension("config.toml"));
        assert!(!pp.matches_extension("config.tmpl.bak"));
    }

    #[test]
    fn matches_custom_extension() {
        let pp =
            TemplatePreprocessor::new(vec!["j2".into()], HashMap::new(), &make_pather()).unwrap();
        assert!(pp.matches_extension("nginx.conf.j2"));
        assert!(!pp.matches_extension("nginx.conf.tmpl"));
    }

    #[test]
    fn stripped_name_removes_either_extension() {
        let pp = new_pp(HashMap::new());
        assert_eq!(pp.stripped_name("config.toml.tmpl"), "config.toml");
        assert_eq!(pp.stripped_name("config.toml.template"), "config.toml");
        assert_eq!(pp.stripped_name("already-stripped"), "already-stripped");
    }

    // ── Reserved variable names ─────────────────────────────────

    #[test]
    fn reserved_dodot_var_rejected() {
        let mut vars = HashMap::new();
        vars.insert("dodot".into(), "x".into());
        let err = TemplatePreprocessor::new(vec!["tmpl".into()], vars, &make_pather()).unwrap_err();
        assert!(
            matches!(err, DodotError::TemplateReservedVar { ref name } if name == "dodot"),
            "got: {err}"
        );
    }

    #[test]
    fn reserved_env_var_rejected() {
        let mut vars = HashMap::new();
        vars.insert("env".into(), "x".into());
        let err = TemplatePreprocessor::new(vec!["tmpl".into()], vars, &make_pather()).unwrap_err();
        assert!(matches!(err, DodotError::TemplateReservedVar { .. }));
    }

    // ── Rendering ───────────────────────────────────────────────

    #[test]
    fn renders_user_var() {
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("greeting.tmpl", "hello {{ name }}")
            .done()
            .build();

        let mut vars = HashMap::new();
        vars.insert("name".into(), "Alice".into());
        let pp = TemplatePreprocessor::new(vec!["tmpl".into()], vars, env.paths.as_ref()).unwrap();

        let source = env.dotfiles_root.join("app/greeting.tmpl");
        let result = pp.expand(&source, env.fs.as_ref()).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].relative_path, PathBuf::from("greeting"));
        assert_eq!(String::from_utf8_lossy(&result[0].content), "hello Alice");
    }

    #[test]
    fn renders_dodot_builtins() {
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file(
                "info.tmpl",
                "home={{ dodot.home }} root={{ dodot.dotfiles_root }} os={{ dodot.os }}",
            )
            .done()
            .build();

        let pp = TemplatePreprocessor::new(vec!["tmpl".into()], HashMap::new(), env.paths.as_ref())
            .unwrap();

        let source = env.dotfiles_root.join("app/info.tmpl");
        let result = pp.expand(&source, env.fs.as_ref()).unwrap();

        let rendered = String::from_utf8_lossy(&result[0].content);
        let home = env.paths.home_dir().display().to_string();
        let root = env.paths.dotfiles_root().display().to_string();
        assert!(
            rendered.contains(&format!("home={home}")),
            "rendered: {rendered}"
        );
        assert!(
            rendered.contains(&format!("root={root}")),
            "rendered: {rendered}"
        );
        assert!(rendered.contains(&format!("os={}", std::env::consts::OS)));
    }

    #[test]
    fn renders_env_var() {
        // Use a likely-present env var with a fallback for determinism.
        // PATH should always be set during tests.
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("has_path.tmpl", "path={{ env.PATH }}")
            .done()
            .build();

        let pp = TemplatePreprocessor::new(vec!["tmpl".into()], HashMap::new(), env.paths.as_ref())
            .unwrap();

        let source = env.dotfiles_root.join("app/has_path.tmpl");
        let result = pp.expand(&source, env.fs.as_ref()).unwrap();
        let rendered = String::from_utf8_lossy(&result[0].content).into_owned();

        assert!(rendered.starts_with("path="));
        assert!(
            rendered.len() > "path=".len(),
            "env.PATH should have some value"
        );
    }

    #[test]
    fn missing_env_var_errors() {
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("bad.tmpl", "value={{ env.DEFINITELY_UNSET_VAR_ZZZ_12345 }}")
            .done()
            .build();

        let pp = TemplatePreprocessor::new(vec!["tmpl".into()], HashMap::new(), env.paths.as_ref())
            .unwrap();

        let source = env.dotfiles_root.join("app/bad.tmpl");
        // Ensure the env var is genuinely unset
        std::env::remove_var("DEFINITELY_UNSET_VAR_ZZZ_12345");
        let err = pp.expand(&source, env.fs.as_ref()).unwrap_err();
        assert!(
            matches!(err, DodotError::TemplateRender { ref source_file, .. } if source_file == &source),
            "got: {err}"
        );
    }

    #[test]
    fn undefined_user_var_errors() {
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("bad.tmpl", "value={{ not_defined }}")
            .done()
            .build();

        let pp = TemplatePreprocessor::new(vec!["tmpl".into()], HashMap::new(), env.paths.as_ref())
            .unwrap();

        let source = env.dotfiles_root.join("app/bad.tmpl");
        let err = pp.expand(&source, env.fs.as_ref()).unwrap_err();
        assert!(
            matches!(err, DodotError::TemplateRender { ref message, .. } if message.contains("not_defined") || message.contains("undefined")),
            "got: {err}"
        );
    }

    #[test]
    fn syntax_error_reports_source_file() {
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("broken.tmpl", "{% if %}unterminated")
            .done()
            .build();

        let pp = TemplatePreprocessor::new(vec!["tmpl".into()], HashMap::new(), env.paths.as_ref())
            .unwrap();

        let source = env.dotfiles_root.join("app/broken.tmpl");
        let err = pp.expand(&source, env.fs.as_ref()).unwrap_err();
        assert!(
            matches!(err, DodotError::TemplateRender { ref source_file, .. } if source_file == &source),
            "got: {err}"
        );
    }

    #[test]
    fn renders_filters_and_conditionals() {
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file(
                "multi.tmpl",
                "NAME={{ name | upper }}\n{% if show %}shown{% else %}hidden{% endif %}",
            )
            .done()
            .build();

        let mut vars = HashMap::new();
        vars.insert("name".into(), "alice".into());
        vars.insert("show".into(), "true".into());
        let pp = TemplatePreprocessor::new(vec!["tmpl".into()], vars, env.paths.as_ref()).unwrap();

        let source = env.dotfiles_root.join("app/multi.tmpl");
        let result = pp.expand(&source, env.fs.as_ref()).unwrap();
        let rendered = String::from_utf8_lossy(&result[0].content);
        assert!(rendered.contains("NAME=ALICE"), "rendered: {rendered}");
        assert!(rendered.contains("shown"), "rendered: {rendered}");
    }

    #[test]
    fn renders_empty_template() {
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("empty.tmpl", "")
            .done()
            .build();

        let pp = TemplatePreprocessor::new(vec!["tmpl".into()], HashMap::new(), env.paths.as_ref())
            .unwrap();

        let source = env.dotfiles_root.join("app/empty.tmpl");
        let result = pp.expand(&source, env.fs.as_ref()).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].content.is_empty());
    }

    #[test]
    fn renders_template_without_substitutions() {
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("plain.tmpl", "just plain text\nno vars here")
            .done()
            .build();

        let pp = TemplatePreprocessor::new(vec!["tmpl".into()], HashMap::new(), env.paths.as_ref())
            .unwrap();

        let source = env.dotfiles_root.join("app/plain.tmpl");
        let result = pp.expand(&source, env.fs.as_ref()).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&result[0].content),
            "just plain text\nno vars here"
        );
    }

    #[test]
    fn extension_with_leading_dot_still_matches() {
        // Tolerate config that writes extensions as `.tmpl` instead of
        // `tmpl`. Without the leading-dot trim, `.ends_with("..tmpl")`
        // would silently never match and templates would not be processed.
        let pp = TemplatePreprocessor::new(
            vec![".tmpl".into(), ".template".into()],
            HashMap::new(),
            &make_pather(),
        )
        .unwrap();
        assert!(pp.matches_extension("config.toml.tmpl"));
        assert!(pp.matches_extension("app.template"));
        assert_eq!(pp.stripped_name("config.toml.tmpl"), "config.toml");
    }

    #[test]
    fn overlapping_suffix_does_not_false_match() {
        // If a user configures an extension that is a suffix of another
        // legitimate filename part (e.g. "mpl" as a suffix of "tmpl"),
        // the matcher must require the literal "." boundary before the
        // extension — otherwise "foo.tmpl" would be wrongly recognised
        // as a "mpl" template and stripped to "foo.t".
        let pp =
            TemplatePreprocessor::new(vec!["mpl".into()], HashMap::new(), &make_pather()).unwrap();
        assert!(!pp.matches_extension("foo.tmpl"));
        assert_eq!(pp.stripped_name("foo.tmpl"), "foo.tmpl");

        // Files that legitimately end with `.mpl` still match.
        assert!(pp.matches_extension("song.mpl"));
        assert_eq!(pp.stripped_name("song.mpl"), "song");
    }

    #[test]
    fn overlapping_extensions_prefer_longest_match() {
        // If a filename ends with both configured extensions (e.g.
        // "foo.j2.tmpl" matches both "tmpl" and "j2.tmpl"), prefer the
        // longest match so behaviour is deterministic regardless of
        // config ordering.
        let pp = TemplatePreprocessor::new(
            vec!["tmpl".into(), "j2.tmpl".into()],
            HashMap::new(),
            &make_pather(),
        )
        .unwrap();
        assert_eq!(pp.stripped_name("config.j2.tmpl"), "config");

        // Opposite config order yields the same result.
        let pp_reversed = TemplatePreprocessor::new(
            vec!["j2.tmpl".into(), "tmpl".into()],
            HashMap::new(),
            &make_pather(),
        )
        .unwrap();
        assert_eq!(pp_reversed.stripped_name("config.j2.tmpl"), "config");
    }

    #[test]
    fn missing_dodot_key_raises_strict_error() {
        // The `build_dodot_context` fix omits `hostname`/`username` from
        // the map when they cannot be detected (rather than injecting
        // empty strings, which would silently deploy broken configs).
        //
        // We avoid manipulating `std::env` here (not thread-safe; other
        // tests read USER) and instead verify the underlying invariant:
        // any missing key on the `dodot` object triggers the
        // strict-undefined error. Under this invariant, an undetected
        // username/hostname behaves the same as any other missing key.
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("uses_missing.tmpl", "value={{ dodot.nonexistent_key_zzz }}")
            .done()
            .build();

        let pp = TemplatePreprocessor::new(vec!["tmpl".into()], HashMap::new(), env.paths.as_ref())
            .unwrap();

        let source = env.dotfiles_root.join("app/uses_missing.tmpl");
        let err = pp.expand(&source, env.fs.as_ref()).unwrap_err();
        assert!(
            matches!(err, DodotError::TemplateRender { .. }),
            "accessing a missing dodot.* key must error, got: {err}"
        );
    }

    #[test]
    fn missing_dodot_key_can_be_defaulted() {
        // Ergonomic escape hatch: Jinja's `default` filter lets users
        // tolerate potentially-missing fields without raising.
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file(
                "defaulted.tmpl",
                "value={{ dodot.nonexistent_key_zzz | default(\"unknown\") }}",
            )
            .done()
            .build();

        let pp = TemplatePreprocessor::new(vec!["tmpl".into()], HashMap::new(), env.paths.as_ref())
            .unwrap();

        let source = env.dotfiles_root.join("app/defaulted.tmpl");
        let result = pp.expand(&source, env.fs.as_ref()).unwrap();
        assert_eq!(String::from_utf8_lossy(&result[0].content), "value=unknown");
    }

    #[test]
    fn build_dodot_context_omits_undetected_optional_keys() {
        // Directly exercise the map-building helper: given a Pather but
        // the detection helpers return None (simulated via testing the
        // helper return invariants), verify the map structure.
        //
        // Since `detect_username`/`detect_hostname` read real env/system
        // state, we can only assert: if they return Some, the key is
        // present; if they return None, the key is absent.
        let ctx = build_dodot_context(&make_pather());

        // These are always present:
        assert!(ctx.contains_key("os"));
        assert!(ctx.contains_key("arch"));
        assert!(ctx.contains_key("home"));
        assert!(ctx.contains_key("dotfiles_root"));

        // Optional keys: present iff the detection helper returned Some.
        assert_eq!(ctx.contains_key("username"), detect_username().is_some());
        assert_eq!(ctx.contains_key("hostname"), detect_hostname().is_some());
    }
}

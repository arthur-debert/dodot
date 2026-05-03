//! Template preprocessor — renders Jinja2-style templates via MiniJinja
//! through burgertocow's [`Tracker`].
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
//!
//! # Tracked render
//!
//! Rendering goes through [`burgertocow::Tracker`] rather than a raw
//! `minijinja::Environment`. The tracker installs a custom formatter that
//! wraps every variable emission in marker bytes, producing a
//! [`TrackedRender`] alongside the visible output. The visible output is
//! identical to the plain-MiniJinja path (modulo the
//! `keep_trailing_newline` setting that Tracker also applies). The
//! marker-annotated string is persisted in the baseline cache so the
//! reverse-merge pipeline (`dodot transform check`, the clean filter)
//! can compute template-space diffs without re-rendering — re-rendering
//! at clean-filter time would re-trigger any secret-provider auth
//! prompts on every `git status`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use burgertocow::Tracker;
use minijinja::value::{Enumerator, Object, ObjectRepr, Value};
use minijinja::{Error as MjError, ErrorKind as MjErrorKind, UndefinedBehavior};
use sha2::{Digest, Sha256};

use crate::fs::Fs;
use crate::paths::Pather;
use crate::preprocessing::{ExpandedFile, Preprocessor, SecretLineRange, TransformType};
use crate::secret::SecretRegistry;
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
///
/// Holds the resolved `dodot.*` map, the user-defined variables, and a
/// pre-computed context hash. Each `expand` call constructs a fresh
/// [`Tracker`], installs the namespaces, registers the source file as a
/// named template, and renders. We don't share the `Tracker` across
/// renders because `add_template` requires `&mut` while `Preprocessor::
/// expand` runs through a `&self` trait method — a per-call tracker is
/// the simplest way to keep the pipeline's existing concurrency shape.
pub struct TemplatePreprocessor {
    extensions: Vec<String>,
    dodot_ns: BTreeMap<String, String>,
    user_vars: BTreeMap<String, String>,
    /// SHA-256 of the deterministic projection of `dodot_ns` and
    /// `user_vars` (sorted keys, length-prefixed). Reused as the
    /// `context_hash` for every render this preprocessor performs.
    ///
    /// `env.*` references are intentionally **not** part of the
    /// context hash and tracking them is out of scope by design — see
    /// `preprocessing-pipeline.lex` §6.4. The cache contract is
    /// "same source bytes + same `dodot.*` namespace + same
    /// `user_vars` → same output." The `env.*` namespace is the
    /// explicitly live-read zone; rotating a referenced env var does
    /// not invalidate the cache, and users pick up the new value via
    /// `dodot up --force`. Stable values that should participate in
    /// invalidation belong in `[preprocessor.template.vars]`
    /// (`user_vars`), not `env.*`.
    context_hash: [u8; 32],
    /// Optional secret-resolution registry. Populated via
    /// [`Self::with_secret_registry`] when secrets are configured.
    /// `None` means `secret(...)` is unavailable in templates and a
    /// `secret(...)` call surfaces as a render error pointing the
    /// user at `[secret] enabled = true`. See `secrets.lex` §5.
    secret_registry: Option<Arc<SecretRegistry>>,
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
    /// (`dodot` or `env`). Resolves the `dodot.*` builtins from
    /// `pather` + system info and computes the context hash now so
    /// every subsequent `expand` reuses the same value.
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

        let dodot_ns = build_dodot_context(pather);
        let user_vars: BTreeMap<String, String> = user_vars.into_iter().collect();
        let context_hash = compute_context_hash(&dodot_ns, &user_vars);

        Ok(Self {
            extensions,
            dodot_ns,
            user_vars,
            context_hash,
            secret_registry: None,
        })
    }

    /// Wire a [`SecretRegistry`] into this preprocessor. Templates
    /// rendered through it can call `{{ secret("op://Vault/Item/Field") }}`
    /// (and other configured schemes); calls dispatch through the
    /// registry, populate the per-render sidecar, and refuse
    /// multi-line values per `secrets.lex` §3.4. Without a registry
    /// (the default), any `secret(...)` call in a template surfaces
    /// as a render error.
    pub fn with_secret_registry(mut self, registry: Arc<SecretRegistry>) -> Self {
        self.secret_registry = Some(registry);
        self
    }

    /// Build a fresh tracker with this preprocessor's namespaces
    /// installed and `UndefinedBehavior::Strict` set. Called per render
    /// because `Tracker::add_template` requires `&mut self`.
    ///
    /// `sidecar` is the per-render secret-tracking accumulator. The
    /// `secret(...)` MiniJinja function returns a unique private-use
    /// sentinel (rather than the raw secret value) and pushes a
    /// [`SecretCallEntry`] into the accumulator. The caller (the
    /// `expand` body) walks the rendered output to find each sentinel,
    /// records its line position, then substitutes the sentinel back
    /// to the real value. This avoids the substring-collision failure
    /// mode where a secret value happens to also appear elsewhere in
    /// the rendered text.
    ///
    /// `render_id` is a per-render monotonic counter used in the
    /// sentinel format so two concurrent renders can't observe each
    /// other's sentinels.
    fn make_tracker(&self, sidecar: Arc<Mutex<Vec<SecretCallEntry>>>, render_id: u64) -> Tracker {
        let mut tracker = Tracker::new();
        let env = tracker.env_mut();
        env.set_undefined_behavior(UndefinedBehavior::Strict);
        env.add_global("dodot", Value::from(self.dodot_ns.clone()));
        env.add_global("env", Value::from_object(EnvLookup));
        for (name, val) in &self.user_vars {
            env.add_global(name.clone(), Value::from(val.clone()));
        }

        // Install the `secret(...)` function. Two cases:
        //
        // - Registry configured: function dispatches through the
        //   registry. Refuses multi-line values per §3.4. Records
        //   the (reference, value) pair into `sidecar` so the
        //   caller can compute line ranges after rendering.
        // - No registry: function still exists, but every call
        //   surfaces a clean render error pointing the user at
        //   `[secret] enabled = true`. The presence-without-function
        //   alternative would surface as MiniJinja's generic
        //   "undefined" error which doesn't tell the user how to
        //   fix the config.
        match &self.secret_registry {
            Some(registry) => {
                let registry = registry.clone();
                let sidecar = sidecar.clone();
                env.add_function(
                    "secret",
                    move |reference: &str| -> std::result::Result<String, MjError> {
                        // Within-run cache: first call for a given
                        // reference goes to the provider; subsequent
                        // calls (in this template or any other
                        // rendered through the same registry
                        // instance) hit the cache and never shell
                        // out. Multi-line / non-UTF-8 are detected
                        // up here so the rich error messages stay
                        // co-located with the callback that surfaces
                        // them; only validated values reach the
                        // cache. See `secrets.lex` §7.4 / §3.4.
                        //
                        // The cache holds `Arc<SecretString>` so the
                        // resolved bytes get zeroized when the
                        // registry's last reference drops. We expose
                        // to `&str` only at this substitution
                        // boundary (and the resulting String is
                        // immediately handed to MiniJinja), keeping
                        // the unsealed plaintext window as narrow as
                        // the rendering pipeline allows.
                        let secret = if let Some(cached) = registry.cache_get(reference) {
                            cached
                        } else {
                            let value = registry.resolve(reference).map_err(|e| {
                                MjError::new(MjErrorKind::InvalidOperation, e.to_string())
                            })?;
                            if value.contains_newline() {
                                return Err(MjError::new(
                                    MjErrorKind::InvalidOperation,
                                    format!(
                                        "secret `{reference}` resolved to a multi-line value. \
                                     Value-injection (`{{{{ secret(...) }}}}`) is single-line only. \
                                     For multi-line secret material (TLS / SSH keys, GPG armored \
                                     keys, service-account JSON files), use the whole-file deploy \
                                     path: encrypt the file, drop it in a pack, reference the \
                                     deployed path from your config. See secrets.lex §4."
                                    ),
                                ));
                            }
                            // Validate UTF-8 before caching — a
                            // non-UTF-8 value never reaches the
                            // cache (the call propagates the rich
                            // error instead).
                            value.expose().map_err(|_| {
                                MjError::new(
                                    MjErrorKind::InvalidOperation,
                                    format!(
                                        "secret `{reference}` resolved to non-UTF-8 bytes; \
                                     value-injection requires UTF-8 strings"
                                    ),
                                )
                            })?;
                            let arc = Arc::new(value);
                            registry.cache_put(reference, Arc::clone(&arc));
                            arc
                        };
                        // expose() can only fail on non-UTF-8, which
                        // we excluded above for cache-miss + the
                        // cache only holds validated UTF-8.
                        let owned = secret.expose().unwrap_or("").to_string();
                        let mut entries = sidecar.lock().unwrap();
                        let sentinel = make_secret_sentinel(render_id, entries.len());
                        entries.push(SecretCallEntry {
                            sentinel: sentinel.clone(),
                            reference: reference.to_string(),
                            value: owned,
                        });
                        // The sentinel is what flows through MiniJinja
                        // and into the rendered output; `expand()`
                        // computes line ranges by locating sentinels
                        // and then substitutes them back to the value.
                        Ok(sentinel)
                    },
                );
            }
            None => {
                env.add_function(
                    "secret",
                    |reference: &str| -> std::result::Result<String, MjError> {
                        Err(MjError::new(
                            MjErrorKind::InvalidOperation,
                            format!(
                                "secret(`{reference}`) was called but no secret providers \
                             are configured. Either set `[secret] enabled = true` and \
                             enable a provider via `[secret.providers.<scheme>] enabled = \
                             true` in your .dodot.toml, or remove the `secret(...)` \
                             reference from the template."
                            ),
                        ))
                    },
                );
            }
        }
        tracker
    }
}

impl Preprocessor for TemplatePreprocessor {
    fn name(&self) -> &str {
        "template"
    }

    fn transform_type(&self) -> TransformType {
        TransformType::Generative
    }

    fn supports_reverse_merge(&self) -> bool {
        // Templates emit a tracked_render and produce baselines; the
        // reverse-merge pipeline (transform check, clean filter) reads
        // those baselines to write template-space diffs back to source.
        true
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

        // Use the source file's path as the template name. Tracker
        // requires named templates; the path is unique per file and
        // surfaces sensibly in any error MiniJinja produces.
        let template_name = source.to_string_lossy().into_owned();

        // Per-render sidecar accumulator. Each `secret(...)` call
        // pushes a `SecretCallEntry { sentinel, reference, value }`;
        // the rendered output carries the sentinel (not the value),
        // and `finalize_secrets` below turns sentinels into line
        // ranges and then substitutes them back to the real value.
        let sidecar: Arc<Mutex<Vec<SecretCallEntry>>> = Arc::new(Mutex::new(Vec::new()));
        let render_id = next_render_id();

        let mut tracker = self.make_tracker(sidecar.clone(), render_id);
        tracker
            .add_template(&template_name, &template_str)
            .map_err(|e| DodotError::TemplateRender {
                source_file: source.to_path_buf(),
                message: format_minijinja_error(&e),
            })?;

        let tracked =
            tracker
                .render(&template_name, ())
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

        let (rendered, tracked_str) = tracked.into_parts();
        let entries = std::mem::take(&mut *sidecar.lock().unwrap());
        let (rendered, tracked_str, secret_line_ranges) =
            finalize_secrets(rendered, tracked_str, &entries);

        Ok(vec![ExpandedFile {
            relative_path: PathBuf::from(stripped),
            content: rendered.into_bytes(),
            is_dir: false,
            tracked_render: Some(tracked_str),
            context_hash: Some(self.context_hash),
            secret_line_ranges,
            deploy_mode: None,
        }])
    }
}

/// Per-call accumulator entry for `secret(...)` resolutions. Carries
/// both the unique private-use sentinel that the MiniJinja function
/// emitted and the real resolved value, so `finalize_secrets` can
/// compute line ranges from the sentinel positions and then swap
/// sentinels for values in the rendered + tracked outputs.
struct SecretCallEntry {
    sentinel: String,
    reference: String,
    value: String,
}

/// Process-wide monotonic counter used to make sentinels unique
/// across concurrent renders. Each `expand()` call gets a fresh id
/// before installing the `secret()` function.
static RENDER_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_render_id() -> u64 {
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
fn make_secret_sentinel(render_id: u64, call_idx: usize) -> String {
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
fn finalize_secrets(
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

/// Produce a deterministic SHA-256 over the rendering context.
///
/// The hash is order-independent (BTreeMap iteration is sorted) and
/// includes both the `dodot.*` namespace and the user-defined variables.
/// Layout: each entry is encoded as `<ns>\x1F<key>\x1F<value>\x1E` so
/// rearranging the boundaries between any two adjacent fields cannot
/// produce a collision (`\x1E` and `\x1F` are the same control bytes
/// burgertocow uses internally; they don't appear in normal
/// configuration content).
fn compute_context_hash(
    dodot_ns: &BTreeMap<String, String>,
    user_vars: &BTreeMap<String, String>,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for (k, v) in dodot_ns {
        hasher.update(b"dodot");
        hasher.update([0x1f]);
        hasher.update(k.as_bytes());
        hasher.update([0x1f]);
        hasher.update(v.as_bytes());
        hasher.update([0x1e]);
    }
    for (k, v) in user_vars {
        hasher.update(b"vars");
        hasher.update([0x1f]);
        hasher.update(k.as_bytes());
        hasher.update([0x1f]);
        hasher.update(v.as_bytes());
        hasher.update([0x1e]);
    }
    hasher.finalize().into()
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
    fn env_var_default_filter_bridges_missing_vars() {
        // The documented escape hatch for optional env vars is
        // `{{ env.NAME | default("...") }}`. If `default` doesn't work,
        // users have no way to reference env vars that might not be set —
        // so this specific pattern must stay functional.
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file(
                "cfg.tmpl",
                "editor={{ env.DODOT_MISSING_VAR_ZZZ | default(\"vim\") }}",
            )
            .done()
            .build();

        let pp = TemplatePreprocessor::new(vec!["tmpl".into()], HashMap::new(), env.paths.as_ref())
            .unwrap();

        let source = env.dotfiles_root.join("app/cfg.tmpl");
        let result = pp.expand(&source, env.fs.as_ref()).unwrap();
        assert_eq!(String::from_utf8_lossy(&result[0].content), "editor=vim");
    }

    #[test]
    fn renders_for_loop_over_user_var() {
        // Regression guard: MiniJinja supports loops, but we want to
        // confirm that user-defined vars (which are plain strings) still
        // work inside a minimal control-flow structure. Strings are
        // iterable as sequences of characters — confirm our value-layer
        // doesn't silently block that.
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file(
                "loop.tmpl",
                "{% for c in word %}{{ c | upper }}{% endfor %}",
            )
            .done()
            .build();

        let mut vars = HashMap::new();
        vars.insert("word".into(), "hi".into());
        let pp = TemplatePreprocessor::new(vec!["tmpl".into()], vars, env.paths.as_ref()).unwrap();

        let source = env.dotfiles_root.join("app/loop.tmpl");
        let result = pp.expand(&source, env.fs.as_ref()).unwrap();
        assert_eq!(String::from_utf8_lossy(&result[0].content), "HI");
    }

    #[test]
    fn renders_unicode_content_and_vars() {
        // Template content and user vars may contain non-ASCII. Confirm
        // both pass through without mangling.
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("greet.tmpl", "こんにちは {{ name }}! 🎉")
            .done()
            .build();

        let mut vars = HashMap::new();
        vars.insert("name".into(), "世界".into());
        let pp = TemplatePreprocessor::new(vec!["tmpl".into()], vars, env.paths.as_ref()).unwrap();

        let source = env.dotfiles_root.join("app/greet.tmpl");
        let result = pp.expand(&source, env.fs.as_ref()).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&result[0].content),
            "こんにちは 世界! 🎉"
        );
    }

    #[test]
    fn rendering_is_deterministic_across_calls() {
        // Calling `expand` multiple times with the same inputs must
        // produce byte-identical output. This guards against any
        // hidden state leaking between renders (e.g. a stale globals
        // cache, a reseeded RNG, or a leaked side-effect into the
        // Environment).
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file(
                "cfg.tmpl",
                "name={{ name }} os={{ dodot.os }} home={{ dodot.home }}",
            )
            .done()
            .build();

        let mut vars = HashMap::new();
        vars.insert("name".into(), "Alice".into());
        let pp = TemplatePreprocessor::new(vec!["tmpl".into()], vars, env.paths.as_ref()).unwrap();

        let source = env.dotfiles_root.join("app/cfg.tmpl");
        let first = pp.expand(&source, env.fs.as_ref()).unwrap();
        let second = pp.expand(&source, env.fs.as_ref()).unwrap();
        let third = pp.expand(&source, env.fs.as_ref()).unwrap();

        assert_eq!(first[0].content, second[0].content);
        assert_eq!(second[0].content, third[0].content);
    }

    #[test]
    fn stripped_name_of_literal_extension_returns_empty() {
        // Edge case recording the current (defensive) behavior: a file
        // named exactly `.tmpl` (extension and nothing else) strips to
        // the empty string. In normal packs the scanner filters dotfiles
        // out before they reach the preprocessor, so this won't happen
        // via user flows. But a misconfigured preprocessor extension or
        // an archive entry with no stem could still produce an empty
        // path downstream, and the pipeline is expected to reject that
        // with a useful error — see
        // `pipeline::rejects_empty_path_from_preprocessor`.
        let pp = new_pp(HashMap::new());
        assert_eq!(pp.stripped_name(".tmpl"), "");
        assert!(pp.matches_extension(".tmpl"));
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

    // ── Tracked render + context hash ────────────────────────────

    #[test]
    fn expand_emits_tracked_render_with_markers_around_each_variable() {
        // The cache layer needs the marker-annotated render to drive
        // burgertocow's reverse-diff. Confirm that each `{{ var }}`
        // emission produces exactly one VAR_START / VAR_END pair in
        // the tracked string.
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("cfg.tmpl", "name={{ name }} count={{ count }}")
            .done()
            .build();

        let mut vars = HashMap::new();
        vars.insert("name".into(), "Alice".into());
        vars.insert("count".into(), "3".into());
        let pp = TemplatePreprocessor::new(vec!["tmpl".into()], vars, env.paths.as_ref()).unwrap();

        let source = env.dotfiles_root.join("app/cfg.tmpl");
        let result = pp.expand(&source, env.fs.as_ref()).unwrap();
        let tracked = result[0]
            .tracked_render
            .as_ref()
            .expect("tracked render must be present for a generative preprocessor");
        assert_eq!(
            tracked.matches(burgertocow::VAR_START).count(),
            2,
            "two variable emissions should produce two start markers, got: {tracked:?}"
        );
        assert_eq!(
            tracked.matches(burgertocow::VAR_END).count(),
            2,
            "two variable emissions should produce two end markers, got: {tracked:?}"
        );
    }

    #[test]
    fn expand_visible_output_matches_tracked_with_markers_stripped() {
        // The visible content (what the symlink target sees) must equal
        // the tracked string with marker bytes removed. Otherwise the
        // baseline cache's `rendered_content` and the deployed file
        // would diverge by exactly the marker characters.
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("cfg.tmpl", "user={{ name }} home={{ dodot.home }}")
            .done()
            .build();

        let mut vars = HashMap::new();
        vars.insert("name".into(), "Alice".into());
        let pp = TemplatePreprocessor::new(vec!["tmpl".into()], vars, env.paths.as_ref()).unwrap();

        let source = env.dotfiles_root.join("app/cfg.tmpl");
        let result = pp.expand(&source, env.fs.as_ref()).unwrap();
        let visible = String::from_utf8(result[0].content.clone()).unwrap();
        let tracked = result[0].tracked_render.as_ref().unwrap();

        let stripped: String = tracked
            .chars()
            .filter(|c| *c != burgertocow::VAR_START && *c != burgertocow::VAR_END)
            .collect();
        assert_eq!(visible, stripped);
    }

    #[test]
    fn context_hash_is_populated_and_stable() {
        // Same constructor inputs should produce the same context hash
        // across runs and across `expand` calls. This is what lets the
        // baseline cache decide "input didn't change" without re-rendering.
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("a.tmpl", "x={{ name }}")
            .done()
            .build();

        let mut vars = HashMap::new();
        vars.insert("name".into(), "Alice".into());
        let pp1 = TemplatePreprocessor::new(vec!["tmpl".into()], vars.clone(), env.paths.as_ref())
            .unwrap();
        let pp2 = TemplatePreprocessor::new(vec!["tmpl".into()], vars, env.paths.as_ref()).unwrap();

        assert_eq!(
            pp1.context_hash, pp2.context_hash,
            "identical inputs must yield identical context hashes"
        );

        let source = env.dotfiles_root.join("app/a.tmpl");
        let r1 = pp1.expand(&source, env.fs.as_ref()).unwrap();
        let r2 = pp1.expand(&source, env.fs.as_ref()).unwrap();
        assert_eq!(r1[0].context_hash, r2[0].context_hash);
        assert_eq!(r1[0].context_hash, Some(pp1.context_hash));
    }

    #[test]
    fn context_hash_changes_when_user_var_changes() {
        // A different user-var value MUST produce a different context
        // hash. Without this, secret rotation through user vars wouldn't
        // re-run install/homebrew sentinels (whose freshness is keyed off
        // the context hash via §3.5 of the secrets spec).
        let mut vars1 = HashMap::new();
        vars1.insert("name".into(), "Alice".into());

        let mut vars2 = HashMap::new();
        vars2.insert("name".into(), "Bob".into());

        let pather = make_pather();
        let pp1 = TemplatePreprocessor::new(vec!["tmpl".into()], vars1, &pather).unwrap();
        let pp2 = TemplatePreprocessor::new(vec!["tmpl".into()], vars2, &pather).unwrap();
        assert_ne!(pp1.context_hash, pp2.context_hash);
    }

    #[test]
    fn context_hash_is_order_independent_for_user_vars() {
        // Hash inputs are gathered from a HashMap, so iteration order
        // is non-deterministic. Sorting via BTreeMap before hashing must
        // produce a stable hash regardless of insertion order.
        let pather = make_pather();

        let mut a = HashMap::new();
        a.insert("alpha".into(), "1".into());
        a.insert("zeta".into(), "26".into());

        let mut b = HashMap::new();
        b.insert("zeta".into(), "26".into());
        b.insert("alpha".into(), "1".into());

        let pp_a = TemplatePreprocessor::new(vec!["tmpl".into()], a, &pather).unwrap();
        let pp_b = TemplatePreprocessor::new(vec!["tmpl".into()], b, &pather).unwrap();
        assert_eq!(pp_a.context_hash, pp_b.context_hash);
    }

    #[test]
    fn empty_template_still_emits_tracked_render() {
        // Edge case: a template with no `{{ ... }}` emissions. The
        // tracked string should be the same as the visible content
        // (no markers added) and still be present, not None.
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("plain.tmpl", "no vars at all")
            .done()
            .build();

        let pp = TemplatePreprocessor::new(vec!["tmpl".into()], HashMap::new(), env.paths.as_ref())
            .unwrap();

        let source = env.dotfiles_root.join("app/plain.tmpl");
        let result = pp.expand(&source, env.fs.as_ref()).unwrap();
        let tracked = result[0].tracked_render.as_ref().unwrap();
        assert!(
            !tracked.contains(burgertocow::VAR_START) && !tracked.contains(burgertocow::VAR_END),
            "no variables → no markers, got: {tracked:?}"
        );
        // And still equal to the visible content.
        assert_eq!(
            String::from_utf8(result[0].content.clone()).unwrap(),
            *tracked
        );
    }

    // ── secret() integration (Phase S1) ────────────────────────

    /// Build a TemplatePreprocessor wired with a registry containing
    /// the given canned `(reference, value)` pairs under one scheme.
    /// The scheme is used as both the URI prefix and the
    /// MockSecretProvider's `scheme()` return.
    fn pp_with_secrets(scheme: &str, pairs: &[(&str, &str)]) -> TemplatePreprocessor {
        use crate::secret::test_support::MockSecretProvider;
        use crate::secret::SecretRegistry;
        use std::sync::Arc;

        let mut mock = MockSecretProvider::new(scheme);
        for (k, v) in pairs {
            mock = mock.with(k.to_string(), v.to_string());
        }
        let mut registry = SecretRegistry::new();
        registry.register(Arc::new(mock));
        new_pp(HashMap::new()).with_secret_registry(Arc::new(registry))
    }

    #[test]
    fn secret_function_resolves_via_registry() {
        // pass:path/to/db -> "hunter2"; the registry strips the scheme
        // and hands "path/to/db" to the provider, which returns the
        // canned value.
        let pp = pp_with_secrets("pass", &[("path/to/db", "hunter2")]);
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file(
                "config.toml.tmpl",
                "password = \"{{ secret('pass:path/to/db') }}\"\n",
            )
            .done()
            .build();
        let source = env.dotfiles_root.join("app/config.toml.tmpl");
        let result = pp.expand(&source, env.fs.as_ref()).unwrap();
        let rendered = String::from_utf8_lossy(&result[0].content);
        assert_eq!(rendered, "password = \"hunter2\"\n");
    }

    #[test]
    fn secret_function_caches_repeated_references_within_a_render() {
        // Same `{{ secret('pass:k') }}` used three times — the
        // provider should only be invoked once. Pin the within-run
        // cache contract from `secrets.lex` §7.4 / Phase S2.
        use crate::secret::test_support::MockSecretProvider;
        use crate::secret::SecretRegistry;

        let mock = Arc::new(MockSecretProvider::new("pass").with("k", "v"));
        let mut registry = SecretRegistry::new();
        registry.register(mock.clone());
        let pp = new_pp(HashMap::new()).with_secret_registry(Arc::new(registry));

        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file(
                "c.tmpl",
                "a = {{ secret('pass:k') }}\nb = {{ secret('pass:k') }}\nc = {{ secret('pass:k') }}\n",
            )
            .done()
            .build();
        let source = env.dotfiles_root.join("app/c.tmpl");
        let result = pp.expand(&source, env.fs.as_ref()).unwrap();
        let rendered = String::from_utf8_lossy(&result[0].content);
        assert_eq!(rendered, "a = v\nb = v\nc = v\n");
        // Cache hit on calls 2 and 3.
        assert_eq!(
            mock.resolve_call_count(),
            1,
            "within-run cache must collapse repeats"
        );
        // Each call still gets its own sidecar entry — sentinels
        // are per-call, line ranges cover all three lines.
        assert_eq!(result[0].secret_line_ranges.len(), 3);
    }

    #[test]
    fn secret_function_caches_across_multiple_expands_on_one_registry() {
        // Building the registry once and rendering N templates
        // through it = one provider call per unique reference,
        // not per template. This pins the `commands::up` flow
        // where one preflighted registry threads through every
        // pack rendered in the run.
        use crate::secret::test_support::MockSecretProvider;
        use crate::secret::SecretRegistry;

        let mock = Arc::new(MockSecretProvider::new("pass").with("k", "v"));
        let mut registry = SecretRegistry::new();
        registry.register(mock.clone());
        let registry = Arc::new(registry);

        // Two independent TemplatePreprocessor instances both wired
        // to the same Arc<SecretRegistry>.
        let pp_a = new_pp(HashMap::new()).with_secret_registry(registry.clone());
        let pp_b = new_pp(HashMap::new()).with_secret_registry(registry.clone());

        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("a.tmpl", "{{ secret('pass:k') }}\n")
            .file("b.tmpl", "{{ secret('pass:k') }}\n")
            .done()
            .build();
        let _ = pp_a
            .expand(&env.dotfiles_root.join("app/a.tmpl"), env.fs.as_ref())
            .unwrap();
        let _ = pp_b
            .expand(&env.dotfiles_root.join("app/b.tmpl"), env.fs.as_ref())
            .unwrap();
        assert_eq!(
            mock.resolve_call_count(),
            1,
            "shared registry should serve the second expand from cache"
        );
    }

    #[test]
    fn secret_function_records_sidecar_entry_with_correct_line_range() {
        let pp = pp_with_secrets("pass", &[("k1", "v1"), ("k2", "v2")]);
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file(
                "c.tmpl",
                "first\nsecond = {{ secret('pass:k1') }}\nthird\nfourth = {{ secret('pass:k2') }}\n",
            )
            .done()
            .build();
        let source = env.dotfiles_root.join("app/c.tmpl");
        let result = pp.expand(&source, env.fs.as_ref()).unwrap();
        // k1's value lands on line 1 (0-indexed), k2's on line 3.
        let ranges = &result[0].secret_line_ranges;
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0].reference, "pass:k1");
        assert_eq!(ranges[0].start, 1);
        assert_eq!(ranges[0].end, 2);
        assert_eq!(ranges[1].reference, "pass:k2");
        assert_eq!(ranges[1].start, 3);
        assert_eq!(ranges[1].end, 4);
    }

    #[test]
    fn secret_function_refuses_multiline_value_per_section_3_4() {
        let pp = pp_with_secrets("pass", &[("multi", "line1\nline2")]);
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("c.tmpl", "x = {{ secret('pass:multi') }}\n")
            .done()
            .build();
        let source = env.dotfiles_root.join("app/c.tmpl");
        let err = pp.expand(&source, env.fs.as_ref()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("multi-line value"));
        assert!(msg.contains("single-line only"));
        // Points the user at the whole-file path, not just rejecting.
        assert!(msg.contains("whole-file deploy"));
    }

    #[test]
    fn secret_function_propagates_provider_resolve_failure() {
        let pp = pp_with_secrets("pass", &[]); // no canned values
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("c.tmpl", "x = {{ secret('pass:missing') }}\n")
            .done()
            .build();
        let source = env.dotfiles_root.join("app/c.tmpl");
        let err = pp.expand(&source, env.fs.as_ref()).unwrap_err();
        let msg = err.to_string();
        // The mock's "no canned value" message comes through.
        assert!(msg.contains("MockSecretProvider"));
        assert!(msg.contains("missing"));
    }

    #[test]
    fn secret_function_unknown_scheme_lists_configured_schemes() {
        // Registry has `pass` only; template references `op://...`.
        let pp = pp_with_secrets("pass", &[("k", "v")]);
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("c.tmpl", "x = {{ secret('op://V/I/F') }}\n")
            .done()
            .build();
        let source = env.dotfiles_root.join("app/c.tmpl");
        let err = pp.expand(&source, env.fs.as_ref()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("no secret provider registered for scheme `op`"));
        assert!(msg.contains("pass")); // configured-schemes listing
    }

    #[test]
    fn secret_function_without_registry_errors_with_actionable_hint() {
        // No `with_secret_registry` call → secret() exists but every
        // call surfaces a config-pointing error rather than
        // MiniJinja's generic "undefined" diagnostic.
        let pp = new_pp(HashMap::new());
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("c.tmpl", "x = {{ secret('pass:k') }}\n")
            .done()
            .build();
        let source = env.dotfiles_root.join("app/c.tmpl");
        let err = pp.expand(&source, env.fs.as_ref()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("no secret providers are configured"));
        assert!(msg.contains("[secret.providers."));
        assert!(msg.contains("pass:k"));
    }

    #[test]
    fn secret_function_supports_multiple_schemes_in_one_template() {
        use crate::secret::test_support::MockSecretProvider;
        use crate::secret::SecretRegistry;
        use std::sync::Arc;

        let mut registry = SecretRegistry::new();
        registry.register(Arc::new(
            MockSecretProvider::new("pass").with("db", "from-pass"),
        ));
        registry.register(Arc::new(
            MockSecretProvider::new("op").with("//V/I/password", "from-op"),
        ));
        let pp = new_pp(HashMap::new()).with_secret_registry(Arc::new(registry));

        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file(
                "c.tmpl",
                "a={{ secret('pass:db') }}\nb={{ secret('op://V/I/password') }}\n",
            )
            .done()
            .build();
        let source = env.dotfiles_root.join("app/c.tmpl");
        let result = pp.expand(&source, env.fs.as_ref()).unwrap();
        let rendered = String::from_utf8_lossy(&result[0].content);
        assert_eq!(rendered, "a=from-pass\nb=from-op\n");
        assert_eq!(result[0].secret_line_ranges.len(), 2);
    }

    #[test]
    fn secret_function_tracks_render_into_baseline() {
        // The secret value must appear in the visible content AND
        // (because templates are reverse-merge-capable) in the
        // tracked_render. Burgertocow markers wrap the variable
        // emission; the secret value appears between markers in the
        // tracked stream.
        let pp = pp_with_secrets("pass", &[("k", "topsecret")]);
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("c.tmpl", "x = {{ secret('pass:k') }}\n")
            .done()
            .build();
        let source = env.dotfiles_root.join("app/c.tmpl");
        let result = pp.expand(&source, env.fs.as_ref()).unwrap();
        let rendered = String::from_utf8_lossy(&result[0].content);
        assert_eq!(rendered, "x = topsecret\n");

        let tracked = result[0]
            .tracked_render
            .as_ref()
            .expect("template render produces tracked stream");
        assert!(
            tracked.contains("topsecret"),
            "tracked render should contain the resolved value, got: {tracked:?}"
        );
    }

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

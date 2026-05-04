//! Conditional running: gate filenames against host facts.
//!
//! A *gate* is a filename or directory token that decides whether dodot
//! deploys an entry on the current host. The grammar is `_<label>`, where
//! `<label>` resolves through a [`GateTable`] to a set of
//! `(dimension, expected_value)` equality checks AND-ed together. dodot
//! ships a built-in seed table covering OS and arch labels (`darwin`,
//! `linux`, `arm64`, …); users extend it from `[gates]` in `.dodot.toml`.
//!
//! See `docs/proposals/conditional-running.lex` for the design rationale
//! and the grammar's exact semantics.
//!
//! # C1 scope
//!
//! This module covers the v1 (Phase C1) surface:
//!
//! - Per-file gates: `<stem>._<label>.<ext>` and extensionless
//!   `<name>._<label>` basename forms.
//! - Built-in labels for `darwin`, `linux`, `macos`, `arm64`, `aarch64`,
//!   `x86_64`.
//! - User-defined labels via `[gates]` config (root or pack-level).
//! - Hard error on unknown labels (typo guard).
//!
//! Directory-segment gates (`_<label>/`) and pack-level `[pack] os`
//! land in subsequent phases.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{DodotError, Result};

// ── Dimensions and host facts ───────────────────────────────────

/// A host trait that gate predicates can match against.
///
/// Mirrors the `dodot.*` namespace exposed to templates so users have
/// a single mental model: anything they can branch on with
/// `{% if dodot.X %}` they can gate on with a label that mentions `X`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dimension {
    Os,
    Arch,
    Hostname,
    Username,
}

impl Dimension {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Os => "os",
            Self::Arch => "arch",
            Self::Hostname => "hostname",
            Self::Username => "username",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "os" => Ok(Self::Os),
            "arch" => Ok(Self::Arch),
            "hostname" => Ok(Self::Hostname),
            "username" => Ok(Self::Username),
            other => Err(DodotError::Config(format!(
                "unknown gate dimension `{other}`: must be one of os, arch, hostname, username"
            ))),
        }
    }
}

/// Snapshot of the host's gate-relevant facts.
///
/// Built once per `dodot up`/`status` run via [`HostFacts::detect`].
/// Tests build the value directly.
#[derive(Debug, Clone)]
pub struct HostFacts {
    pub os: String,
    pub arch: String,
    pub hostname: Option<String>,
    pub username: Option<String>,
}

impl HostFacts {
    /// Detect host facts from the current process environment.
    ///
    /// `os` and `arch` come from compile-time `target_os`/`target_arch`,
    /// matching the values exposed to templates as `dodot.os`/`dodot.arch`.
    /// `hostname` and `username` are best-effort — `None` if detection
    /// fails (consistent with how templates omit those keys when they
    /// can't be detected).
    pub fn detect() -> Self {
        Self {
            os: detect_os(),
            arch: detect_arch(),
            hostname: detect_hostname(),
            username: detect_username(),
        }
    }

    /// Build a fixed HostFacts for tests / fixtures.
    ///
    /// `hostname` and `username` are populated with stable placeholder
    /// values so test predicates against those dimensions are reproducible.
    pub fn for_tests(os: impl Into<String>, arch: impl Into<String>) -> Self {
        Self {
            os: os.into(),
            arch: arch.into(),
            hostname: Some("test-host".into()),
            username: Some("tester".into()),
        }
    }

    /// Lookup the host's value for a given dimension.
    ///
    /// Returns `None` for hostname/username when detection failed; a
    /// gate matching against a missing dimension always evaluates false
    /// (the host can't claim to be `host=foo` if it has no hostname).
    pub fn get(&self, dim: Dimension) -> Option<&str> {
        match dim {
            Dimension::Os => Some(&self.os),
            Dimension::Arch => Some(&self.arch),
            Dimension::Hostname => self.hostname.as_deref(),
            Dimension::Username => self.username.as_deref(),
        }
    }
}

fn detect_os() -> String {
    // Match the values templates already expose so users don't have
    // to learn two name systems.
    if cfg!(target_os = "macos") {
        "darwin".into()
    } else if cfg!(target_os = "linux") {
        "linux".into()
    } else if cfg!(target_os = "windows") {
        "windows".into()
    } else {
        std::env::consts::OS.into()
    }
}

fn detect_arch() -> String {
    std::env::consts::ARCH.into()
}

fn detect_hostname() -> Option<String> {
    // Mirror `preprocessing::template::detect_hostname`: env first,
    // shell out to `hostname(1)` as fallback. Keeps gates and templates
    // honest about agreeing on `dodot.hostname`.
    if let Ok(h) = std::env::var("HOSTNAME") {
        if !h.is_empty() {
            return Some(h);
        }
    }
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

// ── Predicates and table ────────────────────────────────────────

/// A single gate predicate: AND of `(dimension, expected_value)` pairs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GatePredicate {
    /// Equality checks AND-ed together.
    ///
    /// Empty matchers vector means "always true," but that's never
    /// constructed in practice — the parser refuses an empty user entry
    /// at config-load time.
    pub matchers: Vec<(Dimension, String)>,
}

impl GatePredicate {
    /// Single-dimension constructor for the built-in seed.
    pub fn single(dim: Dimension, value: impl Into<String>) -> Self {
        Self {
            matchers: vec![(dim, value.into())],
        }
    }

    /// Evaluate the predicate against host facts.
    ///
    /// Returns true iff every `(dim, val)` pair matches. A missing
    /// dimension on the host (e.g. `hostname` returned None) makes the
    /// predicate false for that pair — the host can't claim a value it
    /// doesn't have.
    pub fn matches(&self, host: &HostFacts) -> bool {
        self.matchers
            .iter()
            .all(|(dim, expected)| host.get(*dim) == Some(expected.as_str()))
    }

    /// Render the predicate as a human-readable string for diagnostics.
    ///
    /// `{ os = "darwin" }` for single-dim, `{ os = "darwin", arch = "aarch64" }`
    /// for compound. Used by `dodot status` to render "gated out" rows.
    pub fn describe(&self) -> String {
        let parts: Vec<String> = self
            .matchers
            .iter()
            .map(|(d, v)| format!("{} = \"{}\"", d.as_str(), v))
            .collect();
        format!("{{ {} }}", parts.join(", "))
    }
}

/// Resolved gate table: built-in seed merged with user labels.
#[derive(Debug, Clone, Default)]
pub struct GateTable {
    labels: HashMap<String, GatePredicate>,
}

impl GateTable {
    /// The built-in seed table.
    ///
    /// Ships compiled-in so `_darwin` works zero-config. User entries
    /// merge over this; user can shadow a built-in but the default is
    /// "extend, not replace."
    pub fn with_builtins() -> Self {
        let mut labels = HashMap::new();
        // OS labels
        labels.insert(
            "darwin".into(),
            GatePredicate::single(Dimension::Os, "darwin"),
        );
        labels.insert(
            "macos".into(),
            GatePredicate::single(Dimension::Os, "darwin"),
        );
        labels.insert(
            "linux".into(),
            GatePredicate::single(Dimension::Os, "linux"),
        );
        // Arch labels (Rust target_arch values)
        labels.insert(
            "aarch64".into(),
            GatePredicate::single(Dimension::Arch, "aarch64"),
        );
        labels.insert(
            "arm64".into(),
            GatePredicate::single(Dimension::Arch, "aarch64"),
        );
        labels.insert(
            "x86_64".into(),
            GatePredicate::single(Dimension::Arch, "x86_64"),
        );
        Self { labels }
    }

    /// Merge a user-supplied label set over the built-ins.
    ///
    /// User entries are accepted as `HashMap<String, HashMap<String, String>>`
    /// — the natural confique/serde shape for `[gates]` in TOML where
    /// each value is an inline table of `dimension = value` pairs.
    pub fn merge_user(&mut self, user: &HashMap<String, HashMap<String, String>>) -> Result<()> {
        for (label, dims) in user {
            if dims.is_empty() {
                return Err(DodotError::Config(format!(
                    "gate label `{label}` has no dimension matchers; \
                     each entry must have at least one of os/arch/hostname/username"
                )));
            }
            let mut matchers = Vec::with_capacity(dims.len());
            // Deterministic order for diagnostics: alphabetical by dim name.
            let mut keys: Vec<&String> = dims.keys().collect();
            keys.sort();
            for key in keys {
                let dim = Dimension::parse(key)
                    .map_err(|e| DodotError::Config(format!("in gate label `{label}`: {e}")))?;
                let val = dims.get(key).cloned().unwrap_or_default();
                if val.is_empty() {
                    return Err(DodotError::Config(format!(
                        "in gate label `{label}`: dimension `{key}` has empty value"
                    )));
                }
                matchers.push((dim, val));
            }
            self.labels
                .insert(label.clone(), GatePredicate { matchers });
        }
        Ok(())
    }

    pub fn lookup(&self, label: &str) -> Option<&GatePredicate> {
        self.labels.get(label)
    }

    pub fn contains(&self, label: &str) -> bool {
        self.labels.contains_key(label)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.labels.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }
}

// ── Filename gate parsing ───────────────────────────────────────

/// Result of inspecting a basename for a gate token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BasenameGate<'a> {
    /// No `._<label>` token in the basename.
    None,
    /// Found a gate. `label` is the token after `_` and before the
    /// extension (or end-of-name); `stripped` is the basename with the
    /// `._<label>` segment removed.
    Found { label: &'a str, stripped: String },
}

/// Parse a basename for a gate token.
///
/// Recognised forms (in order of precedence):
///
/// - `<stem>._<label>.<ext>` — ext is the final dotted segment, label
///   is the token after `_` in the second-to-last segment. Stripped
///   form is `<stem>.<ext>`.
/// - `<name>._<label>` — extensionless. Stripped form is `<name>`.
///
/// A `_<label>` segment NOT preceded by `<stem>.` is not a gate (e.g.
/// `_home` as a top-level dirname is a routing prefix, handled
/// elsewhere). The leading-`._` shape is what marks the suffix as a gate.
///
/// Labels must be non-empty and match `[A-Za-z0-9_-]+`. A token with
/// other characters falls through to "no gate" rather than producing
/// a parse error — gates are opt-in by naming, and a user-named
/// `weird.fil_e.sh` shouldn't be misread.
///
/// Hidden-file-style basenames (start with `.`) are skipped because the
/// scanner already drops them at walk time. We don't special-case them.
pub fn parse_basename_gate(basename: &str) -> BasenameGate<'_> {
    // Scan `._` boundaries from right to left. For each, the label runs
    // from after `_` up to the next `.` (or end of basename for the
    // extensionless form). The first valid label found is the gate.
    //
    // Right-to-left ensures that in `foo._bar._baz.sh` only `._baz` is
    // taken as the gate, leaving `foo._bar` literal — `_bar` would only
    // be a gate if the user wrote `foo._bar.sh`.
    let bytes = basename.as_bytes();
    let mut i = bytes.len();
    while i >= 2 {
        i -= 1;
        if bytes[i] == b'_' && bytes[i - 1] == b'.' {
            let underscore = i;
            let dot_before = i - 1;
            let label_start = underscore + 1;
            let label_end = bytes[label_start..]
                .iter()
                .position(|&b| b == b'.')
                .map(|off| label_start + off)
                .unwrap_or(bytes.len());
            let label = &basename[label_start..label_end];
            if !is_valid_label(label) {
                continue;
            }
            let stem = &basename[..dot_before];
            if stem.is_empty() {
                continue;
            }
            let suffix = &basename[label_end..];
            let stripped = format!("{stem}{suffix}");
            return BasenameGate::Found { label, stripped };
        }
    }
    BasenameGate::None
}

fn is_valid_label(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn host(os: &str, arch: &str) -> HostFacts {
        HostFacts {
            os: os.into(),
            arch: arch.into(),
            hostname: Some("test-host".into()),
            username: Some("tester".into()),
        }
    }

    // ── Builtin table ───────────────────────────────────────────

    #[test]
    fn builtins_cover_os_and_arch() {
        let t = GateTable::with_builtins();
        assert!(t.contains("darwin"));
        assert!(t.contains("linux"));
        assert!(t.contains("macos"));
        assert!(t.contains("arm64"));
        assert!(t.contains("aarch64"));
        assert!(t.contains("x86_64"));
    }

    #[test]
    fn macos_alias_resolves_to_darwin() {
        let t = GateTable::with_builtins();
        let macos = t.lookup("macos").unwrap();
        let h = host("darwin", "x86_64");
        assert!(macos.matches(&h));
    }

    #[test]
    fn arm64_alias_resolves_to_aarch64() {
        let t = GateTable::with_builtins();
        let arm = t.lookup("arm64").unwrap();
        let h = host("darwin", "aarch64");
        assert!(arm.matches(&h));
    }

    // ── Predicate evaluation ────────────────────────────────────

    #[test]
    fn single_dim_predicate_matches() {
        let p = GatePredicate::single(Dimension::Os, "darwin");
        assert!(p.matches(&host("darwin", "aarch64")));
        assert!(!p.matches(&host("linux", "aarch64")));
    }

    #[test]
    fn compound_predicate_is_and() {
        let p = GatePredicate {
            matchers: vec![
                (Dimension::Os, "darwin".into()),
                (Dimension::Arch, "aarch64".into()),
            ],
        };
        assert!(p.matches(&host("darwin", "aarch64")));
        assert!(!p.matches(&host("darwin", "x86_64")));
        assert!(!p.matches(&host("linux", "aarch64")));
    }

    #[test]
    fn missing_dimension_does_not_match() {
        // Predicate requires hostname=foo, but host has no hostname.
        let p = GatePredicate {
            matchers: vec![(Dimension::Hostname, "foo".into())],
        };
        let h = HostFacts {
            os: "linux".into(),
            arch: "x86_64".into(),
            hostname: None,
            username: None,
        };
        assert!(!p.matches(&h));
    }

    #[test]
    fn describe_renders_inline_table() {
        let p = GatePredicate {
            matchers: vec![
                (Dimension::Os, "darwin".into()),
                (Dimension::Arch, "aarch64".into()),
            ],
        };
        assert_eq!(p.describe(), r#"{ os = "darwin", arch = "aarch64" }"#);
    }

    // ── User merge ──────────────────────────────────────────────

    #[test]
    fn merge_user_adds_labels() {
        let mut t = GateTable::with_builtins();
        let mut user = HashMap::new();
        let mut laptop = HashMap::new();
        laptop.insert("hostname".to_string(), "mbp".to_string());
        user.insert("laptop".to_string(), laptop);
        t.merge_user(&user).unwrap();
        assert!(t.contains("laptop"));
    }

    #[test]
    fn merge_user_compound_label_is_and() {
        let mut t = GateTable::with_builtins();
        let mut user = HashMap::new();
        let mut arm_mac = HashMap::new();
        arm_mac.insert("os".into(), "darwin".into());
        arm_mac.insert("arch".into(), "aarch64".into());
        user.insert("arm-mac".into(), arm_mac);
        t.merge_user(&user).unwrap();
        let p = t.lookup("arm-mac").unwrap();
        assert!(p.matches(&host("darwin", "aarch64")));
        assert!(!p.matches(&host("darwin", "x86_64")));
    }

    #[test]
    fn merge_user_unknown_dimension_errors() {
        let mut t = GateTable::with_builtins();
        let mut user = HashMap::new();
        let mut bad = HashMap::new();
        bad.insert("kernel".into(), "linux".into());
        user.insert("bad".into(), bad);
        let err = t.merge_user(&user).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("bad"), "missing label name: {msg}");
        assert!(msg.contains("kernel"), "missing dim name: {msg}");
    }

    #[test]
    fn merge_user_empty_label_errors() {
        let mut t = GateTable::with_builtins();
        let mut user = HashMap::new();
        user.insert("empty".into(), HashMap::new());
        let err = t.merge_user(&user).unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn merge_user_empty_value_errors() {
        let mut t = GateTable::with_builtins();
        let mut user = HashMap::new();
        let mut bad = HashMap::new();
        bad.insert("os".into(), "".into());
        user.insert("blank".into(), bad);
        let err = t.merge_user(&user).unwrap_err();
        assert!(err.to_string().contains("empty value"));
    }

    #[test]
    fn merge_user_can_shadow_builtin() {
        let mut t = GateTable::with_builtins();
        let mut user = HashMap::new();
        let mut darwin = HashMap::new();
        darwin.insert("os".into(), "darwin".into());
        darwin.insert("hostname".into(), "specific-mac".into());
        user.insert("darwin".into(), darwin);
        t.merge_user(&user).unwrap();
        // Now darwin requires the hostname too.
        let p = t.lookup("darwin").unwrap();
        assert_eq!(p.matchers.len(), 2);
    }

    // ── Filename parsing ────────────────────────────────────────

    #[test]
    fn parse_simple_gate_with_extension() {
        let g = parse_basename_gate("install._darwin.sh");
        match g {
            BasenameGate::Found { label, stripped } => {
                assert_eq!(label, "darwin");
                assert_eq!(stripped, "install.sh");
            }
            _ => panic!("expected Found"),
        }
    }

    #[test]
    fn parse_extensionless_gate() {
        let g = parse_basename_gate("Brewfile._darwin");
        match g {
            BasenameGate::Found { label, stripped } => {
                assert_eq!(label, "darwin");
                assert_eq!(stripped, "Brewfile");
            }
            _ => panic!("expected Found, got {g:?}"),
        }
    }

    #[test]
    fn parse_compound_label_with_dash() {
        let g = parse_basename_gate("install._arm-mac.sh");
        match g {
            BasenameGate::Found { label, stripped } => {
                assert_eq!(label, "arm-mac");
                assert_eq!(stripped, "install.sh");
            }
            _ => panic!("expected Found"),
        }
    }

    #[test]
    fn parse_no_gate_in_plain_filename() {
        assert_eq!(parse_basename_gate("install.sh"), BasenameGate::None);
        assert_eq!(parse_basename_gate("Brewfile"), BasenameGate::None);
        assert_eq!(parse_basename_gate("vimrc"), BasenameGate::None);
    }

    #[test]
    fn parse_underscore_without_dot_is_not_a_gate() {
        // `install_darwin.sh` (no dot before `_`) is a regular name.
        assert_eq!(parse_basename_gate("install_darwin.sh"), BasenameGate::None);
    }

    #[test]
    fn parse_dot_underscore_with_empty_label_is_not_a_gate() {
        // `install._.sh` has empty label → no gate, deploy literally.
        assert_eq!(parse_basename_gate("install._.sh"), BasenameGate::None);
    }

    #[test]
    fn parse_template_extension_sees_inner_gate() {
        // `aliases._darwin.sh.tmpl`: stripped basename should be
        // `aliases.sh.tmpl` — the template preprocessor still fires on
        // surviving entries because we preserve the `.sh.tmpl` tail.
        let g = parse_basename_gate("aliases._darwin.sh.tmpl");
        match g {
            BasenameGate::Found { label, stripped } => {
                assert_eq!(label, "darwin");
                assert_eq!(stripped, "aliases.sh.tmpl");
            }
            _ => panic!("expected Found"),
        }
    }

    #[test]
    fn parse_routing_prefix_with_gate() {
        // `home.bashrc._darwin` (extensionless) → routing-prefix
        // `home.bashrc` is preserved in the stripped basename so the
        // symlink resolver still routes via §1 (home.X).
        let g = parse_basename_gate("home.bashrc._darwin");
        match g {
            BasenameGate::Found { label, stripped } => {
                assert_eq!(label, "darwin");
                assert_eq!(stripped, "home.bashrc");
            }
            _ => panic!("expected Found"),
        }
    }

    #[test]
    fn parse_only_takes_rightmost_label() {
        // `foo._bar._baz.sh`: only the rightmost `._baz` is the gate.
        let g = parse_basename_gate("foo._bar._baz.sh");
        match g {
            BasenameGate::Found { label, stripped } => {
                assert_eq!(label, "baz");
                assert_eq!(stripped, "foo._bar.sh");
            }
            _ => panic!("expected Found"),
        }
    }

    // ── HostFacts ───────────────────────────────────────────────

    #[test]
    fn hostfacts_detect_runs() {
        // Smoke test: detect() shouldn't panic and must populate os/arch.
        let h = HostFacts::detect();
        assert!(!h.os.is_empty());
        assert!(!h.arch.is_empty());
    }

    #[test]
    fn hostfacts_get_returns_known_dims() {
        let h = host("darwin", "aarch64");
        assert_eq!(h.get(Dimension::Os), Some("darwin"));
        assert_eq!(h.get(Dimension::Arch), Some("aarch64"));
    }
}

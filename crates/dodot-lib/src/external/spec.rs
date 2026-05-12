//! `externals.toml` schema and parser.
//!
//! The pack-level file is a flat map of entry-name → spec:
//!
//! ```toml
//! [oh-my-zsh]            # entry name = section header
//! type = "git-repo"
//! url  = "..."
//! target = "~/.oh-my-zsh"
//!
//! [shared-aliases]
//! type = "file"
//! url  = "..."
//! target = "~/.config/shared/aliases.sh"
//! sha256 = "..."
//! ```
//!
//! `file` and `git-repo` are implemented; any other `type` value
//! parses to [`FetchSpec::Unsupported`]. The original type string is
//! not retained — `#[serde(other)]` does not carry it over — so the
//! handler's diagnostic surfaces the entry name and a generic
//! "unsupported type" message rather than echoing back what the user
//! wrote. Subsequent PRs add `archive` / `archive-file`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{DodotError, Result};

/// The whole `externals.toml` file: an ordered map of entry name → entry.
///
/// We use `BTreeMap` so iteration order is stable (alphabetical by
/// entry name) — handlers must produce deterministic intents to keep
/// `dodot status` / `up` output reproducible across runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExternalsToml {
    pub entries: BTreeMap<String, ExternalEntry>,
}

/// A single entry in `externals.toml`. The `type` field discriminates
/// the spec variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalEntry {
    /// User-visible target path. Tilde-prefixed paths (`~/…`) are
    /// resolved by the handler against the pather's home dir.
    pub target: String,

    #[serde(flatten)]
    pub spec: FetchSpec,
}

/// The fetch recipe for one external entry.
///
/// Tagged externally by the `type` field per TOML convention.
/// `archive` and `archive-file` arrive in a later PR — until then,
/// those parse into [`FetchSpec::Unsupported`] so the handler can
/// surface "not yet implemented" without choking the whole pack.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum FetchSpec {
    /// A single file fetched over HTTP(S) — or `file://` for tests.
    File {
        url: String,
        /// Required content hash. Mandatory because an unpinned remote
        /// file has no integrity story at all; refusing to fetch
        /// without one is the same posture Home Manager takes.
        sha256: String,
    },

    /// A shallow git clone, tracking `HEAD` of the default branch.
    ///
    /// Freshness is upstream-driven: each `dodot up` runs a cheap
    /// `git ls-remote HEAD`; the actual clone is only re-fetched
    /// when the remote SHA differs from the local one. Sparse-tree
    /// checkout (`subpath`) and pinned `ref` / `commit` arrive in
    /// a follow-up PR; for now the whole repo's HEAD is cloned.
    GitRepo { url: String },

    /// Catchall for type values dodot doesn't implement yet.
    #[serde(other)]
    Unsupported,
}

/// Parse the bytes of an `externals.toml` file.
///
/// Returns a `DodotError::Other` on malformed TOML; callers attach the
/// pack name for context.
///
/// Entry names are validated against a safe-identifier charset because
/// they are used verbatim as datastore subdirectory names and sentinel
/// filenames — letting `..` or `/` through here would create
/// surprising nested layouts and confusing sentinel matches.
pub fn parse_externals_toml(bytes: &[u8]) -> Result<ExternalsToml> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| DodotError::Other(format!("externals.toml is not valid UTF-8: {e}")))?;
    let parsed: BTreeMap<String, ExternalEntry> = toml::from_str(text)
        .map_err(|e| DodotError::Other(format!("externals.toml parse error: {e}")))?;
    for name in parsed.keys() {
        validate_entry_name(name)?;
    }
    Ok(ExternalsToml { entries: parsed })
}

/// Reject entry names that wouldn't be safe as path or sentinel
/// components. Allowed: ASCII letters/digits, plus `-`, `_`, and
/// internal `.` (but not pure `.` or `..`). Everything else — including
/// `/`, `\`, spaces, leading dot, control characters — is a hard error.
fn validate_entry_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(DodotError::Other(
            "externals.toml entry name must not be empty".into(),
        ));
    }
    if name == "." || name == ".." {
        return Err(DodotError::Other(format!(
            "externals.toml entry name {name:?} is reserved"
        )));
    }
    if name.starts_with('.') {
        return Err(DodotError::Other(format!(
            "externals.toml entry name {name:?} must not start with a dot"
        )));
    }
    for ch in name.chars() {
        let ok = ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.');
        if !ok {
            return Err(DodotError::Other(format!(
                "externals.toml entry name {name:?} contains invalid character {ch:?}; \
                 allowed: ASCII letters, digits, `-`, `_`, `.`"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_file_entry() {
        let toml = r#"
            [aliases]
            type   = "file"
            url    = "https://example.com/aliases.sh"
            target = "~/.config/shared/aliases.sh"
            sha256 = "deadbeef"
        "#;
        let parsed = parse_externals_toml(toml.as_bytes()).unwrap();
        assert_eq!(parsed.entries.len(), 1);
        let entry = parsed.entries.get("aliases").unwrap();
        assert_eq!(entry.target, "~/.config/shared/aliases.sh");
        match &entry.spec {
            FetchSpec::File { url, sha256 } => {
                assert_eq!(url, "https://example.com/aliases.sh");
                assert_eq!(sha256, "deadbeef");
            }
            other => panic!("expected File spec, got {other:?}"),
        }
    }

    #[test]
    fn entries_are_alphabetical() {
        let toml = r#"
            [zeta]
            type = "file"
            url = "https://example.com/z"
            target = "~/z"
            sha256 = "00"

            [alpha]
            type = "file"
            url = "https://example.com/a"
            target = "~/a"
            sha256 = "11"
        "#;
        let parsed = parse_externals_toml(toml.as_bytes()).unwrap();
        let names: Vec<&str> = parsed.entries.keys().map(String::as_str).collect();
        assert_eq!(names, vec!["alpha", "zeta"]);
    }

    #[test]
    fn rejects_entry_names_with_path_separator() {
        let toml = r#"
            ["bad/name"]
            type   = "file"
            url    = "https://example.com/x"
            target = "~/x"
            sha256 = "00"
        "#;
        let err = parse_externals_toml(toml.as_bytes()).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("invalid character"), "got: {msg}");
    }

    #[test]
    fn rejects_dotdot_entry_name() {
        let toml = r#"
            [".."]
            type   = "file"
            url    = "https://example.com/x"
            target = "~/x"
            sha256 = "00"
        "#;
        let err = parse_externals_toml(toml.as_bytes()).unwrap_err();
        assert!(format!("{err}").contains("reserved"));
    }

    #[test]
    fn rejects_dot_prefixed_entry_name() {
        let toml = r#"
            [".hidden"]
            type   = "file"
            url    = "https://example.com/x"
            target = "~/x"
            sha256 = "00"
        "#;
        let err = parse_externals_toml(toml.as_bytes()).unwrap_err();
        assert!(format!("{err}").contains("must not start with a dot"));
    }

    #[test]
    fn accepts_internal_dots_dashes_underscores() {
        // TOML treats bare `[a.b]` as nested tables, so use a quoted
        // section header to land a literal dot in the key.
        let toml = r#"
            ["shared.aliases-v2_main"]
            type   = "file"
            url    = "https://example.com/x"
            target = "~/x"
            sha256 = "00"
        "#;
        let parsed = parse_externals_toml(toml.as_bytes()).unwrap();
        assert!(parsed.entries.contains_key("shared.aliases-v2_main"));
    }

    #[test]
    fn parses_git_repo_entry() {
        let toml = r#"
            [omz]
            type   = "git-repo"
            url    = "https://github.com/ohmyzsh/ohmyzsh.git"
            target = "~/.oh-my-zsh"
        "#;
        let parsed = parse_externals_toml(toml.as_bytes()).unwrap();
        let entry = parsed.entries.get("omz").unwrap();
        assert_eq!(entry.target, "~/.oh-my-zsh");
        match &entry.spec {
            FetchSpec::GitRepo { url } => {
                assert_eq!(url, "https://github.com/ohmyzsh/ohmyzsh.git");
            }
            other => panic!("expected GitRepo spec, got {other:?}"),
        }
    }

    #[test]
    fn unknown_type_becomes_unsupported() {
        let toml = r#"
            [bogus]
            type = "this-will-never-exist"
            url = "https://example.com"
            target = "~/x"
        "#;
        let parsed = parse_externals_toml(toml.as_bytes()).unwrap();
        let entry = parsed.entries.get("bogus").unwrap();
        assert!(matches!(entry.spec, FetchSpec::Unsupported));
    }

    #[test]
    fn rejects_malformed_toml() {
        let err = parse_externals_toml(b"this is not = valid [[ toml").unwrap_err();
        assert!(format!("{err}").contains("externals.toml parse error"));
    }

    #[test]
    fn rejects_invalid_utf8() {
        let err = parse_externals_toml(&[0xff, 0xfe, 0xfd]).unwrap_err();
        assert!(format!("{err}").contains("not valid UTF-8"));
    }

    #[test]
    fn missing_sha256_on_file_is_an_error() {
        // sha256 is mandatory; serde should reject the missing field.
        let toml = r#"
            [unpinned]
            type = "file"
            url = "https://example.com/x.sh"
            target = "~/x.sh"
        "#;
        let err = parse_externals_toml(toml.as_bytes()).unwrap_err();
        assert!(format!("{err}").contains("parse error"));
    }
}

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
//! For PR 1 only `type = "file"` is implemented; the other variants
//! parse as [`FetchSpec::Unsupported`] with the requested type
//! preserved so the handler can emit a clean diagnostic.

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
/// Tagged externally by the `type` field per TOML convention. Future
/// PRs add `git-repo`, `archive`, `archive-file` variants — until they
/// land, those parse into [`FetchSpec::Unsupported`] so the handler
/// can surface "not yet implemented" without choking the whole pack.
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

    /// Catchall for type values dodot doesn't implement yet.
    #[serde(other)]
    Unsupported,
}

/// Parse the bytes of an `externals.toml` file.
///
/// Returns a `DodotError::Other` on malformed TOML; callers attach the
/// pack name for context.
pub fn parse_externals_toml(bytes: &[u8]) -> Result<ExternalsToml> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| DodotError::Other(format!("externals.toml is not valid UTF-8: {e}")))?;
    let parsed: BTreeMap<String, ExternalEntry> = toml::from_str(text)
        .map_err(|e| DodotError::Other(format!("externals.toml parse error: {e}")))?;
    Ok(ExternalsToml { entries: parsed })
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
    fn unknown_type_becomes_unsupported() {
        let toml = r#"
            [omz]
            type = "git-repo"
            url = "https://github.com/ohmyzsh/ohmyzsh.git"
            target = "~/.oh-my-zsh"
        "#;
        let parsed = parse_externals_toml(toml.as_bytes()).unwrap();
        let entry = parsed.entries.get("omz").unwrap();
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

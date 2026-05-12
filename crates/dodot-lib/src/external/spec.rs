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

    /// A shallow git clone.
    ///
    /// Freshness rules depend on the pin configuration:
    /// - **Unpinned** (no `ref`, no `commit`): `git ls-remote HEAD`
    ///   on each `up`; refresh only when upstream HEAD moves.
    /// - **`ref = "v1.2.3"`** (tag, branch, or any other reference):
    ///   `git ls-remote <ref>` on each `up`; refresh when that
    ///   reference's SHA changes. Tags don't normally move but the
    ///   mechanism is uniform.
    /// - **`commit = "abc1234..."`** (full SHA): no `ls-remote` at
    ///   all; the local clone is compared against the configured
    ///   commit and refreshed only when the user edits the TOML.
    ///
    /// `subpath = "themes"` triggers a sparse-tree fetch (only that
    /// subdirectory is materialized). The user-visible target
    /// symlink then points at the subpath inside the clone, not
    /// the whole tree.
    GitRepo {
        url: String,
        /// Sparse-checkout pattern (relative to the repo root). When
        /// set, only that subtree is materialized on disk, and the
        /// user-visible target symlink points at it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subpath: Option<String>,
        /// Reference to track (tag, branch, etc.). `None` means HEAD.
        /// Mutually exclusive with [`Self::GitRepo::commit`].
        #[serde(default, rename = "ref", skip_serializing_if = "Option::is_none")]
        git_ref: Option<String>,
        /// Frozen commit SHA. Mutually exclusive with
        /// [`Self::GitRepo::git_ref`]; skips upstream polling.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        commit: Option<String>,
    },

    /// A downloaded archive, extracted whole into the datastore. The
    /// user-visible symlink target points at the extracted tree.
    Archive {
        url: String,
        /// Required content hash of the archive bytes (mandatory for
        /// the same reason as [`Self::File::sha256`] — an unpinned
        /// archive has no integrity story).
        sha256: String,
        /// Optional explicit format. When omitted, the format is
        /// inferred from the URL's filename: `.tar.gz` / `.tgz` →
        /// tar+gzip; `.zip` → zip. Anything else requires this
        /// field.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<ArchiveFormat>,
    },

    /// A single entry extracted from a downloaded archive.
    ///
    /// Use this when the archive contains many files but the user
    /// wants only one deployed to the target.
    ArchiveFile {
        url: String,
        sha256: String,
        /// Path of the archive member to extract, relative to the
        /// archive root (e.g. `"powerlevel10k-master/powerlevel10k.zsh-theme"`).
        member: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<ArchiveFormat>,
    },

    /// Catchall for type values dodot doesn't implement yet.
    #[serde(other)]
    Unsupported,
}

/// Supported archive formats. Both can be deflate-compressed (gzip
/// for tar, deflate for zip).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArchiveFormat {
    TarGz,
    Zip,
}

impl ArchiveFormat {
    /// Infer the archive format from a URL's filename. Returns
    /// `None` when no suffix matches a supported format — caller
    /// should ask for an explicit `format` field in that case.
    pub fn infer_from_url(url: &str) -> Option<Self> {
        // Drop query / fragment so URLs like `…/foo.zip?token=…`
        // still match.
        let stem = url.split(['?', '#']).next().unwrap_or(url);
        let stem_lower = stem.to_ascii_lowercase();
        if stem_lower.ends_with(".tar.gz") || stem_lower.ends_with(".tgz") {
            Some(Self::TarGz)
        } else if stem_lower.ends_with(".zip") {
            Some(Self::Zip)
        } else {
            None
        }
    }
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
///
/// Cross-field invariants on `git-repo` entries are enforced here too,
/// so the executor never has to second-guess a spec it received from
/// the handler.
pub fn parse_externals_toml(bytes: &[u8]) -> Result<ExternalsToml> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| DodotError::Other(format!("externals.toml is not valid UTF-8: {e}")))?;
    let parsed: BTreeMap<String, ExternalEntry> = toml::from_str(text)
        .map_err(|e| DodotError::Other(format!("externals.toml parse error: {e}")))?;
    for (name, entry) in &parsed {
        validate_entry_name(name)?;
        if let FetchSpec::GitRepo {
            subpath,
            git_ref,
            commit,
            ..
        } = &entry.spec
        {
            if git_ref.is_some() && commit.is_some() {
                return Err(DodotError::Other(format!(
                    "externals.toml entry '{name}': `ref` and `commit` are mutually exclusive — pick one"
                )));
            }
            if let Some(p) = subpath {
                validate_subpath(name, p)?;
            }
            if let Some(c) = commit {
                validate_commit_sha(name, c)?;
            }
        }
    }
    Ok(ExternalsToml { entries: parsed })
}

/// Reject `subpath` values that wouldn't be safe to join onto the
/// clone root. We're more permissive than [`validate_entry_name`] —
/// `subpath` is a real on-disk path inside the git tree, so internal
/// slashes and most punctuation are fine. But absolute paths, `..`
/// segments, and leading `-` (which git would treat as an option flag)
/// are hard errors.
fn validate_subpath(entry: &str, subpath: &str) -> Result<()> {
    if subpath.is_empty() {
        return Err(DodotError::Other(format!(
            "externals.toml entry '{entry}': `subpath` must not be empty"
        )));
    }
    if subpath.starts_with('/') || subpath.starts_with('\\') {
        return Err(DodotError::Other(format!(
            "externals.toml entry '{entry}': `subpath` must be relative, not {subpath:?}"
        )));
    }
    if subpath.starts_with('-') {
        return Err(DodotError::Other(format!(
            "externals.toml entry '{entry}': `subpath` must not start with `-` (git would treat it as an option): {subpath:?}"
        )));
    }
    // Reject `..` segments anywhere. Splitting on both `/` and `\`
    // covers POSIX and Windows-style separators.
    for segment in subpath.split(['/', '\\']) {
        if segment == ".." {
            return Err(DodotError::Other(format!(
                "externals.toml entry '{entry}': `subpath` may not contain `..` segments: {subpath:?}"
            )));
        }
    }
    Ok(())
}

/// Reject obviously-malformed commit pins early so the executor's
/// diagnostic budget stays focused on real failures. Accepts 40-char
/// lowercase-or-mixed-case hex (the full sha1 form); short hashes are
/// rejected because shallow git can't reliably resolve them, and
/// they'd also make sentinel filenames ambiguous.
fn validate_commit_sha(entry: &str, commit: &str) -> Result<()> {
    if commit.len() != 40 {
        return Err(DodotError::Other(format!(
            "externals.toml entry '{entry}': `commit` must be a full 40-char SHA, got {} chars",
            commit.len()
        )));
    }
    if !commit.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(DodotError::Other(format!(
            "externals.toml entry '{entry}': `commit` must be hex (a-f, 0-9), got {commit:?}"
        )));
    }
    Ok(())
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
    fn parses_git_repo_entry_minimal() {
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
            FetchSpec::GitRepo {
                url,
                subpath,
                git_ref,
                commit,
            } => {
                assert_eq!(url, "https://github.com/ohmyzsh/ohmyzsh.git");
                assert!(subpath.is_none());
                assert!(git_ref.is_none());
                assert!(commit.is_none());
            }
            other => panic!("expected GitRepo spec, got {other:?}"),
        }
    }

    #[test]
    fn parses_git_repo_entry_with_subpath_and_ref() {
        let toml = r#"
            [p10k]
            type    = "git-repo"
            url     = "https://github.com/romkatv/powerlevel10k.git"
            target  = "~/.config/zsh/themes/p10k"
            subpath = "themes"
            ref     = "v1.20.0"
        "#;
        let parsed = parse_externals_toml(toml.as_bytes()).unwrap();
        let entry = parsed.entries.get("p10k").unwrap();
        match &entry.spec {
            FetchSpec::GitRepo {
                subpath,
                git_ref,
                commit,
                ..
            } => {
                assert_eq!(subpath.as_deref(), Some("themes"));
                assert_eq!(git_ref.as_deref(), Some("v1.20.0"));
                assert!(commit.is_none());
            }
            other => panic!("expected GitRepo, got {other:?}"),
        }
    }

    #[test]
    fn parses_git_repo_entry_with_commit_pin() {
        let toml = r#"
            [tpm]
            type   = "git-repo"
            url    = "https://github.com/tmux-plugins/tpm.git"
            target = "~/.tmux/plugins/tpm"
            commit = "3a8b3f4a5b8d1c2e3f4a5b6c7d8e9f0a1b2c3d4e"
        "#;
        let parsed = parse_externals_toml(toml.as_bytes()).unwrap();
        let entry = parsed.entries.get("tpm").unwrap();
        match &entry.spec {
            FetchSpec::GitRepo { commit, .. } => {
                assert_eq!(
                    commit.as_deref(),
                    Some("3a8b3f4a5b8d1c2e3f4a5b6c7d8e9f0a1b2c3d4e")
                );
            }
            other => panic!("expected GitRepo, got {other:?}"),
        }
    }

    #[test]
    fn rejects_absolute_subpath() {
        let toml = r#"
            [bad]
            type    = "git-repo"
            url     = "https://example.com/x.git"
            target  = "~/x"
            subpath = "/etc"
        "#;
        let err = parse_externals_toml(toml.as_bytes()).unwrap_err();
        assert!(format!("{err}").contains("must be relative"));
    }

    #[test]
    fn rejects_dotdot_in_subpath() {
        let toml = r#"
            [bad]
            type    = "git-repo"
            url     = "https://example.com/x.git"
            target  = "~/x"
            subpath = "themes/../../etc"
        "#;
        let err = parse_externals_toml(toml.as_bytes()).unwrap_err();
        assert!(format!("{err}").contains(".."));
    }

    #[test]
    fn rejects_dash_prefixed_subpath() {
        let toml = r#"
            [bad]
            type    = "git-repo"
            url     = "https://example.com/x.git"
            target  = "~/x"
            subpath = "-experimental"
        "#;
        let err = parse_externals_toml(toml.as_bytes()).unwrap_err();
        assert!(format!("{err}").contains("must not start with `-`"));
    }

    #[test]
    fn rejects_short_commit_sha() {
        let toml = r#"
            [bad]
            type   = "git-repo"
            url    = "https://example.com/x.git"
            target = "~/x"
            commit = "abc1234"
        "#;
        let err = parse_externals_toml(toml.as_bytes()).unwrap_err();
        assert!(format!("{err}").contains("full 40-char SHA"));
    }

    #[test]
    fn rejects_non_hex_commit_sha() {
        let toml = r#"
            [bad]
            type   = "git-repo"
            url    = "https://example.com/x.git"
            target = "~/x"
            commit = "zzzzbeef1234567890abcdef1234567890abcdef"
        "#;
        let err = parse_externals_toml(toml.as_bytes()).unwrap_err();
        assert!(format!("{err}").contains("must be hex"));
    }

    #[test]
    fn rejects_ref_and_commit_simultaneously() {
        let toml = r#"
            [conflicted]
            type   = "git-repo"
            url    = "https://example.com/x.git"
            target = "~/x"
            ref    = "v1"
            commit = "abc1234"
        "#;
        let err = parse_externals_toml(toml.as_bytes()).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("mutually exclusive"), "got: {msg}");
        assert!(msg.contains("conflicted"), "got: {msg}");
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

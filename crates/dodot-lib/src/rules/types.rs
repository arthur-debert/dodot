//! Rule and match types.
//!
//! Pure data: the [`Rule`] schema config emits, the [`PackEntry`] the
//! scanner produces, and the [`RuleMatch`] the matcher emits. No
//! behaviour lives here — see `pattern.rs`, `scanner.rs`, and `grouping.rs`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

/// A rule mapping a file pattern to a handler.
#[derive(Debug, Clone, Serialize)]
pub struct Rule {
    /// Pattern to match against (e.g. `"install.sh"`, `"*.sh"`, `"bin/"`).
    pub pattern: String,

    /// Handler to use for matching files (e.g. `"symlink"`, `"install"`).
    pub handler: String,

    /// Higher priority rules are checked first. Default is 0.
    pub priority: i32,

    /// Match the basename case-insensitively. Set by `mappings.skip`
    /// patterns so README/Readme/readme all hit the same rule without
    /// forcing every config writer to enumerate every casing. Defaults
    /// to false (case-sensitive, matches glob conventions).
    #[serde(default, skip_serializing_if = "is_false")]
    pub case_insensitive: bool,

    /// Handler-specific options passed through from config.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub options: HashMap<String, String>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// A raw file entry discovered during directory walking (before rule matching).
#[derive(Debug, Clone)]
pub struct PackEntry {
    /// Path relative to the pack root (e.g. `"vimrc"`, `"nvim/init.lua"`).
    pub relative_path: PathBuf,
    /// Absolute path to the file.
    pub absolute_path: PathBuf,
    /// Whether this entry is a directory.
    pub is_dir: bool,
    /// When `Some`, this entry was gated out by a directory-segment
    /// gate (`_<label>/`) whose predicate evaluated false on this host.
    /// The scanner emits the gate dir as a single PackEntry with this
    /// set; [`crate::rules::Scanner::match_entries`] converts it to a
    /// `gate`-handler match for the status renderer. `None` for all
    /// other entries.
    pub gate_failure: Option<GateFailure>,
}

/// Diagnostic snapshot of a failed directory-segment gate, attached to
/// a [`PackEntry`] when the scanner gates out a `_<label>/` directory.
#[derive(Debug, Clone)]
pub struct GateFailure {
    pub label: String,
    pub predicate: String,
    pub host: String,
}

/// A file that matched a rule during pack scanning.
#[derive(Debug, Clone, Serialize)]
pub struct RuleMatch {
    /// Path relative to the pack root (e.g. `"vimrc"`, `"nvim/init.lua"`).
    pub relative_path: PathBuf,

    /// Absolute path to the file.
    pub absolute_path: PathBuf,

    /// Name of the pack this file belongs to.
    pub pack: String,

    /// Name of the handler that should process this file.
    pub handler: String,

    /// Whether this entry is a directory.
    pub is_dir: bool,

    /// Handler-specific options from the matched rule.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub options: HashMap<String, String>,

    /// If this file was produced by a preprocessor, the original source path.
    /// `None` for regular (non-preprocessed) files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preprocessor_source: Option<PathBuf>,

    /// In-memory rendered bytes for preprocessor-produced files.
    ///
    /// Populated by `plan_pack_inner` from
    /// `PreprocessResult.rendered_bytes` so that handlers needing
    /// the rendered content for sentinel hashing (`install`,
    /// `homebrew`) can hash these bytes directly instead of reading
    /// the rendered file from disk. That decoupling is the
    /// structural enabler for §7.4 Passive mode (`dodot status`,
    /// `up --dry-run`), where the rendered file is intentionally
    /// not written to disk. See issue #121.
    ///
    /// `None` for regular (non-preprocessed) files; handlers
    /// targeting those still read from `absolute_path` directly.
    /// `Arc<[u8]>` so cloning a `RuleMatch` (e.g. during handler
    /// grouping) doesn't duplicate the buffer.
    #[serde(skip)]
    pub rendered_bytes: Option<Arc<[u8]>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_match_serializes() {
        let m = RuleMatch {
            relative_path: "vimrc".into(),
            absolute_path: "/dots/vim/vimrc".into(),
            pack: "vim".into(),
            handler: "symlink".into(),
            is_dir: false,
            options: HashMap::new(),
            preprocessor_source: None,
            rendered_bytes: None,
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("vimrc"));
        assert!(json.contains("symlink"));
        assert!(!json.contains("options"));
    }
}

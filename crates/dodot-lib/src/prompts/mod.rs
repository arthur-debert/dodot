//! Persistent registry of dismissed prompts.
//!
//! A content-agnostic key/value store for "have I shown the user X yet?"
//! state. Callers pass opaque string keys; the registry tracks which
//! keys have been dismissed and when. Reset clears one or all keys so
//! the next caller-side check fires the prompt again.
//!
//! Used as the foundation for one-time onboarding prompts, install
//! offers, and any other nudge that should not repeat after the user
//! has answered it. The registry itself does NOT render prompts, decide
//! UX, or know what each key means — that is the caller's job.
//!
//! See `docs/proposals/plists.lex` §5.3 for the first user (the plist
//! filter install offer); future callers slot in by picking their own
//! key and updating the catalog in [`catalog`] so `dodot prompts list`
//! can describe them.
//!
//! Storage: JSON at `<data_dir>/prompts.json` (durable, alongside other
//! persistent state). Schema is versioned for forward compatibility.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::fs::Fs;
use crate::{DodotError, Result};

pub mod catalog;

const SCHEMA_VERSION: u32 = 1;

/// One row in the registry: when the user dismissed this prompt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptRecord {
    /// Wall-clock unix timestamp (seconds) of the dismissal.
    pub dismissed_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PromptFile {
    version: u32,
    #[serde(default)]
    prompts: BTreeMap<String, PromptRecord>,
}

impl Default for PromptFile {
    fn default() -> Self {
        Self {
            version: SCHEMA_VERSION,
            prompts: BTreeMap::new(),
        }
    }
}

/// Persistent dismissed-prompt registry.
///
/// `load` populates from disk (returns an empty registry if the file
/// does not exist); mutations are in-memory until `save` is called.
#[derive(Debug, Clone)]
pub struct PromptRegistry {
    path: PathBuf,
    file: PromptFile,
}

impl PromptRegistry {
    /// Load the registry from `path`. A missing file is treated as an
    /// empty registry — first-run users have nothing to read.
    pub fn load(fs: &dyn Fs, path: PathBuf) -> Result<Self> {
        if !fs.exists(&path) {
            return Ok(Self {
                path,
                file: PromptFile::default(),
            });
        }
        let raw = fs.read_to_string(&path)?;
        let file: PromptFile = serde_json::from_str(&raw).map_err(|e| {
            DodotError::Other(format!(
                "failed to parse prompts registry at {}: {e}",
                path.display()
            ))
        })?;
        if file.version != SCHEMA_VERSION {
            return Err(DodotError::Other(format!(
                "prompts registry at {} has unsupported schema version {} (expected {})",
                path.display(),
                file.version,
                SCHEMA_VERSION
            )));
        }
        Ok(Self { path, file })
    }

    /// Persist the registry to its backing path. Creates parent dirs
    /// as needed.
    pub fn save(&self, fs: &dyn Fs) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs.mkdir_all(parent)?;
        }
        let body = serde_json::to_string_pretty(&self.file)
            .map_err(|e| DodotError::Other(format!("failed to serialise prompts: {e}")))?;
        fs.write_file(&self.path, body.as_bytes())?;
        Ok(())
    }

    /// Backing file path (for diagnostic messages and `dodot prompts list`).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// True if the prompt with this key has been dismissed.
    pub fn is_dismissed(&self, key: &str) -> bool {
        self.file.prompts.contains_key(key)
    }

    /// Record that the user dismissed this prompt. Idempotent — calling
    /// twice updates `dismissed_at` to the latest call.
    pub fn dismiss(&mut self, key: &str) {
        self.dismiss_at(key, now_secs_unix())
    }

    /// Test-friendly sibling of [`dismiss`] that takes an explicit
    /// timestamp instead of reading the wall clock.
    pub fn dismiss_at(&mut self, key: &str, dismissed_at: u64) {
        self.file
            .prompts
            .insert(key.to_string(), PromptRecord { dismissed_at });
    }

    /// Clear a single dismissal so the prompt fires again next time.
    /// Returns `true` if the key was present.
    pub fn reset(&mut self, key: &str) -> bool {
        self.file.prompts.remove(key).is_some()
    }

    /// Clear every dismissal. Returns the count cleared.
    pub fn reset_all(&mut self) -> usize {
        let n = self.file.prompts.len();
        self.file.prompts.clear();
        n
    }

    /// Snapshot of dismissed prompts, sorted by key. Suitable for
    /// rendering by `dodot prompts list`. Allocates; if you only need
    /// per-key lookups, prefer [`dismissed_at`](Self::dismissed_at).
    pub fn dismissed(&self) -> Vec<(&str, &PromptRecord)> {
        self.file
            .prompts
            .iter()
            .map(|(k, v)| (k.as_str(), v))
            .collect()
    }

    /// O(log n) lookup of a single prompt's dismissal timestamp.
    /// Returns `None` if the prompt is currently active. Pair with
    /// [`is_dismissed`](Self::is_dismissed) when you only need the
    /// boolean — this returns the timestamp too for UIs that show
    /// "dismissed at …".
    pub fn dismissed_at(&self, key: &str) -> Option<u64> {
        self.file.prompts.get(key).map(|r| r.dismissed_at)
    }
}

/// Wall-clock unix timestamp helper used by [`PromptRegistry::dismiss`].
/// Tests should call [`PromptRegistry::dismiss_at`] with a fixed value.
pub fn now_secs_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Pather;
    use crate::testing::TempEnvironment;

    fn registry(env: &TempEnvironment) -> PromptRegistry {
        let path = env.paths.prompts_path();
        PromptRegistry::load(env.fs.as_ref(), path).expect("load")
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let env = TempEnvironment::builder().build();
        let r = registry(&env);
        assert!(r.dismissed().is_empty());
        assert!(!r.is_dismissed("anything"));
    }

    #[test]
    fn dismiss_then_query() {
        let env = TempEnvironment::builder().build();
        let mut r = registry(&env);
        r.dismiss_at("plist.install_filters", 1714557600);
        assert!(r.is_dismissed("plist.install_filters"));
        assert!(!r.is_dismissed("something.else"));
    }

    #[test]
    fn dismiss_is_idempotent_and_updates_timestamp() {
        let env = TempEnvironment::builder().build();
        let mut r = registry(&env);
        r.dismiss_at("k", 100);
        r.dismiss_at("k", 200);
        assert_eq!(r.dismissed().len(), 1);
        assert_eq!(r.dismissed()[0].1.dismissed_at, 200);
    }

    #[test]
    fn save_and_reload_roundtrip() {
        let env = TempEnvironment::builder().build();
        {
            let mut r = registry(&env);
            r.dismiss_at("a", 100);
            r.dismiss_at("b", 200);
            r.save(env.fs.as_ref()).expect("save");
        }
        let r = registry(&env);
        let dismissed = r.dismissed();
        assert_eq!(dismissed.len(), 2);
        // BTreeMap ordering — keys are sorted.
        assert_eq!(dismissed[0].0, "a");
        assert_eq!(dismissed[1].0, "b");
    }

    #[test]
    fn reset_one_returns_whether_present() {
        let env = TempEnvironment::builder().build();
        let mut r = registry(&env);
        r.dismiss_at("a", 100);
        assert!(r.reset("a"));
        assert!(!r.reset("a")); // already gone
        assert!(!r.reset("never-set"));
    }

    #[test]
    fn reset_all_returns_count_cleared() {
        let env = TempEnvironment::builder().build();
        let mut r = registry(&env);
        r.dismiss_at("a", 1);
        r.dismiss_at("b", 2);
        r.dismiss_at("c", 3);
        assert_eq!(r.reset_all(), 3);
        assert!(r.dismissed().is_empty());
        assert_eq!(r.reset_all(), 0); // already empty
    }

    #[test]
    fn corrupted_file_returns_error() {
        let env = TempEnvironment::builder().build();
        let path = env.paths.prompts_path();
        env.fs.as_ref().mkdir_all(path.parent().unwrap()).unwrap();
        env.fs.as_ref().write_file(&path, b"{not json").unwrap();
        let err = PromptRegistry::load(env.fs.as_ref(), path).unwrap_err();
        assert!(
            format!("{err}").contains("failed to parse"),
            "expected parse error, got: {err}"
        );
    }

    #[test]
    fn unsupported_schema_version_returns_error() {
        let env = TempEnvironment::builder().build();
        let path = env.paths.prompts_path();
        env.fs.as_ref().mkdir_all(path.parent().unwrap()).unwrap();
        env.fs
            .as_ref()
            .write_file(&path, br#"{"version": 999, "prompts": {}}"#)
            .unwrap();
        let err = PromptRegistry::load(env.fs.as_ref(), path).unwrap_err();
        assert!(
            format!("{err}").contains("unsupported schema version"),
            "expected schema error, got: {err}"
        );
    }
}

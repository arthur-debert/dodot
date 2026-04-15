//! Handler trait, types, and implementations.
//!
//! Handlers are the bridge between file matching (rules) and execution
//! (operations). Each handler knows how to transform a set of matched
//! files into [`HandlerIntent`]s that the executor will carry out.
//!
//! Handlers are pure data transformers — they declare what operations
//! they need without performing any I/O themselves.

pub mod homebrew;
pub mod install;
pub mod path;
pub mod shell;
pub mod symlink;

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::datastore::DataStore;
use crate::fs::Fs;
use crate::operations::HandlerIntent;
use crate::paths::Pather;
use crate::rules::RuleMatch;
use crate::Result;

/// Whether a handler manages configuration or executes code.
///
/// Configuration handlers (symlink, shell, path) are safe to run
/// repeatedly. Code execution handlers (install, homebrew) run once
/// and are tracked by sentinels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum HandlerCategory {
    Configuration,
    CodeExecution,
}

/// The status of a handler's operations for a single file.
#[derive(Debug, Clone, Serialize)]
pub struct HandlerStatus {
    /// Which file this status is for (relative to pack root).
    pub file: String,
    /// Which handler produced this status.
    pub handler: String,
    /// Whether the file is currently deployed.
    pub deployed: bool,
    /// Human-readable status message (e.g. "linked to ~/.vimrc").
    pub message: String,
}

/// The core handler abstraction.
///
/// Each handler is a small struct (often zero-sized) that implements
/// this trait. Handlers are stored in a `HashMap<String, Box<dyn Handler>>`
/// registry and dispatched by name at runtime.
///
/// # Object safety
///
/// This trait is designed to be used as `&dyn Handler` and
/// `Box<dyn Handler>`. All methods use `&self` and return owned types.
pub trait Handler: Send + Sync {
    /// Unique name for this handler (e.g. `"symlink"`, `"install"`).
    fn name(&self) -> &str;

    /// Whether this is a configuration or code-execution handler.
    fn category(&self) -> HandlerCategory;

    /// Transform matched files into intents.
    ///
    /// This is the heart of each handler — it declares what operations
    /// are needed without performing any I/O.
    fn to_intents(
        &self,
        matches: &[RuleMatch],
        config: &HandlerConfig,
        paths: &dyn Pather,
    ) -> Result<Vec<HandlerIntent>>;

    /// Check whether a file has been deployed by this handler.
    fn check_status(
        &self,
        file: &Path,
        pack: &str,
        datastore: &dyn DataStore,
    ) -> Result<HandlerStatus>;
}

/// Configuration subset relevant to handlers.
///
/// Populated from `DodotConfig::to_handler_config()`. Carries exactly
/// what handlers need without coupling them to the full config.
#[derive(Debug, Clone, Default, Serialize)]
pub struct HandlerConfig {
    /// Paths that must be forced to `$HOME` (e.g. `["ssh", "bashrc"]`).
    pub force_home: Vec<String>,
    /// Paths that must not be symlinked (e.g. `[".ssh/id_rsa"]`).
    pub protected_paths: Vec<String>,
    /// Per-file custom symlink target overrides.
    /// Key = relative path in pack, Value = target path.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub targets: std::collections::HashMap<String, String>,
}

/// Well-known handler names.
pub const HANDLER_SYMLINK: &str = "symlink";
pub const HANDLER_SHELL: &str = "shell";
pub const HANDLER_PATH: &str = "path";
pub const HANDLER_INSTALL: &str = "install";
pub const HANDLER_HOMEBREW: &str = "homebrew";

/// Create the default handler registry.
///
/// Returns a map from handler name to handler instance. The `fs`
/// reference is needed by install and homebrew handlers for checksum
/// computation.
pub fn create_registry(fs: &dyn Fs) -> HashMap<String, Box<dyn Handler + '_>> {
    let mut registry: HashMap<String, Box<dyn Handler>> = HashMap::new();
    registry.insert(HANDLER_SYMLINK.into(), Box::new(symlink::SymlinkHandler));
    registry.insert(HANDLER_SHELL.into(), Box::new(shell::ShellHandler));
    registry.insert(HANDLER_PATH.into(), Box::new(path::PathHandler));
    registry.insert(
        HANDLER_INSTALL.into(),
        Box::new(install::InstallHandler::new(fs)),
    );
    registry.insert(
        HANDLER_HOMEBREW.into(),
        Box::new(homebrew::HomebrewHandler::new(fs)),
    );
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time check: Handler must be object-safe
    #[allow(dead_code)]
    fn assert_object_safe(_: &dyn Handler) {}

    #[allow(dead_code)]
    fn assert_boxable(_: Box<dyn Handler>) {}

    #[test]
    fn handler_category_eq() {
        assert_eq!(
            HandlerCategory::Configuration,
            HandlerCategory::Configuration
        );
        assert_ne!(
            HandlerCategory::Configuration,
            HandlerCategory::CodeExecution
        );
    }

    #[test]
    fn handler_status_serializes() {
        let status = HandlerStatus {
            file: "vimrc".into(),
            handler: "symlink".into(),
            deployed: true,
            message: "linked to ~/.vimrc".into(),
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("deployed"));
        assert!(json.contains("linked to ~/.vimrc"));
    }

    #[test]
    fn handler_config_default() {
        let config = HandlerConfig::default();
        assert!(config.force_home.is_empty());
        assert!(config.protected_paths.is_empty());
    }
}

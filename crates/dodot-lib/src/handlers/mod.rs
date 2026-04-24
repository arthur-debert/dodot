//! Handler trait, types, and implementations.
//!
//! Handlers are the bridge between file matching (rules) and execution
//! (operations). Each handler knows how to transform a set of matched
//! files into [`HandlerIntent`]s that the executor will carry out.
//!
//! Handlers are intent planners. They may perform **read-only**
//! filesystem inspection (e.g. the symlink handler reads a matched
//! directory's contents to decide between wholesale and per-file
//! linking) but must not mutate anything — mutations are the executor's
//! job. This keeps planning idempotent and safe to re-run.

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

/// The ordered execution phase a handler belongs to.
///
/// Phases run in declaration order — the derived [`Ord`] drives
/// [`crate::rules::handler_execution_order`]. Each handler belongs to
/// exactly one phase, so adding a handler is a deliberate design
/// choice: *where does this fit in the pipeline?* rather than a silent
/// answer from alphabetical sort.
///
/// # Why this order
///
/// - [`Provision`](Self::Provision) installs packages. Anything later
///   (including user `install.sh` scripts) may depend on the tools it
///   put on PATH.
/// - [`Setup`](Self::Setup) runs user-authored setup scripts that can
///   lean on Provision having completed (`brew` and formulas available).
/// - [`PathExport`](Self::PathExport) stages `bin/` directories onto
///   `$PATH`. It runs before [`ShellInit`](Self::ShellInit) so shell
///   init scripts can reference the executables it exposes.
/// - [`ShellInit`](Self::ShellInit) registers shell startup files.
/// - [`Link`](Self::Link) is the catchall symlink phase. It runs last
///   because the symlink handler is catchall — precise handlers above
///   must have already claimed their files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum ExecutionPhase {
    /// Install packages (homebrew).
    Provision,
    /// Run user setup scripts (install).
    Setup,
    /// Stage directories onto `$PATH` (path).
    PathExport,
    /// Register shell init files (shell).
    ShellInit,
    /// Catchall: link remaining files (symlink). Always last.
    Link,
}

impl ExecutionPhase {
    /// The category this phase belongs to.
    ///
    /// Provision and Setup run user-authored code and are gated by
    /// sentinels; the rest are idempotent filesystem work.
    pub fn category(self) -> HandlerCategory {
        match self {
            Self::Provision | Self::Setup => HandlerCategory::CodeExecution,
            Self::PathExport | Self::ShellInit | Self::Link => HandlerCategory::Configuration,
        }
    }
}

/// Whether a handler matches specific names or acts as a catchall.
///
/// [`MatchMode::Precise`] handlers only match whitelisted patterns
/// (e.g. `bin/`, `install.sh`). [`MatchMode::Catchall`] handlers
/// match anything not already claimed and must run after all precise
/// handlers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchMode {
    Precise,
    Catchall,
}

/// Whether a handler's match is consumed or leaves the entry available.
///
/// [`HandlerScope::Exclusive`] handlers consume their matches — once
/// claimed, no other handler sees the entry. [`HandlerScope::Shared`]
/// handlers let other handlers also process the same entry (future
/// use-cases like audit/indexing). At most one `Exclusive` + `Catchall`
/// handler may exist in a registry; this is validated at build time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerScope {
    Exclusive,
    Shared,
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

    /// Execution phase for this handler.
    ///
    /// Determines where this handler runs in the per-pack pipeline.
    /// See [`ExecutionPhase`] for the ordering and the rationale for
    /// each slot.
    fn phase(&self) -> ExecutionPhase;

    /// Whether this is a configuration or code-execution handler.
    ///
    /// Derived from [`Self::phase`]; override only if a handler's
    /// phase doesn't imply its category (none today).
    fn category(&self) -> HandlerCategory {
        self.phase().category()
    }

    /// How this handler decides what to claim.
    ///
    /// Defaults to [`MatchMode::Precise`]. Override to `Catchall` for
    /// a fallback handler (like symlink) that takes anything not
    /// already claimed.
    fn match_mode(&self) -> MatchMode {
        MatchMode::Precise
    }

    /// Whether a match removes the entry from further consideration.
    ///
    /// Defaults to [`HandlerScope::Exclusive`] — a matched entry is
    /// consumed and no other handler will see it.
    fn scope(&self) -> HandlerScope {
        HandlerScope::Exclusive
    }

    /// Transform matched files into intents.
    ///
    /// This is the heart of each handler: it declares what operations
    /// are needed. `fs` is available for **read-only** inspection
    /// (e.g. enumerating a matched directory to decide wholesale vs
    /// per-file linking). Handlers must not write, delete, or rename
    /// anything here — mutations happen in the executor.
    fn to_intents(
        &self,
        matches: &[RuleMatch],
        config: &HandlerConfig,
        paths: &dyn Pather,
        fs: &dyn Fs,
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
#[derive(Debug, Clone, Serialize)]
pub struct HandlerConfig {
    /// Paths that must be forced to `$HOME` (e.g. `["ssh", "bashrc"]`).
    pub force_home: Vec<String>,
    /// Paths that must not be symlinked (e.g. `[".ssh/id_rsa"]`).
    pub protected_paths: Vec<String>,
    /// Per-file custom symlink target overrides.
    /// Key = relative path in pack, Value = target path.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub targets: std::collections::HashMap<String, String>,
    /// Whether to auto-`chmod +x` files in path-handler directories.
    /// See [`PathSection::auto_chmod_exec`](crate::config::PathSection::auto_chmod_exec).
    pub auto_chmod_exec: bool,
    /// Pack-level ignore patterns (from `[pack] ignore`). Handlers that
    /// recurse into a matched directory should apply these so the
    /// per-file fallback doesn't pick up `.DS_Store`, `.git`, etc.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pack_ignore: Vec<String>,
}

impl Default for HandlerConfig {
    fn default() -> Self {
        Self {
            force_home: Vec::new(),
            protected_paths: Vec::new(),
            targets: std::collections::HashMap::new(),
            auto_chmod_exec: true,
            pack_ignore: Vec::new(),
        }
    }
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
    validate_registry(&registry);
    registry
}

/// Enforce registry invariants.
///
/// At most one handler may be simultaneously [`MatchMode::Catchall`]
/// and [`HandlerScope::Exclusive`]. Two such handlers would fight over
/// the same "leftover" entries with no principled way to pick a winner.
///
/// This is a developer-only invariant: the built-in registry is
/// hard-coded and third-party handlers would be added via code, not
/// user input. We use `debug_assert!` so release builds never panic
/// from a misconfiguration here; if the invariant is violated in a
/// dev build the panic surfaces immediately.
fn validate_registry(registry: &HashMap<String, Box<dyn Handler + '_>>) {
    let exclusive_catchalls: Vec<&str> = registry
        .values()
        .filter(|h| h.match_mode() == MatchMode::Catchall && h.scope() == HandlerScope::Exclusive)
        .map(|h| h.name())
        .collect();
    debug_assert!(
        exclusive_catchalls.len() <= 1,
        "at most one exclusive catchall handler allowed, found: {exclusive_catchalls:?}"
    );
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
    fn execution_phase_declaration_order_drives_ord() {
        assert!(ExecutionPhase::Provision < ExecutionPhase::Setup);
        assert!(ExecutionPhase::Setup < ExecutionPhase::PathExport);
        assert!(ExecutionPhase::PathExport < ExecutionPhase::ShellInit);
        assert!(ExecutionPhase::ShellInit < ExecutionPhase::Link);
    }

    #[test]
    fn execution_phase_category_mapping() {
        assert_eq!(
            ExecutionPhase::Provision.category(),
            HandlerCategory::CodeExecution
        );
        assert_eq!(
            ExecutionPhase::Setup.category(),
            HandlerCategory::CodeExecution
        );
        assert_eq!(
            ExecutionPhase::PathExport.category(),
            HandlerCategory::Configuration
        );
        assert_eq!(
            ExecutionPhase::ShellInit.category(),
            HandlerCategory::Configuration
        );
        assert_eq!(
            ExecutionPhase::Link.category(),
            HandlerCategory::Configuration
        );
    }

    #[test]
    fn builtin_handler_phases() {
        let fs = crate::fs::OsFs::new();
        let registry = create_registry(&fs);
        assert_eq!(
            registry[HANDLER_HOMEBREW].phase(),
            ExecutionPhase::Provision
        );
        assert_eq!(registry[HANDLER_INSTALL].phase(), ExecutionPhase::Setup);
        assert_eq!(registry[HANDLER_PATH].phase(), ExecutionPhase::PathExport);
        assert_eq!(registry[HANDLER_SHELL].phase(), ExecutionPhase::ShellInit);
        assert_eq!(registry[HANDLER_SYMLINK].phase(), ExecutionPhase::Link);
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

    #[test]
    fn default_registry_has_exactly_one_exclusive_catchall() {
        let fs = crate::fs::OsFs::new();
        let registry = create_registry(&fs);
        let exclusive_catchalls: Vec<&str> = registry
            .values()
            .filter(|h| {
                h.match_mode() == MatchMode::Catchall && h.scope() == HandlerScope::Exclusive
            })
            .map(|h| h.name())
            .collect();
        assert_eq!(exclusive_catchalls, vec!["symlink"]);
    }

    #[test]
    #[should_panic(expected = "at most one exclusive catchall handler")]
    fn two_exclusive_catchalls_panic() {
        struct FakeCatchall;
        impl Handler for FakeCatchall {
            fn name(&self) -> &str {
                "fake"
            }
            fn phase(&self) -> ExecutionPhase {
                ExecutionPhase::Link
            }
            fn match_mode(&self) -> MatchMode {
                MatchMode::Catchall
            }
            fn scope(&self) -> HandlerScope {
                HandlerScope::Exclusive
            }
            fn to_intents(
                &self,
                _matches: &[RuleMatch],
                _config: &HandlerConfig,
                _paths: &dyn Pather,
                _fs: &dyn Fs,
            ) -> Result<Vec<HandlerIntent>> {
                Ok(Vec::new())
            }
            fn check_status(
                &self,
                _file: &Path,
                _pack: &str,
                _datastore: &dyn DataStore,
            ) -> Result<HandlerStatus> {
                unreachable!()
            }
        }
        let fs = crate::fs::OsFs::new();
        let mut registry = create_registry(&fs);
        registry.insert("fake".into(), Box::new(FakeCatchall));
        validate_registry(&registry);
    }
}

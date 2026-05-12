//! Execution context — the bag of dependencies every command receives.
//!
//! [`ExecutionContext`] is the entry point's dependency-injection
//! envelope: filesystem, datastore, config manager, runner, plus the
//! flags that scope a single invocation (`dry_run`, `force`, view/group
//! mode, …). Production wires up [`ExecutionContext::production`];
//! tests assemble fields directly.
//!
//! Lives in its own file so the orchestration pipeline can stay focused
//! on `execute()` / `plan_pack()` without the constructor weighing
//! every read.

use std::sync::Arc;

use crate::config::ConfigManager;
use crate::datastore::DataStore;
use crate::fs::Fs;
use crate::gates::HostFacts;
use crate::paths::Pather;

/// Everything the pipeline needs to execute.
pub struct ExecutionContext {
    pub fs: Arc<dyn Fs>,
    pub datastore: Arc<dyn DataStore>,
    pub paths: Arc<dyn Pather>,
    pub config_manager: Arc<ConfigManager>,
    /// Pre-flight syntax checker for shell sources. Production wires
    /// up [`SystemSyntaxChecker`](crate::shell::SystemSyntaxChecker)
    /// (spawns real `bash`/`zsh -n`); tests inject a mock.
    pub syntax_checker: Arc<dyn crate::shell::SyntaxChecker>,
    /// Subprocess runner for advisory probes (homebrew-cask lookup,
    /// macOS `mdls`/`mdfind`). Production reuses the same
    /// [`ShellCommandRunner`](crate::datastore::ShellCommandRunner)
    /// the datastore uses for handler-driven commands; tests inject a
    /// mock that returns canned outputs without spawning processes.
    /// See `docs/proposals/macos-paths.lex` §8.
    pub command_runner: Arc<dyn crate::datastore::CommandRunner>,
    pub dry_run: bool,
    pub no_provision: bool,
    pub provision_rerun: bool,
    pub force: bool,
    /// Opt-in drift detection for externals. When true, `status`
    /// hashes each deployed external entry's content and compares
    /// against the configured signature, surfacing any divergence as
    /// a warning. Default `false` because hashing every deployed
    /// external on every `status` invocation is not the right default
    /// for big trees (oh-my-zsh, etc.).
    pub check_drift: bool,
    /// How pack-status output should render rows: `Full` keeps today's
    /// per-file listing, `Short` collapses each pack to one summary
    /// line. Consumed by every command that renders through the
    /// `pack-status` template (`status`, `up`, `down`, `adopt`);
    /// ignored by commands that emit `message` / `list` output.
    pub view_mode: crate::commands::ViewMode,
    /// How packs are ordered in pack-status output: `Name` (flat
    /// alphabetical / discovery order) or `Status` (grouped under
    /// Ignored / Deployed / Pending / Error banners). Consumed by
    /// every command that renders through the `pack-status` template;
    /// ignored by commands that emit `message` / `list` output.
    pub group_mode: crate::commands::GroupMode,
    /// When true, install-script execution streams raw stdout/stderr
    /// to the user's terminal. The default (`false`) keeps output
    /// quiet — only the `# status:` progress markers and the leading
    /// comment block of each script are surfaced. Wired from the CLI
    /// global `--verbose`/`--debug` flag.
    pub verbose: bool,
    /// Snapshot of the host's gate-relevant facts (os, arch, hostname,
    /// username). Detected once per context so per-pack scanning and
    /// matching avoid re-running `hostname(1)`/env reads. Constructed
    /// by [`Self::production`]; tests build via `HostFacts::for_tests`.
    pub host_facts: Arc<HostFacts>,
}

impl ExecutionContext {
    /// Create a default production context from a dotfiles root path.
    ///
    /// Wires up the real filesystem, XDG paths, filesystem-backed
    /// datastore with shell command runner, and clapfig config manager.
    /// `verbose` controls whether install-script stdout/stderr is
    /// streamed to the terminal; the field is also stored on the
    /// returned context for any other consumer that cares. Callers
    /// only need to override specific fields (e.g. `dry_run`).
    pub fn production(dotfiles_root: &std::path::Path, verbose: bool) -> crate::Result<Self> {
        let config_manager = Arc::new(ConfigManager::new(dotfiles_root)?);

        // Honor `app_uses_library = false` by collapsing app_support_dir
        // onto xdg_config_home — that's the "Linux-style ~/.config
        // everywhere even on macOS" escape hatch from
        // `docs/proposals/macos-paths.lex` §6.3 / §11.2.
        //
        // Soft-fail by design: a config-load failure here only blocks
        // the `app_uses_library = false` override from being applied,
        // not context construction itself. Real config errors (parse
        // failures, missing required fields) bubble up the next time a
        // command calls `config_manager.root_config()` — same surface,
        // same error path, just without preempting Pather construction.
        // If the read fails here we leave `app_support_dir` at the
        // platform default and let the actual command surface the error.
        let mut paths_builder = crate::paths::XdgPather::builder().dotfiles_root(dotfiles_root);
        if let Ok(root_config) = config_manager.root_config() {
            if !root_config.symlink.app_uses_library {
                // Resolve XDG the way XdgPatherBuilder will, then pin
                // app_support_dir at the same path. We can't read the
                // builder's resolved xdg back out before build(), so
                // duplicate the precedence here.
                let home = std::env::var("HOME")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|_| std::path::PathBuf::from("/tmp/dodot-unknown-home"));
                let xdg = std::env::var("XDG_CONFIG_HOME")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|_| home.join(".config"));
                paths_builder = paths_builder.app_support_dir(xdg);
            }
        }
        let paths = Arc::new(paths_builder.build()?);
        let fs: Arc<dyn Fs> = Arc::new(crate::fs::OsFs::new());
        let runner: Arc<dyn crate::datastore::CommandRunner> =
            Arc::new(crate::datastore::ShellCommandRunner::new(verbose));
        let datastore: Arc<dyn DataStore> = Arc::new(crate::datastore::FilesystemDataStore::new(
            fs.clone(),
            paths.clone(),
            runner.clone(),
        ));

        Ok(Self {
            fs,
            datastore,
            paths,
            config_manager,
            syntax_checker: Arc::new(crate::shell::SystemSyntaxChecker),
            command_runner: runner,
            dry_run: false,
            no_provision: false,
            provision_rerun: false,
            force: false,
            check_drift: false,
            view_mode: crate::commands::ViewMode::default(),
            group_mode: crate::commands::GroupMode::default(),
            verbose,
            host_facts: Arc::new(HostFacts::detect()),
        })
    }
}

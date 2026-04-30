//! Orchestration pipeline — the single entry point for executing
//! commands across packs.
//!
//! `execute()` owns the outer loop: discover packs → load per-pack
//! config → execute command → aggregate results.

use std::sync::Arc;

use serde::Serialize;
use tracing::{debug, info};

use crate::config::ConfigManager;
use crate::datastore::DataStore;
use crate::execution::Executor;
use crate::fs::Fs;
use crate::handlers;
use crate::operations::OperationResult;
use crate::packs::{self, Pack};
use crate::paths::Pather;
use crate::rules::{self, Scanner};
use crate::Result;

// ── Types ───────────────────────────────────────────────────────

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
    pub dry_run: bool,
    pub no_provision: bool,
    pub provision_rerun: bool,
    pub force: bool,
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
            runner,
        ));

        Ok(Self {
            fs,
            datastore,
            paths,
            config_manager,
            syntax_checker: Arc::new(crate::shell::SystemSyntaxChecker),
            dry_run: false,
            no_provision: false,
            provision_rerun: false,
            force: false,
            view_mode: crate::commands::ViewMode::default(),
            group_mode: crate::commands::GroupMode::default(),
            verbose,
        })
    }
}

/// Result for a single pack.
#[derive(Debug, Serialize)]
pub struct PackResult {
    pub pack_name: String,
    pub success: bool,
    pub operations: Vec<OperationResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Aggregated result across all packs.
#[derive(Debug, Serialize)]
pub struct ExecuteResult {
    pub pack_results: Vec<PackResult>,
    pub total_packs: usize,
    pub successful_packs: usize,
    pub failed_packs: usize,
}

impl ExecuteResult {
    pub fn is_success(&self) -> bool {
        self.failed_packs == 0
    }
}

// ── Command trait ───────────────────────────────────────────────

/// A command that operates on a single pack.
///
/// The orchestration pipeline calls `execute_for_pack` for each
/// discovered pack. Commands implement the specific logic (up, down,
/// status, etc.) while the pipeline handles discovery, config loading,
/// filtering, and aggregation.
pub trait Command: Send + Sync {
    fn name(&self) -> &str;

    fn execute_for_pack(&self, pack: &Pack, ctx: &ExecutionContext) -> Result<PackResult>;
}

// ── Pipeline ────────────────────────────────────────────────────

/// Execute a command across all (or filtered) packs.
///
/// This is the single entry point for the orchestration pipeline:
///
/// 1. Load root config
/// 2. Discover packs (filtering by name if specified)
/// 3. For each pack: load merged config → execute command → collect result
/// 4. Aggregate results
pub fn execute(
    command: &dyn Command,
    pack_filter: Option<&[String]>,
    ctx: &ExecutionContext,
) -> Result<ExecuteResult> {
    info!(command = command.name(), "starting command");

    // Load root config for pack-level ignore patterns
    let root_config = ctx.config_manager.root_config()?;
    debug!(
        ignore_patterns = ?root_config.pack.ignore,
        "loaded root config"
    );

    // Discover packs
    let mut all_packs = packs::discover_packs(
        ctx.fs.as_ref(),
        ctx.paths.dotfiles_root(),
        &root_config.pack.ignore,
    )?;
    info!(
        count = all_packs.len(),
        root = %ctx.paths.dotfiles_root().display(),
        "discovered packs"
    );

    // Validate and apply name filter
    if let Some(names) = pack_filter {
        let _warnings = validate_pack_names(names, ctx)?;
        // Warnings are handled by the calling command (status/up/down)
        debug!(filter = ?names, "applying pack filter");
        all_packs.retain(|p| names.iter().any(|n| n == &p.display_name || n == &p.name));
        info!(count = all_packs.len(), "packs after filter");
    }

    let total_packs = all_packs.len();
    let mut pack_results = Vec::with_capacity(total_packs);
    let mut successful = 0;
    let mut failed = 0;

    for mut pack in all_packs {
        info!(pack = %pack.name, "processing pack");

        // Load pack-specific merged config
        match ctx.config_manager.config_for_pack(&pack.path) {
            Ok(pack_config) => {
                debug!(pack = %pack.name, "loaded pack config");
                pack.config = pack_config.to_handler_config();
            }
            Err(e) => {
                info!(pack = %pack.name, error = %e, "pack config error, skipping");
                failed += 1;
                pack_results.push(PackResult {
                    pack_name: pack.name.clone(),
                    success: false,
                    operations: Vec::new(),
                    error: Some(format!("config error: {e}")),
                });
                continue;
            }
        }

        match command.execute_for_pack(&pack, ctx) {
            Ok(result) => {
                if result.success {
                    info!(pack = %pack.name, ops = result.operations.len(), "pack succeeded");
                    successful += 1;
                } else {
                    info!(pack = %pack.name, ops = result.operations.len(), "pack completed with errors");
                    failed += 1;
                }
                pack_results.push(result);
            }
            Err(e) => {
                info!(pack = %pack.name, error = %e, "pack failed");
                failed += 1;
                pack_results.push(PackResult {
                    pack_name: pack.name.clone(),
                    success: false,
                    operations: Vec::new(),
                    error: Some(e.to_string()),
                });
            }
        }
    }

    info!(
        total = total_packs,
        successful = successful,
        failed = failed,
        "command complete"
    );

    Ok(ExecuteResult {
        pack_results,
        total_packs,
        successful_packs: successful,
        failed_packs: failed,
    })
}

// ── Pack preparation ────────────────────────────────────────────

/// Discover, filter, and load config for all relevant packs.
///
/// Returns the list of packs ready for intent collection or command
/// execution. This is the shared first step for commands that need
/// to inspect multiple packs before acting (e.g. conflict detection).
pub fn prepare_packs(pack_filter: Option<&[String]>, ctx: &ExecutionContext) -> Result<Vec<Pack>> {
    let root_config = ctx.config_manager.root_config()?;

    let mut all_packs = packs::discover_packs(
        ctx.fs.as_ref(),
        ctx.paths.dotfiles_root(),
        &root_config.pack.ignore,
    )?;
    info!(count = all_packs.len(), "discovered packs");

    if let Some(names) = pack_filter {
        let _warnings = validate_pack_names(names, ctx)?;
        debug!(filter = ?names, "applying pack filter");
        all_packs.retain(|p| names.iter().any(|n| n == &p.display_name || n == &p.name));
        info!(count = all_packs.len(), "packs after filter");
    }

    // Load per-pack config
    let mut configured = Vec::with_capacity(all_packs.len());
    for mut pack in all_packs {
        let pack_config = ctx.config_manager.config_for_pack(&pack.path)?;
        debug!(pack = %pack.name, "loaded pack config");
        pack.config = pack_config.to_handler_config();
        configured.push(pack);
    }

    Ok(configured)
}

// ── Built-in "up" pipeline helpers ──────────────────────────────

/// Collect handler intents for a pack **without** executing them.
///
/// Runs the scan → preprocess → match rules → group by handler →
/// to_intents pipeline and returns the generated intents. This is the
/// first half of the two-phase execution model that enables cross-pack
/// conflict detection before any mutations happen.
///
/// Uses the default preprocessor registry
/// ([`crate::preprocessing::default_registry`]).
pub fn collect_pack_intents(
    pack: &Pack,
    ctx: &ExecutionContext,
) -> Result<Vec<crate::operations::HandlerIntent>> {
    let pack_config = ctx.config_manager.config_for_pack(&pack.path)?;
    let registry = crate::preprocessing::default_registry(
        &pack_config.preprocessor.template,
        ctx.paths.as_ref(),
    )?;
    collect_pack_intents_inner(pack, ctx, &pack_config, Some(&registry))
}

/// Like [`collect_pack_intents`], but accepts an explicit preprocessor
/// registry. If `None`, no preprocessing occurs.
///
/// This variant exists for testing: callers can inject a registry with
/// test preprocessors without requiring config-driven registration.
pub fn collect_pack_intents_with_preprocessors(
    pack: &Pack,
    ctx: &ExecutionContext,
    preprocessors: Option<&crate::preprocessing::PreprocessorRegistry>,
) -> Result<Vec<crate::operations::HandlerIntent>> {
    let pack_config = ctx.config_manager.config_for_pack(&pack.path)?;
    collect_pack_intents_inner(pack, ctx, &pack_config, preprocessors)
}

/// Plan for a single pack — the intents the executor will run plus
/// any soft warnings the handlers emitted during planning.
///
/// Warnings are non-fatal, human-readable strings (currently the
/// `_lib/` non-macOS skip notice from
/// `docs/proposals/macos-paths.lex` §4.2). Callers that surface
/// `PackStatusResult.warnings` should consume them; pure-execution
/// callers can ignore the field.
#[derive(Debug, Default, Clone)]
pub struct PackPlan {
    pub intents: Vec<crate::operations::HandlerIntent>,
    pub warnings: Vec<String>,
}

/// Like [`collect_pack_intents`], but returns both intents and any
/// soft warnings the handlers produced during planning.
///
/// Use this when surfacing per-pack warnings in user-facing output
/// (e.g. `commands::up` populating `PackStatusResult.warnings`). Pure
/// execution callers should keep using [`collect_pack_intents`].
pub fn plan_pack(pack: &Pack, ctx: &ExecutionContext) -> Result<PackPlan> {
    let pack_config = ctx.config_manager.config_for_pack(&pack.path)?;
    let registry = crate::preprocessing::default_registry(
        &pack_config.preprocessor.template,
        ctx.paths.as_ref(),
    )?;
    plan_pack_inner(pack, ctx, &pack_config, Some(&registry))
}

/// Shared implementation that takes a pre-loaded pack config. Both
/// entrypoints load the config once and pass it through so we don't
/// re-merge config for every pack (the ConfigManager caches by path,
/// but passing the config explicitly makes the data flow obvious).
fn collect_pack_intents_inner(
    pack: &Pack,
    ctx: &ExecutionContext,
    pack_config: &crate::config::DodotConfig,
    preprocessors: Option<&crate::preprocessing::PreprocessorRegistry>,
) -> Result<Vec<crate::operations::HandlerIntent>> {
    plan_pack_inner(pack, ctx, pack_config, preprocessors).map(|p| p.intents)
}

/// Same scan/preprocess/match/group/intents pipeline as
/// [`collect_pack_intents_inner`], but additionally collects
/// per-handler `warnings_for_matches` output.
fn plan_pack_inner(
    pack: &Pack,
    ctx: &ExecutionContext,
    pack_config: &crate::config::DodotConfig,
    preprocessors: Option<&crate::preprocessing::PreprocessorRegistry>,
) -> Result<PackPlan> {
    let rules = crate::config::mappings_to_rules(&pack_config.mappings);

    // Phase 1: Walk pack directory
    let scanner = Scanner::new(ctx.fs.as_ref());
    let entries = scanner.walk_pack(&pack.path, &pack_config.pack.ignore)?;
    debug!(pack = %pack.name, entries = entries.len(), "walked pack directory");

    // Phase 2: Preprocessing
    let preprocess_result = if let Some(registry) = preprocessors {
        if !registry.is_empty() && pack_config.preprocessor.enabled {
            crate::preprocessing::pipeline::preprocess_pack(
                entries,
                registry,
                pack,
                ctx.fs.as_ref(),
                ctx.datastore.as_ref(),
            )?
        } else {
            crate::preprocessing::pipeline::PreprocessResult::passthrough(entries)
        }
    } else {
        crate::preprocessing::pipeline::PreprocessResult::passthrough(entries)
    };

    // Phase 3: Merge and match rules
    let all_entries = preprocess_result.merged_entries();
    let mut matches = scanner.match_entries(&all_entries, &rules, &pack.name);
    debug!(pack = %pack.name, files = matches.len(), "matched rules");

    // Propagate preprocessor source info into matches
    for m in &mut matches {
        if let Some(source) = preprocess_result.source_map.get(&m.absolute_path) {
            m.preprocessor_source = Some(source.clone());
        }
    }

    // Phase 4: Group by handler
    let groups = rules::group_by_handler(&matches);

    // Build handler registry (drives the phase-based execution order).
    let registry = handlers::create_registry(ctx.fs.as_ref());
    let order = rules::handler_execution_order(&groups, &registry);
    debug!(pack = %pack.name, handlers = ?order, "handler execution order");

    // Generate intents from each handler
    let mut all_intents = Vec::new();
    let mut all_warnings = Vec::new();
    for handler_name in &order {
        let handler = match registry.get(handler_name.as_str()) {
            Some(h) => h,
            None => {
                debug!(pack = %pack.name, handler = %handler_name, "skipping unknown handler");
                continue;
            }
        };

        // Skip code execution handlers if --no-provision
        if ctx.no_provision && handler.category() == handlers::HandlerCategory::CodeExecution {
            debug!(pack = %pack.name, handler = %handler_name, "skipping code-execution handler (--no-provision)");
            continue;
        }

        if let Some(handler_matches) = groups.get(handler_name) {
            let intents = handler.to_intents(
                handler_matches,
                &pack.config,
                ctx.paths.as_ref(),
                ctx.fs.as_ref(),
            )?;
            debug!(
                pack = %pack.name,
                handler = %handler_name,
                intents = intents.len(),
                "generated intents"
            );
            all_intents.extend(intents);

            let warnings =
                handler.warnings_for_matches(handler_matches, &pack.config, ctx.paths.as_ref());
            for w in &warnings {
                tracing::warn!(pack = %pack.name, handler = %handler_name, "{w}");
            }
            all_warnings.extend(warnings);
        }
    }

    info!(
        pack = %pack.name,
        intents = all_intents.len(),
        warnings = all_warnings.len(),
        "collected intents"
    );
    Ok(PackPlan {
        intents: all_intents,
        warnings: all_warnings,
    })
}

/// Execute a pre-collected set of intents.
///
/// This is the second half of the two-phase execution model.
/// Call [`collect_pack_intents`] first, run conflict detection,
/// then call this to actually perform the mutations.
pub fn execute_intents(
    intents: Vec<crate::operations::HandlerIntent>,
    ctx: &ExecutionContext,
) -> Result<Vec<OperationResult>> {
    let count = intents.len();
    info!(
        intents = count,
        dry_run = ctx.dry_run,
        force = ctx.force,
        "executing intents"
    );
    let auto_chmod = ctx.config_manager.root_config()?.path.auto_chmod_exec;
    let executor = Executor::new(
        ctx.datastore.as_ref(),
        ctx.fs.as_ref(),
        ctx.paths.as_ref(),
        ctx.dry_run,
        ctx.force,
        ctx.provision_rerun,
        auto_chmod,
    );
    executor.execute(intents)
}

/// Run the standard handler pipeline for a pack: scan → match rules →
/// group by handler → to_intents → execute.
///
/// Convenience wrapper that combines [`collect_pack_intents`] and
/// [`execute_intents`]. Does **not** perform cross-pack conflict
/// detection — use the two-phase API for that.
pub fn run_handler_pipeline(pack: &Pack, ctx: &ExecutionContext) -> Result<Vec<OperationResult>> {
    let intents = collect_pack_intents(pack, ctx)?;
    execute_intents(intents, ctx)
}

/// Resolve a user-typed pack identifier to its on-disk directory
/// name. Tries display name first, falls back to the raw on-disk
/// name — so `dodot adopt nvim` and `dodot adopt 010-nvim` both find
/// the same pack on disk.
///
/// Use this in commands that take a single pack-name argument and
/// then need the directory path for filesystem or datastore work
/// (`adopt`, `addignore`, `fill`). Errors with [`DodotError::PackNotFound`]
/// when no match exists, and resolves through both active and ignored
/// packs (the caller decides whether being ignored is fatal).
pub fn resolve_pack_dir_name(input: &str, ctx: &ExecutionContext) -> crate::Result<String> {
    let root_config = ctx.config_manager.root_config()?;
    let scanned = packs::scan_packs(
        ctx.fs.as_ref(),
        ctx.paths.dotfiles_root(),
        &root_config.pack.ignore,
    )?;
    if let Some(p) = scanned
        .packs
        .iter()
        .find(|p| p.display_name == *input || p.name == *input)
    {
        return Ok(p.name.clone());
    }
    if let Some(dir) = scanned
        .ignored
        .iter()
        .find(|d| d.as_str() == input || packs::display_name_for(d) == input)
    {
        return Ok(dir.clone());
    }
    Err(crate::DodotError::PackNotFound { name: input.into() })
}

/// Validate that requested pack names exist. Returns error for nonexistent
/// packs and collects warnings for ignored packs.
///
/// Names resolve against the pack's *display name* (e.g. `nvim` for an
/// on-disk `010-nvim`) first, then fall back to the raw on-disk name —
/// so `dodot up nvim` and `dodot up 010-nvim` both find the same pack.
/// The display name is the recommended form.
pub fn validate_pack_names(names: &[String], ctx: &ExecutionContext) -> crate::Result<Vec<String>> {
    let root_config = ctx.config_manager.root_config()?;
    let scanned = packs::scan_packs(
        ctx.fs.as_ref(),
        ctx.paths.dotfiles_root(),
        &root_config.pack.ignore,
    )?;

    let mut warnings = Vec::new();
    for input in names {
        if scanned
            .packs
            .iter()
            .any(|p| p.display_name == *input || p.name == *input)
        {
            continue;
        }
        if scanned
            .ignored
            .iter()
            .any(|dir| dir == input || packs::display_name_for(dir) == input)
        {
            warnings.push(format!("warning: pack '{}' is ignored, skipping", input));
            continue;
        }
        return Err(crate::DodotError::PackNotFound {
            name: input.clone(),
        });
    }
    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datastore::{CommandOutput, CommandRunner, FilesystemDataStore};
    use crate::testing::TempEnvironment;
    use std::sync::Mutex;

    struct MockCommandRunner {
        calls: Mutex<Vec<String>>,
    }

    impl MockCommandRunner {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl CommandRunner for MockCommandRunner {
        fn run(&self, executable: &str, arguments: &[String]) -> Result<CommandOutput> {
            let cmd_str = format!("{} {}", executable, arguments.join(" "));
            self.calls.lock().unwrap().push(cmd_str.trim().to_string());
            Ok(CommandOutput {
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    fn make_context(env: &TempEnvironment) -> ExecutionContext {
        let runner = Arc::new(MockCommandRunner::new());
        let datastore = Arc::new(FilesystemDataStore::new(
            env.fs.clone(),
            env.paths.clone(),
            runner,
        ));
        let config_manager = Arc::new(ConfigManager::new(&env.dotfiles_root).unwrap());

        ExecutionContext {
            fs: env.fs.clone() as Arc<dyn Fs>,
            datastore,
            paths: env.paths.clone() as Arc<dyn Pather>,
            config_manager,
            syntax_checker: Arc::new(crate::shell::NoopSyntaxChecker),
            dry_run: false,
            no_provision: true, // skip install/homebrew in tests
            provision_rerun: false,
            force: false,
            view_mode: crate::commands::ViewMode::Full,
            group_mode: crate::commands::GroupMode::Name,
            verbose: false,
        }
    }

    /// Simple command that runs the handler pipeline.
    struct TestUpCommand;

    impl Command for TestUpCommand {
        fn name(&self) -> &str {
            "test-up"
        }

        fn execute_for_pack(&self, pack: &Pack, ctx: &ExecutionContext) -> Result<PackResult> {
            let operations = run_handler_pipeline(pack, ctx)?;
            let success = operations.iter().all(|r| r.success);
            // Mirror what the real up/down commands do: the user-facing
            // pack identifier carried in `PackResult.pack_name` is the
            // pack's display name, not its raw on-disk directory.
            Ok(PackResult {
                pack_name: pack.display_name.clone(),
                success,
                operations,
                error: None,
            })
        }
    }

    #[test]
    fn execute_discovers_and_processes_packs() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .done()
            .pack("git")
            .file("gitconfig", "[user]\n  name = test")
            .done()
            .build();

        let ctx = make_context(&env);
        let result = execute(&TestUpCommand, None, &ctx).unwrap();

        assert_eq!(result.total_packs, 2);
        assert_eq!(result.successful_packs, 2);
        assert_eq!(result.failed_packs, 0);
        assert!(result.is_success());

        // Both packs should have operations
        for pr in &result.pack_results {
            assert!(pr.success, "pack {} failed", pr.pack_name);
            assert!(
                !pr.operations.is_empty(),
                "pack {} has no operations",
                pr.pack_name
            );
        }
    }

    #[test]
    fn execute_filters_by_pack_name() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .pack("git")
            .file("gitconfig", "x")
            .done()
            .pack("zsh")
            .file("zshrc", "x")
            .done()
            .build();

        let ctx = make_context(&env);
        let filter = vec!["vim".into(), "zsh".into()];
        let result = execute(&TestUpCommand, Some(&filter), &ctx).unwrap();

        assert_eq!(result.total_packs, 2);
        let names: Vec<&str> = result
            .pack_results
            .iter()
            .map(|r| r.pack_name.as_str())
            .collect();
        assert!(names.contains(&"vim"));
        assert!(names.contains(&"zsh"));
        assert!(!names.contains(&"git"));
    }

    #[test]
    fn execute_filter_resolves_display_name_to_prefixed_pack() {
        let env = TempEnvironment::builder()
            .pack("010-brew")
            .file("Brewfile", "x")
            .done()
            .pack("nvim")
            .file("init.lua", "x")
            .done()
            .build();

        let ctx = make_context(&env);
        let filter = vec!["brew".into()];
        let result = execute(&TestUpCommand, Some(&filter), &ctx).unwrap();

        // Filter `brew` resolves to the on-disk `010-brew` pack via display name.
        assert_eq!(result.total_packs, 1);
        assert_eq!(result.pack_results[0].pack_name, "brew");
    }

    #[test]
    fn execute_filter_accepts_raw_directory_name_as_fallback() {
        let env = TempEnvironment::builder()
            .pack("010-brew")
            .file("Brewfile", "x")
            .done()
            .build();

        let ctx = make_context(&env);
        let filter = vec!["010-brew".into()];
        let result = execute(&TestUpCommand, Some(&filter), &ctx).unwrap();

        // The raw directory name is a valid fallback for muscle memory or scripts.
        assert_eq!(result.total_packs, 1);
        // PackResult.pack_name surfaces the display-name form regardless of how
        // the user typed the filter — that's what every render path expects.
        assert_eq!(result.pack_results[0].pack_name, "brew");
    }

    #[test]
    fn resolve_pack_dir_name_finds_pack_by_display_name() {
        let env = TempEnvironment::builder()
            .pack("010-nvim")
            .file("init.lua", "x")
            .done()
            .build();

        let ctx = make_context(&env);
        let resolved = resolve_pack_dir_name("nvim", &ctx).unwrap();
        assert_eq!(resolved, "010-nvim");
    }

    #[test]
    fn resolve_pack_dir_name_finds_pack_by_raw_directory_name() {
        let env = TempEnvironment::builder()
            .pack("010-nvim")
            .file("init.lua", "x")
            .done()
            .build();

        let ctx = make_context(&env);
        let resolved = resolve_pack_dir_name("010-nvim", &ctx).unwrap();
        assert_eq!(resolved, "010-nvim");
    }

    #[test]
    fn resolve_pack_dir_name_errors_on_unknown_pack() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .build();

        let ctx = make_context(&env);
        let err = resolve_pack_dir_name("nope", &ctx).unwrap_err();
        assert!(matches!(
            err,
            crate::DodotError::PackNotFound { ref name } if name == "nope"
        ));
    }

    #[test]
    fn execute_skips_dodotignored_packs() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .pack("disabled")
            .file("stuff", "x")
            .ignored()
            .done()
            .build();

        let ctx = make_context(&env);
        let result = execute(&TestUpCommand, None, &ctx).unwrap();

        assert_eq!(result.total_packs, 1);
        assert_eq!(result.pack_results[0].pack_name, "vim");
    }

    #[test]
    fn run_handler_pipeline_creates_symlinks() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .file("gvimrc", "set guifont=Mono")
            .done()
            .build();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "vim".into(),
            env.dotfiles_root.join("vim"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("vim"))
                .unwrap()
                .to_handler_config(),
        );

        let results = run_handler_pipeline(&pack, &ctx).unwrap();
        assert!(results.iter().all(|r| r.success));

        // Verify symlinks were created
        let vim_symlink_dir = ctx.paths.handler_data_dir("vim", "symlink");
        assert!(ctx.fs.exists(&vim_symlink_dir));
    }

    #[test]
    fn dry_run_produces_results_without_side_effects() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .build();

        let runner = Arc::new(MockCommandRunner::new());
        let datastore = Arc::new(FilesystemDataStore::new(
            env.fs.clone(),
            env.paths.clone(),
            runner,
        ));
        let config_manager = Arc::new(ConfigManager::new(&env.dotfiles_root).unwrap());

        let ctx = ExecutionContext {
            fs: env.fs.clone() as Arc<dyn Fs>,
            datastore,
            paths: env.paths.clone() as Arc<dyn Pather>,
            config_manager,
            syntax_checker: Arc::new(crate::shell::NoopSyntaxChecker),
            dry_run: true,
            no_provision: true,
            provision_rerun: false,
            force: false,
            view_mode: crate::commands::ViewMode::Full,
            group_mode: crate::commands::GroupMode::Name,
            verbose: false,
        };

        let result = execute(&TestUpCommand, None, &ctx).unwrap();
        assert!(result.is_success());
        assert!(!result.pack_results[0].operations.is_empty());

        // No filesystem changes should have been made
        let vim_symlink_dir = ctx.paths.handler_data_dir("vim", "symlink");
        assert!(!ctx.fs.exists(&vim_symlink_dir));
    }

    #[test]
    fn no_provision_skips_install_handler() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .file("install.sh", "#!/bin/sh\necho setup")
            .done()
            .build();

        let ctx = make_context(&env); // no_provision = true

        let pack = Pack::new(
            "vim".into(),
            env.dotfiles_root.join("vim"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("vim"))
                .unwrap()
                .to_handler_config(),
        );

        let results = run_handler_pipeline(&pack, &ctx).unwrap();

        // Should have symlink operations but no RunCommand
        for r in &results {
            assert!(
                !matches!(r.operation, crate::operations::Operation::RunCommand { .. }),
                "RunCommand should be skipped with no_provision"
            );
        }
    }

    // ── Preprocessing integration tests ────────────────────────

    #[test]
    fn preprocessing_identity_file_deploys_via_symlink_handler() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.identity", "host = localhost")
            .done()
            .build();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        let mut registry = crate::preprocessing::PreprocessorRegistry::new();
        registry.register(Box::new(
            crate::preprocessing::identity::IdentityPreprocessor::new(),
        ));

        let intents =
            collect_pack_intents_with_preprocessors(&pack, &ctx, Some(&registry)).unwrap();

        // Should produce a Link intent for "config.toml" (not "config.toml.identity")
        assert_eq!(intents.len(), 1, "intents: {intents:?}");

        match &intents[0] {
            crate::operations::HandlerIntent::Link {
                pack: p,
                handler,
                source,
                user_path,
            } => {
                assert_eq!(p, "app");
                assert_eq!(handler, "symlink");
                // The source should be in the datastore (preprocessed handler dir)
                assert!(
                    source.to_string_lossy().contains("preprocessed"),
                    "source should be in preprocessed dir: {}",
                    source.display()
                );
                // The user_path should NOT contain .identity extension
                let user_str = user_path.to_string_lossy();
                assert!(
                    !user_str.contains("identity"),
                    "user_path should not have .identity: {user_str}"
                );
            }
            other => panic!("expected Link intent, got: {other:?}"),
        }
    }

    #[test]
    fn preprocessing_mixed_pack_deploys_both() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.identity", "preprocessed content")
            .file("readme.txt", "regular content")
            .done()
            .build();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        let mut registry = crate::preprocessing::PreprocessorRegistry::new();
        registry.register(Box::new(
            crate::preprocessing::identity::IdentityPreprocessor::new(),
        ));

        let intents =
            collect_pack_intents_with_preprocessors(&pack, &ctx, Some(&registry)).unwrap();

        // Should have 2 Link intents: one for config.toml (preprocessed), one for readme.txt (regular)
        assert_eq!(intents.len(), 2, "intents: {intents:?}");

        let intent_sources: Vec<String> = intents
            .iter()
            .filter_map(|i| match i {
                crate::operations::HandlerIntent::Link { source, .. } => {
                    Some(source.to_string_lossy().to_string())
                }
                _ => None,
            })
            .collect();

        // One should be in the preprocessed dir, the other in the pack dir
        let has_preprocessed = intent_sources.iter().any(|s| s.contains("preprocessed"));
        let has_regular = intent_sources
            .iter()
            .any(|s| s.contains("dotfiles/app/readme.txt"));
        assert!(
            has_preprocessed,
            "should have a preprocessed source: {intent_sources:?}"
        );
        assert!(
            has_regular,
            "should have a regular source: {intent_sources:?}"
        );
    }

    #[test]
    fn preprocessing_collision_detected() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.identity", "preprocessed")
            .file("config.toml", "regular")
            .done()
            .build();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        let mut registry = crate::preprocessing::PreprocessorRegistry::new();
        registry.register(Box::new(
            crate::preprocessing::identity::IdentityPreprocessor::new(),
        ));

        let err =
            collect_pack_intents_with_preprocessors(&pack, &ctx, Some(&registry)).unwrap_err();
        assert!(
            matches!(err, crate::DodotError::PreprocessorCollision { .. }),
            "expected PreprocessorCollision, got: {err}"
        );
    }

    #[test]
    fn preprocessing_disabled_via_config_treats_files_as_regular() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.identity", "content")
            .done()
            .build();

        // Write config disabling preprocessing
        env.fs
            .write_file(
                &env.dotfiles_root.join(".dodot.toml"),
                b"[preprocessor]\nenabled = false\n",
            )
            .unwrap();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        let mut registry = crate::preprocessing::PreprocessorRegistry::new();
        registry.register(Box::new(
            crate::preprocessing::identity::IdentityPreprocessor::new(),
        ));

        let intents =
            collect_pack_intents_with_preprocessors(&pack, &ctx, Some(&registry)).unwrap();

        // With preprocessing disabled, the .identity file is treated as regular
        // and deployed as-is with the .identity extension preserved
        assert_eq!(intents.len(), 1);
        match &intents[0] {
            crate::operations::HandlerIntent::Link { user_path, .. } => {
                let user_str = user_path.to_string_lossy();
                assert!(
                    user_str.contains("identity"),
                    "with preprocessing disabled, file should keep .identity extension: {user_str}"
                );
            }
            other => panic!("expected Link intent, got: {other:?}"),
        }
    }

    #[test]
    fn preprocessing_no_registry_works_like_before() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .done()
            .build();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "vim".into(),
            env.dotfiles_root.join("vim"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("vim"))
                .unwrap()
                .to_handler_config(),
        );

        // No preprocessor registry at all
        let intents = collect_pack_intents_with_preprocessors(&pack, &ctx, None).unwrap();

        assert_eq!(intents.len(), 1);
        match &intents[0] {
            crate::operations::HandlerIntent::Link { source, .. } => {
                assert!(
                    source.to_string_lossy().contains("vim/vimrc"),
                    "source should be the pack file: {}",
                    source.display()
                );
            }
            other => panic!("expected Link intent, got: {other:?}"),
        }
    }

    #[test]
    fn preprocessing_end_to_end_deploy_and_verify_content() {
        // Full pipeline: preprocess → collect intents → execute → verify user file
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.identity", "host = localhost\nport = 5432")
            .done()
            .build();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        let mut registry = crate::preprocessing::PreprocessorRegistry::new();
        registry.register(Box::new(
            crate::preprocessing::identity::IdentityPreprocessor::new(),
        ));

        let intents =
            collect_pack_intents_with_preprocessors(&pack, &ctx, Some(&registry)).unwrap();

        // Extract the user_path from the intent so we know where to check
        let user_path = match &intents[0] {
            crate::operations::HandlerIntent::Link { user_path, .. } => user_path.clone(),
            other => panic!("expected Link intent, got: {other:?}"),
        };

        let results = execute_intents(intents, &ctx).unwrap();

        assert!(
            results.iter().all(|r| r.success),
            "all operations should succeed: {results:?}"
        );

        // The user file should exist and have the preprocessed content
        assert!(
            ctx.fs.exists(&user_path),
            "user file should exist at: {}",
            user_path.display()
        );
        assert!(
            ctx.fs.is_symlink(&user_path),
            "user file should be a symlink"
        );

        // Content should be the preprocessed (identity = same) content
        let content = ctx.fs.read_to_string(&user_path).unwrap();
        assert_eq!(content, "host = localhost\nport = 5432");
    }

    #[test]
    fn preprocessing_error_propagates_through_pipeline() {
        // Expansion errors should propagate through the pipeline.
        // We test this at the pipeline level (not orchestration) since
        // the scanner won't see a file that doesn't exist. The pipeline
        // tests in pipeline.rs cover this case directly. Here we verify
        // that a valid preprocessor file that triggers an error during
        // a lower-level operation still propagates correctly.
        //
        // Use the unarchive preprocessor with a corrupted archive.
        let env = TempEnvironment::builder()
            .pack("tools")
            .file("bad.tar.gz", "this is not valid gzip data at all")
            .done()
            .build();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "tools".into(),
            env.dotfiles_root.join("tools"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("tools"))
                .unwrap()
                .to_handler_config(),
        );

        let mut registry = crate::preprocessing::PreprocessorRegistry::new();
        registry.register(Box::new(
            crate::preprocessing::unarchive::UnarchivePreprocessor::new(),
        ));

        let err =
            collect_pack_intents_with_preprocessors(&pack, &ctx, Some(&registry)).unwrap_err();
        assert!(
            matches!(err, crate::DodotError::PreprocessorError { .. }),
            "expected PreprocessorError, got: {err}"
        );
    }

    #[test]
    fn preprocessing_multiple_types_in_registry() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.identity", "identity content")
            .done()
            .build();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        // Register both identity and unarchive preprocessors
        let mut registry = crate::preprocessing::PreprocessorRegistry::new();
        registry.register(Box::new(
            crate::preprocessing::identity::IdentityPreprocessor::new(),
        ));
        registry.register(Box::new(
            crate::preprocessing::unarchive::UnarchivePreprocessor::new(),
        ));

        let intents =
            collect_pack_intents_with_preprocessors(&pack, &ctx, Some(&registry)).unwrap();

        // The .identity file should still be handled by the identity preprocessor
        assert_eq!(intents.len(), 1);
        match &intents[0] {
            crate::operations::HandlerIntent::Link { source, .. } => {
                assert!(source.to_string_lossy().contains("preprocessed"));
            }
            other => panic!("expected Link intent, got: {other:?}"),
        }
    }

    #[test]
    fn collect_pack_intents_uses_default_registry() {
        // The normal `collect_pack_intents` entrypoint should wire the
        // default preprocessor registry (not pass `None`). We verify
        // this by putting a `.tar.gz` file in a pack — the default
        // registry contains `UnarchivePreprocessor`, so the archive
        // should be expanded rather than passed through.
        use flate2::write::GzEncoder;
        use flate2::Compression;

        let env = TempEnvironment::builder()
            .pack("tools")
            .file("placeholder", "")
            .done()
            .build();

        // Create a simple tar.gz at the pack's bin/ dir so it maps to
        // the path handler after expansion.
        let archive_path = env.dotfiles_root.join("tools/payload.tar.gz");
        let file = std::fs::File::create(&archive_path).unwrap();
        let enc = GzEncoder::new(file, Compression::default());
        let mut builder = tar::Builder::new(enc);
        let content = b"#!/bin/sh\necho hi";
        let mut header = tar::Header::new_gnu();
        header.set_path("mytool").unwrap();
        header.set_size(content.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append(&header, &content[..]).unwrap();
        let enc = builder.into_inner().unwrap();
        enc.finish().unwrap();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "tools".into(),
            env.dotfiles_root.join("tools"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("tools"))
                .unwrap()
                .to_handler_config(),
        );

        // Call the real production entrypoint — no explicit registry.
        let intents = collect_pack_intents(&pack, &ctx).unwrap();

        // Should include a Link intent for the expanded `mytool` file,
        // with its source in the preprocessed datastore directory.
        let has_expanded_source = intents.iter().any(|i| match i {
            crate::operations::HandlerIntent::Link { source, .. } => {
                source.to_string_lossy().contains("preprocessed")
                    && source.to_string_lossy().contains("mytool")
            }
            _ => false,
        });
        assert!(
            has_expanded_source,
            "production collect_pack_intents should expand .tar.gz via the default registry. Intents: {intents:?}"
        );
    }

    // ── Template preprocessor integration tests ─────────────────

    #[test]
    fn template_deploys_rendered_content_via_symlink_handler() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file(
                "config.toml.tmpl",
                "name = \"{{ name }}\"\nos = \"{{ dodot.os }}\"",
            )
            .config("[preprocessor.template.vars]\nname = \"Alice\"\n")
            .done()
            .build();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        let intents = collect_pack_intents(&pack, &ctx).unwrap();
        let user_path = match &intents[0] {
            crate::operations::HandlerIntent::Link { user_path, .. } => user_path.clone(),
            other => panic!("expected Link intent, got: {other:?}"),
        };

        let results = execute_intents(intents, &ctx).unwrap();
        assert!(
            results.iter().all(|r| r.success),
            "expected success: {results:?}"
        );

        let content = ctx.fs.read_to_string(&user_path).unwrap();
        let expected_os = std::env::consts::OS;
        assert_eq!(content, format!("name = \"Alice\"\nos = \"{expected_os}\""));
    }

    #[test]
    fn template_with_shell_handler_sources_rendered_content() {
        // aliases.sh.tmpl should match the shell handler after stripping.
        let env = TempEnvironment::builder()
            .pack("tools")
            .file("aliases.sh.tmpl", "alias hello='echo {{ greeting }}'")
            .config("[preprocessor.template.vars]\ngreeting = \"world\"\n")
            .done()
            .build();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "tools".into(),
            env.dotfiles_root.join("tools"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("tools"))
                .unwrap()
                .to_handler_config(),
        );

        let intents = collect_pack_intents(&pack, &ctx).unwrap();
        assert_eq!(intents.len(), 1);

        match &intents[0] {
            crate::operations::HandlerIntent::Stage {
                handler, source, ..
            } => {
                assert_eq!(handler, "shell", "shell handler should own this");
                let content = ctx.fs.read_to_string(source).unwrap();
                assert_eq!(content, "alias hello='echo world'");
            }
            other => panic!("expected Stage intent, got: {other:?}"),
        }
    }

    #[test]
    fn template_respects_per_pack_var_overrides() {
        // Root config defines name=Alice; pack overrides to name=Bob.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("greeting.tmpl", "hello {{ name }}")
            .config("[preprocessor.template.vars]\nname = \"Bob\"\n")
            .done()
            .build();

        // Root config: name = Alice
        env.fs
            .write_file(
                &env.dotfiles_root.join(".dodot.toml"),
                b"[preprocessor.template.vars]\nname = \"Alice\"\n",
            )
            .unwrap();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        let intents = collect_pack_intents(&pack, &ctx).unwrap();
        match &intents[0] {
            crate::operations::HandlerIntent::Link { source, .. } => {
                let content = ctx.fs.read_to_string(source).unwrap();
                assert_eq!(content, "hello Bob", "pack-level override should win");
            }
            other => panic!("expected Link intent, got: {other:?}"),
        }
    }

    #[test]
    fn template_disabled_via_config_treats_files_as_regular() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.tmpl", "name = \"{{ name }}\"")
            .done()
            .build();

        env.fs
            .write_file(
                &env.dotfiles_root.join(".dodot.toml"),
                b"[preprocessor]\nenabled = false\n",
            )
            .unwrap();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        let intents = collect_pack_intents(&pack, &ctx).unwrap();
        // With preprocessing disabled, the .tmpl file is treated as a
        // regular file and deployed verbatim (retaining the .tmpl extension).
        assert_eq!(intents.len(), 1);
        match &intents[0] {
            crate::operations::HandlerIntent::Link {
                source, user_path, ..
            } => {
                assert!(
                    source.to_string_lossy().ends_with("config.toml.tmpl"),
                    "source: {}",
                    source.display()
                );
                assert!(
                    user_path.to_string_lossy().contains(".tmpl"),
                    "user_path should keep .tmpl extension: {}",
                    user_path.display()
                );
            }
            other => panic!("expected Link intent, got: {other:?}"),
        }
    }

    #[test]
    fn template_render_error_surfaces_with_source_path() {
        let env = TempEnvironment::builder()
            .pack("app")
            .file("bad.tmpl", "value = \"{{ undefined_var }}\"")
            .done()
            .build();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        let err = collect_pack_intents(&pack, &ctx).unwrap_err();
        match err {
            crate::DodotError::TemplateRender { source_file, .. } => {
                assert!(
                    source_file.ends_with("bad.tmpl"),
                    "source_file: {}",
                    source_file.display()
                );
            }
            other => panic!("expected TemplateRender, got: {other:?}"),
        }
    }

    #[test]
    fn template_reserved_var_fails_fast() {
        // A user tries to define `dodot` as a variable — construction
        // of the preprocessor should fail before any rendering happens.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("file.txt", "x")
            .done()
            .build();

        env.fs
            .write_file(
                &env.dotfiles_root.join(".dodot.toml"),
                b"[preprocessor.template.vars]\ndodot = \"pwn\"\n",
            )
            .unwrap();

        let ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        let err = collect_pack_intents(&pack, &ctx).unwrap_err();
        assert!(
            matches!(err, crate::DodotError::TemplateReservedVar { ref name } if name == "dodot"),
            "got: {err}"
        );
    }

    #[test]
    fn template_with_install_handler_sentinel_reflects_rendered_content() {
        // install.sh.tmpl should render, and the sentinel should be
        // based on the rendered content (so vars changes re-run the
        // script). Verify by checking the sentinel name includes the
        // hash of the rendered content, not the template source.
        let env = TempEnvironment::builder()
            .pack("setup")
            .file(
                "install.sh.tmpl",
                "#!/bin/sh\necho \"installing on {{ dodot.os }}\"",
            )
            .done()
            .build();

        let mut ctx = make_context(&env);
        ctx.no_provision = false; // actually run install this time

        let pack = Pack::new(
            "setup".into(),
            env.dotfiles_root.join("setup"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("setup"))
                .unwrap()
                .to_handler_config(),
        );

        let intents = collect_pack_intents(&pack, &ctx).unwrap();
        let (sentinel, rendered_path) = match &intents[0] {
            crate::operations::HandlerIntent::Run {
                sentinel,
                arguments,
                ..
            } => (
                sentinel.clone(),
                std::path::PathBuf::from(arguments.last().unwrap()),
            ),
            other => panic!("expected Run intent, got: {other:?}"),
        };

        // Sentinel is "install.sh-{checksum}" where checksum is the
        // SHA-256 of the *rendered* script in the datastore.
        assert!(sentinel.starts_with("install.sh-"));

        // Verify the rendered file contains the OS substitution
        let content = ctx.fs.read_to_string(&rendered_path).unwrap();
        assert!(
            content.contains(std::env::consts::OS),
            "rendered content should have OS substituted: {content}"
        );
    }
}

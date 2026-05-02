//! Orchestration pipeline — the single entry point for executing
//! commands across packs.
//!
//! `execute()` owns the outer loop: discover packs → load per-pack
//! config → execute command → aggregate results.

use std::path::PathBuf;
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
///
/// `write_baselines` controls whether the preprocessing pipeline
/// persists baseline-cache records. Pass `true` from `dodot up` (the
/// only mutating caller); pass `false` from read-only callers like
/// `dodot status` so passive runs don't update the cache that
/// captured the state of the last `up`. The §6.4 divergence guard
/// fires regardless — the `write_baselines` flag only controls the
/// optional baseline-write side effect, not the read path.
///
/// `force` controls whether the §6.4 divergence guard is bypassed
/// (overwriting deployed files that have diverged from the baseline).
/// `dodot up` propagates `ctx.force` here. **Read-only callers like
/// `status` must pass `false` regardless of `ctx.force`** — otherwise
/// a `dodot up --force` run that falls back to `status::status()` via
/// `up_or_status_for_conflict` would clobber preserved files during
/// what is nominally a display pass.
pub fn plan_pack(
    pack: &Pack,
    ctx: &ExecutionContext,
    write_baselines: bool,
    force: bool,
) -> Result<PackPlan> {
    let pack_config = ctx.config_manager.config_for_pack(&pack.path)?;
    let registry = crate::preprocessing::default_registry(
        &pack_config.preprocessor.template,
        ctx.paths.as_ref(),
    )?;
    plan_pack_inner(
        pack,
        ctx,
        &pack_config,
        Some(&registry),
        write_baselines,
        force,
    )
}

/// Shared implementation that takes a pre-loaded pack config. Both
/// entrypoints load the config once and pass it through so we don't
/// re-merge config for every pack (the ConfigManager caches by path,
/// but passing the config explicitly makes the data flow obvious).
///
/// Defaults `write_baselines = false` and `force = false` because
/// every caller of this helper (`adopt`, `run_handler_pipeline`)
/// is read-only conflict detection or test scaffolding. The actual
/// `dodot up` deploy flow uses `plan_pack` directly with its own
/// flags. Writing baselines from a read-only inspection command
/// would change later `transform check` / divergence outcomes
/// without any deploy having happened.
fn collect_pack_intents_inner(
    pack: &Pack,
    ctx: &ExecutionContext,
    pack_config: &crate::config::DodotConfig,
    preprocessors: Option<&crate::preprocessing::PreprocessorRegistry>,
) -> Result<Vec<crate::operations::HandlerIntent>> {
    plan_pack_inner(
        pack,
        ctx,
        pack_config,
        preprocessors,
        // Both `false`: this helper feeds read-only callers
        // (`adopt::check_deploy_conflicts`, `run_handler_pipeline`
        // in tests). The actual `dodot up` flow uses `plan_pack`
        // directly (which threads its own `write_baselines = true`
        // and `ctx.force`), so the deploy path is unaffected.
        //
        // `write_baselines = false` matters because baselines
        // represent "the state of the last successful `dodot up`."
        // Writing them from an inspection-only command would
        // change later `transform check` / divergence outcomes
        // without any deploy having happened.
        //
        // `force = false` matters because propagating `ctx.force`
        // would let `dodot adopt --force` bypass the §6.4
        // divergence guard during inspection-only conflict
        // scanning, overwriting user-edited deployed files in
        // *other* packs before adopt has even started.
        /* write_baselines */
        false,
        /* force */ false,
    )
    .map(|p| p.intents)
}

/// Same scan/preprocess/match/group/intents pipeline as
/// [`collect_pack_intents_inner`], but additionally collects
/// per-handler `warnings_for_matches` output.
#[allow(clippy::too_many_arguments)] // pipeline core: every parameter is load-bearing
fn plan_pack_inner(
    pack: &Pack,
    ctx: &ExecutionContext,
    pack_config: &crate::config::DodotConfig,
    preprocessors: Option<&crate::preprocessing::PreprocessorRegistry>,
    write_baselines: bool,
    force: bool,
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
                ctx.paths.as_ref(),
                write_baselines,
                force,
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

    // Set of deployed paths the §6.4 divergence guard preserved
    // (see `preprocess_pack`). When a tracked render is skipped, the
    // virtual entry's `absolute_path` still points at the deployed
    // file — but that file now contains the user's edit, not what
    // dodot would render. For `Symlink` / `Path` / `Shell` handlers
    // that's fine: the user-side link continues to resolve the same
    // bytes the user expects. For `CodeExecution` handlers
    // (`install`, `homebrew`) it is *not* fine: their sentinel and
    // execution input are derived from the file content, so a fresh
    // `up` would derive a sentinel from the user's edit and execute
    // the user's edit as a script. Drop those matches so provisioning
    // stays pinned to the previous successful render's outcome.
    use std::collections::HashSet;
    let skipped_deployed_paths: HashSet<PathBuf> = preprocess_result
        .skipped
        .iter()
        .map(|s| s.deployed_path.clone())
        .collect();

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
            // Filter out preserved-divergent-file matches when the
            // handler is a CodeExecution handler. Owned Vec because
            // `to_intents` and `warnings_for_matches` both want a
            // `&[RuleMatch]` slice.
            let filtered_for_code_exec: Vec<crate::rules::RuleMatch>;
            let matches_for_handler: &[crate::rules::RuleMatch] = if !skipped_deployed_paths
                .is_empty()
                && handler.category() == handlers::HandlerCategory::CodeExecution
            {
                filtered_for_code_exec = handler_matches
                    .iter()
                    .filter(|m| !skipped_deployed_paths.contains(&m.absolute_path))
                    .cloned()
                    .collect();
                if filtered_for_code_exec.len() < handler_matches.len() {
                    debug!(
                        pack = %pack.name,
                        handler = %handler_name,
                        dropped = handler_matches.len() - filtered_for_code_exec.len(),
                        "dropping preserved-divergent matches from code-execution handler"
                    );
                }
                &filtered_for_code_exec
            } else {
                handler_matches.as_slice()
            };

            let intents = handler.to_intents(
                matches_for_handler,
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
                handler.warnings_for_matches(matches_for_handler, &pack.config, ctx.paths.as_ref());
            for w in &warnings {
                tracing::warn!(pack = %pack.name, handler = %handler_name, "{w}");
            }
            all_warnings.extend(warnings);
        }
    }

    // Surface preserved-divergent-file warnings from the preprocessing
    // pipeline. These are the §6.4 "deployed file edited" cases: dodot
    // refused to overwrite the user's edit and held the previous render
    // in place. The user resolves them via `dodot transform check`
    // (auto-merge through the clean filter) or `dodot up --force`
    // (overwrite).
    //
    // We run this after intent collection so the warning can name the
    // user-visible path (`~/.config/...`) rather than the hidden
    // datastore path (`~/.local/share/dodot/.../preprocessed/...`).
    // Match by source: a `Link` intent for a preprocessed file carries
    // the datastore path as its `source`, which equals the
    // `SkippedRender.deployed_path`. Fall back to the datastore path
    // if no `Link` intent matched (e.g. preprocessed file consumed by
    // a non-link handler like `install`).
    for skipped in &preprocess_result.skipped {
        let user_visible_path = all_intents.iter().find_map(|intent| match intent {
            crate::operations::HandlerIntent::Link {
                source, user_path, ..
            } if source == &skipped.deployed_path => Some(user_path.as_path()),
            _ => None,
        });
        let display_path = match user_visible_path {
            Some(p) => display_path_relative_to_home(p, ctx),
            None => display_path_relative_to_home(&skipped.deployed_path, ctx),
        };
        let warning = if skipped.baseline_unreadable {
            // For unreadable baselines, `transform check` will fail
            // on the same corrupt entry — point the user at the
            // recovery paths that actually work: delete the
            // specific corrupt cache file or use `--force`. Use the
            // **actual** path the guard tried to load (which may
            // be the legacy basename layout for upgraders), not a
            // path reconstructed from `virtual_relative` — the
            // latter could point at a file that doesn't exist for
            // legacy nested entries.
            let cache_display = match &skipped.cache_path {
                Some(p) => display_path_relative_to_home(p, ctx),
                None => display_path_relative_to_home(&skipped.deployed_path, ctx),
            };
            format!(
                "preserved {} (baseline cache entry is unreadable). \
                 Delete the corrupt cache file ({}) or re-run with --force to overwrite.",
                display_path, cache_display,
            )
        } else {
            let detail = match skipped.state {
                crate::preprocessing::divergence::DivergenceState::OutputChanged => {
                    "deployed file was edited since the last `dodot up`"
                }
                crate::preprocessing::divergence::DivergenceState::BothChanged => {
                    "both the source template and the deployed file were edited since the last `dodot up`"
                }
                _ => "deployed file diverges from the cached baseline",
            };
            format!(
                "preserved {} ({}). Run `dodot transform check` to reconcile, or re-run with --force to overwrite.",
                display_path, detail,
            )
        };
        tracing::warn!(pack = %pack.name, file = %skipped.virtual_relative.display(), "{warning}");
        all_warnings.push(warning);
    }

    // Missing-target hints (M6 §8.2) — macOS only.
    //
    // For each Link intent that lands under `app_support_dir`, check
    // whether the immediate child folder exists on disk. If not, the
    // user is about to deploy GUI-app config to a directory the app
    // hasn't created yet — usually because the app isn't installed.
    // Surface a soft hint, optionally enriched with a matching brew
    // cask token. Resolver/intent state is unaffected.
    //
    // On Linux `app_support_dir` collapses to `xdg_config_home`, so
    // this check would fire for *every* `~/.config/<X>/` deploy —
    // not what we want. Gate on macOS strictly.
    if cfg!(target_os = "macos") {
        all_warnings.extend(missing_target_hints(&all_intents, ctx));
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

/// Probe each `Link` intent that targets `app_support_dir/<X>/...` and
/// emit a soft hint when the `<X>/` folder is missing on disk.
///
/// macOS-only — caller checks `cfg!(target_os = "macos")` first to
/// Render an absolute path with `$HOME` collapsed to `~` for human
/// display. Falls back to the absolute form when the path is outside
/// the home tree.
fn display_path_relative_to_home(path: &std::path::Path, ctx: &ExecutionContext) -> String {
    let home = ctx.paths.home_dir();
    match path.strip_prefix(home) {
        Ok(rel) => format!("~/{}", rel.display()),
        Err(_) => path.display().to_string(),
    }
}

/// avoid firing on Linux where every XDG-routed entry would otherwise
/// hit this branch.
fn missing_target_hints(
    intents: &[crate::operations::HandlerIntent],
    ctx: &ExecutionContext,
) -> Vec<String> {
    use std::collections::BTreeSet;
    let app_support = ctx.paths.app_support_dir();
    if app_support == ctx.paths.xdg_config_home() {
        // `app_uses_library = false` collapsed the app-support root
        // onto XDG; same Linux-style suppression applies.
        return Vec::new();
    }

    // Distinct `<X>` folders referenced by intents — one warning per
    // missing folder, regardless of how many files target it.
    let mut needed: BTreeSet<String> = BTreeSet::new();
    for intent in intents {
        if let crate::operations::HandlerIntent::Link { user_path, .. } = intent {
            if let Ok(rel) = user_path.strip_prefix(app_support) {
                if let Some(first) = rel.components().find_map(|c| match c {
                    std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                    _ => None,
                }) {
                    needed.insert(first);
                }
            }
        }
    }
    if needed.is_empty() {
        return Vec::new();
    }

    let mut missing: Vec<String> = Vec::new();
    for folder in &needed {
        let target = app_support.join(folder);
        if !ctx.fs.exists(&target) {
            missing.push(folder.clone());
        }
    }
    if missing.is_empty() {
        return Vec::new();
    }

    // Brew enrichment: try to associate each missing folder with an
    // *installed* cask token. Cache-only mode keeps the planner fast:
    // a stale or missing cache entry silently degrades to the
    // unenriched message rather than spawning a `brew info` subprocess
    // per installed cask. The on-demand `dodot probe app` subcommand
    // populates the cache; this hint just consumes it.
    let cache_dir = ctx.paths.probes_brew_cache_dir();
    let now = crate::probe::brew::now_secs_unix();
    let matches = crate::probe::brew::match_folders_to_installed_casks(
        &missing,
        ctx.command_runner.as_ref(),
        &cache_dir,
        now,
        ctx.fs.as_ref(),
        /*cache_only=*/ true,
    );

    missing
        .into_iter()
        .map(|folder| match matches.folder_to_token.get(&folder) {
            // The cask IS installed (we got the token from `brew list`)
            // but the folder is empty — usually the user pre-deployed
            // dotfiles before launching the app for the first time.
            Some(token) => format!(
                "cask `{token}` is installed but `{folder}/` is missing — \
                 entries will deploy, but the app may not have created its \
                 config directory yet (try launching it once)"
            ),
            None => format!(
                "target directory `{}/{folder}` doesn't exist yet — entries will \
                 deploy but no matching installed app appears to provide it",
                app_support.display()
            ),
        })
        .collect()
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
        let runner: Arc<dyn crate::datastore::CommandRunner> = Arc::new(MockCommandRunner::new());
        let datastore = Arc::new(FilesystemDataStore::new(
            env.fs.clone(),
            env.paths.clone(),
            runner.clone(),
        ));
        let config_manager = Arc::new(ConfigManager::new(&env.dotfiles_root).unwrap());

        ExecutionContext {
            fs: env.fs.clone() as Arc<dyn Fs>,
            datastore,
            paths: env.paths.clone() as Arc<dyn Pather>,
            config_manager,
            syntax_checker: Arc::new(crate::shell::NoopSyntaxChecker),
            command_runner: runner,
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

        let runner: Arc<dyn crate::datastore::CommandRunner> = Arc::new(MockCommandRunner::new());
        let datastore = Arc::new(FilesystemDataStore::new(
            env.fs.clone(),
            env.paths.clone(),
            runner.clone(),
        ));
        let config_manager = Arc::new(ConfigManager::new(&env.dotfiles_root).unwrap());

        let ctx = ExecutionContext {
            fs: env.fs.clone() as Arc<dyn Fs>,
            datastore,
            paths: env.paths.clone() as Arc<dyn Pather>,
            config_manager,
            syntax_checker: Arc::new(crate::shell::NoopSyntaxChecker),
            command_runner: runner,
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

    #[test]
    fn divergence_guard_drops_install_intent_for_preserved_template() {
        // Review feedback (PR #118): when the §6.4 guard preserves a
        // user-edited deployed file, the install/homebrew handlers
        // must NOT emit a Run intent against it. Otherwise the
        // sentinel would be derived from the user's edit and the
        // script would execute the user's edit on the next `up`.
        // The symlink intent stays — the user-side link continues to
        // resolve the preserved bytes — but provisioning is held at
        // the previous successful state.
        let env = TempEnvironment::builder()
            .pack("setup")
            .file(
                "install.sh.tmpl",
                "#!/bin/sh\necho \"installing on {{ dodot.os }}\"",
            )
            .done()
            .build();

        let mut ctx = make_context(&env);
        ctx.no_provision = false; // we want the install handler to consider matching
        let pack = Pack::new(
            "setup".into(),
            env.dotfiles_root.join("setup"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("setup"))
                .unwrap()
                .to_handler_config(),
        );

        // First run: render baseline + emit install Run intent.
        let first = plan_pack(
            &pack, &ctx, /* write_baselines */ true, /* force */ false,
        )
        .unwrap();
        let first_run_count = first
            .intents
            .iter()
            .filter(|i| matches!(i, crate::operations::HandlerIntent::Run { .. }))
            .count();
        assert!(
            first_run_count >= 1,
            "first deploy should emit at least one Run intent for install.sh"
        );

        // User edits the deployed install script directly.
        let deployed = env
            .paths
            .handler_data_dir("setup", "preprocessed")
            .join("install.sh");
        env.fs
            .write_file(&deployed, b"#!/bin/sh\necho INJECTED_BY_USER")
            .unwrap();

        // Re-run: guard preserves the file, and the install Run intent
        // for it must NOT be emitted (otherwise the user's edit would
        // execute as a script on the next `up`).
        let second = plan_pack(
            &pack, &ctx, /* write_baselines */ true, /* force */ false,
        )
        .unwrap();
        for intent in &second.intents {
            if let crate::operations::HandlerIntent::Run {
                arguments, handler, ..
            } = intent
            {
                assert!(
                    !arguments.iter().any(|a| a.contains("install.sh")),
                    "preserved install.sh.tmpl must not emit a Run intent (handler={handler}, arguments={arguments:?})"
                );
            }
        }
        // The preservation warning still fires.
        assert!(
            second
                .warnings
                .iter()
                .any(|w| w.contains("preserved") && w.contains("install.sh")),
            "expected preservation warning for install.sh, got: {:?}",
            second.warnings
        );
    }

    #[test]
    fn plan_pack_surfaces_divergence_warnings() {
        // End-to-end: a template-deployed file gets edited by the user,
        // then `plan_pack` runs again. The pipeline preserves the edit
        // and `PackPlan.warnings` carries a human-readable warning that
        // mentions the deployed path, the resolution paths, and `--force`.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.tmpl", "name = original")
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

        // First run: clean deploy, no warnings about preserved files.
        let first = plan_pack(&pack, &ctx, true, false).unwrap();
        assert!(
            first.warnings.iter().all(|w| !w.contains("preserved")),
            "first deploy must not produce a preservation warning: {:?}",
            first.warnings
        );

        // User edits the deployed file.
        let deployed = env
            .paths
            .handler_data_dir("app", "preprocessed")
            .join("config.toml");
        env.fs.write_file(&deployed, b"name = USER EDITED").unwrap();

        // Second run: warning surfaces, with the documented resolution
        // hints — `transform check` and `--force`.
        let second = plan_pack(&pack, &ctx, true, false).unwrap();
        let preserved: Vec<&String> = second
            .warnings
            .iter()
            .filter(|w| w.contains("preserved"))
            .collect();
        assert_eq!(
            preserved.len(),
            1,
            "expected one preservation warning, got: {:?}",
            second.warnings
        );
        let w = preserved[0];
        assert!(
            w.contains("config.toml"),
            "warning should name the file: {w}"
        );
        // The warning must report the user-visible deployed path
        // (`~/.config/app/config.toml`), not the hidden datastore path
        // (`~/.local/share/dodot/.../preprocessed/config.toml`). Users
        // edit through the symlink target, so that's the path they
        // recognise.
        assert!(
            w.contains("~/.config/app/config.toml"),
            "warning should show user-visible path, got: {w}"
        );
        assert!(
            !w.contains("preprocessed"),
            "warning should not leak the datastore path, got: {w}"
        );
        assert!(
            w.contains("transform check"),
            "warning should mention transform check: {w}"
        );
        assert!(w.contains("--force"), "warning should mention --force: {w}");
        // The user's edit must still be on disk.
        assert_eq!(
            env.fs.read_to_string(&deployed).unwrap(),
            "name = USER EDITED"
        );
    }

    #[test]
    fn plan_pack_force_overwrites_and_skips_warning() {
        // With ctx.force=true (the `--force` CLI flag), the guard is
        // bypassed: the deployed file gets re-rendered, no warning is
        // emitted. Documented escape hatch for env-var rotations and
        // similar out-of-band changes.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.tmpl", "name = original")
            .done()
            .build();

        let mut ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        // Prime baseline.
        let _ = plan_pack(&pack, &ctx, true, false).unwrap();
        let deployed = env
            .paths
            .handler_data_dir("app", "preprocessed")
            .join("config.toml");
        env.fs.write_file(&deployed, b"name = USER EDITED").unwrap();

        ctx.force = true;
        let plan = plan_pack(
            &pack, &ctx, /* write_baselines */ true, /* force */ ctx.force,
        )
        .unwrap();
        assert!(
            plan.warnings.iter().all(|w| !w.contains("preserved")),
            "force=true must not emit preservation warnings: {:?}",
            plan.warnings
        );
        assert_eq!(
            env.fs.read_to_string(&deployed).unwrap(),
            "name = original",
            "force must overwrite the user's edit with the rendered content"
        );
    }

    #[test]
    fn plan_pack_force_false_preserves_edit_even_when_ctx_force_is_true() {
        // Review feedback (PR #118 third pass): when `dodot up --force`
        // hits a cross-pack conflict, `up_or_status_for_conflict`
        // falls back to `status::status()` which calls plan_pack.
        // ctx.force is still true at that point, but status's
        // plan_pack call must explicitly pass force=false so the
        // divergence guard remains active during what is nominally
        // a read-only display pass — otherwise preserved files get
        // clobbered while merely rendering status.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.tmpl", "name = original")
            .done()
            .build();

        let mut ctx = make_context(&env);
        let pack = Pack::new(
            "app".into(),
            env.dotfiles_root.join("app"),
            ctx.config_manager
                .config_for_pack(&env.dotfiles_root.join("app"))
                .unwrap()
                .to_handler_config(),
        );

        // Prime baseline.
        let _ = plan_pack(&pack, &ctx, true, false).unwrap();
        let deployed = env
            .paths
            .handler_data_dir("app", "preprocessed")
            .join("config.toml");
        env.fs.write_file(&deployed, b"name = USER EDITED").unwrap();

        // Even with ctx.force=true (mimicking `dodot up --force` that
        // got bounced to status), passing force=false to plan_pack
        // must keep the guard active.
        ctx.force = true;
        let plan = plan_pack(
            &pack, &ctx, /* write_baselines */ false, /* force */ false,
        )
        .unwrap();
        assert_eq!(
            env.fs.read_to_string(&deployed).unwrap(),
            "name = USER EDITED",
            "force=false in plan_pack must preserve user edits regardless of ctx.force"
        );
        assert!(
            plan.warnings.iter().any(|w| w.contains("preserved")),
            "preservation warning should still surface, got: {:?}",
            plan.warnings
        );
    }

    #[test]
    fn plan_pack_with_write_baselines_false_does_not_touch_baseline_cache() {
        // Regression for issue #110 review: `dodot status` calls
        // `plan_pack` for cross-pack conflict detection. With
        // `write_baselines = false`, the planning pass must not update
        // the baseline cache — even on the "InputChanged" path where
        // the source has changed since last `up`. Otherwise a passive
        // `dodot status` run would silently rebaseline templates.
        let env = TempEnvironment::builder()
            .pack("app")
            .file("config.toml.tmpl", "name = original")
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

        // Prime the baseline with a write-enabled run (mirrors `up`).
        let _ = plan_pack(
            &pack, &ctx, /* write_baselines */ true, /* force */ false,
        )
        .unwrap();
        let before = crate::preprocessing::baseline::Baseline::load(
            ctx.fs.as_ref(),
            ctx.paths.as_ref(),
            "app",
            "preprocessed",
            "config.toml",
        )
        .unwrap()
        .unwrap();

        // Edit the source so the next preprocess would re-render and
        // (under the bug) overwrite the baseline.
        env.fs
            .write_file(
                &env.dotfiles_root.join("app/config.toml.tmpl"),
                b"name = changed",
            )
            .unwrap();

        // Run with write_baselines=false (the `dodot status` path).
        let _ = plan_pack(
            &pack, &ctx, /* write_baselines */ false, /* force */ false,
        )
        .unwrap();

        let after = crate::preprocessing::baseline::Baseline::load(
            ctx.fs.as_ref(),
            ctx.paths.as_ref(),
            "app",
            "preprocessed",
            "config.toml",
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            before, after,
            "write_baselines=false must leave the baseline cache untouched"
        );
    }
}

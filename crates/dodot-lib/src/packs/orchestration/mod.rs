//! Orchestration pipeline — the single entry point for executing
//! commands across packs.
//!
//! `execute()` owns the outer loop: discover packs → load per-pack
//! config → execute command → aggregate results.
//!
//! The shared [`ExecutionContext`] (every command's dependency bag) and
//! the result types ([`PackResult`], [`ExecuteResult`]) plus the
//! per-pack [`Command`] trait live in sibling modules
//! ([`crate::packs::context`], [`crate::packs::types`]); they are
//! re-exported here for the historical `crate::packs::orchestration::X`
//! surface.

use tracing::{debug, info};

use crate::execution::Executor;
use crate::operations::OperationResult;
use crate::packs::{self, Pack};
use crate::Result;

pub use crate::packs::context::ExecutionContext;
pub use crate::packs::types::{Command, ExecuteResult, PackResult};

mod planning;
mod resolve;

#[cfg(test)]
mod test_support;

pub(crate) use planning::filter_pre_preprocess_gates;
pub use planning::{
    collect_pack_intents, collect_pack_intents_with_preprocessors, plan_pack, PackPlan,
};
pub use resolve::{resolve_pack_dir_name, validate_pack_names};

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
    let host = ctx.host_facts.as_ref();

    for mut pack in all_packs {
        info!(pack = %pack.name, "processing pack");

        // Load pack-specific merged config
        let pack_config = match ctx.config_manager.config_for_pack(&pack.path) {
            Ok(pack_config) => {
                debug!(pack = %pack.name, "loaded pack config");
                pack.config = pack_config.to_handler_config();
                pack_config
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
        };

        // C3: skip packs gated out by `[pack] os` on this host. Counted
        // as successful (it's the configured behaviour, not a failure)
        // with no operations — same shape `.dodotignore` would have if
        // it reached this loop.
        if !crate::gates::pack_os_active(&pack_config.pack.os, host) {
            debug!(
                pack = %pack.name,
                allowed = ?pack_config.pack.os,
                current_os = %host.os,
                "pack inactive on this OS, skipping"
            );
            successful += 1;
            pack_results.push(PackResult {
                pack_name: pack.name.clone(),
                success: true,
                operations: Vec::new(),
                error: None,
            });
            continue;
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
    let fetcher = crate::external::UreqFetcher::new();
    let git = crate::external::ShellGitRunner::new();
    let executor = Executor::new(
        ctx.datastore.as_ref(),
        ctx.fs.as_ref(),
        ctx.paths.as_ref(),
        ctx.dry_run,
        ctx.force,
        ctx.provision_rerun,
        auto_chmod,
    )
    .with_fetcher(&fetcher)
    .with_git(&git);
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

#[cfg(test)]
mod tests {
    //! Driver-level tests: `execute()` + `prepare_packs()` flow,
    //! pack-filter resolution, dry-run / no-provision plumbing.
    //! Per-area suites live in sibling test modules (planning,
    //! resolve, test_support).

    #![allow(unused_imports)]

    use std::sync::Arc;

    use super::test_support::{make_context, MockCommandRunner, TestUpCommand};
    use super::*;
    use crate::config::ConfigManager;
    use crate::datastore::{CommandRunner, FilesystemDataStore};
    use crate::fs::Fs;
    use crate::paths::Pather;
    use crate::testing::TempEnvironment;

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
            check_drift: false,
            show_diff: false,
            view_mode: crate::commands::ViewMode::Full,
            group_mode: crate::commands::GroupMode::Name,
            verbose: false,
            host_facts: Arc::new(crate::gates::HostFacts::detect()),
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
}

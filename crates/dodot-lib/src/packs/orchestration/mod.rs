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
    ProvisionSkip,
};
pub use resolve::{resolve_pack_dir_name, validate_pack_names};

// ── Pipeline ────────────────────────────────────────────────────

/// Execute a command across all (or filtered) packs — the single entry
/// point for the orchestration pipeline.
pub fn execute(
    command: &dyn Command,
    pack_filter: Option<&[String]>,
    ctx: &ExecutionContext,
) -> Result<ExecuteResult> {
    info!(command = command.name(), "starting command");

    let root_config = ctx.config_manager.root_config()?;
    debug!(
        ignore_patterns = ?root_config.pack.ignore,
        "loaded root config"
    );

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

        // Packs gated out by `[pack] os` on this host count as
        // successful (it's the configured behaviour, not a failure)
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

    let mut configured = Vec::with_capacity(all_packs.len());
    for mut pack in all_packs {
        let pack_config = ctx.config_manager.config_for_pack(&pack.path)?;
        debug!(pack = %pack.name, "loaded pack config");
        pack.config = pack_config.to_handler_config();
        configured.push(pack);
    }

    Ok(configured)
}

/// Result of [`scan_ignored`]: the `.dodotignore`-marked packs split by
/// the two distinct jobs they serve.
///
/// Reporting (the ignored rows) is scoped to what the user asked about, so
/// it respects `pack_filter`. The stale-state sweep is
/// **not** — the init script is regenerated from the *whole* datastore,
/// so a filtered `dodot up <other-pack>` must still tear down every
/// now-ignored pack's leftover state, or it would keep getting sourced.
pub struct IgnoredScan {
    /// Raw on-disk directory names (datastore keys) for **every** ignored
    /// pack, ignoring `pack_filter`. Used by [`sweep_pack_state`] so
    /// the global init regeneration never re-sources a stale pack.
    pub sweep_dir_names: Vec<String>,
    /// Display metadata for ignored packs that match `pack_filter`, for the
    /// rendered ignored rows — the same form and scope `status` surfaces.
    pub display_packs: Vec<crate::packs::IgnoredPack>,
}

/// Scan the dotfiles root for `.dodotignore`-marked packs.
///
/// Centralises the ignored-pack discovery that `up`, `down`, and
/// `status` all need so the three commands report (and sweep) the same
/// set — the divergence that let `dodot up <ignored>` print a generic
/// "Packs deployed." while `dodot status <ignored>` showed the pack in
/// its ignored rows (issue #222). The returned
/// [`IgnoredScan`] separates the filtered reporting set from the
/// unfiltered sweep set; see its docs for why they differ.
pub fn scan_ignored(pack_filter: Option<&[String]>, ctx: &ExecutionContext) -> Result<IgnoredScan> {
    let root_config = ctx.config_manager.root_config()?;
    let all_ignored = packs::scan_packs(
        ctx.fs.as_ref(),
        ctx.paths.dotfiles_root(),
        &root_config.pack.ignore,
    )?
    .ignored;

    let sweep_dir_names: Vec<String> = all_ignored.iter().map(|p| p.name.clone()).collect();

    let display_packs = all_ignored
        .iter()
        .filter(|p| match pack_filter {
            None => true,
            Some(names) => names.iter().any(|n| n == &p.name || n == &p.display_name),
        })
        .cloned()
        .collect();

    Ok(IgnoredScan {
        sweep_dir_names,
        display_packs,
    })
}

/// Tear down all datastore state for the given pack dir names.
///
/// The stale-state sweep behind two symptoms of the same bug class:
///
/// - A pack deployed and *then* `.dodotignore`-marked leaves its shell /
///   path / symlink state in the datastore (issue #222). Callers pass
///   the **unfiltered** [`IgnoredScan::sweep_dir_names`] here: the init
///   script is regenerated from the whole datastore, so even a filtered
///   `up`/`down` must sweep every ignored pack or a now-ignored pack
///   outside the filter would still be sourced.
/// - A pack deployed and *then* deleted from the dotfiles repo leaves
///   "orphaned" state the repo-driven pack discovery never visits
///   (issue #255). `down` passes the [`scan_orphaned`] result here.
///
/// Either way the regenerated init script is driven entirely off the
/// datastore `packs/` tree, so leftover state keeps getting sourced on
/// every shell startup until it is removed. Before a pack's datastore
/// state goes, its live destination symlinks are removed too (via
/// [`remove_live_user_links`], issue #225) — except for orphans, whose
/// deploy destinations can no longer be replanned. Datastore is keyed
/// by on-disk directory name, not display name. Returns the dir names
/// that actually had state removed, so the caller can reflect
/// "something was deactivated" in its message.
pub fn sweep_pack_state(dir_names: &[String], ctx: &ExecutionContext) -> Result<Vec<String>> {
    let mut swept = Vec::new();
    for dir in dir_names {
        let handlers = ctx.datastore.list_pack_handlers(dir)?;
        if handlers.is_empty() {
            continue;
        }
        swept.push(dir.clone());
        remove_live_user_links_for_dir(dir, ctx)?;
        for handler in handlers {
            debug!(pack = %dir, %handler, "sweeping stale pack state");
            ctx.datastore.remove_state(dir, &handler)?;
        }
    }
    Ok(swept)
}

/// Remove the live destination symlinks a pack's deployment created —
/// the teardown counterpart of the executor's user-link creation
/// (issue #225).
///
/// Removing datastore state alone leaves the user-facing symlink (e.g.
/// `~/.config/vim/vimrc` → `<datastore>/packs/vim/symlink/vimrc`)
/// dangling in the user's config tree. This replans the pack passively
/// — the same single source of target truth `status` uses; teardown
/// must not re-derive deploy paths either — and removes each planned
/// destination (`Link` and `Fetch` intents, the two that create user
/// links) only when it is a symlink pointing into that intent's handler
/// data dir. A destination the user repointed elsewhere, or another
/// pack's link at the same path, is never touched.
///
/// Best-effort on the planning side: a pack whose plan cannot be
/// computed (e.g. a config error in a now-ignored pack) is skipped with
/// a debug log — datastore teardown must proceed regardless. Removal
/// failures do propagate. Returns the number of links removed.
pub fn remove_live_user_links(pack: &Pack, ctx: &ExecutionContext) -> Result<usize> {
    let plan = match planning::plan_pack(pack, ctx, crate::preprocessing::PreprocessMode::Passive) {
        Ok(plan) => plan,
        Err(e) => {
            debug!(
                pack = %pack.name,
                error = %e,
                "cannot plan pack for live-link teardown; skipping"
            );
            return Ok(0);
        }
    };
    let mut removed = 0;
    for intent in &plan.intents {
        let (pack_name, handler, user_path) = match intent {
            crate::operations::HandlerIntent::Link {
                pack,
                handler,
                user_path,
                ..
            }
            | crate::operations::HandlerIntent::Fetch {
                pack,
                handler,
                user_path,
                ..
            } => (pack, handler, user_path),
            _ => continue,
        };
        if !ctx.fs.is_symlink(user_path) {
            continue;
        }
        let handler_dir = ctx.paths.handler_data_dir(pack_name, handler);
        match ctx.fs.readlink(user_path) {
            Ok(target) if target.starts_with(&handler_dir) => {
                debug!(pack = %pack.name, path = %user_path.display(), "removing live user link");
                ctx.fs.remove_file(user_path)?;
                removed += 1;
            }
            _ => {}
        }
    }
    Ok(removed)
}

/// [`remove_live_user_links`] keyed by on-disk directory name, for the
/// sweep paths that only know datastore keys. A dir that no longer
/// exists under the dotfiles root (an orphan, issue #255) is skipped:
/// with the pack sources gone there is nothing to replan, so its live
/// links cannot be recovered — the one teardown case that still leaves
/// a dangling link behind.
fn remove_live_user_links_for_dir(dir: &str, ctx: &ExecutionContext) -> Result<usize> {
    let path = ctx.paths.dotfiles_root().join(dir);
    if !ctx.fs.is_dir(&path) {
        return Ok(0);
    }
    let pack_config = match ctx.config_manager.config_for_pack(&path) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                pack = %dir,
                error = %e,
                "cannot load pack config for live-link teardown; skipping"
            );
            return Ok(0);
        }
    };
    let pack = Pack::new(dir.to_string(), path, pack_config.to_handler_config());
    remove_live_user_links(&pack, ctx)
}

/// Count of the given pack dirs that currently hold datastore state,
/// without removing anything. Lets `down --dry-run` report "would
/// deactivate" consistently with what a real run would sweep.
pub fn packs_with_state(dir_names: &[String], ctx: &ExecutionContext) -> Result<usize> {
    let mut n = 0;
    for dir in dir_names {
        if !ctx.datastore.list_pack_handlers(dir)?.is_empty() {
            n += 1;
        }
    }
    Ok(n)
}

/// Scan the datastore for orphaned pack state: `packs/` subtrees that
/// still hold handler state but whose pack directory no longer exists
/// under the dotfiles root (issue #255).
///
/// Pack discovery enumerates the *repo*, so state keyed by a pack name
/// the repo has since deleted is never visited by `up`/`down` — while
/// the init script and deployment map are regenerated from the *whole*
/// datastore, keeping the orphaned state live in every shell. `down`
/// sweeps what this returns; `up` surfaces it as a warning (it does not
/// sweep, so a misresolved dotfiles root can't silently wipe legitimate
/// state on deploy).
///
/// Returns on-disk directory names (datastore keys), sorted. A datastore
/// subtree with no handler state is not reported — there is nothing to
/// remove.
pub fn scan_orphaned(ctx: &ExecutionContext) -> Result<Vec<String>> {
    let root = ctx.paths.dotfiles_root();
    let mut orphans = Vec::new();
    for dir in ctx.datastore.list_packs()? {
        if ctx.fs.exists(&root.join(&dir)) {
            continue;
        }
        if ctx.datastore.list_pack_handlers(&dir)?.is_empty() {
            continue;
        }
        orphans.push(dir);
    }
    orphans.sort();
    Ok(orphans)
}

/// The shared warning line for orphaned state left in place: `up` emits
/// it whenever [`scan_orphaned`] finds anything, and a filtered `down`
/// emits it for orphans outside its filter. Names render in display
/// form (the form `dodot down <pack>` accepts).
pub fn orphan_warning(dir_names: &[String], ctx: &ExecutionContext) -> String {
    let names = dir_names
        .iter()
        .map(|d| packs::display_name_for(d))
        .collect::<Vec<_>>()
        .join(", ");
    let (noun, verb, pronoun) = if dir_names.len() == 1 {
        ("pack", "exists", "it")
    } else {
        ("packs", "exist", "them")
    };
    format!(
        "warning: {noun} with deployed state no longer {verb} in {}: {} — run `dodot down` to remove {pronoun}",
        ctx.paths.dotfiles_root().display(),
        names,
    )
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
            env_stamp: Default::default(),
            tty: false,
            shell_probe: crate::shell::ProbePolicy::Never,
            shell_env: crate::shell::ShellEnv::default(),
        };

        let result = execute(&TestUpCommand, None, &ctx).unwrap();
        assert!(result.is_success());
        assert!(!result.pack_results[0].operations.is_empty());

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

        for r in &results {
            assert!(
                !matches!(r.operation, crate::operations::Operation::RunCommand { .. }),
                "RunCommand should be skipped with no_provision"
            );
        }
    }
}

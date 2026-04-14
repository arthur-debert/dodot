//! Orchestration pipeline — the single entry point for executing
//! commands across packs.
//!
//! `execute()` owns the outer loop: discover packs → load per-pack
//! config → execute command → aggregate results.

use std::sync::Arc;

use serde::Serialize;

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
    pub dry_run: bool,
    pub no_provision: bool,
    pub provision_rerun: bool,
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

    fn execute_for_pack(
        &self,
        pack: &Pack,
        ctx: &ExecutionContext,
    ) -> Result<PackResult>;
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
    // Load root config for pack-level ignore patterns
    let root_config = ctx.config_manager.root_config()?;

    // Discover packs
    let mut all_packs = packs::discover_packs(
        ctx.fs.as_ref(),
        ctx.paths.dotfiles_root(),
        &root_config.pack.ignore,
    )?;

    // Apply name filter
    if let Some(names) = pack_filter {
        all_packs.retain(|p| names.iter().any(|n| n == &p.name));
    }

    let total_packs = all_packs.len();
    let mut pack_results = Vec::with_capacity(total_packs);
    let mut successful = 0;
    let mut failed = 0;

    for mut pack in all_packs {
        // Load pack-specific merged config
        match ctx.config_manager.config_for_pack(&pack.path) {
            Ok(pack_config) => {
                pack.config = pack_config.to_handler_config();
            }
            Err(e) => {
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
                    successful += 1;
                } else {
                    failed += 1;
                }
                pack_results.push(result);
            }
            Err(e) => {
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

    Ok(ExecuteResult {
        pack_results,
        total_packs,
        successful_packs: successful,
        failed_packs: failed,
    })
}

// ── Built-in "up" pipeline helper ───────────────────────────────

/// Run the standard handler pipeline for a pack: scan → match rules →
/// group by handler → to_intents → execute.
///
/// This is the shared logic used by the `up` command (and potentially
/// `status` for intent generation).
pub fn run_handler_pipeline(
    pack: &Pack,
    ctx: &ExecutionContext,
) -> Result<Vec<OperationResult>> {
    let root_config = ctx.config_manager.config_for_pack(&pack.path)?;
    let rules = crate::config::mappings_to_rules(&root_config.mappings);

    // Scan pack files
    let scanner = Scanner::new(ctx.fs.as_ref());
    let matches = scanner.scan_pack(pack, &rules, &root_config.pack.ignore)?;

    // Group by handler
    let groups = rules::group_by_handler(&matches);
    let order = rules::handler_execution_order(&groups);

    // Build handler registry
    let registry = handlers::create_registry(ctx.fs.as_ref());

    // Generate intents from each handler
    let mut all_intents = Vec::new();
    for handler_name in &order {
        let handler = match registry.get(handler_name.as_str()) {
            Some(h) => h,
            None => continue, // skip unknown handlers (e.g. "exclude")
        };

        // Skip code execution handlers if --no-provision
        if ctx.no_provision
            && handler.category() == handlers::HandlerCategory::CodeExecution
        {
            continue;
        }

        if let Some(handler_matches) = groups.get(handler_name) {
            let intents =
                handler.to_intents(handler_matches, &pack.config, ctx.paths.as_ref())?;
            all_intents.extend(intents);
        }
    }

    // Execute intents
    let executor = Executor::new(ctx.datastore.as_ref(), ctx.dry_run);
    executor.execute(all_intents)
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
        fn run(&self, command: &str) -> Result<CommandOutput> {
            self.calls.lock().unwrap().push(command.to_string());
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
        let config_manager =
            Arc::new(ConfigManager::new(&env.dotfiles_root).unwrap());

        ExecutionContext {
            fs: env.fs.clone() as Arc<dyn Fs>,
            datastore,
            paths: env.paths.clone() as Arc<dyn Pather>,
            config_manager,
            dry_run: false,
            no_provision: true, // skip install/homebrew in tests
            provision_rerun: false,
        }
    }

    /// Simple command that runs the handler pipeline.
    struct TestUpCommand;

    impl Command for TestUpCommand {
        fn name(&self) -> &str {
            "test-up"
        }

        fn execute_for_pack(
            &self,
            pack: &Pack,
            ctx: &ExecutionContext,
        ) -> Result<PackResult> {
            let operations = run_handler_pipeline(pack, ctx)?;
            let success = operations.iter().all(|r| r.success);
            Ok(PackResult {
                pack_name: pack.name.clone(),
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
        let pack = Pack {
            name: "vim".into(),
            path: env.dotfiles_root.join("vim"),
            config: ctx
                .config_manager
                .config_for_pack(&env.dotfiles_root.join("vim"))
                .unwrap()
                .to_handler_config(),
        };

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
        let config_manager =
            Arc::new(ConfigManager::new(&env.dotfiles_root).unwrap());

        let ctx = ExecutionContext {
            fs: env.fs.clone() as Arc<dyn Fs>,
            datastore,
            paths: env.paths.clone() as Arc<dyn Pather>,
            config_manager,
            dry_run: true,
            no_provision: true,
            provision_rerun: false,
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

        let pack = Pack {
            name: "vim".into(),
            path: env.dotfiles_root.join("vim"),
            config: ctx
                .config_manager
                .config_for_pack(&env.dotfiles_root.join("vim"))
                .unwrap()
                .to_handler_config(),
        };

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

//! Shared test fixture for the `transform` command suites.
//!
//! `make_ctx` wires a no-op `CommandRunner` into a `FilesystemDataStore`
//! pointed at the test's `TempEnvironment`. Both the check / status
//! tests in `mod.rs` and the install-hook tests in `install_hook.rs`
//! use it via `super::test_support::make_ctx`.

use std::sync::Arc;

use crate::config::ConfigManager;
use crate::datastore::{CommandOutput, CommandRunner, FilesystemDataStore};
use crate::fs::Fs;
use crate::packs::orchestration::ExecutionContext;
use crate::paths::Pather;
use crate::testing::TempEnvironment;
use crate::Result;

struct NoopRunner;

impl CommandRunner for NoopRunner {
    fn run(&self, _e: &str, _a: &[String]) -> Result<CommandOutput> {
        Ok(CommandOutput {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

pub(super) fn make_ctx(env: &TempEnvironment) -> ExecutionContext {
    let runner: Arc<dyn CommandRunner> = Arc::new(NoopRunner);
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
        no_provision: true,
        provision_rerun: false,
        force: false,
        view_mode: crate::commands::ViewMode::Full,
        group_mode: crate::commands::GroupMode::Name,
        verbose: false,
        host_facts: Arc::new(crate::gates::HostFacts::detect()),
    }
}

//! Shared test fixtures for the orchestration test suites.
//!
//! `MockCommandRunner` records every `(executable, arguments)` call so
//! provisioning-flow tests can assert on the recorded shape;
//! `make_context` wires it through a `FilesystemDataStore`, and
//! `TestUpCommand` is a tiny `Command` impl that runs the handler
//! pipeline and reports per-pack results — the same shape `up`/`down`
//! use in production. All three are `pub(super)` so the dispatcher,
//! planning, and resolve test suites can reach them via
//! `super::test_support::*` without duplicating fixtures.

use std::sync::{Arc, Mutex};

use crate::config::ConfigManager;
use crate::datastore::{CommandOutput, CommandRunner, FilesystemDataStore};
use crate::fs::Fs;
use crate::operations::OperationResult;
use crate::packs::context::ExecutionContext;
use crate::packs::types::{Command, PackResult};
use crate::packs::Pack;
use crate::paths::Pather;
use crate::testing::TempEnvironment;
use crate::Result;

use super::run_handler_pipeline;

pub(super) struct MockCommandRunner {
    pub(super) calls: Mutex<Vec<String>>,
}

impl MockCommandRunner {
    pub(super) fn new() -> Self {
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

pub(super) fn make_context(env: &TempEnvironment) -> ExecutionContext {
    let runner: Arc<dyn CommandRunner> = Arc::new(MockCommandRunner::new());
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
        check_drift: false,
        show_diff: false,
        view_mode: crate::commands::ViewMode::Full,
        group_mode: crate::commands::GroupMode::Name,
        verbose: false,
        host_facts: Arc::new(crate::gates::HostFacts::detect()),
    }
}

/// Simple command that runs the handler pipeline.
pub(super) struct TestUpCommand;

impl Command for TestUpCommand {
    fn name(&self) -> &str {
        "test-up"
    }

    fn execute_for_pack(&self, pack: &Pack, ctx: &ExecutionContext) -> Result<PackResult> {
        let operations: Vec<OperationResult> = run_handler_pipeline(pack, ctx)?;
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

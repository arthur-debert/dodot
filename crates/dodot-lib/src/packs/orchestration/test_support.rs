//! Shared orchestration-test fixtures.

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
    }
}

pub(super) struct TestUpCommand;

impl Command for TestUpCommand {
    fn name(&self) -> &str {
        "test-up"
    }

    fn execute_for_pack(&self, pack: &Pack, ctx: &ExecutionContext) -> Result<PackResult> {
        let operations: Vec<OperationResult> = run_handler_pipeline(pack, ctx)?;
        let success = operations.iter().all(|r| r.success);
        // Results use the display name rather than the raw on-disk directory.
        Ok(PackResult {
            pack_name: pack.display_name.clone(),
            success,
            operations,
            error: None,
        })
    }
}

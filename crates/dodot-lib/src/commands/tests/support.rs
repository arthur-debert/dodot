//! Shared command-test fixtures.

use std::sync::Arc;

use crate::config::ConfigManager;
use crate::datastore::{CommandOutput, CommandRunner, FilesystemDataStore};
use crate::fs::Fs;
use crate::packs::orchestration::ExecutionContext;
use crate::paths::Pather;
use crate::testing::TempEnvironment;
use crate::Result;

pub(super) struct MockCommandRunner;
impl CommandRunner for MockCommandRunner {
    fn run(&self, _: &str, _: &[String]) -> Result<CommandOutput> {
        Ok(CommandOutput {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

/// Returns canned command outputs without spawning subprocesses.
pub(super) struct CannedRunner {
    responses: std::sync::Mutex<std::collections::HashMap<Vec<String>, CommandOutput>>,
}

impl CannedRunner {
    pub(super) fn new() -> Self {
        Self {
            responses: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
    pub(super) fn respond(&self, args: &[&str], stdout: &str, exit_code: i32) {
        let key: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        self.responses.lock().unwrap().insert(
            key,
            CommandOutput {
                exit_code,
                stdout: stdout.into(),
                stderr: String::new(),
            },
        );
    }
}

impl CommandRunner for CannedRunner {
    fn run(&self, exe: &str, args: &[String]) -> Result<CommandOutput> {
        let mut full = vec![exe.to_string()];
        full.extend(args.iter().cloned());
        self.responses
            .lock()
            .unwrap()
            .get(&full)
            .cloned()
            .ok_or_else(|| {
                crate::DodotError::Other(format!("CannedRunner: no canned response for {full:?}"))
            })
    }
}

pub(super) fn make_ctx(env: &TempEnvironment) -> ExecutionContext {
    let runner: Arc<dyn CommandRunner> = Arc::new(MockCommandRunner);
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
        env_init_gen: None,
        tty: false,
        shell_probe: crate::shell::ProbePolicy::Never,
        shell_env: crate::shell::ShellEnv::default(),
    }
}

pub(super) fn make_ctx_with_runner(
    env: &TempEnvironment,
    runner: Arc<dyn CommandRunner>,
) -> ExecutionContext {
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
        env_init_gen: None,
        tty: false,
        shell_probe: crate::shell::ProbePolicy::Never,
        shell_env: crate::shell::ShellEnv::default(),
    }
}

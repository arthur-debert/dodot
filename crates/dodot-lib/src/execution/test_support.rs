//! Shared fixtures for per-intent tests.

use std::sync::{Arc, Mutex};

use crate::datastore::{CommandOutput, CommandRunner, CommandSpec, FilesystemDataStore};
use crate::testing::TempEnvironment;
use crate::Result;

pub(super) struct MockCommandRunner {
    pub(super) calls: Mutex<Vec<String>>,
    /// The environment handed to each call, in call order — one entry
    /// per entry in `calls`.
    pub(super) environments: Mutex<Vec<Vec<(String, String)>>>,
}

impl MockCommandRunner {
    pub(super) fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            environments: Mutex::new(Vec::new()),
        }
    }
}

impl CommandRunner for MockCommandRunner {
    fn run(&self, command: CommandSpec<'_>) -> Result<CommandOutput> {
        let CommandSpec {
            executable,
            arguments,
            environment,
            ..
        } = command;
        let cmd_str = format!("{} {}", executable, arguments.join(" "));
        self.calls.lock().unwrap().push(cmd_str.trim().to_string());
        self.environments.lock().unwrap().push(environment.to_vec());
        Ok(CommandOutput {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

pub(super) fn make_datastore(
    env: &TempEnvironment,
) -> (FilesystemDataStore, Arc<MockCommandRunner>) {
    let runner = Arc::new(MockCommandRunner::new());
    let ds = FilesystemDataStore::new(env.fs.clone(), env.paths.clone(), runner.clone());
    (ds, runner)
}

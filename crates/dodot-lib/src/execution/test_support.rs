//! Shared test fixtures for the per-intent test suites.
//!
//! `MockCommandRunner` records every `(executable, arguments)` call so
//! `Run`-intent tests can assert on the recorded shape; `make_datastore`
//! wires it through a `FilesystemDataStore` that points at the test's
//! `TempEnvironment`. Both are `pub(super)` so the per-intent modules
//! (`link`, `stage`, `run`) and the dispatcher tests in `mod.rs` can
//! reach them via `super::test_support::*` without duplicating fixtures.

use std::sync::{Arc, Mutex};

use crate::datastore::{CommandOutput, CommandRunner, FilesystemDataStore};
use crate::testing::TempEnvironment;
use crate::Result;

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

pub(super) fn make_datastore(
    env: &TempEnvironment,
) -> (FilesystemDataStore, Arc<MockCommandRunner>) {
    let runner = Arc::new(MockCommandRunner::new());
    let ds = FilesystemDataStore::new(env.fs.clone(), env.paths.clone(), runner.clone());
    (ds, runner)
}

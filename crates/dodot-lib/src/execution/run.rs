//! `Run` intent: execute a handler-driven command (install scripts,
//! Brewfile bundle), gated by a sentinel so re-runs are idempotent.
//!
//! `provision_rerun = true` skips the sentinel pre-check, forcing the
//! command to run again. Sentinel recording happens regardless.

use tracing::info;

use crate::operations::{HandlerIntent, Operation, OperationResult};
use crate::Result;

use super::Executor;

impl<'a> Executor<'a> {
    pub(super) fn execute_run(&self, intent: &HandlerIntent) -> Result<Vec<OperationResult>> {
        let HandlerIntent::Run {
            pack,
            handler,
            executable,
            arguments,
            sentinel,
        } = intent
        else {
            unreachable!("execute_run called with non-Run intent");
        };

        // Check sentinel first — unless provision_rerun is set
        if !self.provision_rerun {
            let already_done = self.datastore.has_sentinel(pack, handler, sentinel)?;

            if already_done {
                info!(
                    pack,
                    handler = handler.as_str(),
                    sentinel,
                    "sentinel found, skipping"
                );
                let op = Operation::CheckSentinel {
                    pack: pack.clone(),
                    handler: handler.clone(),
                    sentinel: sentinel.clone(),
                };
                return Ok(vec![OperationResult::ok(op, "already completed")]);
            }
        }

        let cmd_str = format!("{} {}", executable, arguments.join(" "));
        info!(pack, handler = handler.as_str(), command = %cmd_str.trim(), "running command");

        // Run the command
        self.datastore.run_and_record(
            pack,
            handler,
            executable,
            arguments,
            sentinel,
            self.provision_rerun,
        )?;

        info!(pack, sentinel, "command completed, sentinel recorded");

        let op = Operation::RunCommand {
            pack: pack.clone(),
            handler: handler.clone(),
            executable: executable.clone(),
            arguments: arguments.clone(),
            sentinel: sentinel.clone(),
        };

        Ok(vec![OperationResult::ok(
            op,
            format!("executed: {}", cmd_str.trim()),
        )])
    }

    pub(super) fn simulate_run(&self, intent: &HandlerIntent) -> Vec<OperationResult> {
        let HandlerIntent::Run {
            pack,
            handler,
            executable,
            arguments,
            sentinel,
        } = intent
        else {
            unreachable!("simulate_run called with non-Run intent");
        };

        let cmd_str = format!("{} {}", executable, arguments.join(" "));
        vec![OperationResult::ok(
            Operation::RunCommand {
                pack: pack.clone(),
                handler: handler.clone(),
                executable: executable.clone(),
                arguments: arguments.clone(),
                sentinel: sentinel.clone(),
            },
            format!("[dry-run] would execute: {}", cmd_str.trim()),
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::make_datastore;
    use super::super::Executor;
    use crate::fs::Fs;
    use crate::operations::HandlerIntent;
    use crate::paths::Pather;
    use crate::testing::TempEnvironment;

    #[test]
    fn execute_run_creates_sentinel() {
        let env = TempEnvironment::builder().build();
        let (ds, runner) = make_datastore(&env);
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        );

        let results = executor
            .execute(vec![HandlerIntent::Run {
                pack: "vim".into(),
                handler: "install".into(),
                executable: "echo".into(),
                arguments: vec!["hello".into()],
                sentinel: "install.sh-abc123".into(),
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert_eq!(runner.calls.lock().unwrap().as_slice(), &["echo hello"]);
        env.assert_sentinel("vim", "install", "install.sh-abc123");
    }

    #[test]
    fn execute_run_skips_when_sentinel_exists() {
        let env = TempEnvironment::builder().build();
        let (ds, runner) = make_datastore(&env);

        // Pre-create sentinel
        let sentinel_dir = env.paths.handler_data_dir("vim", "install");
        env.fs.mkdir_all(&sentinel_dir).unwrap();
        env.fs
            .write_file(&sentinel_dir.join("install.sh-abc123"), b"completed|12345")
            .unwrap();

        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        );
        let results = executor
            .execute(vec![HandlerIntent::Run {
                pack: "vim".into(),
                handler: "install".into(),
                executable: "echo".into(),
                arguments: vec!["should-not-run".into()],
                sentinel: "install.sh-abc123".into(),
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert!(results[0].message.contains("already completed"));
        assert!(runner.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn provision_rerun_ignores_sentinel() {
        let env = TempEnvironment::builder().build();
        let (ds, runner) = make_datastore(&env);

        // Pre-create sentinel
        let sentinel_dir = env.paths.handler_data_dir("vim", "install");
        env.fs.mkdir_all(&sentinel_dir).unwrap();
        env.fs
            .write_file(&sentinel_dir.join("install.sh-abc123"), b"completed|12345")
            .unwrap();

        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            true,
            true,
        );
        let results = executor
            .execute(vec![HandlerIntent::Run {
                pack: "vim".into(),
                handler: "install".into(),
                executable: "echo".into(),
                arguments: vec!["rerun".into()],
                sentinel: "install.sh-abc123".into(),
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert!(
            results[0].message.contains("executed"),
            "msg: {}",
            results[0].message
        );
        assert_eq!(runner.calls.lock().unwrap().as_slice(), &["echo rerun"]);
    }
}

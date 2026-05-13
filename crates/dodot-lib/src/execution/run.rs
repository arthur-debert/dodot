//! `Run` intent: execute a run-once handler command (install scripts,
//! Brewfile bundle, `nix profile install`), gated by [`DataStore::did_run`]'s
//! three-way classification (#169).
//!
//! Policy: run on `NeverRan`, skip silently on `RanCurrent`, skip with
//! a "ran older version" notice on `RanDifferent`. `provision_rerun =
//! true` (the `--force` flag) bypasses both skip cases.

use tracing::info;

use crate::datastore::DidRunStatus;
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
            filename,
            content_hash,
        } = intent
        else {
            unreachable!("execute_run called with non-Run intent");
        };

        // Three-way policy via did_run, unless --force.
        if !self.provision_rerun {
            match self
                .datastore
                .did_run(pack, handler, filename, content_hash)?
            {
                DidRunStatus::RanCurrent => {
                    info!(
                        pack,
                        handler = handler.as_str(),
                        sentinel,
                        "current-version sentinel found, skipping"
                    );
                    let op = Operation::CheckSentinel {
                        pack: pack.clone(),
                        handler: handler.clone(),
                        sentinel: sentinel.clone(),
                    };
                    return Ok(vec![OperationResult::ok(op, "already completed")]);
                }
                DidRunStatus::RanDifferent { previous_hash, .. } => {
                    info!(
                        pack,
                        handler = handler.as_str(),
                        filename,
                        previous_hash,
                        current_hash = content_hash,
                        "older-version sentinel found, skipping (run with --force to apply)"
                    );
                    let op = Operation::CheckSentinel {
                        pack: pack.clone(),
                        handler: handler.clone(),
                        sentinel: sentinel.clone(),
                    };
                    return Ok(vec![OperationResult::ok(
                        op,
                        format!(
                            "ran older version of {filename} — run `dodot up --force` to apply current"
                        ),
                    )]);
                }
                DidRunStatus::NeverRan => {
                    // fall through and run
                }
            }
        }

        let cmd_str = format!("{} {}", executable, arguments.join(" "));
        info!(pack, handler = handler.as_str(), command = %cmd_str.trim(), "running command");

        // Run the command. `force=true` here tells run_and_record to
        // skip its own internal has_sentinel pre-check — we've already
        // made the policy decision above via did_run.
        self.datastore
            .run_and_record(pack, handler, executable, arguments, sentinel, true)?;

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
            filename,
            content_hash,
        } = intent
        else {
            unreachable!("simulate_run called with non-Run intent");
        };

        // Mirror execute_run's three-way policy for dry-run output so
        // the user sees the same skip/notify decisions they'd get on a
        // real run. We don't error on lookup failures — fall through
        // to "would execute" if did_run fails.
        if !self.provision_rerun {
            if let Ok(status) = self
                .datastore
                .did_run(pack, handler, filename, content_hash)
            {
                match status {
                    DidRunStatus::RanCurrent => {
                        let op = Operation::CheckSentinel {
                            pack: pack.clone(),
                            handler: handler.clone(),
                            sentinel: sentinel.clone(),
                        };
                        return vec![OperationResult::ok(
                            op,
                            "[dry-run] would skip (already completed)",
                        )];
                    }
                    DidRunStatus::RanDifferent { .. } => {
                        let op = Operation::CheckSentinel {
                            pack: pack.clone(),
                            handler: handler.clone(),
                            sentinel: sentinel.clone(),
                        };
                        return vec![OperationResult::ok(
                            op,
                            format!(
                                "[dry-run] would skip (ran older version of {filename}; --force to apply)"
                            ),
                        )];
                    }
                    DidRunStatus::NeverRan => {}
                }
            }
        }

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

    fn run_intent(
        pack: &str,
        handler: &str,
        executable: &str,
        args: &[&str],
        filename: &str,
        hash: &str,
    ) -> HandlerIntent {
        HandlerIntent::Run {
            pack: pack.into(),
            handler: handler.into(),
            executable: executable.into(),
            arguments: args.iter().map(|s| (*s).into()).collect(),
            sentinel: format!("{filename}-{hash}"),
            filename: filename.into(),
            content_hash: hash.into(),
        }
    }

    #[test]
    fn execute_run_runs_when_never_ran() {
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
            .execute(vec![run_intent(
                "vim",
                "install",
                "echo",
                &["hello"],
                "install.sh",
                "abc1234567890def",
            )])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert_eq!(runner.calls.lock().unwrap().as_slice(), &["echo hello"]);
        env.assert_sentinel("vim", "install", "install.sh-abc1234567890def");
    }

    #[test]
    fn execute_run_skips_silently_when_current_hash_matches() {
        let env = TempEnvironment::builder().build();
        let (ds, runner) = make_datastore(&env);

        // Pre-create sentinel for the SAME hash as the intent.
        let sentinel_dir = env.paths.handler_data_dir("vim", "install");
        env.fs.mkdir_all(&sentinel_dir).unwrap();
        env.fs
            .write_file(
                &sentinel_dir.join("install.sh-abc1234567890def"),
                b"completed|12345",
            )
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
            .execute(vec![run_intent(
                "vim",
                "install",
                "echo",
                &["should-not-run"],
                "install.sh",
                "abc1234567890def",
            )])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert!(results[0].message.contains("already completed"));
        assert!(runner.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn execute_run_skips_with_notice_when_older_version_ran() {
        // Pre-create a sentinel for a DIFFERENT hash → did_run returns
        // RanDifferent → policy: skip with "ran older version" notice.
        let env = TempEnvironment::builder().build();
        let (ds, runner) = make_datastore(&env);

        let sentinel_dir = env.paths.handler_data_dir("vim", "install");
        env.fs.mkdir_all(&sentinel_dir).unwrap();
        env.fs
            .write_file(
                &sentinel_dir.join("install.sh-aaaaaaaaaaaaaaaa"),
                b"completed|12345",
            )
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
            .execute(vec![run_intent(
                "vim",
                "install",
                "echo",
                &["new-content"],
                "install.sh",
                "bbbbbbbbbbbbbbbb",
            )])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert!(
            results[0].message.contains("ran older version"),
            "msg: {}",
            results[0].message
        );
        assert!(
            results[0].message.contains("--force"),
            "msg: {}",
            results[0].message
        );
        assert!(
            runner.calls.lock().unwrap().is_empty(),
            "command must not run on older-version detection"
        );
    }

    #[test]
    fn provision_rerun_bypasses_skip_when_current() {
        let env = TempEnvironment::builder().build();
        let (ds, runner) = make_datastore(&env);

        let sentinel_dir = env.paths.handler_data_dir("vim", "install");
        env.fs.mkdir_all(&sentinel_dir).unwrap();
        env.fs
            .write_file(
                &sentinel_dir.join("install.sh-abc1234567890def"),
                b"completed|12345",
            )
            .unwrap();

        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            true, // provision_rerun
            true,
        );
        let results = executor
            .execute(vec![run_intent(
                "vim",
                "install",
                "echo",
                &["rerun"],
                "install.sh",
                "abc1234567890def",
            )])
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

    #[test]
    fn provision_rerun_bypasses_skip_when_older_version() {
        let env = TempEnvironment::builder().build();
        let (ds, runner) = make_datastore(&env);

        let sentinel_dir = env.paths.handler_data_dir("vim", "install");
        env.fs.mkdir_all(&sentinel_dir).unwrap();
        env.fs
            .write_file(
                &sentinel_dir.join("install.sh-aaaaaaaaaaaaaaaa"),
                b"completed|12345",
            )
            .unwrap();

        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            true, // provision_rerun
            true,
        );
        let results = executor
            .execute(vec![run_intent(
                "vim",
                "install",
                "echo",
                &["forced"],
                "install.sh",
                "bbbbbbbbbbbbbbbb",
            )])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert!(results[0].message.contains("executed"));
        assert_eq!(runner.calls.lock().unwrap().as_slice(), &["echo forced"]);
    }
}

//! `Run` intent: execute a run-once handler command (install scripts,
//! Brewfile bundle, `nix profile install`), gated by [`DataStore::did_run`]'s
//! three-way classification.
//!
//! Policy: run on `NeverRan`, skip silently on `RanCurrent`, skip with
//! a "ran older version" notice on `RanDifferent`. `provision_rerun =
//! true` (the `--provision-rerun` flag) bypasses both skip cases.
//!
//! A command that fails is a **failed operation, not an aborted pack**.
//! The failure comes back as an `OperationResult::fail` against the
//! run-once file, carrying the command line, its exit code, and the
//! manager's own stderr. The executor keeps going, so the rest of the
//! pack — symlinks, `$PATH` entries, shell init, all of which run in
//! later phases than provisioning — still deploys and still reports.
//! No sentinel is written, so the next `dodot up` retries the file.
//!
//! Only the command failing is contained this way. A datastore I/O
//! error — notably the sentinel write that happens *after* a
//! successful command — still aborts the run, as it does everywhere
//! else in the executor.
//!
//! Two names for the run-once file appear here and mean different
//! things: `relative_path` identifies the file for reporting (status
//! rows are keyed by pack-relative path), and its basename is the
//! sentinel key `did_run` looks up, since sentinels are named
//! `<basename>-<hash>`.

use tracing::info;

use crate::datastore::{CommandSpec, DidRunStatus};
use crate::operations::{HandlerIntent, Operation, OperationResult};
use crate::{DodotError, Result};

use super::Executor;

/// The sentinel key for a run-once file: its basename.
///
/// Sentinels are written as `<basename>-<hash>` in the handler's data
/// directory, so `did_run` matches on the basename even though the
/// intent identifies the file by its pack-relative path.
fn sentinel_key(relative_path: &str) -> &str {
    std::path::Path::new(relative_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(relative_path)
}

impl<'a> Executor<'a> {
    pub(super) fn execute_run(&self, intent: &HandlerIntent) -> Result<Vec<OperationResult>> {
        let HandlerIntent::Run {
            pack,
            handler,
            executable,
            arguments,
            environment,
            sentinel,
            relative_path,
            content_hash,
        } = intent
        else {
            unreachable!("execute_run called with non-Run intent");
        };

        if !self.provision_rerun {
            match self.datastore.did_run(
                pack,
                handler,
                sentinel_key(relative_path),
                content_hash,
            )? {
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
                        relative_path,
                        previous_hash,
                        current_hash = content_hash,
                        "older-version sentinel found, skipping (run with --provision-rerun to apply)"
                    );
                    let op = Operation::CheckSentinel {
                        pack: pack.clone(),
                        handler: handler.clone(),
                        sentinel: sentinel.clone(),
                    };
                    return Ok(vec![OperationResult::ok(
                        op,
                        format!(
                            "ran older version of {relative_path} — run `dodot up --provision-rerun` to apply the current one"
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

        let op = Operation::RunCommand {
            pack: pack.clone(),
            handler: handler.clone(),
            executable: executable.clone(),
            arguments: arguments.clone(),
            environment: environment.clone(),
            sentinel: sentinel.clone(),
            relative_path: relative_path.clone(),
        };

        // `force=true` here tells run_and_record to skip its own
        // internal has_sentinel pre-check — we've already made the
        // policy decision above via did_run.
        //
        // A failing command is reported, not propagated: `?` here
        // would abort the executor's intent loop, and `up` would
        // record the whole pack as failed and drop every operation
        // that already succeeded in it. The error text carries the
        // command, its exit code, and the manager's stderr, which is
        // what the failure row's note shows.
        //
        // Only `CommandFailed` is contained. `run_and_record` also
        // creates the sentinel directory and writes the sentinel
        // *after* the command has already succeeded, and those
        // failures come back as `Fs` errors. Demoting one of those to
        // a failure row would misreport a command that ran fine as a
        // command that failed, and would keep mutating the pack
        // through a datastore that has just proved it cannot be
        // written to. Everything that is not the user's script
        // failing still aborts the run, which is `Executor::execute`'s
        // standing contract for I/O errors.
        match self.datastore.run_and_record(
            pack,
            handler,
            CommandSpec::with_environment(executable, arguments, environment),
            sentinel,
            true,
        ) {
            Ok(()) => {}
            Err(e @ DodotError::CommandFailed { .. }) => {
                info!(
                    pack,
                    handler = handler.as_str(),
                    relative_path,
                    error = %e,
                    "command failed; no sentinel written, remaining intents continue"
                );
                return Ok(vec![OperationResult::fail(op, e.to_string())]);
            }
            Err(e) => return Err(e),
        }

        info!(pack, sentinel, "command completed, sentinel recorded");

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
            environment,
            sentinel,
            relative_path,
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
            if let Ok(status) =
                self.datastore
                    .did_run(pack, handler, sentinel_key(relative_path), content_hash)
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
                                "[dry-run] would skip (ran older version of {relative_path}; run `dodot up --provision-rerun` to apply the current one)"
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
                environment: environment.clone(),
                sentinel: sentinel.clone(),
                relative_path: relative_path.clone(),
            },
            format!("[dry-run] would execute: {}", cmd_str.trim()),
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::make_datastore;
    use super::super::Executor;
    use super::sentinel_key;
    use crate::datastore::CommandSpec;
    use crate::fs::Fs;
    use crate::operations::HandlerIntent;
    use crate::paths::Pather;
    use crate::testing::TempEnvironment;

    fn run_intent(
        pack: &str,
        handler: &str,
        executable: &str,
        args: &[&str],
        relative_path: &str,
        hash: &str,
    ) -> HandlerIntent {
        HandlerIntent::Run {
            pack: pack.into(),
            handler: handler.into(),
            executable: executable.into(),
            arguments: args.iter().map(|s| (*s).into()).collect(),
            environment: Vec::new(),
            sentinel: format!("{}-{hash}", sentinel_key(relative_path)),
            relative_path: relative_path.into(),
            content_hash: hash.into(),
        }
    }

    /// The seam, end to end through the executor: what a handler
    /// declared on the intent is what the spawn layer is handed.
    #[test]
    fn execute_run_hands_the_intents_environment_to_the_runner() {
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

        let mut intent = run_intent(
            "dev",
            "homebrew",
            "echo",
            &["bundle"],
            "Brewfile",
            "abc1234567890def",
        );
        let HandlerIntent::Run { environment, .. } = &mut intent else {
            unreachable!("run_intent builds a Run intent");
        };
        environment.push(("HOMEBREW_NO_AUTO_UPDATE".into(), "1".into()));

        let results = executor.execute(vec![intent]).unwrap();

        assert!(results[0].success);
        assert_eq!(
            runner.environments.lock().unwrap().as_slice(),
            &[vec![(
                "HOMEBREW_NO_AUTO_UPDATE".to_string(),
                "1".to_string()
            )]]
        );
    }

    #[test]
    fn execute_run_passes_no_environment_when_the_intent_declares_none() {
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

        executor
            .execute(vec![run_intent(
                "vim",
                "install",
                "echo",
                &["hello"],
                "install.sh",
                "abc1234567890def",
            )])
            .unwrap();

        assert_eq!(
            runner.environments.lock().unwrap().as_slice(),
            &[Vec::new()]
        );
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
            results[0].message.contains("--provision-rerun"),
            "the notice must name the flag that actually re-runs a run-once \
             handler; `--force` only overwrites pre-existing files at symlink \
             targets. msg: {}",
            results[0].message
        );
        assert!(
            runner.calls.lock().unwrap().is_empty(),
            "command must not run on older-version detection"
        );
    }

    /// `--dry-run` must name the same remedy the real run does. A
    /// preview that points at a different flag than the command it
    /// previews sends the user somewhere the real run won't honour.
    #[test]
    fn dry_run_older_version_notice_names_provision_rerun() {
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
            true, // dry_run
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
        assert!(
            results[0].message.contains("--provision-rerun"),
            "msg: {}",
            results[0].message
        );
        assert!(
            !results[0].message.contains("--force"),
            "msg: {}",
            results[0].message
        );
        assert!(
            runner.calls.lock().unwrap().is_empty(),
            "dry-run must not execute anything"
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

    // ── A failing command is contained ──────────────────────

    /// A runner whose commands all fail the way a real manager does:
    /// non-zero exit, with its own diagnostics on stderr.
    struct FailingRunner {
        stderr: String,
    }

    impl crate::datastore::CommandRunner for FailingRunner {
        fn run(&self, command: CommandSpec<'_>) -> crate::Result<crate::datastore::CommandOutput> {
            let CommandSpec {
                executable,
                arguments,
                ..
            } = command;
            Err(crate::DodotError::CommandFailed {
                command: crate::datastore::format_command_for_display(executable, arguments),
                exit_code: 3,
                stderr: self.stderr.clone(),
            })
        }
    }

    fn failing_datastore(
        env: &TempEnvironment,
        stderr: &str,
    ) -> crate::datastore::FilesystemDataStore {
        crate::datastore::FilesystemDataStore::new(
            env.fs.clone(),
            env.paths.clone(),
            std::sync::Arc::new(FailingRunner {
                stderr: stderr.into(),
            }),
        )
    }

    #[test]
    fn a_failed_command_is_a_failed_operation_carrying_the_managers_output() {
        let env = TempEnvironment::builder().build();
        let ds = failing_datastore(&env, "Error: Cask 'ghostty' is unavailable.");
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
                "tools",
                "homebrew",
                "brew",
                &["bundle", "--file", "/packs/tools/Brewfile"],
                "Brewfile",
                "abc1234567890def",
            )])
            .expect("a failing command must not error out of the executor");

        assert_eq!(results.len(), 1);
        assert!(!results[0].success, "msg: {}", results[0].message);
        assert!(
            results[0].message.contains("Cask 'ghostty' is unavailable"),
            "the manager's own output has to reach the row: {}",
            results[0].message
        );
        assert!(
            results[0].message.contains("exit code 3"),
            "msg: {}",
            results[0].message
        );
        env.assert_no_handler_state("tools", "homebrew");
    }

    /// Containment stops at the command. `run_and_record` writes the
    /// sentinel *after* the command has succeeded, so a datastore
    /// that cannot be written to surfaces here as an `Fs` error —
    /// dodot malfunctioning, not the user's script failing. It has to
    /// abort the run rather than become a failure row that claims the
    /// command failed when it did not.
    #[test]
    fn a_sentinel_write_failure_aborts_instead_of_becoming_a_failure_row() {
        let env = TempEnvironment::builder().build();
        let (ds, runner) = make_datastore(&env);

        // Occupy the sentinel's own path with a directory, so the
        // sentinel write fails after the command has already run.
        // `did_run` still reports NeverRan (it only considers files),
        // so the command is reached exactly as in a healthy run.
        let handler_dir = env.paths.handler_data_dir("vim", "install");
        env.fs
            .mkdir_all(&handler_dir.join("install.sh-abc1234567890def"))
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

        let err = executor
            .execute(vec![run_intent(
                "vim",
                "install",
                "echo",
                &["hello"],
                "install.sh",
                "abc1234567890def",
            )])
            .expect_err("a datastore that cannot record the run must abort execution");

        assert!(
            !matches!(err, crate::DodotError::CommandFailed { .. }),
            "a lost sentinel is not a failed command: {err}"
        );
        // The command itself ran fine — the failure is dodot's, after it.
        assert_eq!(runner.calls.lock().unwrap().as_slice(), &["echo hello"]);
    }

    /// The reason failure containment matters: provisioning runs
    /// before symlinks, `$PATH`, and shell init, so a `Brewfile` that
    /// fails used to cost its pack all three.
    #[test]
    fn work_after_a_failed_command_still_executes_and_still_reports() {
        let env = TempEnvironment::builder()
            .pack("tools")
            .file("vimrc", "set nocompatible")
            .done()
            .build();
        let ds = failing_datastore(&env, "boom");
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
            .execute(vec![
                run_intent(
                    "tools",
                    "install",
                    "bash",
                    &["--", "/packs/tools/install.sh"],
                    "install.sh",
                    "abc1234567890def",
                ),
                HandlerIntent::Link {
                    pack: "tools".into(),
                    handler: "symlink".into(),
                    source: env.dotfiles_root.join("tools/vimrc"),
                    user_path: env.home.join(".vimrc"),
                },
            ])
            .unwrap();

        assert_eq!(results.len(), 2);
        assert!(!results[0].success);
        assert!(
            results[1..].iter().all(|r| r.success),
            "the symlink must deploy despite the earlier failure: {results:?}"
        );
        env.assert_double_link(
            "tools",
            "symlink",
            "vimrc",
            &env.dotfiles_root.join("tools/vimrc"),
            &env.home.join(".vimrc"),
        );
    }
}

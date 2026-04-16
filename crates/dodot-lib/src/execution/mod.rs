//! Executor — converts [`HandlerIntent`]s into [`DataStore`] calls.
//!
//! The executor is where the complexity lives. Handlers just declare
//! what they want; the executor figures out how to make it happen.
//!
//! ## Auto-executable permissions
//!
//! When `auto_chmod_exec` is enabled (the default), the executor
//! ensures that files inside path-handler staged directories have
//! execute permissions (`+x`). This matches the user's intent: files
//! in `bin/` are there to be runnable, but execute bits can be lost
//! in common workflows (git on macOS, manual file creation).
//!
//! Permission failures are reported as warnings in the operation
//! results, not hard errors — the file is still staged and added to
//! `$PATH`, it just won't be directly runnable until the user fixes
//! permissions manually.

use crate::datastore::DataStore;
use crate::fs::Fs;
use crate::handlers::HANDLER_PATH;
use crate::operations::{HandlerIntent, Operation, OperationResult};
use crate::Result;

/// Executes handler intents by dispatching to the DataStore.
pub struct Executor<'a> {
    datastore: &'a dyn DataStore,
    fs: &'a dyn Fs,
    dry_run: bool,
    force: bool,
    provision_rerun: bool,
    auto_chmod_exec: bool,
}

impl<'a> Executor<'a> {
    pub fn new(
        datastore: &'a dyn DataStore,
        fs: &'a dyn Fs,
        dry_run: bool,
        force: bool,
        provision_rerun: bool,
        auto_chmod_exec: bool,
    ) -> Self {
        Self {
            datastore,
            fs,
            dry_run,
            force,
            provision_rerun,
            auto_chmod_exec,
        }
    }

    /// Execute a list of handler intents, returning one result per
    /// atomic operation performed.
    ///
    /// Conflicts (pre-existing files at target paths) are returned as
    /// failed `OperationResult`s — non-fatal, so other intents still
    /// execute. Hard errors (I/O failures, command failures) stop
    /// execution immediately via `?`.
    /// In dry-run mode, all intents are simulated regardless of errors.
    pub fn execute(&self, intents: Vec<HandlerIntent>) -> Result<Vec<OperationResult>> {
        let mut results = Vec::new();

        for intent in intents {
            let intent_results = if self.dry_run {
                self.simulate(&intent)
            } else {
                self.execute_one(&intent)?
            };
            results.extend(intent_results);
        }

        Ok(results)
    }

    /// Execute a single intent, which may produce multiple operations.
    fn execute_one(&self, intent: &HandlerIntent) -> Result<Vec<OperationResult>> {
        match intent {
            HandlerIntent::Link {
                pack,
                handler,
                source,
                user_path,
            } => {
                // Pre-check: does a non-symlink file exist at user_path?
                // We check BEFORE creating the data link to avoid leaving
                // dangling state when the user link would fail.
                if !self.fs.is_symlink(user_path) && self.fs.exists(user_path) {
                    if self.force {
                        // Remove the existing path before creating the symlink
                        if self.fs.is_dir(user_path) {
                            self.fs.remove_dir_all(user_path)?;
                        } else {
                            self.fs.remove_file(user_path)?;
                        }
                    } else {
                        // Return a failed result — non-fatal so other files
                        // in the pack can still be processed.
                        let op = Operation::CreateUserLink {
                            pack: pack.clone(),
                            handler: handler.clone(),
                            datastore_path: Default::default(),
                            user_path: user_path.clone(),
                        };
                        return Ok(vec![OperationResult::fail(
                            op,
                            format!(
                                "conflict: {} already exists (use --force to overwrite)",
                                user_path.display()
                            ),
                        )]);
                    }
                }

                // Step 1: Create data link (source → datastore)
                let datastore_path = self.datastore.create_data_link(pack, handler, source)?;

                // Step 2: Create user link (datastore → user location)
                self.datastore
                    .create_user_link(&datastore_path, user_path)?;

                let op = Operation::CreateUserLink {
                    pack: pack.clone(),
                    handler: handler.clone(),
                    datastore_path: datastore_path.clone(),
                    user_path: user_path.clone(),
                };

                Ok(vec![OperationResult::ok(
                    op,
                    format!(
                        "{} → {}",
                        source.file_name().unwrap_or_default().to_string_lossy(),
                        user_path.display()
                    ),
                )])
            }

            HandlerIntent::Stage {
                pack,
                handler,
                source,
            } => {
                self.datastore.create_data_link(pack, handler, source)?;

                let op = Operation::CreateDataLink {
                    pack: pack.clone(),
                    handler: handler.clone(),
                    source: source.clone(),
                };

                let mut results = vec![OperationResult::ok(
                    op,
                    format!(
                        "staged {}",
                        source.file_name().unwrap_or_default().to_string_lossy(),
                    ),
                )];

                // Auto-chmod +x for path handler directories
                if handler == HANDLER_PATH && self.auto_chmod_exec {
                    results.extend(self.ensure_executable(pack, source));
                }

                Ok(results)
            }

            HandlerIntent::Run {
                pack,
                handler,
                executable,
                arguments,
                sentinel,
            } => {
                // Check sentinel first — unless provision_rerun is set
                if !self.provision_rerun {
                    let already_done = self.datastore.has_sentinel(pack, handler, sentinel)?;

                    if already_done {
                        let op = Operation::CheckSentinel {
                            pack: pack.clone(),
                            handler: handler.clone(),
                            sentinel: sentinel.clone(),
                        };
                        return Ok(vec![OperationResult::ok(op, "already completed")]);
                    }
                }

                // Run the command
                self.datastore.run_and_record(
                    pack,
                    handler,
                    executable,
                    arguments,
                    sentinel,
                    self.provision_rerun,
                )?;

                let cmd_str = format!("{} {}", executable, arguments.join(" "));

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
        }
    }

    /// Ensure all files in a path-handler directory are executable.
    ///
    /// Iterates files in `dir`, checks each for the execute bit, and
    /// adds it if missing. Returns one `OperationResult` per file that
    /// was made executable (or that failed). Files that are already
    /// executable produce no output.
    ///
    /// Permission failures are non-fatal: they are reported as
    /// *successful* operations with a warning message, so they don't
    /// flip the pack to "failed" status. The file is still staged and
    /// visible in `$PATH`, it just won't be runnable until the user
    /// fixes permissions manually.
    fn ensure_executable(&self, pack: &str, dir: &std::path::Path) -> Vec<OperationResult> {
        let mut results = Vec::new();
        let entries = match self.fs.read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                let op = Operation::CreateDataLink {
                    pack: pack.into(),
                    handler: HANDLER_PATH.into(),
                    source: dir.to_path_buf(),
                };
                results.push(OperationResult::ok(
                    op,
                    format!(
                        "warning: could not list {} for auto-chmod: {}",
                        dir.display(),
                        e
                    ),
                ));
                return results;
            }
        };

        for entry in entries {
            if !entry.is_file {
                continue;
            }
            let meta = match self.fs.stat(&entry.path) {
                Ok(m) => m,
                Err(e) => {
                    let op = Operation::CreateDataLink {
                        pack: pack.into(),
                        handler: HANDLER_PATH.into(),
                        source: entry.path.clone(),
                    };
                    results.push(OperationResult::ok(
                        op,
                        format!("warning: could not stat {}: {}", entry.name, e),
                    ));
                    continue;
                }
            };

            let is_exec = meta.mode & 0o111 != 0;
            if is_exec {
                continue;
            }

            // Add user/group/other execute bits, preserving existing permissions.
            let new_mode = meta.mode | 0o111;
            let op = Operation::CreateDataLink {
                pack: pack.into(),
                handler: HANDLER_PATH.into(),
                source: entry.path.clone(),
            };

            match self.fs.set_permissions(&entry.path, new_mode) {
                Ok(()) => {
                    results.push(OperationResult::ok(op, format!("chmod +x {}", entry.name)));
                }
                Err(e) => {
                    // Warning, not failure — don't mark the pack as failed
                    // just because chmod didn't work.
                    results.push(OperationResult::ok(
                        op,
                        format!("warning: could not chmod +x {}: {}", entry.name, e),
                    ));
                }
            }
        }

        results
    }

    /// Report files in a path-handler directory that lack execute
    /// permissions (dry-run mode — no mutations).
    fn report_non_executable(&self, pack: &str, dir: &std::path::Path) -> Vec<OperationResult> {
        let mut results = Vec::new();
        let entries = match self.fs.read_dir(dir) {
            Ok(e) => e,
            Err(_) => return results,
        };

        for entry in entries {
            if !entry.is_file {
                continue;
            }
            let meta = match self.fs.stat(&entry.path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let is_exec = meta.mode & 0o111 != 0;
            if !is_exec {
                let op = Operation::CreateDataLink {
                    pack: pack.into(),
                    handler: HANDLER_PATH.into(),
                    source: entry.path.clone(),
                };
                results.push(OperationResult::ok(
                    op,
                    format!("[dry-run] would chmod +x {}", entry.name),
                ));
            }
        }

        results
    }

    /// Simulate an intent without touching the filesystem.
    fn simulate(&self, intent: &HandlerIntent) -> Vec<OperationResult> {
        match intent {
            HandlerIntent::Link {
                pack,
                handler,
                source,
                user_path,
            } => {
                // Check for conflicts even in dry-run
                if !self.fs.is_symlink(user_path) && self.fs.exists(user_path) {
                    if self.force {
                        return vec![OperationResult::ok(
                            Operation::CreateUserLink {
                                pack: pack.clone(),
                                handler: handler.clone(),
                                datastore_path: Default::default(),
                                user_path: user_path.clone(),
                            },
                            format!(
                                "[dry-run] would overwrite {} → {}",
                                source.file_name().unwrap_or_default().to_string_lossy(),
                                user_path.display()
                            ),
                        )];
                    } else {
                        return vec![OperationResult::fail(
                            Operation::CreateUserLink {
                                pack: pack.clone(),
                                handler: handler.clone(),
                                datastore_path: Default::default(),
                                user_path: user_path.clone(),
                            },
                            format!(
                                "conflict: {} already exists (use --force to overwrite)",
                                user_path.display()
                            ),
                        )];
                    }
                }

                vec![OperationResult::ok(
                    Operation::CreateUserLink {
                        pack: pack.clone(),
                        handler: handler.clone(),
                        datastore_path: Default::default(),
                        user_path: user_path.clone(),
                    },
                    format!(
                        "[dry-run] would link {} → {}",
                        source.file_name().unwrap_or_default().to_string_lossy(),
                        user_path.display()
                    ),
                )]
            }

            HandlerIntent::Stage {
                pack,
                handler,
                source,
            } => {
                let mut results = vec![OperationResult::ok(
                    Operation::CreateDataLink {
                        pack: pack.clone(),
                        handler: handler.clone(),
                        source: source.clone(),
                    },
                    format!(
                        "[dry-run] would stage: {}",
                        source.file_name().unwrap_or_default().to_string_lossy()
                    ),
                )];

                if handler == HANDLER_PATH && self.auto_chmod_exec {
                    results.extend(self.report_non_executable(pack, source));
                }

                results
            }

            HandlerIntent::Run {
                pack,
                handler,
                executable,
                arguments,
                sentinel,
            } => {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datastore::{CommandOutput, CommandRunner, FilesystemDataStore};
    use crate::paths::Pather;
    use crate::testing::TempEnvironment;
    use std::sync::{Arc, Mutex};

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

    fn make_datastore(env: &TempEnvironment) -> (FilesystemDataStore, Arc<MockCommandRunner>) {
        let runner = Arc::new(MockCommandRunner::new());
        let ds = FilesystemDataStore::new(env.fs.clone(), env.paths.clone(), runner.clone());
        (ds, runner)
    }

    #[test]
    fn execute_link_creates_double_link() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(&ds, env.fs.as_ref(), false, false, false, true);

        let source = env.dotfiles_root.join("vim/vimrc");
        let user_path = env.home.join(".vimrc");

        let results = executor
            .execute(vec![HandlerIntent::Link {
                pack: "vim".into(),
                handler: "symlink".into(),
                source: source.clone(),
                user_path: user_path.clone(),
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].success);

        // Verify the double-link chain
        env.assert_double_link("vim", "symlink", "vimrc", &source, &user_path);
    }

    #[test]
    fn execute_link_conflict_returns_failed_result() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .done()
            .home_file(".vimrc", "existing content")
            .build();
        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(&ds, env.fs.as_ref(), false, false, false, true);

        let source = env.dotfiles_root.join("vim/vimrc");
        let user_path = env.home.join(".vimrc");

        let results = executor
            .execute(vec![HandlerIntent::Link {
                pack: "vim".into(),
                handler: "symlink".into(),
                source: source.clone(),
                user_path: user_path.clone(),
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(!results[0].success, "should report conflict");
        assert!(
            results[0].message.contains("conflict"),
            "msg: {}",
            results[0].message
        );
        assert!(
            results[0].message.contains("--force"),
            "msg: {}",
            results[0].message
        );

        // Data link should NOT have been created (pre-check prevents it)
        env.assert_no_handler_state("vim", "symlink");

        // Original file should be untouched
        env.assert_file_contents(&user_path, "existing content");
    }

    #[test]
    fn execute_link_force_overwrites_existing_file() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .done()
            .home_file(".vimrc", "existing content")
            .build();
        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(&ds, env.fs.as_ref(), false, true, false, true);

        let source = env.dotfiles_root.join("vim/vimrc");
        let user_path = env.home.join(".vimrc");

        let results = executor
            .execute(vec![HandlerIntent::Link {
                pack: "vim".into(),
                handler: "symlink".into(),
                source: source.clone(),
                user_path: user_path.clone(),
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].success, "force should succeed");

        // Verify the double-link chain was created
        env.assert_double_link("vim", "symlink", "vimrc", &source, &user_path);

        // Content should now be from the pack
        let content = env.fs.read_to_string(&user_path).unwrap();
        assert_eq!(content, "set nocompatible");
    }

    #[test]
    fn execute_link_conflict_does_not_block_other_intents() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .file("gvimrc", "set guifont=Mono")
            .done()
            .home_file(".vimrc", "existing content")
            .build();
        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(&ds, env.fs.as_ref(), false, false, false, true);

        let results = executor
            .execute(vec![
                HandlerIntent::Link {
                    pack: "vim".into(),
                    handler: "symlink".into(),
                    source: env.dotfiles_root.join("vim/vimrc"),
                    user_path: env.home.join(".vimrc"),
                },
                HandlerIntent::Link {
                    pack: "vim".into(),
                    handler: "symlink".into(),
                    source: env.dotfiles_root.join("vim/gvimrc"),
                    user_path: env.home.join(".gvimrc"),
                },
            ])
            .unwrap();

        assert_eq!(results.len(), 2);
        // First should fail (conflict)
        assert!(!results[0].success);
        // Second should succeed (no conflict)
        assert!(results[1].success);

        // gvimrc should be deployed despite vimrc conflict
        env.assert_double_link(
            "vim",
            "symlink",
            "gvimrc",
            &env.dotfiles_root.join("vim/gvimrc"),
            &env.home.join(".gvimrc"),
        );
    }

    #[test]
    fn execute_stage_creates_data_link_only() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "alias vi=vim")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(&ds, env.fs.as_ref(), false, false, false, true);

        let source = env.dotfiles_root.join("vim/aliases.sh");

        let results = executor
            .execute(vec![HandlerIntent::Stage {
                pack: "vim".into(),
                handler: "shell".into(),
                source: source.clone(),
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].success);

        // Data link should exist
        let datastore_link = env
            .paths
            .handler_data_dir("vim", "shell")
            .join("aliases.sh");
        env.assert_symlink(&datastore_link, &source);
    }

    #[test]
    fn execute_run_creates_sentinel() {
        let env = TempEnvironment::builder().build();
        let (ds, runner) = make_datastore(&env);
        let executor = Executor::new(&ds, env.fs.as_ref(), false, false, false, true);

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

        let executor = Executor::new(&ds, env.fs.as_ref(), false, false, false, true);
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

        let executor = Executor::new(&ds, env.fs.as_ref(), false, false, true, true);
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

    #[test]
    fn dry_run_does_not_modify_filesystem() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(&ds, env.fs.as_ref(), true, false, false, true);

        let results = executor
            .execute(vec![
                HandlerIntent::Link {
                    pack: "vim".into(),
                    handler: "symlink".into(),
                    source: env.dotfiles_root.join("vim/vimrc"),
                    user_path: env.home.join(".vimrc"),
                },
                HandlerIntent::Stage {
                    pack: "vim".into(),
                    handler: "shell".into(),
                    source: env.dotfiles_root.join("vim/vimrc"),
                },
                HandlerIntent::Run {
                    pack: "vim".into(),
                    handler: "install".into(),
                    executable: "echo".into(),
                    arguments: vec!["hi".into()],
                    sentinel: "s1".into(),
                },
            ])
            .unwrap();

        // All should succeed with dry-run messages
        assert_eq!(results.len(), 3); // Link=1, Stage=1, Run=1
        for r in &results {
            assert!(r.success);
            assert!(r.message.contains("[dry-run]"), "msg: {}", r.message);
        }

        // Nothing should have been created
        env.assert_not_exists(&env.home.join(".vimrc"));
        env.assert_no_handler_state("vim", "symlink");
        env.assert_no_handler_state("vim", "shell");
        env.assert_no_handler_state("vim", "install");
    }

    #[test]
    fn dry_run_detects_conflict() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .home_file(".vimrc", "existing")
            .build();
        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(&ds, env.fs.as_ref(), true, false, false, true);

        let results = executor
            .execute(vec![HandlerIntent::Link {
                pack: "vim".into(),
                handler: "symlink".into(),
                source: env.dotfiles_root.join("vim/vimrc"),
                user_path: env.home.join(".vimrc"),
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(results[0].message.contains("conflict"));
    }

    #[test]
    fn execute_multiple_intents_sequentially() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .file("gvimrc", "set guifont=Mono")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(&ds, env.fs.as_ref(), false, false, false, true);

        let results = executor
            .execute(vec![
                HandlerIntent::Link {
                    pack: "vim".into(),
                    handler: "symlink".into(),
                    source: env.dotfiles_root.join("vim/vimrc"),
                    user_path: env.home.join(".vimrc"),
                },
                HandlerIntent::Link {
                    pack: "vim".into(),
                    handler: "symlink".into(),
                    source: env.dotfiles_root.join("vim/gvimrc"),
                    user_path: env.home.join(".gvimrc"),
                },
            ])
            .unwrap();

        assert_eq!(results.len(), 2); // 1 op per link
        assert!(results.iter().all(|r| r.success));

        env.assert_double_link(
            "vim",
            "symlink",
            "vimrc",
            &env.dotfiles_root.join("vim/vimrc"),
            &env.home.join(".vimrc"),
        );
        env.assert_double_link(
            "vim",
            "symlink",
            "gvimrc",
            &env.dotfiles_root.join("vim/gvimrc"),
            &env.home.join(".gvimrc"),
        );
    }

    // ── Auto-chmod +x for path handler ─────────────────────────

    #[test]
    fn path_stage_adds_execute_permission() {
        let env = TempEnvironment::builder()
            .pack("tools")
            .file("bin/mytool", "#!/bin/sh\necho hello")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);

        // Verify the file starts without execute permission
        let tool_path = env.dotfiles_root.join("tools/bin/mytool");
        let meta_before = env.fs.stat(&tool_path).unwrap();
        assert_eq!(
            meta_before.mode & 0o111,
            0,
            "file should start non-executable"
        );

        let executor = Executor::new(&ds, env.fs.as_ref(), false, false, false, true);
        let results = executor
            .execute(vec![HandlerIntent::Stage {
                pack: "tools".into(),
                handler: "path".into(),
                source: env.dotfiles_root.join("tools/bin"),
            }])
            .unwrap();

        // Should have the stage result + chmod result
        assert!(results.len() >= 2, "results: {results:?}");
        let chmod_result = results.iter().find(|r| r.message.contains("chmod +x"));
        assert!(
            chmod_result.is_some(),
            "should have a chmod +x result: {results:?}"
        );
        assert!(chmod_result.unwrap().success);

        // Verify file is now executable
        let meta_after = env.fs.stat(&tool_path).unwrap();
        assert_ne!(
            meta_after.mode & 0o111,
            0,
            "file should be executable after up"
        );
    }

    #[test]
    fn path_stage_skips_already_executable() {
        let env = TempEnvironment::builder()
            .pack("tools")
            .file("bin/mytool", "#!/bin/sh\necho hello")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);

        // Pre-set execute permission
        let tool_path = env.dotfiles_root.join("tools/bin/mytool");
        env.fs.set_permissions(&tool_path, 0o755).unwrap();

        let executor = Executor::new(&ds, env.fs.as_ref(), false, false, false, true);
        let results = executor
            .execute(vec![HandlerIntent::Stage {
                pack: "tools".into(),
                handler: "path".into(),
                source: env.dotfiles_root.join("tools/bin"),
            }])
            .unwrap();

        // Should only have the stage result — no chmod needed
        let chmod_results: Vec<_> = results
            .iter()
            .filter(|r| r.message.contains("chmod"))
            .collect();
        assert!(
            chmod_results.is_empty(),
            "already-executable file should not produce chmod result: {chmod_results:?}"
        );
    }

    #[test]
    fn path_stage_auto_chmod_disabled() {
        let env = TempEnvironment::builder()
            .pack("tools")
            .file("bin/mytool", "#!/bin/sh\necho hello")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);

        // auto_chmod_exec = false
        let executor = Executor::new(&ds, env.fs.as_ref(), false, false, false, false);
        let results = executor
            .execute(vec![HandlerIntent::Stage {
                pack: "tools".into(),
                handler: "path".into(),
                source: env.dotfiles_root.join("tools/bin"),
            }])
            .unwrap();

        // Should only have the stage result — no chmod attempted
        let chmod_results: Vec<_> = results
            .iter()
            .filter(|r| r.message.contains("chmod"))
            .collect();
        assert!(
            chmod_results.is_empty(),
            "auto_chmod_exec=false should skip chmod: {chmod_results:?}"
        );

        // File should remain non-executable
        let tool_path = env.dotfiles_root.join("tools/bin/mytool");
        let meta = env.fs.stat(&tool_path).unwrap();
        assert_eq!(meta.mode & 0o111, 0, "file should remain non-executable");
    }

    #[test]
    fn path_stage_skips_directories() {
        let env = TempEnvironment::builder()
            .pack("tools")
            .file("bin/subdir/nested", "#!/bin/sh")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);

        let executor = Executor::new(&ds, env.fs.as_ref(), false, false, false, true);
        let results = executor
            .execute(vec![HandlerIntent::Stage {
                pack: "tools".into(),
                handler: "path".into(),
                source: env.dotfiles_root.join("tools/bin"),
            }])
            .unwrap();

        // The chmod should only apply to files, not the subdir directory
        let chmod_results: Vec<_> = results
            .iter()
            .filter(|r| r.message.contains("chmod"))
            .collect();
        // subdir is a directory, not a file — should not be chmod'd
        for r in &chmod_results {
            assert!(
                !r.message.contains("subdir"),
                "directories should not be chmod'd: {}",
                r.message
            );
        }
    }

    #[test]
    fn shell_stage_does_not_auto_chmod() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "alias vi=vim")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);

        let executor = Executor::new(&ds, env.fs.as_ref(), false, false, false, true);
        let results = executor
            .execute(vec![HandlerIntent::Stage {
                pack: "vim".into(),
                handler: "shell".into(),
                source: env.dotfiles_root.join("vim/aliases.sh"),
            }])
            .unwrap();

        let chmod_results: Vec<_> = results
            .iter()
            .filter(|r| r.message.contains("chmod"))
            .collect();
        assert!(
            chmod_results.is_empty(),
            "shell handler should not auto-chmod: {chmod_results:?}"
        );
    }

    #[test]
    fn dry_run_reports_non_executable_without_modifying() {
        let env = TempEnvironment::builder()
            .pack("tools")
            .file("bin/mytool", "#!/bin/sh\necho hello")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);

        let executor = Executor::new(&ds, env.fs.as_ref(), true, false, false, true);
        let results = executor
            .execute(vec![HandlerIntent::Stage {
                pack: "tools".into(),
                handler: "path".into(),
                source: env.dotfiles_root.join("tools/bin"),
            }])
            .unwrap();

        // Should report what would be chmod'd
        let chmod_results: Vec<_> = results
            .iter()
            .filter(|r| r.message.contains("chmod"))
            .collect();
        assert!(
            !chmod_results.is_empty(),
            "dry-run should report non-executable files"
        );
        assert!(chmod_results[0].message.contains("[dry-run]"));

        // File should NOT have been modified
        let tool_path = env.dotfiles_root.join("tools/bin/mytool");
        let meta = env.fs.stat(&tool_path).unwrap();
        assert_eq!(
            meta.mode & 0o111,
            0,
            "dry-run should not modify permissions"
        );
    }

    #[test]
    fn path_stage_auto_chmod_multiple_files() {
        let env = TempEnvironment::builder()
            .pack("tools")
            .file("bin/tool-a", "#!/bin/sh\necho a")
            .file("bin/tool-b", "#!/bin/sh\necho b")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);

        let executor = Executor::new(&ds, env.fs.as_ref(), false, false, false, true);
        let results = executor
            .execute(vec![HandlerIntent::Stage {
                pack: "tools".into(),
                handler: "path".into(),
                source: env.dotfiles_root.join("tools/bin"),
            }])
            .unwrap();

        let chmod_results: Vec<_> = results
            .iter()
            .filter(|r| r.message.contains("chmod +x"))
            .collect();
        assert_eq!(
            chmod_results.len(),
            2,
            "should chmod both files: {chmod_results:?}"
        );

        // Both files should be executable
        for name in ["tool-a", "tool-b"] {
            let path = env.dotfiles_root.join(format!("tools/bin/{name}"));
            let meta = env.fs.stat(&path).unwrap();
            assert_ne!(meta.mode & 0o111, 0, "{name} should be executable");
        }
    }
}

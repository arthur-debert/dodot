//! Executor — converts [`HandlerIntent`]s into [`DataStore`] calls.
//!
//! The executor is where the complexity lives. Handlers just declare
//! what they want; the executor figures out how to make it happen.

use crate::datastore::DataStore;
use crate::fs::Fs;
use crate::operations::{HandlerIntent, Operation, OperationResult};
use crate::Result;

/// Executes handler intents by dispatching to the DataStore.
pub struct Executor<'a> {
    datastore: &'a dyn DataStore,
    fs: &'a dyn Fs,
    dry_run: bool,
    force: bool,
    provision_rerun: bool,
}

impl<'a> Executor<'a> {
    pub fn new(
        datastore: &'a dyn DataStore,
        fs: &'a dyn Fs,
        dry_run: bool,
        force: bool,
        provision_rerun: bool,
    ) -> Self {
        Self {
            datastore,
            fs,
            dry_run,
            force,
            provision_rerun,
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

                Ok(vec![OperationResult::ok(
                    op,
                    format!(
                        "staged {}",
                        source.file_name().unwrap_or_default().to_string_lossy(),
                    ),
                )])
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

                // Remove existing sentinel so run_and_record will re-run
                if self.provision_rerun {
                    let sentinel_path = self.datastore.sentinel_path(pack, handler, sentinel);
                    if self.fs.exists(&sentinel_path) {
                        self.fs.remove_file(&sentinel_path)?;
                    }
                }

                // Run the command
                self.datastore
                    .run_and_record(pack, handler, executable, arguments, sentinel)?;

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
                vec![OperationResult::ok(
                    Operation::CreateDataLink {
                        pack: pack.clone(),
                        handler: handler.clone(),
                        source: source.clone(),
                    },
                    format!(
                        "[dry-run] would stage: {}",
                        source.file_name().unwrap_or_default().to_string_lossy()
                    ),
                )]
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
        let executor = Executor::new(&ds, env.fs.as_ref(), false, false, false);

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
        let executor = Executor::new(&ds, env.fs.as_ref(), false, false, false);

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
        let executor = Executor::new(&ds, env.fs.as_ref(), false, true, false);

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
        let executor = Executor::new(&ds, env.fs.as_ref(), false, false, false);

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
        let executor = Executor::new(&ds, env.fs.as_ref(), false, false, false);

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
        let executor = Executor::new(&ds, env.fs.as_ref(), false, false, false);

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

        let executor = Executor::new(&ds, env.fs.as_ref(), false, false, false);
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

        let executor = Executor::new(&ds, env.fs.as_ref(), false, false, true);
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
        let executor = Executor::new(&ds, env.fs.as_ref(), true, false, false);

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
        let executor = Executor::new(&ds, env.fs.as_ref(), true, false, false);

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
        let executor = Executor::new(&ds, env.fs.as_ref(), false, false, false);

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
}

//! Executor — converts [`HandlerIntent`]s into [`DataStore`] calls.
//!
//! The executor is where the complexity lives. Handlers just declare
//! what they want; the executor figures out how to make it happen.

use crate::datastore::DataStore;
use crate::operations::{HandlerIntent, Operation, OperationResult};
use crate::Result;

/// Executes handler intents by dispatching to the DataStore.
pub struct Executor<'a> {
    datastore: &'a dyn DataStore,
    dry_run: bool,
}

impl<'a> Executor<'a> {
    pub fn new(datastore: &'a dyn DataStore, dry_run: bool) -> Self {
        Self { datastore, dry_run }
    }

    /// Execute a list of handler intents, returning one result per
    /// atomic operation performed.
    ///
    /// In normal mode, execution stops on the first error.
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
                // Step 1: Create data link (source → datastore)
                let datastore_path = self.datastore.create_data_link(pack, handler, source)?;

                let op1 = Operation::CreateDataLink {
                    pack: pack.clone(),
                    handler: handler.clone(),
                    source: source.clone(),
                };

                // Step 2: Create user link (datastore → user location)
                self.datastore
                    .create_user_link(&datastore_path, user_path)?;

                let op2 = Operation::CreateUserLink {
                    pack: pack.clone(),
                    handler: handler.clone(),
                    datastore_path: datastore_path.clone(),
                    user_path: user_path.clone(),
                };

                Ok(vec![
                    OperationResult::ok(op1, format!("data link created: {}", source.display())),
                    OperationResult::ok(
                        op2,
                        format!(
                            "linked {} → {}",
                            user_path.display(),
                            datastore_path.display()
                        ),
                    ),
                ])
            }

            HandlerIntent::Stage {
                pack,
                handler,
                source,
            } => {
                let datastore_path = self.datastore.create_data_link(pack, handler, source)?;

                let op = Operation::CreateDataLink {
                    pack: pack.clone(),
                    handler: handler.clone(),
                    source: source.clone(),
                };

                Ok(vec![OperationResult::ok(
                    op,
                    format!(
                        "staged: {} → {}",
                        source.display(),
                        datastore_path.display()
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
                // Check sentinel first
                let already_done = self.datastore.has_sentinel(pack, handler, sentinel)?;

                if already_done {
                    let op = Operation::CheckSentinel {
                        pack: pack.clone(),
                        handler: handler.clone(),
                        sentinel: sentinel.clone(),
                    };
                    return Ok(vec![OperationResult::ok(op, "already completed")]);
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
                vec![
                    OperationResult::ok(
                        Operation::CreateDataLink {
                            pack: pack.clone(),
                            handler: handler.clone(),
                            source: source.clone(),
                        },
                        format!("[dry-run] would create data link: {}", source.display()),
                    ),
                    OperationResult::ok(
                        Operation::CreateUserLink {
                            pack: pack.clone(),
                            handler: handler.clone(),
                            datastore_path: Default::default(),
                            user_path: user_path.clone(),
                        },
                        format!("[dry-run] would link {} → datastore", user_path.display()),
                    ),
                ]
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
                    format!("[dry-run] would stage: {}", source.display()),
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
    use crate::fs::Fs;
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
        let executor = Executor::new(&ds, false);

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

        assert_eq!(results.len(), 2);
        assert!(results[0].success);
        assert!(results[1].success);

        // Verify the double-link chain
        env.assert_double_link("vim", "symlink", "vimrc", &source, &user_path);
    }

    #[test]
    fn execute_stage_creates_data_link_only() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "alias vi=vim")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(&ds, false);

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
        let executor = Executor::new(&ds, false);

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

        let executor = Executor::new(&ds, false);
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
    fn dry_run_does_not_modify_filesystem() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(&ds, true);

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
        assert_eq!(results.len(), 4); // Link=2 ops, Stage=1, Run=1
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
    fn execute_multiple_intents_sequentially() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .file("gvimrc", "set guifont=Mono")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(&ds, false);

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

        assert_eq!(results.len(), 4); // 2 ops per link
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

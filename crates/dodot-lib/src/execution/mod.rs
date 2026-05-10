//! Executor — converts [`HandlerIntent`]s into [`DataStore`] calls.
//!
//! The executor is where the complexity lives. Handlers just declare
//! what they want; the executor figures out how to make it happen.
//!
//! Per-intent logic lives in sibling files: [`mod@link`] for symlink
//! deployment (with ancestor-cycle and conflict handling), [`mod@stage`]
//! for datastore staging (with auto-chmod for path handler bins), and
//! [`mod@run`] for sentinel-gated command execution. This file owns the
//! Executor struct, the per-call `execute()` entry point, and the
//! match-based dispatchers (`execute_one`, `simulate`).
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

mod link;
mod run;
mod stage;

use tracing::debug;

use crate::datastore::DataStore;
use crate::fs::Fs;
use crate::operations::{HandlerIntent, OperationResult};
use crate::paths::Pather;
use crate::Result;

/// Executes handler intents by dispatching to the DataStore.
pub struct Executor<'a> {
    datastore: &'a dyn DataStore,
    fs: &'a dyn Fs,
    paths: &'a dyn Pather,
    dry_run: bool,
    force: bool,
    provision_rerun: bool,
    auto_chmod_exec: bool,
}

impl<'a> Executor<'a> {
    pub fn new(
        datastore: &'a dyn DataStore,
        fs: &'a dyn Fs,
        paths: &'a dyn Pather,
        dry_run: bool,
        force: bool,
        provision_rerun: bool,
        auto_chmod_exec: bool,
    ) -> Self {
        Self {
            datastore,
            fs,
            paths,
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
        debug!(
            count = intents.len(),
            dry_run = self.dry_run,
            force = self.force,
            "executor starting"
        );
        let mut results = Vec::new();

        for intent in intents {
            let intent_results = if self.dry_run {
                self.simulate(&intent)
            } else {
                self.execute_one(&intent)?
            };
            results.extend(intent_results);
        }

        let succeeded = results.iter().filter(|r| r.success).count();
        let failed = results.iter().filter(|r| !r.success).count();
        debug!(succeeded, failed, "executor finished");

        Ok(results)
    }

    /// Execute a single intent, which may produce multiple operations.
    fn execute_one(&self, intent: &HandlerIntent) -> Result<Vec<OperationResult>> {
        match intent {
            HandlerIntent::Link { .. } => self.execute_link(intent),
            HandlerIntent::Stage { .. } => self.execute_stage(intent),
            HandlerIntent::Run { .. } => self.execute_run(intent),
        }
    }

    /// Simulate an intent without touching the filesystem.
    fn simulate(&self, intent: &HandlerIntent) -> Vec<OperationResult> {
        match intent {
            HandlerIntent::Link { .. } => self.simulate_link(intent),
            HandlerIntent::Stage { .. } => self.simulate_stage(intent),
            HandlerIntent::Run { .. } => self.simulate_run(intent),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datastore::{CommandOutput, CommandRunner, FilesystemDataStore};
    use crate::paths::Pather;
    use crate::testing::TempEnvironment;
    use std::path::Path;
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
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        );

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
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        );

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
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            true,
            false,
            true,
        );

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
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        );

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

    #[test]
    fn dry_run_does_not_modify_filesystem() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            true,
            false,
            false,
            true,
        );

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
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            true,
            false,
            false,
            true,
        );

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
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            false,
        );
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

        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            true,
            false,
            false,
            true,
        );
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

    // ── Ancestor-cycle detection ────────────────────────────────────

    /// If an ancestor of `user_path` is a symlink into the pack store,
    /// deploy must refuse before touching anything — otherwise the
    /// write goes through the ancestor and lands inside the pack,
    /// clobbering source files or building a pack↔data-dir cycle.
    #[test]
    fn link_refuses_when_user_path_parent_symlinks_into_pack() {
        let env = TempEnvironment::builder()
            .pack("warp")
            .file("keybindings.yaml", "keep me")
            .done()
            .build();
        // Legacy setup: ~/.config/warp is a symlink into the pack itself.
        let pack_dir = env.dotfiles_root.join("warp");
        let config_warp = env.config_home.join("warp");
        env.fs.mkdir_all(&env.config_home).unwrap();
        env.fs.symlink(&pack_dir, &config_warp).unwrap();

        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        );

        let source = pack_dir.join("keybindings.yaml");
        let user_path = config_warp.join("keybindings.yaml");

        let results = executor
            .execute(vec![HandlerIntent::Link {
                pack: "warp".into(),
                handler: "symlink".into(),
                source: source.clone(),
                user_path: user_path.clone(),
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(!results[0].success, "expected failure, got: {:?}", results);
        assert!(
            results[0].message.contains("cycle"),
            "expected cycle message, got: {}",
            results[0].message
        );

        // No data link created, source file untouched.
        env.assert_no_handler_state("warp", "symlink");
        env.assert_file_contents(&source, "keep me");
    }

    /// Same check but the ancestor points into `data_dir`. Writing
    /// through it would land in the datastore and still wedge the
    /// system.
    #[test]
    fn link_refuses_when_user_path_parent_symlinks_into_data_dir() {
        let env = TempEnvironment::builder()
            .pack("warp")
            .file("keybindings.yaml", "keep me")
            .done()
            .build();
        let config_warp = env.config_home.join("warp");
        env.fs.mkdir_all(&env.config_home).unwrap();
        env.fs.mkdir_all(&env.data_dir).unwrap();
        env.fs.symlink(&env.data_dir, &config_warp).unwrap();

        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        );

        let source = env.dotfiles_root.join("warp/keybindings.yaml");
        let user_path = config_warp.join("keybindings.yaml");

        let results = executor
            .execute(vec![HandlerIntent::Link {
                pack: "warp".into(),
                handler: "symlink".into(),
                source: source.clone(),
                user_path: user_path.clone(),
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(results[0].message.contains("cycle"));
        env.assert_no_handler_state("warp", "symlink");
    }

    /// Dry-run must surface the same error, not silently report
    /// "would link".
    #[test]
    fn simulate_link_reports_ancestor_cycle() {
        let env = TempEnvironment::builder()
            .pack("warp")
            .file("keybindings.yaml", "keep me")
            .done()
            .build();
        let pack_dir = env.dotfiles_root.join("warp");
        let config_warp = env.config_home.join("warp");
        env.fs.mkdir_all(&env.config_home).unwrap();
        env.fs.symlink(&pack_dir, &config_warp).unwrap();

        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            true, // dry_run
            false,
            false,
            true,
        );

        let source = pack_dir.join("keybindings.yaml");
        let user_path = config_warp.join("keybindings.yaml");

        let results = executor
            .execute(vec![HandlerIntent::Link {
                pack: "warp".into(),
                handler: "symlink".into(),
                source,
                user_path,
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(
            results[0].message.contains("cycle"),
            "msg: {}",
            results[0].message
        );
    }

    /// --force must NOT bypass the ancestor-cycle check. A cycle can
    /// never be "forced through" — it would corrupt the pack.
    #[test]
    fn force_does_not_bypass_ancestor_cycle_check() {
        let env = TempEnvironment::builder()
            .pack("warp")
            .file("keybindings.yaml", "keep me")
            .done()
            .build();
        let pack_dir = env.dotfiles_root.join("warp");
        let config_warp = env.config_home.join("warp");
        env.fs.mkdir_all(&env.config_home).unwrap();
        env.fs.symlink(&pack_dir, &config_warp).unwrap();

        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            true, // force
            false,
            true,
        );

        let source = pack_dir.join("keybindings.yaml");
        let user_path = config_warp.join("keybindings.yaml");

        let results = executor
            .execute(vec![HandlerIntent::Link {
                pack: "warp".into(),
                handler: "symlink".into(),
                source: source.clone(),
                user_path,
            }])
            .unwrap();

        assert!(!results[0].success, "force must not bypass cycle check");
        env.assert_file_contents(&source, "keep me");
    }

    /// Relative-target ancestor symlinks must also be detected. A link
    /// like `~/.config/warp -> ../../h/dotfiles/warp` joins lexically to
    /// a path containing `..` segments that wouldn't naively pass
    /// `starts_with(dotfiles_root)` — we normalize first.
    #[test]
    fn link_refuses_relative_ancestor_symlink_into_pack() {
        let env = TempEnvironment::builder()
            .pack("warp")
            .file("keybindings.yaml", "keep me")
            .done()
            .build();
        let pack_dir = env.dotfiles_root.join("warp");
        let config_warp = env.config_home.join("warp");
        env.fs.mkdir_all(&env.config_home).unwrap();

        // config_home is home/.config, dotfiles_root is home/dotfiles,
        // so the relative hop is `../dotfiles/warp` — exactly the shape
        // Copilot flagged: contains `..`, joins to a path that would
        // NOT naively `starts_with(dotfiles_root)` without normalization.
        let rel_target = Path::new("../dotfiles/warp");
        env.fs.symlink(rel_target, &config_warp).unwrap();

        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        );

        let source = pack_dir.join("keybindings.yaml");
        let user_path = config_warp.join("keybindings.yaml");

        let results = executor
            .execute(vec![HandlerIntent::Link {
                pack: "warp".into(),
                handler: "symlink".into(),
                source: source.clone(),
                user_path,
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(
            !results[0].success,
            "relative ancestor symlink must still be caught: {:?}",
            results
        );
        assert!(results[0].message.contains("cycle"));
        env.assert_no_handler_state("warp", "symlink");
        env.assert_file_contents(&source, "keep me");
    }
}

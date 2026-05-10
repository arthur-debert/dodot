//! `Stage` intent: copy a pack source into the datastore. The path
//! handler also gets auto-chmod +x for files inside `bin/` so dropped
//! execute bits (a common loss in git-on-macOS / manual-create flows)
//! don't leave dead-end shims on `$PATH`.

use tracing::{debug, info};

use crate::handlers::HANDLER_PATH;
use crate::operations::{HandlerIntent, Operation, OperationResult};
use crate::Result;

use super::Executor;

impl<'a> Executor<'a> {
    pub(super) fn execute_stage(&self, intent: &HandlerIntent) -> Result<Vec<OperationResult>> {
        let HandlerIntent::Stage {
            pack,
            handler,
            source,
        } = intent
        else {
            unreachable!("execute_stage called with non-Stage intent");
        };

        let filename = source.file_name().unwrap_or_default().to_string_lossy();
        info!(pack, handler = handler.as_str(), file = %filename, "staging file");

        self.datastore.create_data_link(pack, handler, source)?;

        let op = Operation::CreateDataLink {
            pack: pack.clone(),
            handler: handler.clone(),
            source: source.clone(),
        };

        let mut results = vec![OperationResult::ok(op, format!("staged {}", filename))];

        // Auto-chmod +x for path handler directories
        if handler == HANDLER_PATH && self.auto_chmod_exec {
            debug!(pack, source = %source.display(), "checking executable permissions");
            results.extend(self.ensure_executable(pack, source));
        }

        Ok(results)
    }

    pub(super) fn simulate_stage(&self, intent: &HandlerIntent) -> Vec<OperationResult> {
        let HandlerIntent::Stage {
            pack,
            handler,
            source,
        } = intent
        else {
            unreachable!("simulate_stage called with non-Stage intent");
        };

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
                    info!(pack, file = %entry.name, mode = format!("{:o}", new_mode), "chmod +x");
                    results.push(OperationResult::ok(op, format!("chmod +x {}", entry.name)));
                }
                Err(e) => {
                    info!(pack, file = %entry.name, error = %e, "chmod +x failed");
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
}

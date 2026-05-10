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

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

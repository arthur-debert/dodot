//! `up` command — deploy packs (create symlinks, run provisioning).

use crate::commands::{
    handler_description, handler_symbol, status_style, DisplayFile, DisplayPack, PackStatusResult,
};
use crate::datastore::format_command_for_display;
use crate::packs::orchestration::{self, Command, ExecutionContext, PackResult};
use crate::packs::Pack;
use crate::shell;
use crate::Result;

/// Run the `up` command: deploy packs and regenerate shell init.
pub fn up(pack_filter: Option<&[String]>, ctx: &ExecutionContext) -> Result<PackStatusResult> {
    let command = UpCommand;
    let result = orchestration::execute(&command, pack_filter, ctx)?;

    // Regenerate shell init script
    if !ctx.dry_run {
        shell::write_init_script(ctx.fs.as_ref(), ctx.paths.as_ref())?;
    }

    // Convert to display format
    let home = ctx.paths.home_dir();
    let packs: Vec<DisplayPack> = result
        .pack_results
        .iter()
        .map(|pr| {
            let mut files: Vec<DisplayFile> = pr
                .operations
                .iter()
                .map(|op| {
                    let (handler, name, user_target) = extract_op_info(&op.operation, home);
                    let status = if op.success {
                        status_style(true).into()
                    } else {
                        "error".into()
                    };
                    DisplayFile {
                        name: name.clone(),
                        symbol: handler_symbol(&handler).into(),
                        description: handler_description(&handler, &name, user_target.as_deref()),
                        status,
                        status_label: op.message.clone(),
                        handler,
                    }
                })
                .collect();

            // Include error from orchestration (e.g. pack-level config error)
            if let Some(err) = &pr.error {
                files.push(DisplayFile {
                    name: String::new(),
                    symbol: "×".into(),
                    description: String::new(),
                    status: "error".into(),
                    status_label: err.clone(),
                    handler: String::new(),
                });
            }

            DisplayPack {
                name: pr.pack_name.clone(),
                files,
                error: pr.error.clone(),
            }
        })
        .collect();

    // Message reflects actual results
    let has_failures = result.failed_packs > 0
        || result
            .pack_results
            .iter()
            .any(|pr| pr.operations.iter().any(|op| !op.success));
    let message = if has_failures {
        "Packs deployed with errors.".into()
    } else {
        "Packs deployed.".into()
    };

    Ok(PackStatusResult {
        message: Some(message),
        dry_run: ctx.dry_run,
        packs,
        warnings: Vec::new(),
    })
}

/// Extract handler name, display name, and optional user target from an operation.
fn extract_op_info(
    op: &crate::operations::Operation,
    home: &std::path::Path,
) -> (String, String, Option<String>) {
    match op {
        crate::operations::Operation::CreateDataLink {
            handler, source, ..
        } => (
            handler.clone(),
            source
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            None,
        ),
        crate::operations::Operation::CreateUserLink {
            handler,
            datastore_path,
            user_path,
            ..
        } => {
            // Name: filename from the datastore path (pack-relative name)
            let name = datastore_path
                .file_name()
                .unwrap_or_else(|| user_path.file_name().unwrap_or_default())
                .to_string_lossy()
                .into_owned();
            // Target: user_path displayed relative to ~ for readability
            let target = if let Ok(rel) = user_path.strip_prefix(home) {
                format!("~/{}", rel.display())
            } else {
                user_path.display().to_string()
            };
            (handler.clone(), name, Some(target))
        }
        crate::operations::Operation::RunCommand {
            handler,
            executable,
            arguments,
            ..
        } => (
            handler.clone(),
            format_command_for_display(executable, arguments),
            None,
        ),
        crate::operations::Operation::CheckSentinel {
            handler, sentinel, ..
        } => (handler.clone(), sentinel.clone(), None),
    }
}

struct UpCommand;

impl Command for UpCommand {
    fn name(&self) -> &str {
        "up"
    }

    fn execute_for_pack(&self, pack: &Pack, ctx: &ExecutionContext) -> Result<PackResult> {
        let operations = orchestration::run_handler_pipeline(pack, ctx)?;
        let success = operations.iter().all(|r| r.success);
        Ok(PackResult {
            pack_name: pack.name.clone(),
            success,
            operations,
            error: None,
        })
    }
}

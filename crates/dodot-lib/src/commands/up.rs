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
    let packs: Vec<DisplayPack> = result
        .pack_results
        .iter()
        .map(|pr| {
            let files: Vec<DisplayFile> = pr
                .operations
                .iter()
                .filter(|op| op.success)
                .map(|op| {
                    let (handler, source) = extract_op_info(&op.operation);
                    DisplayFile {
                        name: source.clone(),
                        symbol: handler_symbol(&handler).into(),
                        description: handler_description(&handler, &source, None),
                        status: status_style(true).into(),
                        status_label: op.message.clone(),
                        handler,
                    }
                })
                .collect();

            DisplayPack {
                name: pr.pack_name.clone(),
                files,
            }
        })
        .collect();

    Ok(PackStatusResult {
        message: Some("Packs deployed.".into()),
        dry_run: ctx.dry_run,
        packs,
    })
}

/// Extract handler name and source info from an operation.
fn extract_op_info(op: &crate::operations::Operation) -> (String, String) {
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
        ),
        crate::operations::Operation::CreateUserLink {
            handler, user_path, ..
        } => (handler.clone(), user_path.to_string_lossy().into_owned()),
        crate::operations::Operation::RunCommand {
            handler,
            executable,
            arguments,
            ..
        } => (handler.clone(), format_command_for_display(executable, arguments)),
        crate::operations::Operation::CheckSentinel {
            handler, sentinel, ..
        } => (handler.clone(), sentinel.clone()),
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

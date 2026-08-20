//! What a failing provisioning command costs, and what it must not.
//!
//! Provisioning runs before `PathExport`, `ShellInit`, and `Link`, so
//! a `Brewfile` or `install.sh` that fails used to take its pack's
//! symlinks, `$PATH` entries, and shell init down with it — and `up`
//! reported success to the shell either way. These tests pin the
//! three halves of the fix: the rest of the pack still deploys, the
//! failure is reported against the file that failed, and the run's
//! `failed` flag (the CLI's exit code) says what happened.

use std::sync::Arc;

use crate::commands::{self, DisplayFile};
use crate::datastore::{CommandOutput, CommandRunner};
use crate::fs::Fs;
use crate::packs::orchestration::ExecutionContext;
use crate::testing::TempEnvironment;
use crate::Result;

/// Fails any command whose arguments name `install.sh`, the way an
/// install script exiting non-zero does; everything else (the brew
/// bootstrap `up` captures, for one) succeeds silently.
struct FailingInstallRunner {
    stderr: String,
}

impl CommandRunner for FailingInstallRunner {
    fn run(&self, executable: &str, arguments: &[String]) -> Result<CommandOutput> {
        if arguments.iter().any(|a| a.ends_with("install.sh")) {
            return Err(crate::DodotError::CommandFailed {
                command: crate::datastore::format_command_for_display(executable, arguments),
                exit_code: 1,
                stderr: self.stderr.clone(),
            });
        }
        Ok(CommandOutput {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

/// A pack with one script that fails and one file that symlinks.
fn env_with_a_failing_script() -> TempEnvironment {
    TempEnvironment::builder()
        .pack("tools")
        .file("install.sh", "#!/bin/sh\nexit 1\n")
        .file("home.vimrc", "set nocompatible")
        .done()
        .build()
}

/// A provisioning-enabled context whose install script always fails.
fn failing_ctx(env: &TempEnvironment) -> ExecutionContext {
    let runner: Arc<dyn CommandRunner> = Arc::new(FailingInstallRunner {
        stderr: "install.sh: line 2: brew: command not found".into(),
    });
    let mut ctx = super::support::make_ctx_with_runner(env, runner);
    ctx.no_provision = false;
    ctx
}

fn row<'a>(result: &'a commands::PackStatusResult, name: &str) -> &'a DisplayFile {
    result.packs[0]
        .files
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| {
            panic!(
                "no row for `{name}`; rows: {:?}",
                result.packs[0]
                    .files
                    .iter()
                    .map(|f| (&f.name, &f.status))
                    .collect::<Vec<_>>()
            )
        })
}

#[test]
fn a_failing_script_does_not_take_the_rest_of_its_pack_with_it() {
    let env = env_with_a_failing_script();
    let ctx = failing_ctx(&env);

    let result = commands::up::up(None, &ctx).unwrap();

    // The symlink is deployed on disk...
    env.assert_double_link(
        "tools",
        "symlink",
        "home.vimrc",
        &env.dotfiles_root.join("tools/home.vimrc"),
        &env.home.join(".vimrc"),
    );
    // ...and reported, rather than discarded along with the pack.
    assert_eq!(
        row(&result, "home.vimrc").status,
        "deployed",
        "the symlink survived the failure and has to say so"
    );
}

#[test]
fn the_failure_is_reported_against_the_file_that_failed() {
    let env = env_with_a_failing_script();
    let ctx = failing_ctx(&env);

    let result = commands::up::up(None, &ctx).unwrap();

    let failed = row(&result, "install.sh");
    assert_eq!(failed.status, "error");
    assert_eq!(failed.handler, "install");

    let note = failed
        .note_ref
        .map(|n| result.notes[(n - 1) as usize].body.clone())
        .expect("the failing row must carry the note");
    assert!(
        note.contains("brew: command not found"),
        "the script's own output belongs in the note: {note}"
    );

    // Nothing else in the pack is marked as the cause.
    assert_eq!(
        result.packs[0]
            .files
            .iter()
            .filter(|f| f.status == "error")
            .count(),
        1,
        "exactly one row failed; rows: {:?}",
        result.packs[0]
            .files
            .iter()
            .map(|f| (&f.name, &f.status))
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_failed_run_writes_no_sentinel_so_the_next_up_retries() {
    let env = env_with_a_failing_script();
    let ctx = failing_ctx(&env);

    commands::up::up(None, &ctx).unwrap();

    env.assert_no_handler_state("tools", "install");
}

#[test]
fn a_run_with_a_failure_reports_a_non_zero_exit_code() {
    let env = env_with_a_failing_script();
    let ctx = failing_ctx(&env);

    let result = commands::up::up(None, &ctx).unwrap();

    assert!(result.failed);
    assert_eq!(result.exit_code(), 1);
}

#[test]
fn a_clean_run_reports_exit_code_zero() {
    let env = TempEnvironment::builder()
        .pack("tools")
        .file("install.sh", "#!/bin/sh\necho hi\n")
        .file("home.vimrc", "set nocompatible")
        .done()
        .build();
    // The default runner succeeds at everything, install script included.
    let mut ctx = super::support::make_ctx(&env);
    ctx.no_provision = false;

    let result = commands::up::up(None, &ctx).unwrap();

    assert!(!result.failed);
    assert_eq!(result.exit_code(), 0);
    assert_eq!(row(&result, "install.sh").status, "deployed");
}

#[test]
fn a_dry_run_reports_the_failure_without_becoming_one() {
    let env = env_with_a_failing_script();
    let mut ctx = failing_ctx(&env);
    ctx.dry_run = true;

    let result = commands::up::up(None, &ctx).unwrap();

    assert!(
        !result.failed,
        "a dry run attempted nothing, so it cannot have failed"
    );
    assert_eq!(result.exit_code(), 0);
    assert!(result.dry_run);
    // And it stayed a preview: nothing ran, nothing deployed.
    assert!(!env.fs.exists(&env.home.join(".vimrc")));
    env.assert_no_handler_state("tools", "install");
}

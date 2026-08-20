//! What a user actually reads about a run-once file.
//!
//! The run-once policy has four reportable outcomes — never ran, ran
//! the current content, ran an older version, and skipped because the
//! user passed `--no-provision` — and each one reaches the user as a
//! row in `dodot status` / `dodot up` output. These tests pin the
//! rendered row text (and, for the older-version case, the footnote
//! that names the remedy) rather than the internal state behind it,
//! because the internal state was already correct while the row was
//! not: the executor's remedy message rode on a *successful*
//! `OperationResult`, and the post-`up` renderer overlays only
//! failures, so the message never appeared anywhere.

use crate::commands;
use crate::fs::Fs;
use crate::testing::TempEnvironment;

use super::support::make_ctx;

/// The pack every test here uses: one install script, nothing else.
fn install_pack(script: &str) -> TempEnvironment {
    TempEnvironment::builder()
        .pack("setup")
        .file("install.sh", script)
        .done()
        .build()
}

/// The single `install`-handler row of the first pack in a result.
fn install_row(result: &commands::PackStatusResult) -> &commands::DisplayFile {
    result
        .packs
        .iter()
        .flat_map(|p| p.files.iter())
        .find(|f| f.handler == "install")
        .unwrap_or_else(|| panic!("no install row in {:?}", result.packs))
}

#[test]
fn never_ran_row_reads_the_handler_pending_copy() {
    let env = install_pack("#!/bin/sh\necho hi\n");
    let mut ctx = make_ctx(&env);
    ctx.no_provision = false;

    let result = commands::status::status(None, &ctx).unwrap();
    let row = install_row(&result);

    // `install`'s own copy for the never-ran state (`RunOnceCommand::status_pending`).
    assert_eq!(row.status_label, "never run");
    assert_eq!(row.status, "pending");
    assert_eq!(
        row.note_ref, None,
        "a file that has never run needs no footnote — running `up` is the whole story"
    );
}

#[test]
fn ran_current_row_reads_the_handler_deployed_copy() {
    let env = install_pack("#!/bin/sh\necho hi\n");
    let mut ctx = make_ctx(&env);
    ctx.no_provision = false;

    let result = commands::up::up(None, &ctx).unwrap();
    let row = install_row(&result);

    // `install`'s own copy for the ran-current state (`RunOnceCommand::status_deployed`).
    assert_eq!(row.status_label, "installed");
    assert_eq!(row.status, "deployed");
    assert_eq!(row.note_ref, None, "nothing to remedy on a current run");
}

#[test]
fn ran_older_version_row_carries_a_footnote_naming_provision_rerun() {
    let env = install_pack("#!/bin/sh\necho hi\n");
    let mut ctx = make_ctx(&env);
    ctx.no_provision = false;

    commands::up::up(None, &ctx).unwrap();

    // Edit the source: the sentinel now records a hash the file no
    // longer has, so the next `up` reports rather than re-runs.
    env.fs
        .write_file(
            &env.dotfiles_root.join("setup/install.sh"),
            b"#!/bin/sh\necho hi\necho there\n",
        )
        .unwrap();

    let result = commands::up::up(None, &ctx).unwrap();
    let row = install_row(&result);

    assert!(
        row.status_label.contains("older version"),
        "row should still state the condition, got: {}",
        row.status_label
    );
    let note_ref = row.note_ref.expect(
        "the row states a condition and no remedy on its own; without a footnote the user \
         is told their edit was ignored and nothing about how to apply it",
    );
    let note = &result.notes[(note_ref - 1) as usize];
    assert!(
        note.body.contains("--provision-rerun"),
        "footnote must name the flag that applies the edit, got: {}",
        note.body
    );
    assert!(
        !note.body.contains("--force"),
        "`--force` overwrites pre-existing files at symlink targets and re-fetches \
         externals; it re-runs nothing. Got: {}",
        note.body
    );
    assert_eq!(
        note.kind, "warning",
        "the previous run succeeded — an edit awaiting apply is not a failure"
    );
}

#[test]
fn no_provision_row_reports_the_skip_the_user_asked_for() {
    let env = install_pack("#!/bin/sh\necho hi\n");
    let ctx = make_ctx(&env); // no_provision defaults to true here

    let result = commands::up::up(None, &ctx).unwrap();
    let row = install_row(&result);

    assert_eq!(row.status_label, "skipped (--no-provision)");
    assert_eq!(row.status, "skipped");
}

#[test]
fn no_provision_dry_run_still_shows_the_skipped_file() {
    let env = install_pack("#!/bin/sh\necho hi\n");
    let mut ctx = make_ctx(&env);
    ctx.dry_run = true;

    // The dry-run renderer draws rows from simulated operations, and a
    // dropped handler produces no operation at all — so without an
    // explicit record of the skip the preview omits install.sh
    // entirely, which reads as "dodot found nothing here".
    let result = commands::up::up(None, &ctx).unwrap();
    let row = install_row(&result);

    assert_eq!(row.status_label, "skipped (--no-provision)");
    assert_eq!(row.status, "skipped");
}

#[test]
fn a_no_provision_skip_and_a_gated_out_file_read_differently() {
    // Three skip conditions must stay distinguishable: the user opted
    // out for this run, versus the host doesn't match the file's gate.
    // Collapsing them into one "skipped" label would leave the user
    // guessing which remedy applies.
    let foreign = if cfg!(target_os = "macos") {
        "linux"
    } else if cfg!(target_os = "linux") {
        "darwin"
    } else {
        return;
    };
    let gated_name = format!("install._{foreign}.sh");
    let env = TempEnvironment::builder()
        .pack("setup")
        .file("install.sh", "#!/bin/sh\necho hi\n")
        .file(&gated_name, "#!/bin/sh\necho nope\n")
        .done()
        .build();

    let ctx = make_ctx(&env); // no_provision = true
    let result = commands::up::up(None, &ctx).unwrap();
    let files = &result.packs[0].files;

    let skipped = files
        .iter()
        .find(|f| f.name == "install.sh")
        .expect("install.sh row");
    assert_eq!(skipped.status_label, "skipped (--no-provision)");

    let gated = files
        .iter()
        .find(|f| f.name == gated_name)
        .unwrap_or_else(|| panic!("{gated_name} row in {files:?}"));
    assert!(
        gated.status_label.starts_with("gated out ("),
        "a host-gated file must not read as a --no-provision skip, got: {}",
        gated.status_label
    );
    assert_ne!(
        skipped.status_label, gated.status_label,
        "two different conditions, two different remedies"
    );
}

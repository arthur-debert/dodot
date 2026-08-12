//! Tests for `dodot reset` — the factory-reset escape hatch.

use crate::commands;
use crate::fs::Fs;
use crate::paths::Pather;
use crate::testing::TempEnvironment;

use super::support::make_ctx;

/// Drop representative dodot-owned state under the env's data dir:
/// the datastore tree, the deployment map, shell init, probes, and
/// prompts/tutorial state.
fn seed_data_dir(env: &TempEnvironment) {
    let data = env.paths.data_dir().to_path_buf();
    let fs = &env.fs;
    fs.mkdir_all(&data.join("packs").join("vim").join("symlink"))
        .unwrap();
    fs.write_file(
        &data.join("packs").join("vim").join("symlink").join("vimrc"),
        b"link",
    )
    .unwrap();
    fs.mkdir_all(&data.join("shell")).unwrap();
    fs.write_file(&data.join("shell").join("dodot-init.sh"), b"# init")
        .unwrap();
    fs.mkdir_all(&data.join("probes").join("shell-init"))
        .unwrap();
    fs.write_file(&data.join("deployment-map.tsv"), b"a\tb\n")
        .unwrap();
    fs.write_file(&data.join("prompts.json"), b"{}").unwrap();
    fs.write_file(&data.join("tutorial.json"), b"{}").unwrap();
}

#[test]
fn reset_removes_everything_under_data_dir() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .build();
    seed_data_dir(&env);
    let ctx = make_ctx(&env);

    let result = commands::reset::reset(&ctx).unwrap();

    let data = env.paths.data_dir();
    assert!(env.fs.is_dir(data), "data dir itself is kept");
    assert!(
        env.fs.read_dir(data).unwrap().is_empty(),
        "data dir must be empty after reset"
    );
    assert!(result.message.contains("Run `dodot up` to redeploy"));
    // read_dir sorts by name, so details are deterministic.
    assert_eq!(
        result.details,
        vec![
            "removed deployment-map.tsv",
            "removed packs/",
            "removed probes/",
            "removed prompts.json",
            "removed shell/",
            "removed tutorial.json",
        ]
    );
}

#[test]
fn reset_does_not_touch_the_dotfiles_repo() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .build();
    seed_data_dir(&env);
    let ctx = make_ctx(&env);

    commands::reset::reset(&ctx).unwrap();

    assert!(
        env.fs.exists(&env.dotfiles_root.join("vim").join("vimrc")),
        "sources must survive a reset"
    );
}

#[test]
fn reset_dry_run_lists_without_removing() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .build();
    seed_data_dir(&env);
    let mut ctx = make_ctx(&env);
    ctx.dry_run = true;

    let result = commands::reset::reset(&ctx).unwrap();

    assert!(result.message.starts_with("Would reset"));
    assert!(result
        .details
        .iter()
        .all(|d| d.starts_with("would remove ")));
    assert_eq!(result.details.len(), 6);
    let data = env.paths.data_dir();
    assert_eq!(
        env.fs.read_dir(data).unwrap().len(),
        6,
        "dry-run must not remove anything"
    );
}

#[test]
fn reset_with_no_state_reports_nothing_to_do() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .build();
    // TempEnvironment pre-creates empty `packs/` and `shell/` dirs;
    // clear them so this exercises the truly-empty data dir.
    for entry in env.fs.read_dir(env.paths.data_dir()).unwrap() {
        env.fs.remove_dir_all(&entry.path).unwrap();
    }
    let ctx = make_ctx(&env);

    let result = commands::reset::reset(&ctx).unwrap();

    assert!(result.message.starts_with("Nothing to reset"));
    assert!(result.details.is_empty());
}

/// A top-level symlink in the data dir is removed as a link — reset
/// must not follow it and delete the target's contents.
#[test]
fn reset_removes_symlink_entries_without_following() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .build();
    let data = env.paths.data_dir().to_path_buf();
    env.fs.mkdir_all(&data).unwrap();
    // Link inside the data dir pointing at the dotfiles repo.
    env.fs
        .symlink(&env.dotfiles_root.join("vim"), &data.join("stray-link"))
        .unwrap();
    let ctx = make_ctx(&env);

    commands::reset::reset(&ctx).unwrap();

    assert!(!env.fs.is_symlink(&data.join("stray-link")));
    assert!(
        env.fs.exists(&env.dotfiles_root.join("vim").join("vimrc")),
        "reset must not delete through a symlink"
    );
}

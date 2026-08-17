//! Tests for `dodot reset` — the factory-reset escape hatch.

use crate::commands;
use crate::fs::Fs;
use crate::paths::Pather;
use crate::safety_lock::{
    RootIdentity, SafetyLockConfig, TrustFileTransaction, SAFETY_LOCK_FILE_NAME,
    SAFETY_LOCK_LOCK_FILE_NAME,
};
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
    let leftovers: Vec<String> = env
        .fs
        .read_dir(data)
        .unwrap()
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(
        leftovers,
        vec![SAFETY_LOCK_LOCK_FILE_NAME.to_string()],
        "after reset the data dir holds only the trust writer lock"
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

/// The trust writer lock file survives reset: unlinking it while a
/// writer holds it would split the lock across two inodes, letting two
/// "exclusive" writers run at once. The trust FILE itself is still
/// swept, and the sweep report never names the lock file.
#[test]
fn reset_preserves_the_writer_lock_file() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .build();
    seed_data_dir(&env);
    let data = env.paths.data_dir().to_path_buf();
    // A prior transaction leaves both the trust file and the lock file.
    let transaction = TrustFileTransaction::begin(&data).unwrap();
    let mut config = transaction.load().unwrap();
    config
        .roots
        .approved
        .push(RootIdentity::new("/home/alice/dots").unwrap());
    transaction.persist(&config).unwrap();
    drop(transaction);
    let ctx = make_ctx(&env);

    let result = commands::reset::reset(&ctx).unwrap();

    assert!(
        env.fs.exists(&data.join(SAFETY_LOCK_LOCK_FILE_NAME)),
        "the writer lock file must survive reset"
    );
    assert!(
        !env.fs.exists(&data.join(SAFETY_LOCK_FILE_NAME)),
        "the trust file itself is dodot state and must be swept"
    );
    assert!(
        result
            .details
            .iter()
            .all(|d| !d.contains(SAFETY_LOCK_LOCK_FILE_NAME)),
        "the sweep report must not name the lock file: {:?}",
        result.details
    );
}

/// The interleaving the writer lock exists for: a transaction is
/// mid-flight when reset runs. Reset must wait for it, sweep whatever
/// it persisted, and leave a subsequent writer applying to the empty
/// post-reset state — a pre-reset approval can never be resurrected.
#[test]
fn reset_waits_for_an_active_transaction_and_clears_its_write() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .build();
    seed_data_dir(&env);
    let data = env.paths.data_dir().to_path_buf();

    // Active transaction: lock held, pre-reset document loaded.
    let transaction = TrustFileTransaction::begin(&data).unwrap();
    let mut config = transaction.load().unwrap();
    config
        .roots
        .approved
        .push(RootIdentity::new("/home/alice/dots").unwrap());

    let ctx = make_ctx(&env);
    let reset_thread = std::thread::spawn(move || commands::reset::reset(&ctx));

    // While the transaction holds the lock, reset must not have swept.
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(
        env.fs.is_dir(&data.join("packs")),
        "reset must block on the active transaction"
    );

    transaction.persist(&config).unwrap();
    drop(transaction);
    reset_thread.join().unwrap().unwrap();

    assert!(
        !env.fs.exists(&data.join(SAFETY_LOCK_FILE_NAME)),
        "the approval persisted before reset must be swept with everything else"
    );

    // A subsequent writer sees the empty post-reset state, not a
    // resurrected pre-reset document.
    let transaction = TrustFileTransaction::begin(&data).unwrap();
    let mut config = transaction.load().unwrap();
    assert!(
        config.roots.approved.is_empty(),
        "post-reset state must start empty"
    );
    config
        .roots
        .approved
        .push(RootIdentity::new("/home/alice/new-dots").unwrap());
    transaction.persist(&config).unwrap();
    drop(transaction);

    let final_state = SafetyLockConfig::load_from(&data).unwrap();
    assert_eq!(
        final_state.roots.approved,
        vec![RootIdentity::new("/home/alice/new-dots").unwrap()],
        "only the post-reset approval may exist"
    );
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

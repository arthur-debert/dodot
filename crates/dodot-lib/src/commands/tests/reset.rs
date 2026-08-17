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

/// Starting from an ABSENT data dir, a writer that persisted while
/// holding the lock either finishes before reset lists (and its write
/// is swept) or begins after — its pre-reset approval can never
/// survive into the cleared state. Sequenced by channels, not timing:
/// the writer holds the lock continuously from `begin` through
/// `persist`, and reset is only spawned once the approval is already
/// persisted, so reset can list only after the drop — every
/// interleaving must end swept. (The unconditional-lock contract that
/// makes the absent-dir observation race impossible is pinned
/// separately by `reset_never_acts_on_an_absent_data_dir_observation`.)
#[test]
fn reset_from_absent_data_dir_locks_against_a_racing_writer() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .build();
    let data = env.paths.data_dir().to_path_buf();
    // Start from a truly absent data dir (TempEnvironment pre-creates
    // some entries).
    env.fs.remove_dir_all(&data).unwrap();
    assert!(!env.fs.is_dir(&data));

    let (persisted_tx, persisted_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let writer_data = data.clone();
    let writer = std::thread::spawn(move || {
        // Creates the data dir + lock file, persists under the lock.
        let transaction = TrustFileTransaction::begin(&writer_data).unwrap();
        let mut config = transaction.load().unwrap();
        config
            .roots
            .approved
            .push(RootIdentity::new("/home/alice/dots").unwrap());
        transaction.persist(&config).unwrap();
        persisted_tx.send(()).unwrap();
        // Keep holding the lock until the main thread has spawned reset.
        release_rx.recv().unwrap();
        drop(transaction);
    });

    // The approval exists on disk, lock still held by the writer.
    persisted_rx.recv().unwrap();
    let ctx = make_ctx(&env);
    let reset_thread = std::thread::spawn(move || commands::reset::reset(&ctx));
    release_tx.send(()).unwrap();
    writer.join().unwrap();
    let result = reset_thread.join().unwrap().unwrap();

    assert!(
        !env.fs.exists(&data.join(SAFETY_LOCK_FILE_NAME)),
        "the writer's pre-reset approval must be swept: {:?}",
        result.details
    );
    assert!(
        result
            .details
            .iter()
            .any(|d| d.contains(SAFETY_LOCK_FILE_NAME)),
        "reset must have listed and removed the trust file: {:?}",
        result.details
    );
    assert!(
        env.fs.exists(&data.join(SAFETY_LOCK_LOCK_FILE_NAME)),
        "the writer lock file must survive"
    );
}

/// Delegates to [`crate::fs::OsFs`], firing at exactly the moment the
/// pre-fix implementation was vulnerable: `is_dir(data_dir)` about to
/// report ABSENT. At that instant the probe performs the racing write
/// itself — begins a [`TrustFileTransaction`] (creating the dir and
/// lock), persists an approval, and KEEPS the transaction held — then
/// returns the now-stale `false`. Conditional locking (`!is_dir` →
/// skip the lock) acts on that stale observation and proceeds to
/// sweep the approval while another writer holds the lock; the test
/// below detects the fired probe and fails. Unconditional locking
/// takes the lock (creating the dir) before any observation, so the
/// vulnerable moment is unreachable and the probe never fires.
struct AbsentObservationProbe {
    inner: std::sync::Arc<crate::fs::OsFs>,
    data_dir: std::path::PathBuf,
    observed_absent: std::sync::atomic::AtomicBool,
    held_transaction: std::sync::Mutex<Option<TrustFileTransaction>>,
}

impl Fs for AbsentObservationProbe {
    fn stat(&self, path: &std::path::Path) -> crate::Result<crate::fs::FsMetadata> {
        self.inner.stat(path)
    }
    fn lstat(&self, path: &std::path::Path) -> crate::Result<crate::fs::FsMetadata> {
        self.inner.lstat(path)
    }
    fn open_read(
        &self,
        path: &std::path::Path,
    ) -> crate::Result<Box<dyn std::io::Read + Send + Sync>> {
        self.inner.open_read(path)
    }
    fn read_file(&self, path: &std::path::Path) -> crate::Result<Vec<u8>> {
        self.inner.read_file(path)
    }
    fn read_to_string(&self, path: &std::path::Path) -> crate::Result<String> {
        self.inner.read_to_string(path)
    }
    fn write_file(&self, path: &std::path::Path, contents: &[u8]) -> crate::Result<()> {
        self.inner.write_file(path, contents)
    }
    fn mkdir_all(&self, path: &std::path::Path) -> crate::Result<()> {
        self.inner.mkdir_all(path)
    }
    fn symlink(&self, original: &std::path::Path, link: &std::path::Path) -> crate::Result<()> {
        self.inner.symlink(original, link)
    }
    fn readlink(&self, path: &std::path::Path) -> crate::Result<std::path::PathBuf> {
        self.inner.readlink(path)
    }
    fn remove_file(&self, path: &std::path::Path) -> crate::Result<()> {
        self.inner.remove_file(path)
    }
    fn remove_dir_all(&self, path: &std::path::Path) -> crate::Result<()> {
        self.inner.remove_dir_all(path)
    }
    fn exists(&self, path: &std::path::Path) -> bool {
        self.inner.exists(path)
    }
    fn is_symlink(&self, path: &std::path::Path) -> bool {
        self.inner.is_symlink(path)
    }
    fn is_dir(&self, path: &std::path::Path) -> bool {
        let result = self.inner.is_dir(path);
        if !result && path == self.data_dir {
            self.observed_absent
                .store(true, std::sync::atomic::Ordering::SeqCst);
            // The racing writer, injected at the vulnerable moment:
            // create the dir + lock, persist, keep holding the lock.
            let transaction = TrustFileTransaction::begin(&self.data_dir).unwrap();
            let mut config = transaction.load().unwrap();
            config
                .roots
                .approved
                .push(RootIdentity::new("/home/alice/dots").unwrap());
            transaction.persist(&config).unwrap();
            *self.held_transaction.lock().unwrap() = Some(transaction);
        }
        result
    }
    fn read_dir(&self, path: &std::path::Path) -> crate::Result<Vec<crate::fs::DirEntry>> {
        self.inner.read_dir(path)
    }
    fn rename(&self, from: &std::path::Path, to: &std::path::Path) -> crate::Result<()> {
        self.inner.rename(from, to)
    }
    fn copy_file(&self, from: &std::path::Path, to: &std::path::Path) -> crate::Result<()> {
        self.inner.copy_file(from, to)
    }
    fn set_permissions(&self, path: &std::path::Path, mode: u32) -> crate::Result<()> {
        self.inner.set_permissions(path, mode)
    }
}

/// The decisive probe for the unconditional-lock contract (see
/// [`AbsentObservationProbe`]): a real-run reset must NEVER act on an
/// "absent data dir" observation, because `begin` — which creates the
/// dir and lock — precedes every observation. Deterministic and
/// single-threaded: fails under the former conditional locking, where
/// the probe fires and reset sweeps a lock-holding writer's approval;
/// passes under unconditional locking, where the probe's moment is
/// unreachable.
#[test]
fn reset_never_acts_on_an_absent_data_dir_observation() {
    let env = TempEnvironment::builder()
        .pack("vim")
        .file("vimrc", "set nocompatible")
        .done()
        .build();
    let data = env.paths.data_dir().to_path_buf();
    env.fs.remove_dir_all(&data).unwrap();
    assert!(!env.fs.is_dir(&data));

    let probe = std::sync::Arc::new(AbsentObservationProbe {
        inner: env.fs.clone(),
        data_dir: data.clone(),
        observed_absent: std::sync::atomic::AtomicBool::new(false),
        held_transaction: std::sync::Mutex::new(None),
    });
    let mut ctx = make_ctx(&env);
    ctx.fs = probe.clone();

    let result = commands::reset::reset(&ctx).unwrap();

    assert!(
        !probe
            .observed_absent
            .load(std::sync::atomic::Ordering::SeqCst),
        "a real-run reset acted on an absent-dir observation — the writer \
         lock was not taken before observing the data dir: {:?}",
        result.details
    );
    assert!(
        !env.fs.exists(&data.join(SAFETY_LOCK_FILE_NAME)),
        "no approval may exist: the probe's racing writer must have been unreachable"
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

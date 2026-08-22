//! Shared command-test fixtures.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::config::ConfigManager;
use crate::datastore::{CommandOutput, CommandRunner, CommandSpec, FilesystemDataStore};
use crate::fs::Fs;
use crate::packs::orchestration::ExecutionContext;
use crate::paths::Pather;
use crate::testing::TempEnvironment;
use crate::Result;

pub(super) struct MockCommandRunner;
impl CommandRunner for MockCommandRunner {
    fn run(&self, _command: CommandSpec<'_>) -> Result<CommandOutput> {
        Ok(CommandOutput {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

/// Returns canned command outputs without spawning subprocesses.
pub(super) struct CannedRunner {
    responses: std::sync::Mutex<std::collections::HashMap<Vec<String>, CommandOutput>>,
}

impl CannedRunner {
    pub(super) fn new() -> Self {
        Self {
            responses: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
    pub(super) fn respond(&self, args: &[&str], stdout: &str, exit_code: i32) {
        let key: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        self.responses.lock().unwrap().insert(
            key,
            CommandOutput {
                exit_code,
                stdout: stdout.into(),
                stderr: String::new(),
            },
        );
    }
}

impl CommandRunner for CannedRunner {
    fn run(&self, command: CommandSpec<'_>) -> Result<CommandOutput> {
        let CommandSpec {
            executable: exe,
            arguments: args,
            ..
        } = command;
        let mut full = vec![exe.to_string()];
        full.extend(args.iter().cloned());
        self.responses
            .lock()
            .unwrap()
            .get(&full)
            .cloned()
            .ok_or_else(|| {
                crate::DodotError::Other(format!("CannedRunner: no canned response for {full:?}"))
            })
    }
}

pub(super) fn make_ctx(env: &TempEnvironment) -> ExecutionContext {
    let runner: Arc<dyn CommandRunner> = Arc::new(MockCommandRunner);
    let datastore = Arc::new(FilesystemDataStore::new(
        env.fs.clone(),
        env.paths.clone(),
        runner.clone(),
    ));
    let config_manager = Arc::new(ConfigManager::new(&env.dotfiles_root).unwrap());

    ExecutionContext {
        fs: env.fs.clone() as Arc<dyn Fs>,
        datastore,
        paths: env.paths.clone() as Arc<dyn Pather>,
        config_manager,
        syntax_checker: Arc::new(crate::shell::NoopSyntaxChecker),
        command_runner: runner,
        dry_run: false,
        no_provision: true,
        provision_rerun: false,
        force: false,
        check_drift: false,
        show_diff: false,
        view_mode: crate::commands::ViewMode::Full,
        group_mode: crate::commands::GroupMode::Name,
        verbose: false,
        host_facts: Arc::new(crate::gates::HostFacts::detect()),
        env_stamp: Default::default(),
        tty: false,
        shell_probe: crate::shell::ProbePolicy::Never,
        provision_host: Arc::new(
            crate::provisioners::availability::ProvisionHost::assume_present(),
        ),
        shell_env: crate::shell::ShellEnv::default(),
    }
}

pub(super) fn make_ctx_with_runner(
    env: &TempEnvironment,
    runner: Arc<dyn CommandRunner>,
) -> ExecutionContext {
    let datastore = Arc::new(FilesystemDataStore::new(
        env.fs.clone(),
        env.paths.clone(),
        runner.clone(),
    ));
    let config_manager = Arc::new(ConfigManager::new(&env.dotfiles_root).unwrap());
    ExecutionContext {
        fs: env.fs.clone() as Arc<dyn Fs>,
        datastore,
        paths: env.paths.clone() as Arc<dyn Pather>,
        config_manager,
        syntax_checker: Arc::new(crate::shell::NoopSyntaxChecker),
        command_runner: runner,
        dry_run: false,
        no_provision: true,
        provision_rerun: false,
        force: false,
        check_drift: false,
        show_diff: false,
        view_mode: crate::commands::ViewMode::Full,
        group_mode: crate::commands::GroupMode::Name,
        verbose: false,
        host_facts: Arc::new(crate::gates::HostFacts::detect()),
        env_stamp: Default::default(),
        tty: false,
        shell_probe: crate::shell::ProbePolicy::Never,
        provision_host: Arc::new(
            crate::provisioners::availability::ProvisionHost::assume_present(),
        ),
        shell_env: crate::shell::ShellEnv::default(),
    }
}

/// Like [`make_ctx`], but with the filesystem the commands see replaced.
///
/// Pair with [`InterposedFs`] to put a filesystem failure at an exact
/// step of a command and check what the command leaves behind.
pub(super) fn make_ctx_with_fs(env: &TempEnvironment, fs: Arc<dyn Fs>) -> ExecutionContext {
    let mut ctx = make_ctx(env);
    ctx.fs = fs;
    ctx
}

/// A filesystem operation [`InterposedFs`] hands to its hook before
/// delegating it.
///
/// Only the operations tests need to intercept are listed; everything
/// else on [`Fs`] passes straight through.
// Each variant carries the whole operation, so a hook can match on any
// part of it. Which parts the tests currently read is a property of the
// tests, not of the shape a hook is offered.
#[allow(dead_code)]
pub(super) enum FsOp<'a> {
    CopyFile { from: &'a Path, to: &'a Path },
    Rename { from: &'a Path, to: &'a Path },
    RenameNoReplace { from: &'a Path, to: &'a Path },
    MkdirAll { path: &'a Path },
    MkdirExclusive { path: &'a Path },
    Symlink { original: &'a Path, link: &'a Path },
}

/// Wraps a real filesystem and runs a hook immediately before each
/// intercepted operation.
///
/// A hook returning `Err` becomes that operation's error and the
/// operation never reaches the inner filesystem — which is how a test
/// puts a failure at the one step whose recovery it is checking. A hook
/// returning `Ok` may still have changed the world first, which is how a
/// test stages a race: a path that appears between two steps of a
/// command that checked for it earlier.
pub(super) struct InterposedFs {
    inner: Arc<dyn Fs>,
    #[allow(clippy::type_complexity)]
    before: Box<dyn Fn(FsOp<'_>) -> Result<()> + Send + Sync>,
}

impl InterposedFs {
    pub(super) fn wrap(
        inner: Arc<dyn Fs>,
        before: impl Fn(FsOp<'_>) -> Result<()> + Send + Sync + 'static,
    ) -> Arc<dyn Fs> {
        Arc::new(InterposedFs {
            inner,
            before: Box::new(before),
        })
    }
}

impl Fs for InterposedFs {
    fn copy_file(&self, from: &Path, to: &Path) -> Result<()> {
        (self.before)(FsOp::CopyFile { from, to })?;
        self.inner.copy_file(from, to)
    }
    fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        (self.before)(FsOp::Rename { from, to })?;
        self.inner.rename(from, to)
    }
    fn rename_noreplace(&self, from: &Path, to: &Path) -> Result<()> {
        (self.before)(FsOp::RenameNoReplace { from, to })?;
        self.inner.rename_noreplace(from, to)
    }
    fn stat(&self, path: &Path) -> Result<crate::fs::FsMetadata> {
        self.inner.stat(path)
    }
    fn lstat(&self, path: &Path) -> Result<crate::fs::FsMetadata> {
        self.inner.lstat(path)
    }
    fn open_read(&self, path: &Path) -> Result<Box<dyn std::io::Read + Send + Sync>> {
        self.inner.open_read(path)
    }
    fn read_file(&self, path: &Path) -> Result<Vec<u8>> {
        self.inner.read_file(path)
    }
    fn read_to_string(&self, path: &Path) -> Result<String> {
        self.inner.read_to_string(path)
    }
    fn mkdir_all(&self, path: &Path) -> Result<()> {
        (self.before)(FsOp::MkdirAll { path })?;
        self.inner.mkdir_all(path)
    }
    fn mkdir_exclusive(&self, path: &Path) -> Result<()> {
        (self.before)(FsOp::MkdirExclusive { path })?;
        self.inner.mkdir_exclusive(path)
    }
    fn symlink(&self, original: &Path, link: &Path) -> Result<()> {
        (self.before)(FsOp::Symlink { original, link })?;
        self.inner.symlink(original, link)
    }
    fn write_file(&self, path: &Path, contents: &[u8]) -> Result<()> {
        self.inner.write_file(path, contents)
    }
    fn readlink(&self, path: &Path) -> Result<PathBuf> {
        self.inner.readlink(path)
    }
    fn remove_file(&self, path: &Path) -> Result<()> {
        self.inner.remove_file(path)
    }
    fn remove_dir_all(&self, path: &Path) -> Result<()> {
        self.inner.remove_dir_all(path)
    }
    fn exists(&self, path: &Path) -> bool {
        self.inner.exists(path)
    }
    fn is_symlink(&self, path: &Path) -> bool {
        self.inner.is_symlink(path)
    }
    fn is_dir(&self, path: &Path) -> bool {
        self.inner.is_dir(path)
    }
    fn read_dir(&self, path: &Path) -> Result<Vec<crate::fs::DirEntry>> {
        self.inner.read_dir(path)
    }
    fn set_permissions(&self, path: &Path, mode: u32) -> Result<()> {
        self.inner.set_permissions(path, mode)
    }
    fn modified(&self, path: &Path) -> Result<std::time::SystemTime> {
        self.inner.modified(path)
    }
    fn set_modified(&self, path: &Path, time: std::time::SystemTime) -> Result<()> {
        self.inner.set_modified(path, time)
    }
}

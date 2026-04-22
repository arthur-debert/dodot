use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::datastore::{CommandRunner, DataStore};
use crate::fs::Fs;
use crate::paths::Pather;
use crate::{DodotError, Result};

/// Validate that `raw` is a safe relative path to be used under `base`.
///
/// Defense-in-depth against path traversal: rejects absolute paths, `..`
/// components, and anything that would escape `base`. Returns the
/// normalised relative `PathBuf` on success.
fn validate_safe_relative(raw: &str, base: &Path) -> Result<PathBuf> {
    let candidate = Path::new(raw);
    let mut cleaned = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::Normal(n) => cleaned.push(n),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(DodotError::Other(format!(
                    "unsafe datastore path: {} (would escape {})",
                    raw,
                    base.display()
                )));
            }
        }
    }
    if cleaned.as_os_str().is_empty() {
        return Err(DodotError::Other(format!(
            "empty datastore path (from {raw:?})"
        )));
    }
    Ok(cleaned)
}

/// [`DataStore`] implementation backed by the real filesystem.
///
/// State is stored as symlinks and sentinel files under the XDG data
/// directory. The double-link architecture works as follows:
///
/// ```text
/// ~/dotfiles/vim/vimrc                              (source)
///   -> ~/.local/share/dodot/packs/vim/symlink/vimrc (data link)
///     -> ~/.vimrc                                   (user link)
/// ```
pub struct FilesystemDataStore {
    fs: Arc<dyn Fs>,
    paths: Arc<dyn Pather>,
    runner: Arc<dyn CommandRunner>,
}

impl FilesystemDataStore {
    pub fn new(fs: Arc<dyn Fs>, paths: Arc<dyn Pather>, runner: Arc<dyn CommandRunner>) -> Self {
        Self { fs, paths, runner }
    }
}

impl DataStore for FilesystemDataStore {
    fn create_data_link(&self, pack: &str, handler: &str, source_file: &Path) -> Result<PathBuf> {
        let filename = source_file.file_name().ok_or_else(|| {
            crate::DodotError::Other(format!(
                "source file has no filename: {}",
                source_file.display()
            ))
        })?;

        let link_dir = self.paths.handler_data_dir(pack, handler);
        let link_path = link_dir.join(filename);

        self.fs.mkdir_all(&link_dir)?;

        // Idempotent: if the link already points to the correct source, skip.
        if self.fs.is_symlink(&link_path) {
            if let Ok(current_target) = self.fs.readlink(&link_path) {
                if current_target == source_file {
                    return Ok(link_path);
                }
            }
            // Wrong target — remove and re-create.
            self.fs.remove_file(&link_path)?;
        }

        self.fs.symlink(source_file, &link_path)?;
        Ok(link_path)
    }

    fn create_user_link(&self, datastore_path: &Path, user_path: &Path) -> Result<()> {
        // Create parent directory
        if let Some(parent) = user_path.parent() {
            self.fs.mkdir_all(parent)?;
        }

        // If something already exists at user_path, handle it
        if self.fs.is_symlink(user_path) {
            // Existing symlink — check if it's correct
            if let Ok(current_target) = self.fs.readlink(user_path) {
                if current_target == datastore_path {
                    return Ok(()); // Already correct
                }
            }
            // Wrong target — remove and re-create
            self.fs.remove_file(user_path)?;
        } else if self.fs.exists(user_path) {
            // Exists but is not a symlink — conflict
            return Err(crate::DodotError::SymlinkConflict {
                path: user_path.to_path_buf(),
            });
        }

        self.fs.symlink(datastore_path, user_path)
    }

    fn run_and_record(
        &self,
        pack: &str,
        handler: &str,
        executable: &str,
        arguments: &[String],
        sentinel: &str,
        force: bool,
    ) -> Result<()> {
        // Idempotent: skip if sentinel exists
        if !force && self.has_sentinel(pack, handler, sentinel)? {
            return Ok(());
        }

        // Provisioning scripts are consequential and can take a while; surface
        // start/end markers on stderr so the user knows what's running and
        // whether it succeeded. The script's own stdout/stderr still flows
        // through the runner as before.
        let display_name = arguments
            .iter()
            .rev()
            .find_map(|arg| {
                Path::new(arg)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .filter(|n| n.contains('.'))
            })
            .unwrap_or_else(|| executable.to_string());
        let header = format!("==== {pack} → {handler} → {display_name}");
        let tty = std::io::IsTerminal::is_terminal(&std::io::stderr());
        let dim = if tty { "\x1b[2m" } else { "" };
        let green = if tty { "\x1b[32m" } else { "" };
        let red = if tty { "\x1b[31m" } else { "" };
        let reset = if tty { "\x1b[0m" } else { "" };
        eprintln!("{header}  {dim}running…{reset}");

        let result = self.runner.run(executable, arguments);
        match &result {
            Ok(_) => eprintln!("{header}  {green}OK{reset}"),
            Err(_) => eprintln!("{header}  {red}FAILED{reset}"),
        }
        result?;

        // Record sentinel
        let sentinel_dir = self.paths.handler_data_dir(pack, handler);
        self.fs.mkdir_all(&sentinel_dir)?;

        let sentinel_path = sentinel_dir.join(sentinel);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let content = format!("completed|{timestamp}");
        self.fs.write_file(&sentinel_path, content.as_bytes())
    }

    fn has_sentinel(&self, pack: &str, handler: &str, sentinel: &str) -> Result<bool> {
        let sentinel_path = self.paths.handler_data_dir(pack, handler).join(sentinel);
        Ok(self.fs.exists(&sentinel_path))
    }

    fn remove_state(&self, pack: &str, handler: &str) -> Result<()> {
        let state_dir = self.paths.handler_data_dir(pack, handler);
        if !self.fs.exists(&state_dir) {
            return Ok(());
        }
        self.fs.remove_dir_all(&state_dir)
    }

    fn has_handler_state(&self, pack: &str, handler: &str) -> Result<bool> {
        let state_dir = self.paths.handler_data_dir(pack, handler);
        if !self.fs.exists(&state_dir) {
            return Ok(false);
        }
        let entries = self.fs.read_dir(&state_dir)?;
        Ok(!entries.is_empty())
    }

    fn list_pack_handlers(&self, pack: &str) -> Result<Vec<String>> {
        let pack_dir = self.paths.pack_data_dir(pack);
        if !self.fs.exists(&pack_dir) {
            return Ok(Vec::new());
        }
        let entries = self.fs.read_dir(&pack_dir)?;
        Ok(entries
            .into_iter()
            .filter(|e| e.is_dir)
            .map(|e| e.name)
            .collect())
    }

    fn list_handler_sentinels(&self, pack: &str, handler: &str) -> Result<Vec<String>> {
        let handler_dir = self.paths.handler_data_dir(pack, handler);
        if !self.fs.exists(&handler_dir) {
            return Ok(Vec::new());
        }
        let entries = self.fs.read_dir(&handler_dir)?;
        Ok(entries
            .into_iter()
            .filter(|e| e.is_file)
            .map(|e| e.name)
            .collect())
    }

    fn write_rendered_file(
        &self,
        pack: &str,
        handler: &str,
        filename: &str,
        content: &[u8],
    ) -> Result<PathBuf> {
        let dir = self.paths.handler_data_dir(pack, handler);
        let relative = validate_safe_relative(filename, &dir)?;
        let path = dir.join(&relative);
        // Create the full parent chain (supports nested filenames like "sub/file.txt")
        if let Some(parent) = path.parent() {
            self.fs.mkdir_all(parent)?;
        } else {
            self.fs.mkdir_all(&dir)?;
        }
        self.fs.write_file(&path, content)?;
        Ok(path)
    }

    fn write_rendered_dir(&self, pack: &str, handler: &str, relative: &str) -> Result<PathBuf> {
        let dir = self.paths.handler_data_dir(pack, handler);
        let rel = validate_safe_relative(relative, &dir)?;
        let path = dir.join(&rel);
        self.fs.mkdir_all(&path)?;
        Ok(path)
    }

    fn sentinel_path(&self, pack: &str, handler: &str, sentinel: &str) -> PathBuf {
        self.paths.handler_data_dir(pack, handler).join(sentinel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datastore::{CommandOutput, CommandRunner};
    use crate::testing::TempEnvironment;
    use std::sync::Mutex;

    /// Mock command runner that records calls and can be configured to
    /// succeed or fail.
    struct MockCommandRunner {
        calls: Mutex<Vec<String>>,
        should_fail: bool,
    }

    impl MockCommandRunner {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                should_fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                should_fail: true,
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl CommandRunner for MockCommandRunner {
        fn run(&self, executable: &str, arguments: &[String]) -> Result<CommandOutput> {
            let cmd_str = format!("{} {}", executable, arguments.join(" "));
            self.calls.lock().unwrap().push(cmd_str.trim().to_string());
            if self.should_fail {
                Err(crate::DodotError::CommandFailed {
                    command: cmd_str.trim().to_string(),
                    exit_code: 1,
                    stderr: "mock failure".to_string(),
                })
            } else {
                Ok(CommandOutput {
                    exit_code: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                })
            }
        }
    }

    fn make_datastore(env: &TempEnvironment) -> (FilesystemDataStore, Arc<MockCommandRunner>) {
        let runner = Arc::new(MockCommandRunner::new());
        let ds = FilesystemDataStore::new(env.fs.clone(), env.paths.clone(), runner.clone());
        (ds, runner)
    }

    // ── create_data_link ────────────────────────────────────────

    #[test]
    fn create_data_link_creates_symlink() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);

        let source = env.dotfiles_root.join("vim/vimrc");
        let link_path = ds.create_data_link("vim", "symlink", &source).unwrap();

        // Link should be in the handler data dir
        assert_eq!(
            link_path,
            env.paths.handler_data_dir("vim", "symlink").join("vimrc")
        );

        // Link should point to source
        env.assert_symlink(&link_path, &source);
    }

    #[test]
    fn create_data_link_is_idempotent() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);

        let source = env.dotfiles_root.join("vim/vimrc");

        let path1 = ds.create_data_link("vim", "symlink", &source).unwrap();
        let path2 = ds.create_data_link("vim", "symlink", &source).unwrap();

        assert_eq!(path1, path2);
        env.assert_symlink(&path1, &source);
    }

    #[test]
    fn create_data_link_replaces_wrong_target() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "v1")
            .file("vimrc-new", "v2")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);

        let source1 = env.dotfiles_root.join("vim/vimrc");
        let source2 = env.dotfiles_root.join("vim/vimrc-new");

        // Create initial link to source1
        let link_dir = env.paths.handler_data_dir("vim", "symlink");
        env.fs.mkdir_all(&link_dir).unwrap();
        // Manually create a link named "vimrc-new" pointing to source1 (wrong target)
        let wrong_link = link_dir.join("vimrc-new");
        env.fs.symlink(&source1, &wrong_link).unwrap();

        // Now create_data_link should fix it to point at source2
        let link_path = ds.create_data_link("vim", "symlink", &source2).unwrap();
        env.assert_symlink(&link_path, &source2);
    }

    // ── create_user_link ────────────────────────────────────────

    #[test]
    fn create_user_link_creates_symlink() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);

        let datastore_path = env.data_dir.join("packs/vim/symlink/vimrc");
        let user_path = env.home.join(".vimrc");

        // Create the datastore file so the symlink target exists
        env.fs.mkdir_all(datastore_path.parent().unwrap()).unwrap();
        env.fs.write_file(&datastore_path, b"link target").unwrap();

        ds.create_user_link(&datastore_path, &user_path).unwrap();

        env.assert_symlink(&user_path, &datastore_path);
    }

    #[test]
    fn create_user_link_is_idempotent() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);

        let datastore_path = env.data_dir.join("packs/vim/symlink/vimrc");
        let user_path = env.home.join(".vimrc");

        env.fs.mkdir_all(datastore_path.parent().unwrap()).unwrap();
        env.fs.write_file(&datastore_path, b"x").unwrap();

        ds.create_user_link(&datastore_path, &user_path).unwrap();
        ds.create_user_link(&datastore_path, &user_path).unwrap();

        env.assert_symlink(&user_path, &datastore_path);
    }

    #[test]
    fn create_user_link_conflict_with_regular_file() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);

        let datastore_path = env.data_dir.join("packs/vim/symlink/vimrc");
        let user_path = env.home.join(".vimrc");

        // Create a regular file at the user path
        env.fs.write_file(&user_path, b"existing content").unwrap();

        let err = ds
            .create_user_link(&datastore_path, &user_path)
            .unwrap_err();
        assert!(
            matches!(err, crate::DodotError::SymlinkConflict { .. }),
            "expected SymlinkConflict, got: {err}"
        );
    }

    #[test]
    fn create_user_link_replaces_wrong_symlink() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);

        let wrong_target = env.data_dir.join("wrong");
        let correct_target = env.data_dir.join("correct");
        let user_path = env.home.join(".vimrc");

        env.fs.mkdir_all(&env.data_dir).unwrap();
        env.fs.write_file(&wrong_target, b"wrong").unwrap();
        env.fs.write_file(&correct_target, b"right").unwrap();

        // Create wrong symlink
        env.fs.symlink(&wrong_target, &user_path).unwrap();

        // Should fix it
        ds.create_user_link(&correct_target, &user_path).unwrap();
        env.assert_symlink(&user_path, &correct_target);
    }

    // ── Double-link chain ───────────────────────────────────────

    #[test]
    fn full_double_link_chain() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);

        let source = env.dotfiles_root.join("vim/vimrc");
        let user_path = env.home.join(".vimrc");

        // Step 1: data link
        let datastore_path = ds.create_data_link("vim", "symlink", &source).unwrap();

        // Step 2: user link
        ds.create_user_link(&datastore_path, &user_path).unwrap();

        // Verify the full chain
        env.assert_double_link("vim", "symlink", "vimrc", &source, &user_path);

        // Reading through the chain should yield the original content
        let content = env.fs.read_to_string(&user_path).unwrap();
        assert_eq!(content, "set nocompatible");
    }

    // ── run_and_record / has_sentinel ───────────────────────────

    #[test]
    fn run_and_record_creates_sentinel() {
        let env = TempEnvironment::builder().build();
        let (ds, runner) = make_datastore(&env);

        assert!(!ds.has_sentinel("vim", "install", "install.sh-abc").unwrap());

        ds.run_and_record(
            "vim",
            "install",
            "echo",
            &["hello".into()],
            "install.sh-abc",
            false,
        )
        .unwrap();

        assert!(ds.has_sentinel("vim", "install", "install.sh-abc").unwrap());
        assert_eq!(runner.calls(), vec!["echo hello"]);

        // Sentinel file should contain "completed|..."
        let sentinel_path = env
            .paths
            .handler_data_dir("vim", "install")
            .join("install.sh-abc");
        let content = env.fs.read_to_string(&sentinel_path).unwrap();
        assert!(content.starts_with("completed|"), "got: {content}");
    }

    #[test]
    fn run_and_record_is_idempotent() {
        let env = TempEnvironment::builder().build();
        let (ds, runner) = make_datastore(&env);

        ds.run_and_record("vim", "install", "echo", &["first".into()], "s1", false)
            .unwrap();
        ds.run_and_record("vim", "install", "echo", &["second".into()], "s1", false)
            .unwrap();

        // Command only ran once
        assert_eq!(runner.calls(), vec!["echo first"]);
    }

    #[test]
    fn run_and_record_propagates_command_failure() {
        let env = TempEnvironment::builder().build();
        let runner = Arc::new(MockCommandRunner::failing());
        let ds = FilesystemDataStore::new(env.fs.clone(), env.paths.clone(), runner);

        let err = ds
            .run_and_record("vim", "install", "bad-cmd", &[], "s1", false)
            .unwrap_err();

        assert!(
            matches!(err, crate::DodotError::CommandFailed { .. }),
            "expected CommandFailed, got: {err}"
        );

        // No sentinel should be created on failure
        assert!(!ds.has_sentinel("vim", "install", "s1").unwrap());
    }

    // ── remove_state ────────────────────────────────────────────

    #[test]
    fn remove_state_clears_handler_dir() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);

        let source = env.dotfiles_root.join("vim/vimrc");
        ds.create_data_link("vim", "symlink", &source).unwrap();
        assert!(ds.has_handler_state("vim", "symlink").unwrap());

        ds.remove_state("vim", "symlink").unwrap();
        env.assert_no_handler_state("vim", "symlink");
    }

    #[test]
    fn remove_state_is_noop_when_no_state() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);

        // Should not error
        ds.remove_state("nonexistent", "handler").unwrap();
    }

    // ── has_handler_state ───────────────────────────────────────

    #[test]
    fn has_handler_state_false_when_no_dir() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);

        assert!(!ds.has_handler_state("vim", "symlink").unwrap());
    }

    #[test]
    fn has_handler_state_false_when_empty_dir() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);

        let dir = env.paths.handler_data_dir("vim", "symlink");
        env.fs.mkdir_all(&dir).unwrap();

        assert!(!ds.has_handler_state("vim", "symlink").unwrap());
    }

    #[test]
    fn has_handler_state_true_when_entries_exist() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);

        let source = env.dotfiles_root.join("vim/vimrc");
        ds.create_data_link("vim", "symlink", &source).unwrap();

        assert!(ds.has_handler_state("vim", "symlink").unwrap());
    }

    // ── list_pack_handlers ──────────────────────────────────────

    #[test]
    fn list_pack_handlers_returns_handler_dirs() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .file("aliases.sh", "y")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);

        let source1 = env.dotfiles_root.join("vim/vimrc");
        let source2 = env.dotfiles_root.join("vim/aliases.sh");
        ds.create_data_link("vim", "symlink", &source1).unwrap();
        ds.create_data_link("vim", "shell", &source2).unwrap();

        let mut handlers = ds.list_pack_handlers("vim").unwrap();
        handlers.sort();
        assert_eq!(handlers, vec!["shell", "symlink"]);
    }

    #[test]
    fn list_pack_handlers_empty_when_no_pack_state() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);

        let handlers = ds.list_pack_handlers("nonexistent").unwrap();
        assert!(handlers.is_empty());
    }

    // ── list_handler_sentinels ──────────────────────────────────

    #[test]
    fn list_handler_sentinels_returns_file_names() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);

        ds.run_and_record(
            "vim",
            "install",
            "echo",
            &["a".into()],
            "install.sh-aaa",
            false,
        )
        .unwrap();
        ds.run_and_record(
            "vim",
            "install",
            "echo",
            &["b".into()],
            "install.sh-bbb",
            false,
        )
        .unwrap();

        let mut sentinels = ds.list_handler_sentinels("vim", "install").unwrap();
        sentinels.sort();
        assert_eq!(sentinels, vec!["install.sh-aaa", "install.sh-bbb"]);
    }

    #[test]
    fn list_handler_sentinels_empty_when_no_state() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);

        let sentinels = ds.list_handler_sentinels("vim", "install").unwrap();
        assert!(sentinels.is_empty());
    }

    // ── write_rendered_file ───────────────────────────────────────

    #[test]
    fn write_rendered_file_creates_regular_file() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);

        let path = ds
            .write_rendered_file("app", "preprocessed", "config.toml", b"host = localhost")
            .unwrap();

        assert!(env.fs.exists(&path));
        assert!(!env.fs.is_symlink(&path));
        assert_eq!(env.fs.read_to_string(&path).unwrap(), "host = localhost");
    }

    #[test]
    fn write_rendered_file_overwrites_existing() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);

        let path1 = ds
            .write_rendered_file("app", "preprocessed", "config.toml", b"version 1")
            .unwrap();
        let path2 = ds
            .write_rendered_file("app", "preprocessed", "config.toml", b"version 2")
            .unwrap();

        assert_eq!(path1, path2);
        assert_eq!(env.fs.read_to_string(&path1).unwrap(), "version 2");
    }

    #[test]
    fn write_rendered_file_empty_content() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);

        let path = ds
            .write_rendered_file("app", "preprocessed", "empty.conf", b"")
            .unwrap();

        assert!(env.fs.exists(&path));
        assert_eq!(env.fs.read_to_string(&path).unwrap(), "");
    }

    #[test]
    fn write_rendered_file_binary_content() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);

        let binary = vec![0u8, 1, 2, 255, 254, 253];
        let path = ds
            .write_rendered_file("app", "preprocessed", "data.bin", &binary)
            .unwrap();

        assert_eq!(env.fs.read_file(&path).unwrap(), binary);
    }

    #[test]
    fn write_rendered_file_creates_parent_dirs() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);

        // handler_data_dir doesn't exist yet — write_rendered_file should create it
        let handler_dir = env.paths.handler_data_dir("newpack", "preprocessed");
        assert!(!env.fs.exists(&handler_dir));

        let path = ds
            .write_rendered_file("newpack", "preprocessed", "file.txt", b"hello")
            .unwrap();

        assert!(env.fs.exists(&path));
        assert_eq!(env.fs.read_to_string(&path).unwrap(), "hello");
    }

    #[test]
    fn write_rendered_file_rejects_absolute_path() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);

        let err = ds
            .write_rendered_file("app", "preprocessed", "/etc/passwd", b"x")
            .unwrap_err();
        assert!(
            matches!(err, crate::DodotError::Other(ref m) if m.contains("unsafe")),
            "expected unsafe-path error, got: {err}"
        );
    }

    #[test]
    fn write_rendered_file_rejects_parent_dir() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);

        let err = ds
            .write_rendered_file("app", "preprocessed", "../escape.txt", b"x")
            .unwrap_err();
        assert!(
            matches!(err, crate::DodotError::Other(ref m) if m.contains("unsafe")),
            "expected unsafe-path error, got: {err}"
        );
    }

    #[test]
    fn write_rendered_dir_creates_dir() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);

        let path = ds
            .write_rendered_dir("app", "preprocessed", "sub/nested")
            .unwrap();

        assert!(env.fs.is_dir(&path));
        assert!(!env.fs.is_symlink(&path));
    }

    #[test]
    fn write_rendered_dir_is_idempotent() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);

        let p1 = ds.write_rendered_dir("app", "preprocessed", "d").unwrap();
        let p2 = ds.write_rendered_dir("app", "preprocessed", "d").unwrap();
        assert_eq!(p1, p2);
        assert!(env.fs.is_dir(&p1));
    }

    #[test]
    fn write_rendered_dir_rejects_unsafe_paths() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);

        assert!(ds
            .write_rendered_dir("app", "preprocessed", "/abs")
            .is_err());
        assert!(ds
            .write_rendered_dir("app", "preprocessed", "../esc")
            .is_err());
    }

    #[test]
    fn write_rendered_file_supports_nested_filename() {
        // A filename containing `/` should be written to the corresponding
        // nested directory under the handler data dir, creating any needed
        // parents. This preserves source structure for preprocessor output.
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);

        let path = ds
            .write_rendered_file("app", "preprocessed", "sub/nested/file.txt", b"deep")
            .unwrap();

        assert!(env.fs.exists(&path));
        assert!(!env.fs.is_symlink(&path));
        assert_eq!(env.fs.read_to_string(&path).unwrap(), "deep");
        assert!(
            path.to_string_lossy().contains("sub/nested/file.txt"),
            "path should contain nested structure: {}",
            path.display()
        );
    }

    // ── Object safety ───────────────────────────────────────────

    #[allow(dead_code)]
    fn assert_object_safe(_: &dyn DataStore) {}
}

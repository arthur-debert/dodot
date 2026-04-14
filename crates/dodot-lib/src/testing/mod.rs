//! Test infrastructure for dodot.
//!
//! Provides [`TempEnvironment`] — an isolated, real-filesystem test
//! environment with builder-pattern setup and rich assertion helpers.
//!
//! # Example
//!
//! ```rust,ignore
//! let env = TempEnvironment::builder()
//!     .pack("vim")
//!         .file("vimrc", "set nocompatible")
//!         .file("aliases.sh", "alias vi=vim")
//!         .done()
//!     .pack("git")
//!         .file("gitconfig", "[user]\n  name = test")
//!         .done()
//!     .build();
//!
//! assert!(env.fs.exists(&env.dotfiles_root.join("vim/vimrc")));
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tempfile::TempDir;

use crate::fs::{Fs, OsFs};
use crate::paths::{Pather, XdgPather};

/// An isolated test environment backed by real filesystem operations
/// inside a temporary directory.
///
/// All XDG paths, HOME, and DOTFILES_ROOT point inside the temp dir.
/// The directory is cleaned up when this struct is dropped.
pub struct TempEnvironment {
    /// Held to keep the temp directory alive for the lifetime of the env.
    _temp_dir: TempDir,

    /// Simulated HOME directory.
    pub home: PathBuf,

    /// Simulated dotfiles root (DOTFILES_ROOT).
    pub dotfiles_root: PathBuf,

    /// Simulated XDG data dir for dodot.
    pub data_dir: PathBuf,

    /// Simulated XDG config home (e.g. for symlink target mapping).
    pub config_home: PathBuf,

    /// Real filesystem handle.
    pub fs: Arc<OsFs>,

    /// Path resolver with all paths pointing at the temp directory.
    pub paths: Arc<XdgPather>,
}

impl TempEnvironment {
    /// Start building a new test environment.
    pub fn builder() -> TempEnvironmentBuilder {
        TempEnvironmentBuilder {
            packs: Vec::new(),
            extra_home_files: Vec::new(),
        }
    }

    // ── Assertion helpers ───────────────────────────────────────────

    /// Assert that a symlink exists at `link` and points to `target`.
    pub fn assert_symlink(&self, link: &Path, target: &Path) {
        assert!(
            self.fs.is_symlink(link),
            "expected symlink at {}, but it is not a symlink",
            link.display()
        );
        let actual_target = self
            .fs
            .readlink(link)
            .unwrap_or_else(|e| panic!("failed to readlink {}: {e}", link.display()));
        assert_eq!(
            actual_target, target,
            "symlink {} points to {}, expected {}",
            link.display(),
            actual_target.display(),
            target.display()
        );
    }

    /// Assert that a file exists at `path` with exactly `expected` contents.
    pub fn assert_file_contents(&self, path: &Path, expected: &str) {
        let actual = self
            .fs
            .read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        assert_eq!(
            actual, expected,
            "file {} has unexpected contents",
            path.display()
        );
    }

    /// Assert that a file or directory exists at `path`.
    pub fn assert_exists(&self, path: &Path) {
        assert!(
            self.fs.exists(path),
            "expected {} to exist, but it does not",
            path.display()
        );
    }

    /// Assert that nothing exists at `path`.
    pub fn assert_not_exists(&self, path: &Path) {
        assert!(
            !self.fs.exists(path),
            "expected {} to not exist, but it does",
            path.display()
        );
    }

    /// Assert that `path` is a directory.
    pub fn assert_dir_exists(&self, path: &Path) {
        assert!(
            self.fs.is_dir(path),
            "expected {} to be a directory",
            path.display()
        );
    }

    /// Assert the full double-link chain used by dodot:
    /// `source -> datastore_link -> user_link`.
    ///
    /// - `source`: the original file inside the pack
    /// - The datastore link at `handler_data_dir(pack, handler) / filename`
    ///   must point to `source`
    /// - `user_path` must be a symlink pointing to the datastore link
    pub fn assert_double_link(
        &self,
        pack: &str,
        handler: &str,
        filename: &str,
        source: &Path,
        user_path: &Path,
    ) {
        let datastore_link = self.paths.handler_data_dir(pack, handler).join(filename);

        // Datastore link -> source
        self.assert_symlink(&datastore_link, source);

        // User link -> datastore link
        self.assert_symlink(user_path, &datastore_link);
    }

    /// Assert that no state exists for a pack/handler pair
    /// (i.e. the handler data directory does not exist or is empty).
    pub fn assert_no_handler_state(&self, pack: &str, handler: &str) {
        let dir = self.paths.handler_data_dir(pack, handler);
        if self.fs.exists(&dir) {
            let entries = self.fs.read_dir(&dir).unwrap_or_default();
            assert!(
                entries.is_empty(),
                "expected no state for {pack}/{handler}, but found {} entries in {}",
                entries.len(),
                dir.display()
            );
        }
    }

    /// Assert that a sentinel file exists for a pack/handler.
    pub fn assert_sentinel(&self, pack: &str, handler: &str, sentinel: &str) {
        let sentinel_path = self.paths.handler_data_dir(pack, handler).join(sentinel);
        assert!(
            self.fs.exists(&sentinel_path),
            "expected sentinel {} at {}",
            sentinel,
            sentinel_path.display()
        );
    }

    /// Returns the list of file names in a directory.
    pub fn list_dir_names(&self, path: &Path) -> Vec<String> {
        self.fs
            .read_dir(path)
            .unwrap_or_default()
            .into_iter()
            .map(|e| e.name)
            .collect()
    }
}

// ── Builder ─────────────────────────────────────────────────────────

/// Builder for [`TempEnvironment`].
pub struct TempEnvironmentBuilder {
    packs: Vec<PackSpec>,
    extra_home_files: Vec<(String, String)>,
}

struct PackSpec {
    name: String,
    files: Vec<(String, String)>,
    config: Option<String>,
    dodotignore: bool,
}

impl TempEnvironmentBuilder {
    /// Start defining a pack. Returns a [`PackSpecBuilder`] that must
    /// be finished with [`.done()`](PackSpecBuilder::done).
    pub fn pack(self, name: &str) -> PackSpecBuilder {
        PackSpecBuilder {
            parent: self,
            name: name.to_string(),
            files: Vec::new(),
            config: None,
            dodotignore: false,
        }
    }

    /// Add a file under the simulated HOME directory.
    /// Useful for testing adopt (moving existing files into packs).
    pub fn home_file(mut self, relative_path: &str, contents: &str) -> Self {
        self.extra_home_files
            .push((relative_path.to_string(), contents.to_string()));
        self
    }

    /// Build the environment, creating all directories and files.
    pub fn build(self) -> TempEnvironment {
        let temp_dir = TempDir::new().expect("failed to create temp directory");
        let fs = Arc::new(OsFs::new());

        // Set up directory hierarchy
        let home = temp_dir.path().join("home");
        let dotfiles_root = home.join("dotfiles");
        let data_dir = home.join(".local").join("share").join("dodot");
        let config_home = home.join(".config");
        let cache_dir = home.join(".cache").join("dodot");
        let shell_dir = data_dir.join("shell");
        let packs_data_dir = data_dir.join("packs");

        // Create base directories
        for dir in [
            &home,
            &dotfiles_root,
            &data_dir,
            &config_home,
            &cache_dir,
            &shell_dir,
            &packs_data_dir,
        ] {
            fs.mkdir_all(dir)
                .unwrap_or_else(|e| panic!("failed to create {}: {e}", dir.display()));
        }

        // Create packs
        for pack in &self.packs {
            let pack_dir = dotfiles_root.join(&pack.name);
            fs.mkdir_all(&pack_dir).unwrap();

            for (rel_path, contents) in &pack.files {
                let file_path = pack_dir.join(rel_path);
                if let Some(parent) = file_path.parent() {
                    fs.mkdir_all(parent).unwrap();
                }
                fs.write_file(&file_path, contents.as_bytes()).unwrap();
            }

            if let Some(config_toml) = &pack.config {
                let config_path = pack_dir.join(".dodot.toml");
                fs.write_file(&config_path, config_toml.as_bytes())
                    .unwrap();
            }

            if pack.dodotignore {
                let ignore_path = pack_dir.join(".dodotignore");
                fs.write_file(&ignore_path, b"").unwrap();
            }
        }

        // Create extra home files
        for (rel_path, contents) in &self.extra_home_files {
            let file_path = home.join(rel_path);
            if let Some(parent) = file_path.parent() {
                fs.mkdir_all(parent).unwrap();
            }
            fs.write_file(&file_path, contents.as_bytes()).unwrap();
        }

        // Build the pather with all paths pointing inside temp dir
        let paths = Arc::new(
            XdgPather::builder()
                .home(&home)
                .dotfiles_root(&dotfiles_root)
                .data_dir(&data_dir)
                .config_dir(config_home.join("dodot"))
                .cache_dir(&cache_dir)
                .xdg_config_home(&config_home)
                .build()
                .expect("failed to build XdgPather for test environment"),
        );

        TempEnvironment {
            _temp_dir: temp_dir,
            home,
            dotfiles_root,
            data_dir,
            config_home,
            fs,
            paths,
        }
    }
}

/// Builder for a single pack within a [`TempEnvironmentBuilder`].
pub struct PackSpecBuilder {
    parent: TempEnvironmentBuilder,
    name: String,
    files: Vec<(String, String)>,
    config: Option<String>,
    dodotignore: bool,
}

impl PackSpecBuilder {
    /// Add a file to this pack.
    pub fn file(mut self, relative_path: &str, contents: &str) -> Self {
        self.files
            .push((relative_path.to_string(), contents.to_string()));
        self
    }

    /// Set the `.dodot.toml` config for this pack.
    pub fn config(mut self, toml_contents: &str) -> Self {
        self.config = Some(toml_contents.to_string());
        self
    }

    /// Mark this pack as ignored (creates `.dodotignore`).
    pub fn ignored(mut self) -> Self {
        self.dodotignore = true;
        self
    }

    /// Finish this pack and return to the parent builder.
    pub fn done(mut self) -> TempEnvironmentBuilder {
        self.parent.packs.push(PackSpec {
            name: self.name,
            files: self.files,
            config: self.config,
            dodotignore: self.dodotignore,
        });
        self.parent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_creates_directory_structure() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .file("gvimrc", "set guifont=Mono")
            .done()
            .pack("git")
            .file("gitconfig", "[user]\n  name = test")
            .done()
            .build();

        // Home and dotfiles root exist
        env.assert_dir_exists(&env.home);
        env.assert_dir_exists(&env.dotfiles_root);

        // Pack directories exist
        env.assert_dir_exists(&env.dotfiles_root.join("vim"));
        env.assert_dir_exists(&env.dotfiles_root.join("git"));

        // Files have correct contents
        env.assert_file_contents(
            &env.dotfiles_root.join("vim/vimrc"),
            "set nocompatible",
        );
        env.assert_file_contents(
            &env.dotfiles_root.join("vim/gvimrc"),
            "set guifont=Mono",
        );
        env.assert_file_contents(
            &env.dotfiles_root.join("git/gitconfig"),
            "[user]\n  name = test",
        );

        // XDG directories exist
        env.assert_dir_exists(&env.data_dir);
        env.assert_dir_exists(&env.config_home);
    }

    #[test]
    fn builder_creates_nested_files() {
        let env = TempEnvironment::builder()
            .pack("nvim")
            .file("nvim/init.lua", "require('config')")
            .file("nvim/lua/config.lua", "return {}")
            .done()
            .build();

        env.assert_file_contents(
            &env.dotfiles_root.join("nvim/nvim/init.lua"),
            "require('config')",
        );
        env.assert_file_contents(
            &env.dotfiles_root.join("nvim/nvim/lua/config.lua"),
            "return {}",
        );
    }

    #[test]
    fn builder_creates_pack_config() {
        let env = TempEnvironment::builder()
            .pack("shell")
            .file("aliases.sh", "alias ll='ls -la'")
            .config("[pack]\nignore = [\"*.bak\"]")
            .done()
            .build();

        env.assert_file_contents(
            &env.dotfiles_root.join("shell/.dodot.toml"),
            "[pack]\nignore = [\"*.bak\"]",
        );
    }

    #[test]
    fn builder_creates_dodotignore() {
        let env = TempEnvironment::builder()
            .pack("disabled")
            .file("something", "x")
            .ignored()
            .done()
            .build();

        env.assert_exists(&env.dotfiles_root.join("disabled/.dodotignore"));
    }

    #[test]
    fn builder_creates_home_files() {
        let env = TempEnvironment::builder()
            .home_file(".bashrc", "# my bashrc")
            .home_file(".config/nvim/init.lua", "-- nvim config")
            .build();

        env.assert_file_contents(&env.home.join(".bashrc"), "# my bashrc");
        env.assert_file_contents(
            &env.home.join(".config/nvim/init.lua"),
            "-- nvim config",
        );
    }

    #[test]
    fn pather_points_at_temp_dirs() {
        let env = TempEnvironment::builder().build();

        assert_eq!(env.paths.home_dir(), env.home);
        assert_eq!(env.paths.dotfiles_root(), env.dotfiles_root);
        assert_eq!(env.paths.data_dir(), env.data_dir);
        assert_eq!(env.paths.xdg_config_home(), env.config_home);
    }

    #[test]
    fn handler_data_dir_within_temp() {
        let env = TempEnvironment::builder().build();

        let dir = env.paths.handler_data_dir("vim", "symlink");
        assert!(
            dir.starts_with(&env.data_dir),
            "handler_data_dir {} should be under data_dir {}",
            dir.display(),
            env.data_dir.display()
        );
        assert!(dir.ends_with("packs/vim/symlink"));
    }

    #[test]
    fn assert_symlink_works() {
        let env = TempEnvironment::builder().build();

        let original = env.home.join("original.txt");
        let link = env.home.join("link.txt");
        env.fs.write_file(&original, b"content").unwrap();
        env.fs.symlink(&original, &link).unwrap();

        env.assert_symlink(&link, &original);
    }

    #[test]
    fn assert_no_handler_state_passes_when_empty() {
        let env = TempEnvironment::builder().build();

        // No state dir exists at all -- should pass
        env.assert_no_handler_state("vim", "symlink");

        // Empty state dir -- should also pass
        let dir = env.paths.handler_data_dir("vim", "symlink");
        env.fs.mkdir_all(&dir).unwrap();
        env.assert_no_handler_state("vim", "symlink");
    }

    #[test]
    #[should_panic(expected = "expected no state")]
    fn assert_no_handler_state_fails_when_state_exists() {
        let env = TempEnvironment::builder().build();

        let dir = env.paths.handler_data_dir("vim", "symlink");
        env.fs.mkdir_all(&dir).unwrap();
        env.fs.write_file(&dir.join("vimrc"), b"link").unwrap();

        env.assert_no_handler_state("vim", "symlink");
    }

    #[test]
    fn assert_sentinel_works() {
        let env = TempEnvironment::builder().build();

        let dir = env.paths.handler_data_dir("vim", "install");
        env.fs.mkdir_all(&dir).unwrap();
        env.fs
            .write_file(&dir.join("install.sh-abc123"), b"completed|2026-01-01")
            .unwrap();

        env.assert_sentinel("vim", "install", "install.sh-abc123");
    }

    #[test]
    fn multiple_environments_coexist() {
        let env1 = TempEnvironment::builder()
            .pack("a")
            .file("f1", "one")
            .done()
            .build();

        let env2 = TempEnvironment::builder()
            .pack("b")
            .file("f2", "two")
            .done()
            .build();

        // Each has its own isolated dotfiles
        env1.assert_exists(&env1.dotfiles_root.join("a/f1"));
        env1.assert_not_exists(&env1.dotfiles_root.join("b"));

        env2.assert_exists(&env2.dotfiles_root.join("b/f2"));
        env2.assert_not_exists(&env2.dotfiles_root.join("a"));

        // Different temp directories
        assert_ne!(env1.home, env2.home);
    }

    #[test]
    fn list_dir_names_helper() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "")
            .file("gvimrc", "")
            .done()
            .build();

        let mut names = env.list_dir_names(&env.dotfiles_root.join("vim"));
        names.sort();
        assert_eq!(names, vec!["gvimrc", "vimrc"]);
    }

    #[test]
    fn empty_environment_has_basic_structure() {
        let env = TempEnvironment::builder().build();

        env.assert_dir_exists(&env.home);
        env.assert_dir_exists(&env.dotfiles_root);
        env.assert_dir_exists(&env.data_dir);
        env.assert_dir_exists(&env.config_home);
        env.assert_dir_exists(env.paths.shell_dir());
    }
}

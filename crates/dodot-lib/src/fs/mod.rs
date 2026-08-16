mod os;

pub use os::OsFs;

use std::path::{Path, PathBuf};

use crate::Result;

/// Metadata about a filesystem entry.
#[derive(Debug, Clone)]
pub struct FsMetadata {
    pub is_file: bool,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub len: u64,
    /// Unix permission mode (e.g. `0o755`).
    pub mode: u32,
}

/// A single directory entry returned by [`Fs::read_dir`].
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub is_file: bool,
    pub is_symlink: bool,
}

/// Filesystem abstraction.
///
/// All dodot code accesses the filesystem through this trait so that:
/// - Tests can use isolated temp directories with a real implementation
/// - Every `io::Error` is wrapped with the path that caused it
///
/// Use `&dyn Fs` (trait objects) throughout the codebase. The operations
/// are I/O-bound so dynamic dispatch costs nothing meaningful, and generics
/// would infect every type signature.
pub trait Fs: Send + Sync {
    /// Returns metadata for the path, following symlinks.
    fn stat(&self, path: &Path) -> Result<FsMetadata>;

    /// Returns metadata for the path without following symlinks.
    fn lstat(&self, path: &Path) -> Result<FsMetadata>;

    /// Opens the file for reading in a streaming fashion.
    ///
    /// Errors that occur while opening the file are returned through this
    /// method's [`Result`] and include path context. Once opened, the
    /// returned reader is a raw [`std::io::Read`], so any later `read()`
    /// errors are reported as plain [`std::io::Error`] values and are not
    /// automatically wrapped with the path.
    fn open_read(&self, path: &Path) -> Result<Box<dyn std::io::Read + Send + Sync>>;

    /// Reads the entire file into bytes.
    fn read_file(&self, path: &Path) -> Result<Vec<u8>>;

    /// Reads the entire file as a UTF-8 string.
    fn read_to_string(&self, path: &Path) -> Result<String>;

    /// Writes `contents` to `path`, creating or truncating the file.
    fn write_file(&self, path: &Path, contents: &[u8]) -> Result<()>;

    /// Writes `contents` to `path`, creating or truncating the file
    /// **with `mode` applied at creation time** (not via a follow-up
    /// `chmod`). Used by whole-file secret preprocessors so the
    /// rendered plaintext never lives at the umask-default mode,
    /// even briefly — closing the race window between
    /// `write_file` (lands at e.g. 0644 on a typical 022 umask)
    /// and `set_permissions` (tightens to 0600). See
    /// `secrets.lex` §4.3.
    ///
    /// The default impl is the racy `write_file` +
    /// `set_permissions` pair; `OsFs` overrides it to apply the
    /// mode via `OpenOptions::mode` instead.
    fn write_file_with_mode(&self, path: &Path, contents: &[u8], mode: u32) -> Result<()> {
        self.write_file(path, contents)?;
        self.set_permissions(path, mode)
    }

    /// Writes `contents` to `path` through a temp sibling renamed into
    /// place, so a concurrent reader never sees a partial file.
    ///
    /// `write_file` truncates in place: a reader that opens the file
    /// mid-write gets a prefix of it, cut at an arbitrary byte. That
    /// is fine for files nothing reads concurrently, but wrong for
    /// any artifact with a live reader — the shell init script and
    /// the Homebrew cache are both sourced/read by *every shell the
    /// user opens* while `dodot up` may be rewriting them. Use this
    /// method for any such file.
    ///
    /// The temp is a sibling (same directory ⇒ same filesystem ⇒ the
    /// rename is atomic on POSIX) with a pid+nonce suffix (two racing
    /// writers cannot write one another's temp half-finished). A
    /// failed rename removes the temp rather than abandoning it; an
    /// interrupted write leaves only the temp behind, never a torn
    /// target.
    ///
    /// Note the trade-off: the rename replaces the *directory entry*,
    /// so `path` gets a fresh inode. Do not use this on files the
    /// user may hold as a hard link or that must stay a symlink —
    /// e.g. rc files, which are commonly symlinks into a dotfiles
    /// repo that a rename would silently replace with a regular file.
    fn write_atomic(&self, path: &Path, contents: &[u8]) -> Result<()> {
        let tmp = temp_sibling(path);
        self.write_file(&tmp, contents)?;
        if let Err(e) = self.rename(&tmp, path) {
            let _ = self.remove_file(&tmp);
            return Err(e);
        }
        Ok(())
    }

    /// [`Fs::write_atomic`], with `mode` applied to the temp file
    /// **before** the rename — the visible file carries the intended
    /// mode from its first instant, never a umask-default one. Used
    /// for the init script, which is documented (and tested) as
    /// executable.
    fn write_atomic_with_mode(&self, path: &Path, contents: &[u8], mode: u32) -> Result<()> {
        let tmp = temp_sibling(path);
        self.write_file_with_mode(&tmp, contents, mode)?;
        if let Err(e) = self.rename(&tmp, path) {
            let _ = self.remove_file(&tmp);
            return Err(e);
        }
        Ok(())
    }

    /// Creates `path` and all parent directories.
    fn mkdir_all(&self, path: &Path) -> Result<()>;

    /// Creates a symbolic link at `link` pointing to `original`.
    fn symlink(&self, original: &Path, link: &Path) -> Result<()>;

    /// Reads the target of a symbolic link.
    fn readlink(&self, path: &Path) -> Result<PathBuf>;

    /// Removes a file or symlink (not a directory).
    fn remove_file(&self, path: &Path) -> Result<()>;

    /// Removes a directory and all of its contents.
    fn remove_dir_all(&self, path: &Path) -> Result<()>;

    /// Returns `true` if `path` exists (follows symlinks).
    fn exists(&self, path: &Path) -> bool;

    /// Returns `true` if `path` is a symlink (does not follow).
    fn is_symlink(&self, path: &Path) -> bool;

    /// Returns `true` if `path` is a directory (follows symlinks).
    fn is_dir(&self, path: &Path) -> bool;

    /// Lists entries in a directory, sorted by name.
    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>>;

    /// Renames (moves) `from` to `to`.
    fn rename(&self, from: &Path, to: &Path) -> Result<()>;

    /// Copies a file from `from` to `to`.
    fn copy_file(&self, from: &Path, to: &Path) -> Result<()>;

    /// Sets file permissions (Unix mode).
    fn set_permissions(&self, path: &Path, mode: u32) -> Result<()>;

    /// Returns the modification time of `path` (follows symlinks).
    /// Used by `dodot refresh` to compare deployed-side mtimes against
    /// source-side mtimes when deciding whether to touch the source.
    ///
    /// **Default implementation panics.** Override in `Fs` impls that
    /// need mtime support (currently `OsFs`).
    fn modified(&self, _path: &Path) -> Result<std::time::SystemTime> {
        unimplemented!("Fs::modified is only implemented by OsFs")
    }

    /// Sets the modification time of `path` to `time`. Used by
    /// `dodot refresh` to copy the deployed file's mtime onto the
    /// template source so git's stat-cache invalidates and the next
    /// `git status` re-reads the file (invoking the clean filter on
    /// repos that have it installed).
    ///
    /// **Default implementation panics.** Override in `Fs` impls that
    /// need mtime support (currently `OsFs`).
    fn set_modified(&self, _path: &Path, _time: std::time::SystemTime) -> Result<()> {
        unimplemented!("Fs::set_modified is only implemented by OsFs")
    }
}

/// A dotted temp path in `path`'s own directory, for the write-then-
/// rename in [`Fs::write_atomic`]. Same directory means same
/// filesystem, which is what makes the rename atomic; the pid+nonce
/// means two writers racing each other cannot write one another's
/// temp half-finished.
fn temp_sibling(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new("."));
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    parent.join(format!(
        ".dodot-{name}.{}-{nonce:x}.tmp",
        std::process::id()
    ))
}

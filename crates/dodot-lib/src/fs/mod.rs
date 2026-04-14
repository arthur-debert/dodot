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

    /// Reads the entire file into bytes.
    fn read_file(&self, path: &Path) -> Result<Vec<u8>>;

    /// Reads the entire file as a UTF-8 string.
    fn read_to_string(&self, path: &Path) -> Result<String>;

    /// Writes `contents` to `path`, creating or truncating the file.
    fn write_file(&self, path: &Path, contents: &[u8]) -> Result<()>;

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
}

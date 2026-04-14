use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::error::fs_err;
use crate::fs::{DirEntry, Fs, FsMetadata};
use crate::Result;

/// Filesystem implementation that delegates to `std::fs`.
///
/// Every `io::Error` is wrapped with the path that caused it via
/// [`DodotError::Fs`](crate::DodotError::Fs).
#[derive(Debug, Clone, Copy)]
pub struct OsFs;

impl OsFs {
    pub fn new() -> Self {
        Self
    }
}

impl Default for OsFs {
    fn default() -> Self {
        Self::new()
    }
}

impl Fs for OsFs {
    fn stat(&self, path: &Path) -> Result<FsMetadata> {
        let meta = fs::metadata(path).map_err(|e| fs_err(path, e))?;
        Ok(metadata_from_std(&meta, false))
    }

    fn lstat(&self, path: &Path) -> Result<FsMetadata> {
        let meta = fs::symlink_metadata(path).map_err(|e| fs_err(path, e))?;
        let is_symlink = meta.file_type().is_symlink();
        Ok(metadata_from_std(&meta, is_symlink))
    }

    fn read_file(&self, path: &Path) -> Result<Vec<u8>> {
        fs::read(path).map_err(|e| fs_err(path, e))
    }

    fn read_to_string(&self, path: &Path) -> Result<String> {
        fs::read_to_string(path).map_err(|e| fs_err(path, e))
    }

    fn write_file(&self, path: &Path, contents: &[u8]) -> Result<()> {
        fs::write(path, contents).map_err(|e| fs_err(path, e))
    }

    fn mkdir_all(&self, path: &Path) -> Result<()> {
        fs::create_dir_all(path).map_err(|e| fs_err(path, e))
    }

    fn symlink(&self, original: &Path, link: &Path) -> Result<()> {
        std::os::unix::fs::symlink(original, link).map_err(|e| fs_err(link, e))
    }

    fn readlink(&self, path: &Path) -> Result<PathBuf> {
        fs::read_link(path).map_err(|e| fs_err(path, e))
    }

    fn remove_file(&self, path: &Path) -> Result<()> {
        fs::remove_file(path).map_err(|e| fs_err(path, e))
    }

    fn remove_dir_all(&self, path: &Path) -> Result<()> {
        fs::remove_dir_all(path).map_err(|e| fs_err(path, e))
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_symlink(&self, path: &Path) -> bool {
        path.symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>> {
        let entries = fs::read_dir(path).map_err(|e| fs_err(path, e))?;

        let mut result = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| fs_err(path, e))?;
            let file_type = entry.file_type().map_err(|e| fs_err(entry.path(), e))?;
            let name = entry.file_name().to_string_lossy().into_owned();

            result.push(DirEntry {
                path: entry.path(),
                name,
                is_dir: file_type.is_dir(),
                is_file: file_type.is_file(),
                is_symlink: file_type.is_symlink(),
            });
        }

        result.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(result)
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        fs::rename(from, to).map_err(|e| fs_err(from, e))
    }

    fn copy_file(&self, from: &Path, to: &Path) -> Result<()> {
        fs::copy(from, to)
            .map(|_| ())
            .map_err(|e| fs_err(from, e))
    }

    fn set_permissions(&self, path: &Path, mode: u32) -> Result<()> {
        let perms = fs::Permissions::from_mode(mode);
        fs::set_permissions(path, perms).map_err(|e| fs_err(path, e))
    }
}

fn metadata_from_std(meta: &fs::Metadata, is_symlink: bool) -> FsMetadata {
    FsMetadata {
        is_file: meta.is_file(),
        is_dir: meta.is_dir(),
        is_symlink,
        len: meta.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn write_and_read_file() {
        let tmp = TempDir::new().unwrap();
        let fs = OsFs::new();
        let path = tmp.path().join("hello.txt");

        fs.write_file(&path, b"hello world").unwrap();
        let contents = fs.read_to_string(&path).unwrap();
        assert_eq!(contents, "hello world");
    }

    #[test]
    fn read_file_bytes() {
        let tmp = TempDir::new().unwrap();
        let fs = OsFs::new();
        let path = tmp.path().join("data.bin");

        let data = vec![0u8, 1, 2, 255];
        fs.write_file(&path, &data).unwrap();
        let read_back = fs.read_file(&path).unwrap();
        assert_eq!(read_back, data);
    }

    #[test]
    fn mkdir_all_creates_nested_dirs() {
        let tmp = TempDir::new().unwrap();
        let fs = OsFs::new();
        let deep = tmp.path().join("a").join("b").join("c");

        fs.mkdir_all(&deep).unwrap();
        assert!(fs.is_dir(&deep));
    }

    #[test]
    fn symlink_and_readlink_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let fs = OsFs::new();
        let original = tmp.path().join("original.txt");
        let link = tmp.path().join("link.txt");

        fs.write_file(&original, b"content").unwrap();
        fs.symlink(&original, &link).unwrap();

        assert!(fs.is_symlink(&link));
        assert_eq!(fs.readlink(&link).unwrap(), original);

        // Reading through the symlink works
        let content = fs.read_to_string(&link).unwrap();
        assert_eq!(content, "content");
    }

    #[test]
    fn stat_follows_symlinks() {
        let tmp = TempDir::new().unwrap();
        let fs = OsFs::new();
        let original = tmp.path().join("file.txt");
        let link = tmp.path().join("link.txt");

        fs.write_file(&original, b"data").unwrap();
        fs.symlink(&original, &link).unwrap();

        let meta = fs.stat(&link).unwrap();
        assert!(meta.is_file);
        assert!(!meta.is_symlink);
    }

    #[test]
    fn lstat_does_not_follow_symlinks() {
        let tmp = TempDir::new().unwrap();
        let fs = OsFs::new();
        let original = tmp.path().join("file.txt");
        let link = tmp.path().join("link.txt");

        fs.write_file(&original, b"data").unwrap();
        fs.symlink(&original, &link).unwrap();

        let meta = fs.lstat(&link).unwrap();
        assert!(meta.is_symlink);
    }

    #[test]
    fn exists_and_is_dir() {
        let tmp = TempDir::new().unwrap();
        let fs = OsFs::new();

        assert!(fs.exists(tmp.path()));
        assert!(fs.is_dir(tmp.path()));
        assert!(!fs.exists(&tmp.path().join("nope")));
    }

    #[test]
    fn read_dir_sorted() {
        let tmp = TempDir::new().unwrap();
        let fs = OsFs::new();

        fs.write_file(&tmp.path().join("c.txt"), b"").unwrap();
        fs.write_file(&tmp.path().join("a.txt"), b"").unwrap();
        fs.write_file(&tmp.path().join("b.txt"), b"").unwrap();

        let entries = fs.read_dir(tmp.path()).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["a.txt", "b.txt", "c.txt"]);
    }

    #[test]
    fn remove_file_and_remove_dir_all() {
        let tmp = TempDir::new().unwrap();
        let fs = OsFs::new();

        let file = tmp.path().join("file.txt");
        fs.write_file(&file, b"x").unwrap();
        assert!(fs.exists(&file));
        fs.remove_file(&file).unwrap();
        assert!(!fs.exists(&file));

        let dir = tmp.path().join("subdir");
        fs.mkdir_all(&dir.join("nested")).unwrap();
        fs.write_file(&dir.join("nested").join("f.txt"), b"y").unwrap();
        assert!(fs.exists(&dir));
        fs.remove_dir_all(&dir).unwrap();
        assert!(!fs.exists(&dir));
    }

    #[test]
    fn rename_file() {
        let tmp = TempDir::new().unwrap();
        let fs = OsFs::new();

        let from = tmp.path().join("old.txt");
        let to = tmp.path().join("new.txt");
        fs.write_file(&from, b"moved").unwrap();
        fs.rename(&from, &to).unwrap();

        assert!(!fs.exists(&from));
        assert_eq!(fs.read_to_string(&to).unwrap(), "moved");
    }

    #[test]
    fn copy_file_preserves_content() {
        let tmp = TempDir::new().unwrap();
        let fs = OsFs::new();

        let from = tmp.path().join("src.txt");
        let to = tmp.path().join("dst.txt");
        fs.write_file(&from, b"copied").unwrap();
        fs.copy_file(&from, &to).unwrap();

        assert!(fs.exists(&from));
        assert_eq!(fs.read_to_string(&to).unwrap(), "copied");
    }

    #[test]
    fn error_contains_path() {
        let fs = OsFs::new();
        let bad_path = Path::new("/nonexistent/path/to/file.txt");

        let err = fs.read_file(bad_path).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("/nonexistent/path/to/file.txt"),
            "error should contain the path: {msg}"
        );
    }

    #[test]
    fn set_permissions_works() {
        let tmp = TempDir::new().unwrap();
        let fs = OsFs::new();

        let file = tmp.path().join("script.sh");
        fs.write_file(&file, b"#!/bin/sh").unwrap();
        fs.set_permissions(&file, 0o755).unwrap();

        let meta = std::fs::metadata(&file).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o755);
    }

    // Compile-time check: Fs must be object-safe
    #[allow(dead_code)]
    fn assert_object_safe(_: &dyn Fs) {}
}

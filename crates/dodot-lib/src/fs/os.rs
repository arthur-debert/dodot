use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::error::{fs_between_err, fs_err};
use crate::fs::{DirEntry, FileId, Fs, FsMetadata};
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

    fn open_read(&self, path: &Path) -> Result<Box<dyn std::io::Read + Send + Sync>> {
        let f = fs::File::open(path).map_err(|e| fs_err(path, e))?;
        Ok(Box::new(f))
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

    fn write_file_with_mode(&self, path: &Path, contents: &[u8], mode: u32) -> Result<()> {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt;
        // Open with `mode` set at creation time so the file never
        // lives at the umask-default mode. Truncates if the file
        // already exists; mode is applied to a freshly-created
        // file but NOT to an existing one (POSIX `open(2)`
        // semantics) — we set it explicitly afterward so
        // overwriting an existing 0644 file still ends at the
        // requested mode. The window between the truncating open
        // and the chmod is narrower than `write_file` +
        // `set_permissions` because no bytes have been written
        // yet (no readable plaintext at risk).
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(mode)
            .open(path)
            .map_err(|e| fs_err(path, e))?;
        let perms = fs::Permissions::from_mode(mode);
        fs::set_permissions(path, perms).map_err(|e| fs_err(path, e))?;
        file.write_all(contents).map_err(|e| fs_err(path, e))?;
        file.sync_all().map_err(|e| fs_err(path, e))?;
        Ok(())
    }

    fn mkdir_all(&self, path: &Path) -> Result<()> {
        fs::create_dir_all(path).map_err(|e| fs_err(path, e))
    }

    fn mkdir_exclusive(&self, path: &Path) -> Result<()> {
        // `create_dir` is one `mkdir(2)`, which fails with EEXIST
        // rather than adopting whatever is already at `path`.
        fs::create_dir(path).map_err(|e| fs_err(path, e))
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

    fn remove_dir_empty(&self, path: &Path) -> Result<()> {
        fs::remove_dir(path).map_err(|e| fs_err(path, e))
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
        fs::rename(from, to).map_err(|e| fs_between_err("renaming", from, to, e))
    }

    fn rename_noreplace(&self, from: &Path, to: &Path) -> Result<()> {
        rename_noreplace_raw(from, to).map_err(|e| fs_between_err("renaming", from, to, e))
    }

    fn copy_file(&self, from: &Path, to: &Path) -> Result<()> {
        fs::copy(from, to)
            .map(|_| ())
            .map_err(|e| fs_between_err("copying", from, to, e))
    }

    fn set_permissions(&self, path: &Path, mode: u32) -> Result<()> {
        let perms = fs::Permissions::from_mode(mode);
        fs::set_permissions(path, perms).map_err(|e| fs_err(path, e))
    }

    fn modified(&self, path: &Path) -> Result<std::time::SystemTime> {
        fs::metadata(path)
            .and_then(|m| m.modified())
            .map_err(|e| fs_err(path, e))
    }

    fn set_modified(&self, path: &Path, time: std::time::SystemTime) -> Result<()> {
        // `File::set_modified` (stable since 1.75) needs an existing
        // file handle. We deliberately open with `.write(true)` (NOT
        // `.create(true)`, NOT `.truncate(true)`) so we get a handle
        // to the existing file without touching its content.
        let file = fs::OpenOptions::new()
            .write(true)
            .open(path)
            .map_err(|e| fs_err(path, e))?;
        file.set_modified(time).map_err(|e| fs_err(path, e))
    }
}

/// `rename` that refuses to replace an existing destination, decided
/// inside the one operation that moves the file.
///
/// std exposes no wrapper for it: the flag lives in a platform
/// extension of `rename` on both systems dodot supports — `renameat2`
/// with `RENAME_NOREPLACE` on Linux (kernel 3.15+), `renamex_np` with
/// `RENAME_EXCL` on macOS (10.12+). Nothing portable can stand in:
/// plain `rename` replaces its destination, so any emulation built on
/// a separate existence test reopens the very window this closes.
///
/// A filesystem driver that does not implement the flag answers
/// `EINVAL`/`ENOSYS` (Linux) or `ENOTSUP` (macOS), and that error
/// reaches the caller — the operation never silently degrades into a
/// replacing rename.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn rename_noreplace_raw(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::io;
    use std::os::unix::ffi::OsStrExt;

    let cstr = |p: &Path| {
        CString::new(p.as_os_str().as_bytes())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))
    };
    let from_c = cstr(from)?;
    let to_c = cstr(to)?;

    #[cfg(target_os = "linux")]
    let rc = {
        // Called as a raw syscall rather than through the libc
        // `renameat2` wrapper: that wrapper is a glibc 2.28 symbol, and
        // linking against it would refuse to run on older glibc even
        // where the kernel has the call.
        unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                libc::AT_FDCWD,
                from_c.as_ptr(),
                libc::AT_FDCWD,
                to_c.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        }
    };
    #[cfg(target_os = "macos")]
    let rc =
        i64::from(unsafe { libc::renamex_np(from_c.as_ptr(), to_c.as_ptr(), libc::RENAME_EXCL) });

    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Refuses outright everywhere else: an atomic no-replace rename has
/// no portable spelling, and quietly falling back to a replacing
/// `rename` would hand the caller the race it asked to be free of.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn rename_noreplace_raw(_from: &Path, _to: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "no atomic no-replace rename on this platform (Linux renameat2 / macOS renamex_np only)",
    ))
}

fn metadata_from_std(meta: &fs::Metadata, is_symlink: bool) -> FsMetadata {
    FsMetadata {
        is_file: meta.is_file(),
        is_dir: meta.is_dir(),
        is_symlink,
        len: meta.len(),
        mode: meta.permissions().mode(),
        id: FileId {
            dev: meta.dev(),
            ino: meta.ino(),
            ctime: meta.ctime(),
            ctime_nsec: meta.ctime_nsec(),
        },
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
        fs.write_file(&dir.join("nested").join("f.txt"), b"y")
            .unwrap();
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
    fn mkdir_exclusive_refuses_an_existing_path() {
        let tmp = TempDir::new().unwrap();
        let fs = OsFs::new();

        let dir = tmp.path().join("claimed");
        fs.mkdir_exclusive(&dir).unwrap();
        fs.write_file(&dir.join("mine.txt"), b"mine").unwrap();

        // Second claimant is turned away, and the first one's content
        // is untouched — the point of the exclusive create.
        let err = fs.mkdir_exclusive(&dir).unwrap_err();
        assert!(
            crate::fs::is_already_exists(&err),
            "expected an AlreadyExists refusal, got: {err}"
        );
        assert_eq!(fs.read_to_string(&dir.join("mine.txt")).unwrap(), "mine");

        // A file is in the way just as much as a directory is.
        let occupied = tmp.path().join("occupied");
        fs.write_file(&occupied, b"not a directory").unwrap();
        assert!(crate::fs::is_already_exists(
            &fs.mkdir_exclusive(&occupied).unwrap_err()
        ));
    }

    /// A path whose file was replaced never reads as the entry that
    /// was there before, even where the kernel hands the freed inode
    /// number straight back to the replacement — which ext4 and tmpfs
    /// routinely do for `rm f` followed by a fresh `f`. The ctime is
    /// what separates the two, and a caller deciding whether to move
    /// or remove what it finds at a path depends on that separation.
    #[test]
    fn a_replaced_file_reads_as_a_different_entry_even_on_a_reused_inode() {
        let tmp = TempDir::new().unwrap();
        let fs = OsFs::new();
        let path = tmp.path().join("f");

        fs.write_file(&path, b"first").unwrap();
        let before = fs.lstat(&path).unwrap().id;

        fs.remove_file(&path).unwrap();
        fs.write_file(&path, b"second").unwrap();
        let after = fs.lstat(&path).unwrap().id;

        assert_ne!(
            before, after,
            "a file replaced at the same path is a different entry"
        );
    }

    /// Renaming an entry keeps it the same entry — the whole reason a
    /// caller can read an id before its own move and compare it after.
    /// The ctime moves with the rename, which is why that comparison
    /// is [`FileId::same_entry`] rather than equality.
    #[test]
    fn a_renamed_file_stays_the_same_entry_with_a_new_ctime() {
        let tmp = TempDir::new().unwrap();
        let fs = OsFs::new();
        let from = tmp.path().join("from");
        let to = tmp.path().join("to");

        fs.write_file(&from, b"content").unwrap();
        let before = fs.lstat(&from).unwrap().id;
        fs.rename_noreplace(&from, &to).unwrap();
        let after = fs.lstat(&to).unwrap().id;

        assert!(
            after.same_entry(&before),
            "a rename carries the entry: {before:?} vs {after:?}"
        );
    }

    #[test]
    fn rename_noreplace_moves_only_onto_a_free_path() {
        let tmp = TempDir::new().unwrap();
        let fs = OsFs::new();

        let from = tmp.path().join("staged");
        fs.mkdir_all(&from.join("inner")).unwrap();
        fs.write_file(&from.join("inner/file.txt"), b"staged")
            .unwrap();

        let to = tmp.path().join("published");
        fs.rename_noreplace(&from, &to).unwrap();
        assert!(!fs.exists(&from));
        assert_eq!(
            fs.read_to_string(&to.join("inner/file.txt")).unwrap(),
            "staged"
        );
    }

    #[test]
    fn rename_noreplace_refuses_the_destinations_plain_rename_replaces() {
        let tmp = TempDir::new().unwrap();
        let fs = OsFs::new();

        // A staged tree, and the two destination shapes a POSIX
        // `rename` swallows without a word.
        let staged = |name: &str| {
            let from = tmp.path().join(format!("from-{name}"));
            fs.mkdir_all(&from).unwrap();
            fs.write_file(&from.join("file.txt"), b"staged").unwrap();
            from
        };

        let empty_dir = tmp.path().join("empty-dir");
        std::fs::create_dir(&empty_dir).unwrap();
        let from = staged("empty-dir");
        let err = fs.rename_noreplace(&from, &empty_dir).unwrap_err();
        assert!(
            crate::fs::is_already_exists(&err),
            "expected an AlreadyExists refusal for an empty directory, got: {err}"
        );
        assert_eq!(fs.read_to_string(&from.join("file.txt")).unwrap(), "staged");
        assert!(fs.read_dir(&empty_dir).unwrap().is_empty());

        let link = tmp.path().join("a-symlink");
        std::os::unix::fs::symlink("elsewhere", &link).unwrap();
        let from = staged("a-symlink");
        let err = fs.rename_noreplace(&from, &link).unwrap_err();
        assert!(
            crate::fs::is_already_exists(&err),
            "expected an AlreadyExists refusal for a symlink, got: {err}"
        );
        assert_eq!(fs.read_to_string(&from.join("file.txt")).unwrap(), "staged");
        assert_eq!(fs.readlink(&link).unwrap(), Path::new("elsewhere"));
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

    #[allow(dead_code)]
    fn assert_object_safe(_: &dyn Fs) {}
}

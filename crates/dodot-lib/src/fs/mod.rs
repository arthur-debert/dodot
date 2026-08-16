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
    /// rename is atomic on POSIX) named by pid plus a process-global
    /// counter, so no two live writers — threads or processes — can
    /// ever select the same temp. Any failure after the temp is
    /// created (a failed write or rename — plus a failed chmod in
    /// [`Fs::write_atomic_with_mode`]) removes it rather than
    /// abandoning it; only a crash mid-write can leave a temp
    /// behind, and never a torn target.
    ///
    /// Note the trade-off: the rename replaces the *directory entry*,
    /// so `path` gets a fresh inode. Do not use this on files the
    /// user may hold as a hard link or that must stay a symlink —
    /// e.g. rc files, which are commonly symlinks into a dotfiles
    /// repo that a rename would silently replace with a regular file.
    ///
    /// The temp name is unique but predictable, and creation does not
    /// use `O_EXCL`: this is a primitive for directories the user
    /// owns (the datastore, `$XDG_*` paths), not for shared,
    /// world-writable directories like `/tmp`, where a predictable
    /// name would open a symlink pre-creation attack.
    fn write_atomic(&self, path: &Path, contents: &[u8]) -> Result<()> {
        let tmp = temp_sibling(path);
        if let Err(e) = self.write_file(&tmp, contents) {
            let _ = self.remove_file(&tmp);
            return Err(e);
        }
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
        if let Err(e) = self.write_file_with_mode(&tmp, contents, mode) {
            let _ = self.remove_file(&tmp);
            return Err(e);
        }
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
/// filesystem, which is what makes the rename atomic. The suffix is
/// the pid plus a process-global counter: pids are unique among live
/// processes and the counter within this one, so two live writers —
/// threads or processes — can never select the same temp; uniqueness
/// is structural, not probabilistic. (After pid reuse a name can
/// recur, but only colliding with a stale leftover of a dead process
/// that nothing holds open — truncating it is harmless.)
fn temp_sibling(path: &Path) -> PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let parent = path.parent().unwrap_or(Path::new("."));
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    parent.join(format!(".dodot-{name}.{}-{seq:x}.tmp", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DodotError;
    use std::collections::HashSet;
    use std::sync::Barrier;
    use tempfile::TempDir;

    #[test]
    fn temp_sibling_never_repeats_across_threads_and_calls() {
        // The atomicity guarantee rests on no two live writers ever
        // selecting the same temp. Hammer temp_sibling from parallel
        // threads and assert every returned path is distinct.
        let path = Path::new("/some/dir/file.txt");
        let mut seen = HashSet::new();
        std::thread::scope(|s| {
            let handles: Vec<_> = (0..8)
                .map(|_| s.spawn(|| (0..1000).map(|_| temp_sibling(path)).collect::<Vec<_>>()))
                .collect();
            for h in handles {
                for p in h.join().unwrap() {
                    assert!(seen.insert(p), "temp_sibling returned a duplicate path");
                }
            }
        });
    }

    #[test]
    fn concurrent_writers_to_one_target_leave_one_complete_file_and_no_temps() {
        // Racing write_atomic calls must each go through a private
        // temp: whichever rename lands last wins whole — the target
        // is exactly one writer's full payload, never an interleaving
        // — and every loser's temp is gone (consumed by its rename).
        const WRITERS: u8 = 8;
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("target.bin");
        let payloads: Vec<Vec<u8>> = (0..WRITERS).map(|i| vec![i; 64 * 1024]).collect();
        let barrier = Barrier::new(WRITERS as usize);
        std::thread::scope(|s| {
            for payload in &payloads {
                s.spawn(|| {
                    barrier.wait();
                    OsFs::new().write_atomic(&target, payload).unwrap();
                });
            }
        });
        let survivor = std::fs::read(&target).unwrap();
        assert!(
            payloads.contains(&survivor),
            "target must be exactly one writer's full payload"
        );
        let leftovers: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp siblings left behind: {leftovers:?}"
        );
    }

    /// Delegates to [`OsFs`], injecting a failure into one operation —
    /// for proving the write-atomic methods clean up their temp on
    /// every post-creation error path, not just a failed rename.
    ///
    /// The injected `write_file` failure lands the bytes first and
    /// *then* errors, mimicking the interesting real-world shape
    /// (ENOSPC mid-write): the temp exists on disk when the error
    /// surfaces, so returning it unremoved would leak it.
    struct FaultFs {
        inner: OsFs,
        fail_write: bool,
        fail_chmod: bool,
    }

    impl FaultFs {
        fn injected(&self, path: &Path) -> DodotError {
            DodotError::Fs {
                path: path.to_path_buf(),
                source: std::io::Error::other("injected fault"),
            }
        }
    }

    impl Fs for FaultFs {
        fn stat(&self, path: &Path) -> Result<FsMetadata> {
            self.inner.stat(path)
        }
        fn lstat(&self, path: &Path) -> Result<FsMetadata> {
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
        fn write_file(&self, path: &Path, contents: &[u8]) -> Result<()> {
            self.inner.write_file(path, contents)?;
            if self.fail_write {
                return Err(self.injected(path));
            }
            Ok(())
        }
        fn set_permissions(&self, path: &Path, mode: u32) -> Result<()> {
            if self.fail_chmod {
                return Err(self.injected(path));
            }
            self.inner.set_permissions(path, mode)
        }
        fn mkdir_all(&self, path: &Path) -> Result<()> {
            self.inner.mkdir_all(path)
        }
        fn symlink(&self, original: &Path, link: &Path) -> Result<()> {
            self.inner.symlink(original, link)
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
        fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>> {
            self.inner.read_dir(path)
        }
        fn rename(&self, from: &Path, to: &Path) -> Result<()> {
            self.inner.rename(from, to)
        }
        fn copy_file(&self, from: &Path, to: &Path) -> Result<()> {
            self.inner.copy_file(from, to)
        }
    }

    fn tmp_leftovers(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect()
    }

    #[test]
    fn write_atomic_removes_the_temp_when_the_write_fails() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("target.txt");
        let fs = FaultFs {
            inner: OsFs::new(),
            fail_write: true,
            fail_chmod: false,
        };
        assert!(fs.write_atomic(&target, b"data").is_err());
        assert!(!target.exists(), "target must not appear on a failed write");
        let leftovers = tmp_leftovers(dir.path());
        assert!(
            leftovers.is_empty(),
            "temp siblings left behind: {leftovers:?}"
        );
    }

    #[test]
    fn write_atomic_with_mode_removes_the_temp_when_the_chmod_fails() {
        // The chmod happens after the temp is fully written — the
        // temp is guaranteed on disk when the error surfaces, so
        // this pins the cleanup, not just error propagation.
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("target.sh");
        let fs = FaultFs {
            inner: OsFs::new(),
            fail_write: false,
            fail_chmod: true,
        };
        assert!(fs
            .write_atomic_with_mode(&target, b"#!/bin/sh\n", 0o755)
            .is_err());
        assert!(!target.exists(), "target must not appear on a failed chmod");
        let leftovers = tmp_leftovers(dir.path());
        assert!(
            leftovers.is_empty(),
            "temp siblings left behind: {leftovers:?}"
        );
    }
}

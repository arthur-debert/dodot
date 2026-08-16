//! A filesystem stated as data, for the selection tests.
//!
//! Root selection asks the filesystem three questions ([`PathProbe`]), and
//! every interesting answer — a symlink, a missing path, a file, an unreadable
//! directory, a path that cannot be resolved at all — is awkward or
//! unportable to arrange for real. Stating them as data makes each case one
//! line, and keeps the decision tests free of ambient process state: no
//! temporary directory, no process working directory, no repository.
//!
//! The fake also *records* what it was asked. That is how the tests prove the
//! invariants that are about restraint rather than results: a failing
//! `DOTFILES_ROOT` never probes an implicit candidate, and one invocation
//! canonicalizes exactly once (ADR-0001).
//!
//! Selection is still proven against the real filesystem too — each selection
//! module carries an [`OsPathProbe`](super::util::OsPathProbe) case — so the
//! fake's answers cannot be the only thing the behaviour holds against.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::util::PathProbe;

/// What a path is, as far as the probe is concerned.
#[derive(Debug, Clone)]
pub enum Entry {
    ReadableDir,
    UnreadableDir,
    NotADirectory,
    Unresolvable(io::ErrorKind),
}

/// What each path canonicalizes to, and what it is once canonical. Anything
/// unregistered is missing.
#[derive(Debug, Default)]
pub struct FakeProbe {
    links: Vec<(PathBuf, PathBuf)>,
    entries: Vec<(PathBuf, Entry)>,
    canonicalized: Mutex<Vec<PathBuf>>,
}

impl FakeProbe {
    /// A filesystem holding one path, which is `entry`.
    pub fn with(path: impl Into<PathBuf>, entry: Entry) -> Self {
        Self::default().add(path, entry)
    }

    /// A filesystem holding one readable directory.
    pub fn dir(path: impl Into<PathBuf>) -> Self {
        Self::with(path, Entry::ReadableDir)
    }

    /// Add another path.
    pub fn add(mut self, path: impl Into<PathBuf>, entry: Entry) -> Self {
        self.entries.push((path.into(), entry));
        self
    }

    /// Add another readable directory.
    pub fn and_dir(self, path: impl Into<PathBuf>) -> Self {
        self.add(path, Entry::ReadableDir)
    }

    /// `path` canonicalizes to `target`, which is a readable directory.
    pub fn link(mut self, path: impl Into<PathBuf>, target: impl Into<PathBuf>) -> Self {
        let target = target.into();
        self.links.push((path.into(), target.clone()));
        self.and_dir(target)
    }

    fn entry(&self, path: &Path) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|(known, _)| known == path)
            .map(|(_, entry)| entry)
    }

    /// Every path `canonicalize` was asked about, in order — the proof that a
    /// candidate was anchored where the test says, and that resolution
    /// happened once.
    pub fn canonicalized(&self) -> Vec<PathBuf> {
        self.canonicalized.lock().unwrap().clone()
    }
}

impl PathProbe for FakeProbe {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        self.canonicalized.lock().unwrap().push(path.to_path_buf());

        if let Some((_, target)) = self.links.iter().find(|(known, _)| known == path) {
            return Ok(target.clone());
        }

        match self.entry(path) {
            Some(Entry::Unresolvable(kind)) => Err(io::Error::from(*kind)),
            Some(_) => Ok(path.to_path_buf()),
            None => Err(io::Error::from(io::ErrorKind::NotFound)),
        }
    }

    fn is_directory(&self, path: &Path) -> bool {
        matches!(
            self.entry(path),
            Some(Entry::ReadableDir | Entry::UnreadableDir)
        )
    }

    fn is_readable_dir(&self, path: &Path) -> bool {
        matches!(self.entry(path), Some(Entry::ReadableDir))
    }
}

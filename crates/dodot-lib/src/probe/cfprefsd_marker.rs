//! cfprefsd cache-invalidation marker.
//!
//! macOS aggressively caches plist values in `cfprefsd`, so a plist
//! file the user just deployed (or that an app rewrote between
//! `dodot up` runs) may not be visible to the running app until
//! cfprefsd is restarted. A `killall cfprefsd` clears the cache and
//! cfprefsd respawns immediately — no data loss.
//!
//! `dodot up` decides whether to prompt the user for cfprefsd
//! invalidation. It writes this marker file when it observes plist
//! drift (any plist file in any active pack with mtime newer than the
//! previous `last-up-at`). The CLI's post-`up` prompt reads the
//! marker, asks the user once, and clears it on yes/no.
//!
//! Format: presence of the file is the entire signal. The contents
//! are unused; we write a one-line note to make the file
//! self-documenting if someone discovers it.
//!
//! See [`docs/proposals/plists.lex`] §6.4 (cfprefsd caching) and
//! issue #109.
//!
//! Lives under `data_dir` (not `cache_dir`) because clearing the
//! cache should not silently drop a pending user prompt.

use std::path::PathBuf;

use crate::fs::Fs;
use crate::paths::Pather;
use crate::Result;

const MARKER_FILENAME: &str = "cfprefsd-needs-invalidation";
const MARKER_NOTE: &[u8] =
    b"dodot detected plist drift on the last `up`; the CLI prompt clears this file.\n";

/// On-disk path of the marker.
pub fn cfprefsd_marker_path(paths: &dyn Pather) -> PathBuf {
    paths.data_dir().join(MARKER_FILENAME)
}

/// True if the marker is present on disk.
pub fn cfprefsd_marker_exists(fs: &dyn Fs, paths: &dyn Pather) -> bool {
    fs.exists(&cfprefsd_marker_path(paths))
}

/// Write the marker. Idempotent: re-writing is a no-op.
pub fn write_cfprefsd_marker(fs: &dyn Fs, paths: &dyn Pather) -> Result<()> {
    let path = cfprefsd_marker_path(paths);
    fs.mkdir_all(paths.data_dir())?;
    fs.write_file(&path, MARKER_NOTE)
}

/// Remove the marker if it exists. Best-effort: failures are not
/// surfaced — the marker's only consumer is the prompt, which would
/// re-fire on a subsequent `up` if removal silently failed (and the
/// user can re-dismiss).
pub fn clear_cfprefsd_marker(fs: &dyn Fs, paths: &dyn Pather) {
    let path = cfprefsd_marker_path(paths);
    if fs.exists(&path) {
        let _ = fs.remove_file(&path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempEnvironment;

    #[test]
    fn marker_round_trip() {
        let env = TempEnvironment::builder().build();
        assert!(!cfprefsd_marker_exists(env.fs.as_ref(), env.paths.as_ref()));
        write_cfprefsd_marker(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert!(cfprefsd_marker_exists(env.fs.as_ref(), env.paths.as_ref()));
        clear_cfprefsd_marker(env.fs.as_ref(), env.paths.as_ref());
        assert!(!cfprefsd_marker_exists(env.fs.as_ref(), env.paths.as_ref()));
    }

    #[test]
    fn write_is_idempotent() {
        let env = TempEnvironment::builder().build();
        write_cfprefsd_marker(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        write_cfprefsd_marker(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert!(cfprefsd_marker_exists(env.fs.as_ref(), env.paths.as_ref()));
    }

    #[test]
    fn clear_is_idempotent_when_missing() {
        let env = TempEnvironment::builder().build();
        clear_cfprefsd_marker(env.fs.as_ref(), env.paths.as_ref());
        assert!(!cfprefsd_marker_exists(env.fs.as_ref(), env.paths.as_ref()));
    }
}

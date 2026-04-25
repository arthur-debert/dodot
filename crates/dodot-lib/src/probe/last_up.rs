//! Last-up marker — records the unix timestamp of the most recent
//! successful `dodot up` to `<data_dir>/last-up-at`.
//!
//! Used by `dodot probe shell-init` to flag profiles captured before
//! that timestamp as stale: a profile only gets written when a new
//! shell sources `dodot-init.sh`, so running `dodot up` followed by
//! `dodot probe shell-init` from the same shell otherwise displays
//! pre-up timings without any indication that they no longer reflect
//! reality.
//!
//! Format: a single line of ASCII decimal — no trailing newline
//! required, but the reader tolerates it. Anything that fails to parse
//! is treated as "no marker", same as a missing file.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::fs::Fs;
use crate::paths::Pather;
use crate::Result;

/// Write the current unix timestamp to `<data_dir>/last-up-at`.
pub fn write_last_up_marker(fs: &dyn Fs, paths: &dyn Pather) -> Result<()> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    fs.mkdir_all(paths.data_dir())?;
    fs.write_file(&paths.last_up_path(), ts.to_string().as_bytes())
}

/// Read the unix timestamp of the most recent `dodot up`. Returns
/// `None` if the marker is missing or unparseable — callers should
/// treat that as "no last-up known" (e.g., a fresh install that never
/// ran `up`) rather than as zero, which would make every profile look
/// fresh.
pub fn read_last_up_marker(fs: &dyn Fs, paths: &dyn Pather) -> Option<u64> {
    let path = paths.last_up_path();
    if !fs.exists(&path) {
        return None;
    }
    fs.read_to_string(&path)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempEnvironment;

    #[test]
    fn missing_marker_reads_as_none() {
        let env = TempEnvironment::builder().build();
        assert_eq!(
            read_last_up_marker(env.fs.as_ref(), env.paths.as_ref()),
            None
        );
    }

    #[test]
    fn write_then_read_round_trip() {
        let env = TempEnvironment::builder().build();
        write_last_up_marker(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        let ts = read_last_up_marker(env.fs.as_ref(), env.paths.as_ref())
            .expect("marker should exist after write");
        // Rough sanity check: the timestamp should be after a fixed
        // historical instant. We can't pin the exact value in a
        // deterministic test without injecting a clock.
        assert!(
            ts > 1_700_000_000,
            "timestamp should look like a recent unix ts, got {ts}"
        );
    }

    #[test]
    fn unparseable_marker_reads_as_none() {
        let env = TempEnvironment::builder().build();
        env.fs.mkdir_all(env.paths.data_dir()).unwrap();
        env.fs
            .write_file(&env.paths.last_up_path(), b"not a number")
            .unwrap();
        assert_eq!(
            read_last_up_marker(env.fs.as_ref(), env.paths.as_ref()),
            None
        );
    }
}

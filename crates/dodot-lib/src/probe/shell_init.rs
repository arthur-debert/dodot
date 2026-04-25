//! Shell-init profile reader, aggregator, and rotation.
//!
//! Profile files are written by the timing wrapper that
//! [`crate::shell::generate_init_script`] embeds in `dodot-init.sh`.
//! Each shell startup emits one TSV under
//! `<data_dir>/probes/shell-init/profile-<unix_ts>-<pid>-<rand>.tsv`
//! with the shape:
//!
//! ```text
//! # dodot shell-init profile v1
//! # shell\tbash 5.3.9(1)-release
//! # start_t\t1714000000.123456
//! # init_script\t/home/alice/.local/share/dodot/shell/dodot-init.sh
//! # columns\tphase\tpack\thandler\ttarget\tstart_t\tend_t\texit_status
//! path\tvim\tpath\t/home/alice/dotfiles/vim/bin\t1714000000.123500\t1714000000.123502\t0
//! source\tvim\tshell\t/home/alice/dotfiles/vim/aliases.sh\t1714000000.123600\t1714000000.124900\t0
//! # end_t\t1714000000.125100
//! ```
//!
//! Both the reader and the rotator are tolerant of malformed input —
//! a partial write from a crashed shell should leave the next `dodot
//! probe shell-init` working, just with a short report.

use serde::Serialize;

use crate::fs::Fs;
use crate::paths::Pather;
use crate::Result;

/// One parsed entry row from a profile TSV.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProfileEntry {
    /// `"path"` for an export, `"source"` for a sourced shell file.
    pub phase: String,
    pub pack: String,
    pub handler: String,
    pub target: String,
    /// Microseconds the entry took to execute (computed from
    /// `end_t - start_t` at parse time).
    pub duration_us: u64,
    /// Exit status reported by the source. Always `0` for `path`
    /// entries (PATH export can't fail meaningfully).
    pub exit_status: i32,
}

/// A whole profile file, post-parse.
#[derive(Debug, Clone, Serialize)]
pub struct Profile {
    /// Source filename (basename only). Useful for showing "which
    /// run am I looking at" in the rendered report.
    pub filename: String,
    /// `bash 5.3.9` etc; empty if the preamble was missing.
    pub shell: String,
    /// Whole-script wall time in microseconds, from `# start_t` to
    /// `# end_t`. `0` if either marker is missing (e.g. crashed shell).
    pub total_duration_us: u64,
    pub entries: Vec<ProfileEntry>,
}

impl Profile {
    /// Convenience: total time spent in the entry rows. Lets the
    /// renderer show "user-sourced time" vs "dodot framing" by
    /// subtracting this from `total_duration_us`.
    pub fn entries_duration_us(&self) -> u64 {
        self.entries.iter().map(|e| e.duration_us).sum()
    }

    /// `total_duration_us - entries_duration_us`, saturating at zero.
    /// Represents the shell-side overhead of dodot's wrapper itself
    /// (and any work between wrapper invocations).
    pub fn framing_duration_us(&self) -> u64 {
        self.total_duration_us
            .saturating_sub(self.entries_duration_us())
    }
}

/// Read the most recently written profile under `<data_dir>/probes/shell-init/`,
/// or `None` if the directory is empty / missing.
pub fn read_latest_profile(fs: &dyn Fs, paths: &dyn Pather) -> Result<Option<Profile>> {
    let dir = paths.probes_shell_init_dir();
    if !fs.is_dir(&dir) {
        return Ok(None);
    }
    let mut entries = fs.read_dir(&dir)?;
    // Filenames start with `profile-<unix_ts>-…`, so a lexical sort
    // is equivalent to chronological order (the timestamp segment is
    // fixed-width as long as we stay below year 2286 — fine).
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let Some(latest) = entries
        .into_iter()
        .rfind(|e| e.is_file && e.name.starts_with("profile-") && e.name.ends_with(".tsv"))
    else {
        return Ok(None);
    };
    let content = fs.read_to_string(&latest.path)?;
    Ok(Some(parse_profile(&latest.name, &content)))
}

/// Parse the textual content of a profile file. Tolerates missing
/// preamble lines, unknown comments, and malformed rows (skipped).
pub fn parse_profile(filename: &str, content: &str) -> Profile {
    let mut shell = String::new();
    let mut start_t: Option<f64> = None;
    let mut end_t: Option<f64> = None;
    let mut entries: Vec<ProfileEntry> = Vec::new();

    for raw_line in content.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix('#') {
            // Comment lines, format: `# key\tvalue` (the leading hash
            // and one space are conventional).
            let trimmed = rest.trim_start();
            if let Some((key, val)) = trimmed.split_once('\t') {
                match key {
                    "shell" => shell = val.to_string(),
                    "start_t" => start_t = val.parse::<f64>().ok(),
                    "end_t" => end_t = val.parse::<f64>().ok(),
                    _ => {} // unknown header — ignore
                }
            }
            continue;
        }
        if let Some(entry) = parse_row(line) {
            entries.push(entry);
        }
        // Otherwise: malformed row, silently dropped.
    }

    let total_duration_us = match (start_t, end_t) {
        (Some(s), Some(e)) if e >= s => seconds_to_micros(e - s),
        _ => 0,
    };

    Profile {
        filename: filename.to_string(),
        shell,
        total_duration_us,
        entries,
    }
}

fn parse_row(line: &str) -> Option<ProfileEntry> {
    let mut parts = line.splitn(7, '\t');
    let phase = parts.next()?;
    let pack = parts.next()?;
    let handler = parts.next()?;
    let target = parts.next()?;
    let start = parts.next()?.parse::<f64>().ok()?;
    let end = parts.next()?.parse::<f64>().ok()?;
    let exit_status = parts.next()?.parse::<i32>().ok()?;
    if !matches!(phase, "path" | "source") {
        return None;
    }
    let duration_us = if end >= start {
        seconds_to_micros(end - start)
    } else {
        0
    };
    Some(ProfileEntry {
        phase: phase.to_string(),
        pack: pack.to_string(),
        handler: handler.to_string(),
        target: target.to_string(),
        duration_us,
        exit_status,
    })
}

fn seconds_to_micros(secs: f64) -> u64 {
    if !secs.is_finite() || secs < 0.0 {
        return 0;
    }
    (secs * 1_000_000.0).round() as u64
}

/// Prune `<data_dir>/probes/shell-init/` to the newest `keep` files
/// (by filename). Returns the number of files removed. `keep == 0`
/// is treated as "no pruning" — we don't want a stray miscalibrated
/// config to wipe the whole profile history.
pub fn rotate_profiles(fs: &dyn Fs, paths: &dyn Pather, keep: usize) -> Result<usize> {
    if keep == 0 {
        return Ok(0);
    }
    let dir = paths.probes_shell_init_dir();
    if !fs.is_dir(&dir) {
        return Ok(0);
    }
    let mut entries: Vec<_> = fs
        .read_dir(&dir)?
        .into_iter()
        .filter(|e| e.is_file && e.name.starts_with("profile-") && e.name.ends_with(".tsv"))
        .collect();
    if entries.len() <= keep {
        return Ok(0);
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let to_remove = entries.len() - keep;
    let mut removed = 0;
    for entry in entries.into_iter().take(to_remove) {
        if fs.remove_file(&entry.path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

/// Aggregate one profile by `(pack, handler)` for the rendered table.
#[derive(Debug, Clone, Serialize)]
pub struct GroupedProfile {
    pub groups: Vec<ProfileGroup>,
    pub user_total_us: u64,
    pub framing_us: u64,
    pub total_us: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileGroup {
    pub pack: String,
    pub handler: String,
    pub rows: Vec<ProfileEntry>,
    pub group_total_us: u64,
}

/// Roll up a profile into per-(pack, handler) groups for display.
/// The pack-then-handler ordering matches the deployment-map view.
pub fn group_profile(profile: &Profile) -> GroupedProfile {
    let user_total_us = profile.entries_duration_us();
    let total_us = profile.total_duration_us.max(user_total_us);
    let framing_us = total_us.saturating_sub(user_total_us);

    let mut groups: Vec<ProfileGroup> = Vec::new();
    for entry in &profile.entries {
        let key = (&entry.pack, &entry.handler);
        let pos = groups
            .iter()
            .position(|g| (&g.pack, &g.handler) == (key.0, key.1));
        match pos {
            Some(i) => {
                groups[i].rows.push(entry.clone());
                groups[i].group_total_us += entry.duration_us;
            }
            None => groups.push(ProfileGroup {
                pack: entry.pack.clone(),
                handler: entry.handler.clone(),
                rows: vec![entry.clone()],
                group_total_us: entry.duration_us,
            }),
        }
    }

    GroupedProfile {
        groups,
        user_total_us,
        framing_us,
        total_us,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempEnvironment;

    fn write_profile(env: &TempEnvironment, name: &str, content: &str) -> std::path::PathBuf {
        let dir = env.paths.probes_shell_init_dir();
        env.fs.mkdir_all(&dir).unwrap();
        let path = dir.join(name);
        env.fs.write_file(&path, content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn parser_extracts_preamble_and_rows() {
        let content = "# dodot shell-init profile v1\n\
# shell\tbash 5.2\n\
# start_t\t1714000000.000000\n\
# init_script\t/x/dodot-init.sh\n\
# columns\tphase\tpack\thandler\ttarget\tstart_t\tend_t\texit_status\n\
path\tvim\tpath\t/x/bin\t1714000000.001000\t1714000000.001005\t0\n\
source\tgit\tshell\t/x/aliases.sh\t1714000000.002000\t1714000000.005000\t0\n\
# end_t\t1714000000.010000\n";
        let p = parse_profile("profile-1714000000-1-1.tsv", content);
        assert_eq!(p.shell, "bash 5.2");
        assert_eq!(p.entries.len(), 2);
        assert_eq!(p.entries[0].phase, "path");
        assert_eq!(p.entries[0].duration_us, 5);
        assert_eq!(p.entries[1].duration_us, 3000);
        assert_eq!(p.total_duration_us, 10_000);
    }

    #[test]
    fn parser_skips_malformed_rows() {
        let content = "# columns\tphase\tpack\thandler\ttarget\tstart_t\tend_t\texit_status\n\
junk\trow\twith\ttoo\tfew\tcols\n\
path\tvim\tpath\t/x\t1.0\t1.001\t0\n\
weird\tphase\twrong\t/x\t1.0\t1.001\t0\n";
        let p = parse_profile("p.tsv", content);
        assert_eq!(p.entries.len(), 1);
        assert_eq!(p.entries[0].phase, "path");
    }

    #[test]
    fn parser_handles_missing_end_marker() {
        // Crashed shell: writes start_t and rows, but never reaches the
        // epilogue. We still want a usable Profile.
        let content = "# start_t\t1714000000.000000\n\
source\tvim\tshell\t/x\t1714000000.001000\t1714000000.002000\t0\n";
        let p = parse_profile("p.tsv", content);
        assert_eq!(p.total_duration_us, 0); // no end_t → 0 total
        assert_eq!(p.entries.len(), 1);
        assert_eq!(p.entries[0].duration_us, 1000);
    }

    #[test]
    fn read_latest_returns_none_when_dir_missing() {
        let env = TempEnvironment::builder().build();
        let r = read_latest_profile(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn read_latest_picks_highest_filename_lexicographically() {
        let env = TempEnvironment::builder().build();
        write_profile(&env, "profile-1000-1-1.tsv", "# shell\told\n");
        write_profile(&env, "profile-2000-1-1.tsv", "# shell\tnew\n");
        write_profile(&env, "profile-1500-1-1.tsv", "# shell\tmid\n");
        let p = read_latest_profile(env.fs.as_ref(), env.paths.as_ref())
            .unwrap()
            .unwrap();
        assert_eq!(p.shell, "new");
        assert_eq!(p.filename, "profile-2000-1-1.tsv");
    }

    #[test]
    fn rotate_keeps_newest_n() {
        let env = TempEnvironment::builder().build();
        for i in 0..10 {
            write_profile(&env, &format!("profile-{i:04}-1-1.tsv"), "x");
        }
        let removed = rotate_profiles(env.fs.as_ref(), env.paths.as_ref(), 3).unwrap();
        assert_eq!(removed, 7);
        let remaining: Vec<String> = env
            .fs
            .read_dir(&env.paths.probes_shell_init_dir())
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        // The three highest-numbered (newest) files survive.
        assert_eq!(
            remaining,
            vec![
                "profile-0007-1-1.tsv".to_string(),
                "profile-0008-1-1.tsv".to_string(),
                "profile-0009-1-1.tsv".to_string(),
            ]
        );
    }

    #[test]
    fn rotate_with_keep_zero_is_a_noop() {
        // Defensive: a misconfigured keep_last_runs = 0 must not wipe
        // the user's profile history. Treat as "no rotation".
        let env = TempEnvironment::builder().build();
        for i in 0..3 {
            write_profile(&env, &format!("profile-{i}-1-1.tsv"), "x");
        }
        let removed = rotate_profiles(env.fs.as_ref(), env.paths.as_ref(), 0).unwrap();
        assert_eq!(removed, 0);
        let count = env
            .fs
            .read_dir(&env.paths.probes_shell_init_dir())
            .unwrap()
            .len();
        assert_eq!(count, 3);
    }

    #[test]
    fn rotate_below_threshold_is_a_noop() {
        let env = TempEnvironment::builder().build();
        write_profile(&env, "profile-1-1-1.tsv", "x");
        let removed = rotate_profiles(env.fs.as_ref(), env.paths.as_ref(), 100).unwrap();
        assert_eq!(removed, 0);
    }

    #[test]
    fn rotate_ignores_non_profile_files() {
        let env = TempEnvironment::builder().build();
        let dir = env.paths.probes_shell_init_dir();
        env.fs.mkdir_all(&dir).unwrap();
        env.fs
            .write_file(&dir.join("profile-1-1-1.tsv"), b"")
            .unwrap();
        env.fs
            .write_file(&dir.join("README"), b"do not delete")
            .unwrap();
        env.fs
            .write_file(&dir.join("notes.txt"), b"keep me")
            .unwrap();

        let removed = rotate_profiles(env.fs.as_ref(), env.paths.as_ref(), 0).unwrap();
        assert_eq!(removed, 0);
        // All non-profile files survive even if we asked to rotate.
        assert!(env.fs.exists(&dir.join("README")));
        assert!(env.fs.exists(&dir.join("notes.txt")));
    }

    #[test]
    fn group_profile_aggregates_by_pack_handler() {
        let p = Profile {
            filename: "x".into(),
            shell: "bash".into(),
            total_duration_us: 10_000,
            entries: vec![
                ProfileEntry {
                    phase: "source".into(),
                    pack: "vim".into(),
                    handler: "shell".into(),
                    target: "/a".into(),
                    duration_us: 100,
                    exit_status: 0,
                },
                ProfileEntry {
                    phase: "source".into(),
                    pack: "vim".into(),
                    handler: "shell".into(),
                    target: "/b".into(),
                    duration_us: 200,
                    exit_status: 0,
                },
                ProfileEntry {
                    phase: "path".into(),
                    pack: "vim".into(),
                    handler: "path".into(),
                    target: "/bin".into(),
                    duration_us: 5,
                    exit_status: 0,
                },
            ],
        };
        let g = group_profile(&p);
        assert_eq!(g.groups.len(), 2);
        assert_eq!(g.groups[0].pack, "vim");
        assert_eq!(g.groups[0].handler, "shell");
        assert_eq!(g.groups[0].group_total_us, 300);
        assert_eq!(g.groups[1].handler, "path");
        assert_eq!(g.user_total_us, 305);
        assert_eq!(g.total_us, 10_000);
        assert_eq!(g.framing_us, 9_695);
    }

    #[test]
    fn group_profile_clamps_framing_when_total_below_entries() {
        // If `total_duration_us` is missing (parse left it at 0) but
        // we have entries, framing_us must be 0 — not negative.
        let p = Profile {
            filename: "x".into(),
            shell: "".into(),
            total_duration_us: 0,
            entries: vec![ProfileEntry {
                phase: "source".into(),
                pack: "vim".into(),
                handler: "shell".into(),
                target: "/a".into(),
                duration_us: 500,
                exit_status: 0,
            }],
        };
        let g = group_profile(&p);
        assert_eq!(g.user_total_us, 500);
        assert_eq!(g.total_us, 500);
        assert_eq!(g.framing_us, 0);
    }
}

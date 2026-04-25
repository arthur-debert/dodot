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
    let mut profiles = read_recent_profiles(fs, paths, 1)?;
    Ok(profiles.pop())
}

/// Read up to `limit` most recent profiles, newest first.
///
/// Profiles are returned in reverse chronological order (newest first).
/// The cap exists because callers know how much they need — `--runs 5`
/// asks for five — and the directory may have hundreds of files.
///
/// Implementation: `Fs::read_dir` already returns entries sorted by
/// name, and `profile-<unix_ts>-…` is fixed-prefix monotonic, so
/// lexical-ascending == chronological-ascending. We `.rev()` the
/// iterator to walk newest-first, filter, and `take(limit)` so we
/// only allocate the rows we'll actually return.
pub fn read_recent_profiles(fs: &dyn Fs, paths: &dyn Pather, limit: usize) -> Result<Vec<Profile>> {
    let dir = paths.probes_shell_init_dir();
    if !fs.is_dir(&dir) || limit == 0 {
        return Ok(Vec::new());
    }
    let entries: Vec<_> = fs
        .read_dir(&dir)?
        .into_iter()
        .rev()
        .filter(|e| e.is_file && e.name.starts_with("profile-") && e.name.ends_with(".tsv"))
        .take(limit)
        .collect();

    let mut profiles = Vec::with_capacity(entries.len());
    for entry in entries {
        let content = fs.read_to_string(&entry.path)?;
        profiles.push(parse_profile(&entry.name, &content));
    }
    Ok(profiles)
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
    // `Fs::read_dir` returns entries already sorted by name, and
    // `profile-<unix_ts>-…` is fixed-prefix monotonic, so the result
    // is chronological-ascending; oldest entries are at the front.
    let entries: Vec<_> = fs
        .read_dir(&dir)?
        .into_iter()
        .filter(|e| e.is_file && e.name.starts_with("profile-") && e.name.ends_with(".tsv"))
        .collect();
    if entries.len() <= keep {
        return Ok(0);
    }
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
///
/// Groups are returned sorted by `(pack, handler)` so the rendered
/// table ordering matches `dodot probe deployment-map` — eyeballing
/// the two side-by-side should not require mental remapping. The init
/// script emits PATH lines before shell sources (a phase-ordering
/// concern of its own), so the raw entry order would otherwise show
/// all `path` groups before any `shell` group, which is not what the
/// user expects when reading a per-pack table.
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

    groups.sort_by(|a, b| a.pack.cmp(&b.pack).then(a.handler.cmp(&b.handler)));

    GroupedProfile {
        groups,
        user_total_us,
        framing_us,
        total_us,
    }
}

// ── Multi-run aggregation (`probe shell-init --runs N`) ───────────────

/// Per-target stats across N runs.
#[derive(Debug, Clone, Serialize)]
pub struct AggregatedTarget {
    pub pack: String,
    pub handler: String,
    pub target: String,
    pub p50_us: u64,
    pub p95_us: u64,
    pub max_us: u64,
    /// How many of the considered runs had this target. Surfaced because
    /// targets can appear/disappear across deploys; "p95 from 2/10 runs"
    /// is a different signal than "p95 from 10/10".
    pub runs_seen: usize,
    pub runs_total: usize,
}

/// Aggregate output, one row per `(pack, handler, target)` keyed by all
/// the profiles passed in.
#[derive(Debug, Clone, Serialize)]
pub struct AggregatedView {
    pub runs: usize,
    pub targets: Vec<AggregatedTarget>,
}

/// Roll up a slice of profiles into per-target percentile stats.
///
/// Targets are returned sorted by `(pack, handler, target)` for
/// readability (matches `group_profile`'s ordering convention).
pub fn aggregate_profiles(profiles: &[Profile]) -> AggregatedView {
    use std::collections::BTreeMap;

    // Bucket durations by (pack, handler, target).
    let mut buckets: BTreeMap<(String, String, String), Vec<u64>> = BTreeMap::new();
    for p in profiles {
        for e in &p.entries {
            buckets
                .entry((e.pack.clone(), e.handler.clone(), e.target.clone()))
                .or_default()
                .push(e.duration_us);
        }
    }

    let runs_total = profiles.len();
    let targets = buckets
        .into_iter()
        .map(|((pack, handler, target), mut durs)| {
            durs.sort_unstable();
            AggregatedTarget {
                pack,
                handler,
                target,
                p50_us: percentile(&durs, 50),
                p95_us: percentile(&durs, 95),
                max_us: *durs.last().unwrap_or(&0),
                runs_seen: durs.len(),
                runs_total,
            }
        })
        .collect();

    AggregatedView {
        runs: runs_total,
        targets,
    }
}

/// Nearest-rank percentile (no interpolation): the smallest value at
/// or above the cumulative-frequency threshold. For p95 over 1 sample
/// returns that sample; over 10 samples returns the 10th (max).
///
/// `sorted` must be sorted ascending. Returns 0 for an empty slice.
fn percentile(sorted: &[u64], pct: u8) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let n = sorted.len();
    // ceil(p/100 * n) maps to a 1-indexed rank; subtract 1 for 0-indexed.
    // Saturate at n-1 to handle any rounding edge cases.
    let rank = ((pct as f64 / 100.0) * n as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(n - 1);
    sorted[idx]
}

// ── History (`probe shell-init --history`) ────────────────────────────

/// One row in the history view: a single run's headline metrics.
#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntry {
    pub filename: String,
    /// Best-effort unix timestamp parsed from the filename. `0` if the
    /// filename doesn't match `profile-<unix_ts>-…`.
    pub unix_ts: u64,
    pub shell: String,
    pub total_us: u64,
    pub user_total_us: u64,
    /// Count of entries with non-zero exit_status — surfaces silent
    /// breakage at a glance.
    pub failed_entries: usize,
    pub entry_count: usize,
}

/// Build a history view from a slice of profiles (newest-first input,
/// preserved order in the output — caller decides cadence).
pub fn summarize_history(profiles: &[Profile]) -> Vec<HistoryEntry> {
    profiles.iter().map(history_entry_from).collect()
}

fn history_entry_from(profile: &Profile) -> HistoryEntry {
    HistoryEntry {
        filename: profile.filename.clone(),
        unix_ts: parse_unix_ts_from_filename(&profile.filename),
        shell: profile.shell.clone(),
        total_us: profile.total_duration_us,
        user_total_us: profile.entries_duration_us(),
        failed_entries: profile
            .entries
            .iter()
            .filter(|e| e.exit_status != 0)
            .count(),
        entry_count: profile.entries.len(),
    }
}

/// Extract the leading `<unix_ts>` from `profile-<unix_ts>-<pid>-<rand>.tsv`,
/// returning `0` for any unparseable filename. The renderer formats this
/// into a date string; storing it as an integer keeps JSON output stable.
pub fn parse_unix_ts_from_filename(filename: &str) -> u64 {
    filename
        .strip_prefix("profile-")
        .and_then(|rest| rest.split('-').next())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
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
        // Five profile files (more than `keep`), plus two non-profile
        // files that the rotator must leave alone.
        for i in 1..=5 {
            env.fs
                .write_file(&dir.join(format!("profile-{i}-1-1.tsv")), b"")
                .unwrap();
        }
        env.fs
            .write_file(&dir.join("README"), b"do not delete")
            .unwrap();
        env.fs
            .write_file(&dir.join("notes.txt"), b"keep me")
            .unwrap();

        // keep=2 forces the pruning path (5 profiles → 2 should remain).
        let removed = rotate_profiles(env.fs.as_ref(), env.paths.as_ref(), 2).unwrap();
        assert_eq!(removed, 3);

        // The two newest profiles survive.
        assert!(env.fs.exists(&dir.join("profile-4-1-1.tsv")));
        assert!(env.fs.exists(&dir.join("profile-5-1-1.tsv")));
        // The three oldest are gone.
        assert!(!env.fs.exists(&dir.join("profile-1-1-1.tsv")));
        assert!(!env.fs.exists(&dir.join("profile-2-1-1.tsv")));
        assert!(!env.fs.exists(&dir.join("profile-3-1-1.tsv")));

        // Non-profile files are untouched.
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
        // Groups are sorted by (pack, handler), not by emission order:
        // "path" comes before "shell" alphabetically within `vim`.
        assert_eq!(g.groups[0].pack, "vim");
        assert_eq!(g.groups[0].handler, "path");
        assert_eq!(g.groups[0].group_total_us, 5);
        assert_eq!(g.groups[1].handler, "shell");
        assert_eq!(g.groups[1].group_total_us, 300);
        assert_eq!(g.user_total_us, 305);
        assert_eq!(g.total_us, 10_000);
        assert_eq!(g.framing_us, 9_695);
    }

    #[test]
    fn group_profile_sorts_across_packs() {
        // Entries arrive in deliberately scrambled order; the result
        // must still be (pack, handler)-sorted.
        let p = Profile {
            filename: "x".into(),
            shell: "bash".into(),
            total_duration_us: 0,
            entries: vec![
                entry("vim", "shell", "/a", 1),
                entry("git", "symlink", "/b", 1),
                entry("vim", "path", "/c", 1),
                entry("git", "shell", "/d", 1),
            ],
        };
        let g = group_profile(&p);
        let keys: Vec<(String, String)> = g
            .groups
            .iter()
            .map(|gp| (gp.pack.clone(), gp.handler.clone()))
            .collect();
        assert_eq!(
            keys,
            vec![
                ("git".into(), "shell".into()),
                ("git".into(), "symlink".into()),
                ("vim".into(), "path".into()),
                ("vim".into(), "shell".into()),
            ]
        );
    }

    fn entry(pack: &str, handler: &str, target: &str, dur_us: u64) -> ProfileEntry {
        ProfileEntry {
            phase: "source".into(),
            pack: pack.into(),
            handler: handler.into(),
            target: target.into(),
            duration_us: dur_us,
            exit_status: 0,
        }
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

    // ── read_recent_profiles ──────────────────────────────────────

    #[test]
    fn read_recent_returns_newest_first_capped_at_limit() {
        let env = TempEnvironment::builder().build();
        for i in 1..=5 {
            write_profile(
                &env,
                &format!("profile-{i}-1-1.tsv"),
                "# columns\tphase\tpack\thandler\ttarget\tstart_t\tend_t\texit_status\n",
            );
        }
        let recent = read_recent_profiles(env.fs.as_ref(), env.paths.as_ref(), 3).unwrap();
        let names: Vec<&str> = recent.iter().map(|p| p.filename.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "profile-5-1-1.tsv",
                "profile-4-1-1.tsv",
                "profile-3-1-1.tsv",
            ]
        );
    }

    #[test]
    fn read_recent_with_limit_zero_returns_empty() {
        let env = TempEnvironment::builder().build();
        write_profile(&env, "profile-1-1-1.tsv", "x");
        let recent = read_recent_profiles(env.fs.as_ref(), env.paths.as_ref(), 0).unwrap();
        assert!(recent.is_empty());
    }

    #[test]
    fn read_recent_handles_fewer_files_than_limit() {
        let env = TempEnvironment::builder().build();
        write_profile(&env, "profile-1-1-1.tsv", "");
        let recent = read_recent_profiles(env.fs.as_ref(), env.paths.as_ref(), 100).unwrap();
        assert_eq!(recent.len(), 1);
    }

    // ── percentile + aggregate ────────────────────────────────────

    #[test]
    fn percentile_nearest_rank_basic_cases() {
        // Ten samples 1..=10. p50 = 5 (lower-median, nearest-rank
        // doesn't interpolate); p95 = 10 (max-ish, since ceil(0.95*10)=10).
        let v: Vec<u64> = (1..=10).collect();
        assert_eq!(percentile(&v, 50), 5);
        assert_eq!(percentile(&v, 95), 10);
        // Single sample → all percentiles return it.
        assert_eq!(percentile(&[42], 50), 42);
        assert_eq!(percentile(&[42], 95), 42);
        // Empty slice safely returns 0.
        assert_eq!(percentile(&[], 50), 0);
    }

    #[test]
    fn aggregate_profiles_buckets_by_pack_handler_target() {
        let p1 = Profile {
            filename: "profile-1-1-1.tsv".into(),
            shell: "bash".into(),
            total_duration_us: 0,
            entries: vec![
                entry("vim", "shell", "/a", 100),
                entry("vim", "shell", "/b", 200),
            ],
        };
        let p2 = Profile {
            filename: "profile-2-1-1.tsv".into(),
            shell: "bash".into(),
            total_duration_us: 0,
            entries: vec![
                entry("vim", "shell", "/a", 110),
                entry("vim", "shell", "/b", 250),
            ],
        };
        let p3 = Profile {
            filename: "profile-3-1-1.tsv".into(),
            shell: "bash".into(),
            total_duration_us: 0,
            entries: vec![entry("vim", "shell", "/a", 120)],
            // /b absent in this run (sparse target presence).
        };
        let agg = aggregate_profiles(&[p1, p2, p3]);
        assert_eq!(agg.runs, 3);
        assert_eq!(agg.targets.len(), 2);
        let a = agg.targets.iter().find(|t| t.target == "/a").unwrap();
        assert_eq!(a.runs_seen, 3);
        assert_eq!(a.runs_total, 3);
        assert_eq!(a.p50_us, 110); // sorted: 100,110,120 → idx ceil(0.5*3)-1=1
        assert_eq!(a.max_us, 120);
        let b = agg.targets.iter().find(|t| t.target == "/b").unwrap();
        assert_eq!(b.runs_seen, 2);
        assert_eq!(b.runs_total, 3);
        assert_eq!(b.max_us, 250);
    }

    #[test]
    fn aggregate_empty_profiles_returns_empty_view() {
        let agg = aggregate_profiles(&[]);
        assert_eq!(agg.runs, 0);
        assert!(agg.targets.is_empty());
    }

    #[test]
    fn aggregate_targets_sort_by_pack_handler_target() {
        // Inputs scrambled; output must be (pack, handler, target)-sorted.
        let p = Profile {
            filename: "p".into(),
            shell: "".into(),
            total_duration_us: 0,
            entries: vec![
                entry("vim", "shell", "/z", 1),
                entry("git", "shell", "/a", 1),
                entry("vim", "path", "/x", 1),
                entry("git", "shell", "/y", 1),
            ],
        };
        let agg = aggregate_profiles(&[p]);
        let keys: Vec<(&str, &str, &str)> = agg
            .targets
            .iter()
            .map(|t| (t.pack.as_str(), t.handler.as_str(), t.target.as_str()))
            .collect();
        assert_eq!(
            keys,
            vec![
                ("git", "shell", "/a"),
                ("git", "shell", "/y"),
                ("vim", "path", "/x"),
                ("vim", "shell", "/z"),
            ]
        );
    }

    // ── history ───────────────────────────────────────────────────

    #[test]
    fn summarize_history_pulls_basic_metrics_per_run() {
        let p1 = Profile {
            filename: "profile-1714000000-12-34.tsv".into(),
            shell: "bash 5.3".into(),
            total_duration_us: 500,
            entries: vec![
                entry("vim", "shell", "/a", 100),
                ProfileEntry {
                    phase: "source".into(),
                    pack: "gh".into(),
                    handler: "shell".into(),
                    target: "/x".into(),
                    duration_us: 50,
                    exit_status: 1, // hidden failure
                },
            ],
        };
        let h = summarize_history(&[p1]);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].unix_ts, 1714000000);
        assert_eq!(h[0].shell, "bash 5.3");
        assert_eq!(h[0].total_us, 500);
        assert_eq!(h[0].user_total_us, 150);
        assert_eq!(h[0].failed_entries, 1);
        assert_eq!(h[0].entry_count, 2);
    }

    #[test]
    fn parse_unix_ts_handles_unparseable_filenames() {
        // Best-effort: an unrecognised filename returns 0 rather than
        // crashing the history view.
        assert_eq!(
            parse_unix_ts_from_filename("profile-1714000000-1-1.tsv"),
            1714000000
        );
        assert_eq!(parse_unix_ts_from_filename("garbage.txt"), 0);
        assert_eq!(parse_unix_ts_from_filename("profile-notanum-1-1.tsv"), 0);
    }
}

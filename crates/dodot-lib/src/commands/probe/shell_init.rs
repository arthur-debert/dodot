//! Entry points and helpers for the `probe shell-init` family.
//!
//! Five public entry points produce the five `ShellInit*` `ProbeResult`
//! variants:
//!
//! - [`shell_init`] — most recent profile, grouped by (pack, handler)
//! - [`shell_init_aggregate`] — percentile stats across last N runs
//! - [`shell_init_history`] — one summary row per recent profile
//! - [`shell_init_filter`] — drill-down by `<pack>[/<file>]`
//! - [`shell_init_errors`] — non-zero-exit entries across the window

use crate::commands::probe::types::{
    ProbeResult, ShellInitAggregateRow, ShellInitAggregateView, ShellInitErrorsView,
    ShellInitFilterRun, ShellInitFilterTarget, ShellInitFilterView, ShellInitGroup,
    ShellInitHistoryRow, ShellInitHistoryView, ShellInitRow, ShellInitView,
};
use crate::packs::orchestration::ExecutionContext;
use crate::probe::{
    aggregate_profiles, group_profile, parse_unix_ts_from_filename, read_last_up_marker,
    read_latest_profile, read_recent_profiles, summarize_history, AggregatedTarget, GroupedProfile,
    HistoryEntry,
};
use crate::Result;

/// Render the most recent shell-init profile.
///
/// When no profile has been written yet (fresh install, or profiling
/// disabled, or the user hasn't started a shell since the last `up`),
/// returns a "no data" view with `has_profile = false`. The template
/// uses that flag to print a hint instead of an empty table.
pub fn shell_init(ctx: &ExecutionContext) -> Result<ProbeResult> {
    let root_config = ctx.config_manager.root_config()?;
    let profiling_enabled = root_config.profiling.enabled;

    let profile_opt = read_latest_profile(ctx.fs.as_ref(), ctx.paths.as_ref())?;
    let profiles_dir = ctx.paths.probes_shell_init_dir().display().to_string();
    let last_up_ts = read_last_up_marker(ctx.fs.as_ref(), ctx.paths.as_ref());
    let last_up_when = last_up_ts.map(format_unix_ts).unwrap_or_default();

    let view = match profile_opt {
        Some(profile) => {
            let grouped = group_profile(&profile);
            let profile_ts = parse_unix_ts_from_filename(&profile.filename);
            let stale = is_stale(profile_ts, last_up_ts);
            ShellInitView {
                filename: profile.filename.clone(),
                shell: profile.shell.clone(),
                profiling_enabled,
                has_profile: true,
                groups: shell_init_groups(&grouped),
                user_total_us: grouped.user_total_us,
                framing_us: grouped.framing_us,
                total_us: grouped.total_us,
                profiles_dir,
                stale,
                profile_when: format_unix_ts(profile_ts),
                last_up_when,
            }
        }
        None => ShellInitView {
            filename: String::new(),
            shell: String::new(),
            profiling_enabled,
            has_profile: false,
            groups: Vec::new(),
            user_total_us: 0,
            framing_us: 0,
            total_us: 0,
            profiles_dir,
            stale: false,
            profile_when: String::new(),
            last_up_when,
        },
    };

    Ok(ProbeResult::ShellInit(view))
}

/// Decide whether a profile timestamp predates the last `dodot up`.
/// Returns false when either timestamp is unknown — we never warn on
/// guesswork, only when we have both reference points.
fn is_stale(profile_ts: u64, last_up_ts: Option<u64>) -> bool {
    matches!(last_up_ts, Some(last) if profile_ts > 0 && profile_ts < last)
}

fn shell_init_groups(grouped: &GroupedProfile) -> Vec<ShellInitGroup> {
    grouped
        .groups
        .iter()
        .map(|g| ShellInitGroup {
            pack: g.pack.clone(),
            handler: g.handler.clone(),
            rows: g
                .rows
                .iter()
                .map(|r| ShellInitRow {
                    target: short_target(&r.target),
                    duration_us: r.duration_us,
                    duration_label: humanize_us(r.duration_us),
                    exit_status: r.exit_status,
                    status_class: if r.exit_status == 0 {
                        "deployed"
                    } else {
                        "error"
                    },
                })
                .collect(),
            group_total_us: g.group_total_us,
            group_total_label: humanize_us(g.group_total_us),
        })
        .collect()
}

/// Display-friendly basename for a target path. The fully-qualified
/// path is in the on-disk profile already; the rendered table is
/// narrow.
fn short_target(target: &str) -> String {
    std::path::Path::new(target)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| target.to_string())
}

/// Compact human duration: "0 µs" / "1.2 ms" / "350 ms" / "1.4 s".
pub fn humanize_us(us: u64) -> String {
    if us < 1_000 {
        format!("{us} µs")
    } else if us < 1_000_000 {
        format!("{:.1} ms", us as f64 / 1_000.0)
    } else {
        format!("{:.2} s", us as f64 / 1_000_000.0)
    }
}

/// Aggregate the last `runs` profiles into per-target percentile stats.
///
/// The CLI applies a default of 10 when the user passes `--runs`
/// without a value (see `clap`'s `default_missing_value` in
/// `dodot-cli/src/main.rs`); this function takes the resolved count
/// directly so it stays useful from external callers (tests, custom
/// harnesses) that pick their own N.
pub fn shell_init_aggregate(ctx: &ExecutionContext, runs: usize) -> Result<ProbeResult> {
    let root_config = ctx.config_manager.root_config()?;
    let profiling_enabled = root_config.profiling.enabled;
    let profiles = read_recent_profiles(ctx.fs.as_ref(), ctx.paths.as_ref(), runs)?;
    // read_recent_profiles returns newest-first, so the first entry's
    // filename is the most recent capture.
    let latest_profile_ts = profiles
        .first()
        .map(|p| parse_unix_ts_from_filename(&p.filename))
        .unwrap_or(0);
    let view = aggregate_profiles(&profiles);
    let profiles_dir = ctx.paths.probes_shell_init_dir().display().to_string();
    let last_up_ts = read_last_up_marker(ctx.fs.as_ref(), ctx.paths.as_ref());

    Ok(ProbeResult::ShellInitAggregate(ShellInitAggregateView {
        runs: view.runs,
        requested_runs: runs,
        profiling_enabled,
        profiles_dir,
        rows: view.targets.into_iter().map(into_aggregate_row).collect(),
        stale: is_stale(latest_profile_ts, last_up_ts),
        latest_profile_when: format_unix_ts(latest_profile_ts),
        last_up_when: last_up_ts.map(format_unix_ts).unwrap_or_default(),
    }))
}

fn into_aggregate_row(t: AggregatedTarget) -> ShellInitAggregateRow {
    ShellInitAggregateRow {
        pack: t.pack,
        handler: t.handler,
        target: short_target(&t.target),
        p50_label: humanize_us(t.p50_us),
        p95_label: humanize_us(t.p95_us),
        max_label: humanize_us(t.max_us),
        p50_us: t.p50_us,
        p95_us: t.p95_us,
        max_us: t.max_us,
        seen_label: format!("{}/{}", t.runs_seen, t.runs_total),
        runs_seen: t.runs_seen,
        runs_total: t.runs_total,
    }
}

/// Match a profile target path against a filename-or-subpath filter.
///
/// Returns true when:
/// - the filter is a bare basename (`env.sh`) and `target`'s last path
///   component equals it, or
/// - the filter is a subpath (`subdir/env.sh`) and `target` ends with
///   that subpath at a path boundary.
///
/// The boundary check (`/{filter}` suffix) prevents `env.sh` from
/// matching `nvenv.sh` or other filenames that happen to end with the
/// same characters.
fn target_matches_filter(target: &str, filter: &str) -> bool {
    if !filter.contains('/') {
        return std::path::Path::new(target)
            .file_name()
            .is_some_and(|s| s == std::ffi::OsStr::new(filter));
    }
    // Subpath form: must end at a path boundary so `dir/env.sh` doesn't
    // accidentally match `otherdir/env.sh`.
    target.ends_with(&format!("/{filter}")) || target == filter
}

/// Render the filtered drill-down view for a `<pack>[/<file>]` filter.
///
/// `runs` controls how many recent profiles are examined; the caller
/// passes [`crate::commands::probe::DEFAULT_FILTER_RUNS`] unless it has
/// a specific reason to look further or fewer.
pub fn shell_init_filter(ctx: &ExecutionContext, filter: &str, runs: usize) -> Result<ProbeResult> {
    let root_config = ctx.config_manager.root_config()?;
    let profiling_enabled = root_config.profiling.enabled;
    let profiles_dir = ctx.paths.probes_shell_init_dir().display().to_string();
    let last_up_ts = read_last_up_marker(ctx.fs.as_ref(), ctx.paths.as_ref());
    let last_up_when = last_up_ts.map(format_unix_ts).unwrap_or_default();

    // Filter parsing: `pack` or `pack/file`. Trim a leading `./` and a
    // trailing `/` defensively so users can paste tab-completed paths.
    let trimmed = filter.trim().trim_start_matches("./").trim_end_matches('/');
    let (filter_pack, filter_filename) = match trimmed.split_once('/') {
        Some((p, f)) if !p.is_empty() && !f.is_empty() => (p.to_string(), Some(f.to_string())),
        _ => (trimmed.to_string(), None),
    };

    let profiles = read_recent_profiles(ctx.fs.as_ref(), ctx.paths.as_ref(), runs)?;
    let latest_profile_ts = profiles
        .first()
        .map(|p| parse_unix_ts_from_filename(&p.filename))
        .unwrap_or(0);

    // Bucket per `(pack, handler, target)`. Order: targets sorted by
    // path so output is stable; runs within each target stay newest-
    // first (matching the input slice order).
    use std::collections::BTreeMap;
    let mut buckets: BTreeMap<(String, String, String), Vec<ShellInitFilterRun>> = BTreeMap::new();

    for profile in &profiles {
        let when = format_unix_ts(parse_unix_ts_from_filename(&profile.filename));
        for entry in &profile.entries {
            if entry.pack != filter_pack {
                continue;
            }
            if let Some(name) = &filter_filename {
                if !target_matches_filter(&entry.target, name) {
                    continue;
                }
            }
            let stderr_lines: Vec<String> = profile
                .errors
                .iter()
                .find(|er| er.target == entry.target)
                .map(|er| {
                    er.message
                        .trim_end()
                        .lines()
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default();
            buckets
                .entry((
                    entry.pack.clone(),
                    entry.handler.clone(),
                    entry.target.clone(),
                ))
                .or_default()
                .push(ShellInitFilterRun {
                    when: when.clone(),
                    duration_us: entry.duration_us,
                    duration_label: humanize_us(entry.duration_us),
                    exit_status: entry.exit_status,
                    status_class: if entry.exit_status == 0 {
                        "deployed"
                    } else {
                        "error"
                    },
                    stderr_lines,
                    profile_filename: profile.filename.clone(),
                });
        }
    }

    let targets: Vec<ShellInitFilterTarget> = buckets
        .into_iter()
        .map(|((pack, handler, target), runs_vec)| {
            let display_target = std::path::Path::new(&target)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| target.clone());
            let failure_count = runs_vec.iter().filter(|r| r.exit_status != 0).count();
            ShellInitFilterTarget {
                target,
                display_target,
                pack,
                handler,
                runs: runs_vec,
                failure_count,
            }
        })
        .collect();

    Ok(ProbeResult::ShellInitFilter(ShellInitFilterView {
        profiling_enabled,
        profiles_dir,
        filter: filter.trim().to_string(),
        filter_pack,
        filter_filename,
        runs_examined: profiles.len(),
        targets,
        stale: is_stale(latest_profile_ts, last_up_ts),
        latest_profile_when: format_unix_ts(latest_profile_ts),
        last_up_when,
    }))
}

/// Render the cross-history errors view.
///
/// Scans the last `runs` profiles, keeps only entries with non-zero
/// exit status, groups them by target, and orders by failure count
/// (most-broken first). The `runs` parameter follows the same window
/// convention as [`shell_init_filter`].
pub fn shell_init_errors(ctx: &ExecutionContext, runs: usize) -> Result<ProbeResult> {
    let root_config = ctx.config_manager.root_config()?;
    let profiling_enabled = root_config.profiling.enabled;
    let profiles_dir = ctx.paths.probes_shell_init_dir().display().to_string();
    let last_up_ts = read_last_up_marker(ctx.fs.as_ref(), ctx.paths.as_ref());
    let last_up_when = last_up_ts.map(format_unix_ts).unwrap_or_default();

    let profiles = read_recent_profiles(ctx.fs.as_ref(), ctx.paths.as_ref(), runs)?;
    let latest_profile_ts = profiles
        .first()
        .map(|p| parse_unix_ts_from_filename(&p.filename))
        .unwrap_or(0);

    use std::collections::BTreeMap;
    let mut buckets: BTreeMap<(String, String, String), Vec<ShellInitFilterRun>> = BTreeMap::new();

    for profile in &profiles {
        let when = format_unix_ts(parse_unix_ts_from_filename(&profile.filename));
        for entry in &profile.entries {
            // Errors-only: skip clean runs entirely.
            if entry.exit_status == 0 {
                continue;
            }
            let stderr_lines: Vec<String> = profile
                .errors
                .iter()
                .find(|er| er.target == entry.target)
                .map(|er| {
                    er.message
                        .trim_end()
                        .lines()
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default();
            buckets
                .entry((
                    entry.pack.clone(),
                    entry.handler.clone(),
                    entry.target.clone(),
                ))
                .or_default()
                .push(ShellInitFilterRun {
                    when: when.clone(),
                    duration_us: entry.duration_us,
                    duration_label: humanize_us(entry.duration_us),
                    exit_status: entry.exit_status,
                    status_class: "error",
                    stderr_lines,
                    profile_filename: profile.filename.clone(),
                });
        }
    }

    let mut targets: Vec<ShellInitFilterTarget> = buckets
        .into_iter()
        .map(|((pack, handler, target), runs_vec)| {
            let display_target = std::path::Path::new(&target)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| target.clone());
            let failure_count = runs_vec.len();
            ShellInitFilterTarget {
                target,
                display_target,
                pack,
                handler,
                runs: runs_vec,
                failure_count,
            }
        })
        .collect();

    // Sort: most-broken first, with a stable (pack, handler, target)
    // tiebreaker so two targets with the same failure count don't swap
    // positions across runs.
    targets.sort_by(|a, b| {
        b.failure_count
            .cmp(&a.failure_count)
            .then_with(|| a.pack.cmp(&b.pack))
            .then_with(|| a.handler.cmp(&b.handler))
            .then_with(|| a.target.cmp(&b.target))
    });

    Ok(ProbeResult::ShellInitErrors(ShellInitErrorsView {
        profiling_enabled,
        profiles_dir,
        runs_examined: profiles.len(),
        targets,
        stale: is_stale(latest_profile_ts, last_up_ts),
        latest_profile_when: format_unix_ts(latest_profile_ts),
        last_up_when,
    }))
}

/// Render the per-run history view (one summary line per profile).
pub fn shell_init_history(ctx: &ExecutionContext, limit: usize) -> Result<ProbeResult> {
    let root_config = ctx.config_manager.root_config()?;
    let profiling_enabled = root_config.profiling.enabled;
    let profiles = read_recent_profiles(ctx.fs.as_ref(), ctx.paths.as_ref(), limit)?;
    // `read_recent_profiles` already returns newest-first, which is the
    // order users expect for a history listing (most recent at the top
    // of the table). Don't reverse.
    let latest_profile_ts = profiles
        .first()
        .map(|p| parse_unix_ts_from_filename(&p.filename))
        .unwrap_or(0);
    let history = summarize_history(&profiles);
    let profiles_dir = ctx.paths.probes_shell_init_dir().display().to_string();
    let last_up_ts = read_last_up_marker(ctx.fs.as_ref(), ctx.paths.as_ref());

    Ok(ProbeResult::ShellInitHistory(ShellInitHistoryView {
        profiling_enabled,
        profiles_dir,
        rows: history.into_iter().map(into_history_row).collect(),
        stale: is_stale(latest_profile_ts, last_up_ts),
        latest_profile_when: format_unix_ts(latest_profile_ts),
        last_up_when: last_up_ts.map(format_unix_ts).unwrap_or_default(),
    }))
}

fn into_history_row(h: HistoryEntry) -> ShellInitHistoryRow {
    ShellInitHistoryRow {
        filename: h.filename,
        unix_ts: h.unix_ts,
        when: format_unix_ts(h.unix_ts),
        shell: h.shell,
        total_label: humanize_us(h.total_us),
        user_total_label: humanize_us(h.user_total_us),
        total_us: h.total_us,
        user_total_us: h.user_total_us,
        failed_entries: h.failed_entries,
        entry_count: h.entry_count,
    }
}

/// Format a unix timestamp as `YYYY-MM-DD HH:MM` in UTC. Returns an
/// empty string for `0` (parse-failure sentinel) so the renderer can
/// just print a blank cell.
///
/// Does the calendar math by hand to avoid pulling a dep — chrono is
/// overkill for one display string. Algorithm: Howard Hinnant's
/// civil_from_days.
pub fn format_unix_ts(ts: u64) -> String {
    // 0 is the parse-failure sentinel from `parse_unix_ts_from_filename`;
    // anything past year 9999 is also nonsense in a shell-startup
    // profile (the file format itself is the giveaway). Returning an
    // empty string keeps the renderer predictable even in the face of
    // a tampered-with filename, and bounds the i64 cast on `days`
    // safely below i64::MAX regardless of input.
    const MAX_REASONABLE_TS: u64 = 253_402_300_799; // 9999-12-31T23:59:59 UTC.
    if ts == 0 || ts > MAX_REASONABLE_TS {
        return String::new();
    }
    let secs_per_day: u64 = 86_400;
    let days = (ts / secs_per_day) as i64; // safe: ts < 2.5e11 → days < 3e6
    let secs_of_day = ts % secs_per_day;
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02} {hour:02}:{minute:02}")
}

/// Howard Hinnant's `civil_from_days`: convert days since 1970-01-01
/// (UTC) into `(year, month, day)`. Public-domain algorithm.
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_unix_ts_handles_zero_and_out_of_range() {
        // Sentinel for parse-failure → empty, not a date.
        assert_eq!(format_unix_ts(0), "");
        // Real timestamp → formatted.
        assert_eq!(format_unix_ts(1_714_000_000), "2024-04-24 23:06");
        // Past year 9999 → empty (defensive ceiling so a tampered
        // filename doesn't produce a nonsense date or risk overflow
        // during the i64 cast on `days`).
        assert_eq!(format_unix_ts(u64::MAX), "");
        assert_eq!(format_unix_ts(253_402_300_800), ""); // 1s past year 9999.
                                                         // Right below the ceiling still renders.
        assert_eq!(format_unix_ts(253_402_300_799), "9999-12-31 23:59");
    }
}

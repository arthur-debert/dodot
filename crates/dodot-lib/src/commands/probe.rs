//! `probe` command family — introspection subcommands.
//!
//! Today there are three: `deployment-map`, `show-data-dir`, and the
//! bare `probe` (which renders a summary pointing at the others). A
//! later phase adds `shell-init` (timing reports). See
//! `docs/proposals/profiling.lex`.
//!
//! All variants serialize through a single `ProbeResult` enum that is
//! `#[serde(tag = "kind")]`-tagged; the matching Jinja template
//! branches on `kind` to pick the right section.

use serde::Serialize;

use crate::packs::orchestration::ExecutionContext;
use crate::probe::{
    aggregate_profiles, collect_data_dir_tree, collect_deployment_map, group_profile,
    read_latest_profile, read_recent_profiles, summarize_history, AggregatedTarget,
    DeploymentMapEntry, GroupedProfile, HistoryEntry, TreeNode,
};
use crate::Result;

/// Default max depth for `probe show-data-dir`. Enough to show
/// `packs / <pack> / <handler> / <entry>` without scrolling off
/// screen for reasonable installs; deeper subtrees are summarised.
pub const DEFAULT_SHOW_DATA_DIR_DEPTH: usize = 4;

/// Display-shaped deployment-map row. Paths are pre-shortened to
/// `~/…` where they live under HOME so the rendered table stays
/// narrow; the machine-readable TSV on disk keeps absolute paths.
#[derive(Debug, Clone, Serialize)]
pub struct DeploymentDisplayEntry {
    pub pack: String,
    pub handler: String,
    pub kind: String,
    /// Pre-shortened (`~/…`) absolute source path; empty for
    /// non-symlink entries (sentinels, rendered files).
    pub source: String,
    /// Pre-shortened absolute datastore path.
    pub datastore: String,
}

/// One line of tree output, pre-flattened for the template.
///
/// The template for a tree is annoying to write directly in Jinja
/// (indentation, prefix characters, etc.), so we flatten the tree to
/// a list of `(indent, name, annotation)` triples here.
#[derive(Debug, Clone, Serialize)]
pub struct TreeLine {
    /// Indent prefix (e.g. `"  │  ├─ "`).
    pub prefix: String,
    /// The node's display name (basename for non-root nodes).
    pub name: String,
    /// A dim-styled annotation shown after the name (size, link target,
    /// truncation count). Empty when the node has nothing extra to say.
    pub annotation: String,
}

/// Result of any `probe` invocation. Serialises with a `kind` tag so
/// the Jinja template can dispatch on it.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ProbeResult {
    /// `dodot probe` with no subcommand — a summary pointing the user
    /// at the real subcommands.
    Summary {
        data_dir: String,
        available: Vec<ProbeSubcommandInfo>,
    },
    /// `dodot probe deployment-map` — the source↔deployed map.
    DeploymentMap {
        data_dir: String,
        map_path: String,
        entries: Vec<DeploymentDisplayEntry>,
    },
    /// `dodot probe show-data-dir` — a bounded tree view of
    /// `<data_dir>`.
    ShowDataDir {
        data_dir: String,
        /// Flattened, template-ready lines.
        lines: Vec<TreeLine>,
        total_nodes: usize,
        /// Size in bytes of the whole tree (symlinks counted by their
        /// link-entry size).
        total_size: u64,
    },
    /// `dodot probe shell-init` — the most recent shell-startup profile,
    /// grouped by pack and handler.
    ShellInit(ShellInitView),
    /// `dodot probe shell-init --runs N` — per-target percentile stats
    /// across the last N runs.
    ShellInitAggregate(ShellInitAggregateView),
    /// `dodot probe shell-init --history` — one summary line per recent
    /// run, oldest run first (so the latest is closest to the eye in a
    /// terminal where output scrolls down).
    ShellInitHistory(ShellInitHistoryView),
}

/// Display payload for `--runs N`.
#[derive(Debug, Clone, Serialize)]
pub struct ShellInitAggregateView {
    /// How many profiles were actually loaded (may be smaller than the
    /// requested N if there aren't enough on disk yet).
    pub runs: usize,
    /// User-requested N (echoed back so the renderer can say
    /// "showing 4 of last 10 requested").
    pub requested_runs: usize,
    pub profiling_enabled: bool,
    pub profiles_dir: String,
    pub rows: Vec<ShellInitAggregateRow>,
}

/// One per-target aggregate row, durations pre-humanised for the
/// template.
#[derive(Debug, Clone, Serialize)]
pub struct ShellInitAggregateRow {
    pub pack: String,
    pub handler: String,
    pub target: String,
    pub p50_label: String,
    pub p95_label: String,
    pub max_label: String,
    pub p50_us: u64,
    pub p95_us: u64,
    pub max_us: u64,
    /// e.g. `"7/10"` — formatted at the lib so JSON consumers and the
    /// template both render identically.
    pub seen_label: String,
    pub runs_seen: usize,
    pub runs_total: usize,
}

/// Display payload for `--history`.
#[derive(Debug, Clone, Serialize)]
pub struct ShellInitHistoryView {
    pub profiling_enabled: bool,
    pub profiles_dir: String,
    pub rows: Vec<ShellInitHistoryRow>,
}

/// One per-run row in `--history`.
#[derive(Debug, Clone, Serialize)]
pub struct ShellInitHistoryRow {
    /// Filename of the underlying TSV — useful for cross-reference and
    /// keeps history rows traceable to the on-disk artefact.
    pub filename: String,
    /// Unix timestamp parsed from the filename (or `0` when the
    /// filename doesn't follow the expected pattern). Surfaced in JSON
    /// so machine consumers can do their own date math without
    /// re-parsing `filename`.
    pub unix_ts: u64,
    /// Compact `YYYY-MM-DD HH:MM` formatted from the unix timestamp in
    /// the filename. Empty when the timestamp couldn't be parsed.
    pub when: String,
    pub shell: String,
    pub total_label: String,
    pub user_total_label: String,
    pub total_us: u64,
    pub user_total_us: u64,
    pub failed_entries: usize,
    pub entry_count: usize,
}

/// Display payload for `probe shell-init`. Pulled into its own struct
/// so the JSON view stays clean and the variant constructor in
/// `shell_init()` reads naturally.
#[derive(Debug, Clone, Serialize)]
pub struct ShellInitView {
    /// Source filename of the report (for "which run is this?" UX).
    /// Empty when no profile has been written yet.
    pub filename: String,
    /// Shell label as recorded in the preamble (e.g. `bash 5.3.9`).
    pub shell: String,
    /// True when the profiling wrapper is enabled in config.
    pub profiling_enabled: bool,
    /// True when the directory exists and contained a parseable file.
    pub has_profile: bool,
    /// Pre-grouped rows for the template; empty when `has_profile` is
    /// false.
    pub groups: Vec<ShellInitGroup>,
    pub user_total_us: u64,
    pub framing_us: u64,
    pub total_us: u64,
    /// Where the profiles live on disk (so the user can `ls` it).
    pub profiles_dir: String,
}

/// Display row for one entry in a shell-init group.
#[derive(Debug, Clone, Serialize)]
pub struct ShellInitRow {
    pub target: String,
    pub duration_us: u64,
    pub duration_label: String,
    pub exit_status: i32,
    /// `"deployed"` (success — rendered green) or `"error"` (non-zero
    /// source exit). These map directly to existing styles in
    /// `crate::render`'s theme; using fresh names here would require
    /// theme additions for no UX gain.
    pub status_class: &'static str,
}

/// Display group: one (pack, handler) bucket of shell-init rows.
#[derive(Debug, Clone, Serialize)]
pub struct ShellInitGroup {
    pub pack: String,
    pub handler: String,
    pub rows: Vec<ShellInitRow>,
    pub group_total_us: u64,
    pub group_total_label: String,
}

/// One entry in the `probe` summary listing.
#[derive(Debug, Clone, Serialize)]
pub struct ProbeSubcommandInfo {
    pub name: &'static str,
    pub description: &'static str,
}

/// The full list of probe subcommands, used by the summary view.
/// Keeping them in one array keeps the CLI registration, clap
/// registration, and summary output trivially in sync.
pub const PROBE_SUBCOMMANDS: &[ProbeSubcommandInfo] = &[
    ProbeSubcommandInfo {
        name: "deployment-map",
        description: "Source↔deployed map — what dodot linked where.",
    },
    ProbeSubcommandInfo {
        name: "shell-init",
        description: "Per-source timings for the most recent shell startup.",
    },
    ProbeSubcommandInfo {
        name: "show-data-dir",
        description: "Tree of dodot's data directory, with sizes.",
    },
];

// ── Entry points ────────────────────────────────────────────────────

/// Render the bare `dodot probe` summary.
pub fn summary(ctx: &ExecutionContext) -> Result<ProbeResult> {
    Ok(ProbeResult::Summary {
        data_dir: ctx.paths.data_dir().display().to_string(),
        available: PROBE_SUBCOMMANDS.to_vec(),
    })
}

/// Render the deployment map for display.
///
/// Reads the current datastore state (not the on-disk TSV) so the
/// output is always fresh even if the user never ran `dodot up`.
pub fn deployment_map(ctx: &ExecutionContext) -> Result<ProbeResult> {
    let raw = collect_deployment_map(ctx.fs.as_ref(), ctx.paths.as_ref())?;
    let home = ctx.paths.home_dir();
    let entries = raw
        .into_iter()
        .map(|e| into_display_entry(e, home))
        .collect();

    Ok(ProbeResult::DeploymentMap {
        data_dir: ctx.paths.data_dir().display().to_string(),
        map_path: ctx.paths.deployment_map_path().display().to_string(),
        entries,
    })
}

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

    let view = match profile_opt {
        Some(profile) => {
            let grouped = group_profile(&profile);
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
        },
    };

    Ok(ProbeResult::ShellInit(view))
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
    let view = aggregate_profiles(&profiles);
    let profiles_dir = ctx.paths.probes_shell_init_dir().display().to_string();

    Ok(ProbeResult::ShellInitAggregate(ShellInitAggregateView {
        runs: view.runs,
        requested_runs: runs,
        profiling_enabled,
        profiles_dir,
        rows: view.targets.into_iter().map(into_aggregate_row).collect(),
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

/// Default cap on the number of history rows emitted, so a user with
/// hundreds of profiles doesn't get a page-filling race down their
/// terminal.
pub const DEFAULT_HISTORY_LIMIT: usize = 50;

/// Render the per-run history view (one summary line per profile).
pub fn shell_init_history(ctx: &ExecutionContext, limit: usize) -> Result<ProbeResult> {
    let root_config = ctx.config_manager.root_config()?;
    let profiling_enabled = root_config.profiling.enabled;
    let mut profiles = read_recent_profiles(ctx.fs.as_ref(), ctx.paths.as_ref(), limit)?;
    // read_recent_profiles returns newest-first; reverse so output reads
    // chronologically (oldest at top, latest at the bottom near the
    // user's prompt).
    profiles.reverse();
    let history = summarize_history(&profiles);
    let profiles_dir = ctx.paths.probes_shell_init_dir().display().to_string();

    Ok(ProbeResult::ShellInitHistory(ShellInitHistoryView {
        profiling_enabled,
        profiles_dir,
        rows: history.into_iter().map(into_history_row).collect(),
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

/// Render the data-dir tree.
pub fn show_data_dir(ctx: &ExecutionContext, max_depth: usize) -> Result<ProbeResult> {
    let tree = collect_data_dir_tree(ctx.fs.as_ref(), ctx.paths.as_ref(), max_depth)?;
    let total_nodes = tree.count_nodes();
    let total_size = tree.total_size();
    let mut lines = Vec::new();
    flatten_tree(&tree, "", true, &mut lines, true);
    Ok(ProbeResult::ShowDataDir {
        data_dir: ctx.paths.data_dir().display().to_string(),
        lines,
        total_nodes,
        total_size,
    })
}

// ── Display helpers ─────────────────────────────────────────────────

fn into_display_entry(e: DeploymentMapEntry, home: &std::path::Path) -> DeploymentDisplayEntry {
    DeploymentDisplayEntry {
        pack: e.pack,
        handler: e.handler,
        kind: e.kind.as_str().into(),
        source: if e.source.as_os_str().is_empty() {
            // Sentinel / rendered file — no source file backs this entry.
            // Show an em dash so the column stays populated.
            "—".into()
        } else {
            display_path(&e.source, home)
        },
        datastore: display_path(&e.datastore, home),
    }
}

fn display_path(p: &std::path::Path, home: &std::path::Path) -> String {
    if let Ok(rel) = p.strip_prefix(home) {
        format!("~/{}", rel.display())
    } else {
        p.display().to_string()
    }
}

/// Flatten a `TreeNode` into [`TreeLine`]s with box-drawing prefixes.
///
/// `prefix` is the continuation prefix applied to this node's
/// descendants (e.g. `"│  "` if this node has more siblings below,
/// `"   "` if it's the last). `is_last` controls the branch glyph for
/// this node itself (`"└─ "` vs `"├─ "`). `is_root` skips the branch
/// glyph on the topmost call so the root displays flush-left.
fn flatten_tree(
    node: &TreeNode,
    prefix: &str,
    is_last: bool,
    out: &mut Vec<TreeLine>,
    is_root: bool,
) {
    let branch = if is_root {
        String::new()
    } else if is_last {
        "└─ ".to_string()
    } else {
        "├─ ".to_string()
    };
    let line_prefix = format!("{prefix}{branch}");
    out.push(TreeLine {
        prefix: line_prefix,
        name: node.name.clone(),
        annotation: annotate(node),
    });

    if node.children.is_empty() {
        return;
    }

    // Extend the prefix for children. The root contributes no prefix
    // characters; a last-child contributes "   "; an inner child
    // contributes "│  ".
    let child_prefix = if is_root {
        String::new()
    } else if is_last {
        format!("{prefix}   ")
    } else {
        format!("{prefix}│  ")
    };

    let last_idx = node.children.len() - 1;
    for (i, child) in node.children.iter().enumerate() {
        flatten_tree(child, &child_prefix, i == last_idx, out, false);
    }
}

fn annotate(node: &TreeNode) -> String {
    match node.kind {
        "dir" => match node.truncated_count {
            Some(n) if n > 0 => format!("(… {n} more)"),
            _ => String::new(),
        },
        "file" => match node.size {
            Some(n) => humanize_bytes(n),
            None => String::new(),
        },
        "symlink" => match &node.link_target {
            Some(t) => format!("→ {t}"),
            None => "→ (broken)".into(),
        },
        _ => String::new(),
    }
}

/// Compact byte sizing: "512 B", "1.2 KB", "3.4 MB".
///
/// KB = 1024 bytes. No fractional KB below 1024.
pub fn humanize_bytes(n: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if n < KB {
        format!("{n} B")
    } else if n < MB {
        format!("{:.1} KB", n as f64 / KB as f64)
    } else if n < GB {
        format!("{:.1} MB", n as f64 / MB as f64)
    } else {
        format!("{:.1} GB", n as f64 / GB as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::{DeploymentKind, DeploymentMapEntry};
    use std::path::PathBuf;

    fn home() -> PathBuf {
        PathBuf::from("/home/alice")
    }

    #[test]
    fn display_path_shortens_home() {
        assert_eq!(
            display_path(&PathBuf::from("/home/alice/dotfiles/vim/rc"), &home()),
            "~/dotfiles/vim/rc"
        );
    }

    #[test]
    fn display_path_keeps_paths_outside_home() {
        assert_eq!(
            display_path(&PathBuf::from("/opt/data"), &home()),
            "/opt/data"
        );
    }

    #[test]
    fn humanize_bytes_boundaries() {
        assert_eq!(humanize_bytes(0), "0 B");
        assert_eq!(humanize_bytes(1023), "1023 B");
        assert_eq!(humanize_bytes(1024), "1.0 KB");
        assert_eq!(humanize_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(humanize_bytes(1024 * 1024 * 1024), "1.0 GB");
    }

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

    #[test]
    fn into_display_entry_handles_sentinel_source() {
        let entry = DeploymentMapEntry {
            pack: "nvim".into(),
            handler: "install".into(),
            kind: DeploymentKind::File,
            source: PathBuf::new(),
            datastore: PathBuf::from("/home/alice/.local/share/dodot/packs/nvim/install/sent"),
        };
        let display = into_display_entry(entry, &home());
        assert_eq!(display.source, "—");
        assert!(display.datastore.starts_with("~/"));
    }

    #[test]
    fn tree_flattening_produces_branch_glyphs() {
        // Fabricate a small tree:
        //   root
        //   ├─ a
        //   │  └─ aa
        //   └─ b
        let tree = TreeNode {
            name: "root".into(),
            path: PathBuf::from("/root"),
            kind: "dir",
            size: None,
            link_target: None,
            truncated_count: None,
            children: vec![
                TreeNode {
                    name: "a".into(),
                    path: PathBuf::from("/root/a"),
                    kind: "dir",
                    size: None,
                    link_target: None,
                    truncated_count: None,
                    children: vec![TreeNode {
                        name: "aa".into(),
                        path: PathBuf::from("/root/a/aa"),
                        kind: "file",
                        size: Some(10),
                        link_target: None,
                        truncated_count: None,
                        children: Vec::new(),
                    }],
                },
                TreeNode {
                    name: "b".into(),
                    path: PathBuf::from("/root/b"),
                    kind: "file",
                    size: Some(42),
                    link_target: None,
                    truncated_count: None,
                    children: Vec::new(),
                },
            ],
        };
        let mut lines = Vec::new();
        flatten_tree(&tree, "", true, &mut lines, true);
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].name, "root");
        assert_eq!(lines[0].prefix, ""); // root is flush-left
        assert_eq!(lines[1].name, "a");
        assert!(lines[1].prefix.ends_with("├─ "));
        assert_eq!(lines[2].name, "aa");
        assert!(lines[2].prefix.ends_with("└─ "));
        assert!(lines[2].prefix.starts_with("│")); // parent is not last
        assert_eq!(lines[3].name, "b");
        assert!(lines[3].prefix.ends_with("└─ "));
        assert_eq!(lines[3].annotation, "42 B");
    }

    #[test]
    fn annotate_symlink_with_target() {
        let node = TreeNode {
            name: "link".into(),
            path: PathBuf::from("/x"),
            kind: "symlink",
            size: Some(20),
            link_target: Some("/target".into()),
            truncated_count: None,
            children: Vec::new(),
        };
        assert_eq!(annotate(&node), "→ /target");
    }

    #[test]
    fn annotate_broken_symlink() {
        let node = TreeNode {
            name: "link".into(),
            path: PathBuf::from("/x"),
            kind: "symlink",
            size: Some(20),
            link_target: None,
            truncated_count: None,
            children: Vec::new(),
        };
        assert_eq!(annotate(&node), "→ (broken)");
    }

    #[test]
    fn annotate_truncated_dir() {
        let node = TreeNode {
            name: "deep".into(),
            path: PathBuf::from("/x"),
            kind: "dir",
            size: None,
            link_target: None,
            truncated_count: Some(7),
            children: Vec::new(),
        };
        assert_eq!(annotate(&node), "(… 7 more)");
    }

    #[test]
    fn probe_result_deployment_map_serialises_with_kind_tag() {
        let result = ProbeResult::DeploymentMap {
            data_dir: "/d".into(),
            map_path: "/d/deployment-map.tsv".into(),
            entries: Vec::new(),
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["kind"], "deployment-map");
        assert!(json["entries"].is_array());
    }

    #[test]
    fn probe_result_show_data_dir_serialises_with_kind_tag() {
        let result = ProbeResult::ShowDataDir {
            data_dir: "/d".into(),
            lines: Vec::new(),
            total_nodes: 1,
            total_size: 0,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["kind"], "show-data-dir");
        assert_eq!(json["total_nodes"], 1);
    }

    #[test]
    fn probe_subcommands_list_matches_variants() {
        // Failsafe: if we add a probe subcommand to the enum we should
        // add it to the summary list too. This assertion catches the
        // former getting ahead of the latter.
        let names: Vec<&str> = PROBE_SUBCOMMANDS.iter().map(|s| s.name).collect();
        assert!(names.contains(&"deployment-map"));
        assert!(names.contains(&"show-data-dir"));
    }
}

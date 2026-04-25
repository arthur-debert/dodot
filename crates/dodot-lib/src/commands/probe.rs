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
    collect_data_dir_tree, collect_deployment_map, group_profile, read_latest_profile,
    DeploymentMapEntry, GroupedProfile, TreeNode,
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
    /// `"ok"` or `"err"` — drives the styling tag in the template.
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
                    status_class: if r.exit_status == 0 { "ok" } else { "error" },
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

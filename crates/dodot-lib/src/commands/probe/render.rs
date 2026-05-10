//! Display helpers for probe output: tree flattening, path shortening,
//! byte humanisation. Pure functions used by `mod.rs` (deployment-map +
//! show-data-dir entry points) and by the shell-init formatters.

use crate::commands::probe::types::{DeploymentDisplayEntry, TreeLine};
use crate::probe::{DeploymentMapEntry, TreeNode};

pub(super) fn into_display_entry(
    e: DeploymentMapEntry,
    home: &std::path::Path,
) -> DeploymentDisplayEntry {
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

pub(super) fn display_path(p: &std::path::Path, home: &std::path::Path) -> String {
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
pub(super) fn flatten_tree(
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

pub(super) fn annotate(node: &TreeNode) -> String {
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
}

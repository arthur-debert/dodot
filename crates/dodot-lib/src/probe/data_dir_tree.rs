//! Tree walk of `<data_dir>` for `dodot probe show-data-dir`.
//!
//! The walk is bounded: directories beyond `max_depth` are summarised
//! as a single child with the entry count, so the output stays readable
//! on installs with large pack sets. Symlinks are never followed — we
//! report the link target as a string instead, which is what the user
//! debugging "where did dodot put X?" wants to see.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::fs::{DirEntry, Fs};
use crate::paths::Pather;
use crate::Result;

/// One node in the data-dir tree. Directories have children; files have
/// a size; symlinks carry the resolved target path so the renderer can
/// show `→ /…`.
#[derive(Debug, Clone, Serialize)]
pub struct TreeNode {
    /// Display name — the file/dir basename for everything except the
    /// root, which uses the full data_dir path.
    pub name: String,
    /// Absolute path to this node.
    pub path: PathBuf,
    /// `"dir"`, `"file"`, `"symlink"`, or `"truncated"`. The renderer
    /// picks an icon based on this.
    pub kind: &'static str,
    /// Size in bytes for files (via `lstat`, so symlinks report the
    /// link-entry size, not the target's).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Symlink target as a plain string (not resolved). None for
    /// non-symlinks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_target: Option<String>,
    /// For `kind == "truncated"`, the number of children not expanded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated_count: Option<usize>,
    /// Children of a directory. Empty for files/symlinks.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub children: Vec<TreeNode>,
}

/// Walk `<data_dir>` to `max_depth` levels deep and return a tree of
/// [`TreeNode`]s rooted at the data directory.
///
/// `max_depth = 0` returns just the root with no children. `max_depth = 1`
/// shows immediate children of data_dir but doesn't recurse into them,
/// and so on. A reasonable default for display is 4 (enough to show
/// `packs / <pack> / <handler> / <entry>`).
pub fn collect_data_dir_tree(
    fs: &dyn Fs,
    paths: &dyn Pather,
    max_depth: usize,
) -> Result<TreeNode> {
    let root = paths.data_dir().to_path_buf();
    walk(fs, &root, &root_name(&root), max_depth)
}

fn root_name(root: &Path) -> String {
    root.display().to_string()
}

fn walk(fs: &dyn Fs, path: &Path, display_name: &str, remaining_depth: usize) -> Result<TreeNode> {
    // The root may not exist on a fresh install. Represent it as an
    // empty directory rather than erroring — `probe show-data-dir` on a
    // brand-new system should print "empty", not fail.
    if !fs.exists(path) {
        return Ok(TreeNode {
            name: display_name.to_string(),
            path: path.to_path_buf(),
            kind: "dir",
            size: None,
            link_target: None,
            truncated_count: None,
            children: Vec::new(),
        });
    }

    // Classify via lstat so symlinks are never followed.
    let meta = fs.lstat(path)?;

    if meta.is_symlink {
        let target = fs.readlink(path).ok().map(|p| p.display().to_string());
        return Ok(TreeNode {
            name: display_name.to_string(),
            path: path.to_path_buf(),
            kind: "symlink",
            size: Some(meta.len),
            link_target: target,
            truncated_count: None,
            children: Vec::new(),
        });
    }

    if !meta.is_dir {
        return Ok(TreeNode {
            name: display_name.to_string(),
            path: path.to_path_buf(),
            kind: "file",
            size: Some(meta.len),
            link_target: None,
            truncated_count: None,
            children: Vec::new(),
        });
    }

    // Directory.
    if remaining_depth == 0 {
        // Report how many entries are hidden so the user knows the
        // subtree wasn't empty.
        let count = fs.read_dir(path).map(|v| v.len()).unwrap_or(0);
        return Ok(TreeNode {
            name: display_name.to_string(),
            path: path.to_path_buf(),
            kind: "dir",
            size: None,
            link_target: None,
            truncated_count: if count > 0 { Some(count) } else { None },
            children: Vec::new(),
        });
    }

    let mut entries = fs.read_dir(path)?;
    entries.sort_by(|a, b| {
        directory_order(a)
            .cmp(&directory_order(b))
            .then(a.name.cmp(&b.name))
    });

    let mut children = Vec::with_capacity(entries.len());
    for entry in entries {
        children.push(walk(fs, &entry.path, &entry.name, remaining_depth - 1)?);
    }

    Ok(TreeNode {
        name: display_name.to_string(),
        path: path.to_path_buf(),
        kind: "dir",
        size: None,
        link_target: None,
        truncated_count: None,
        children,
    })
}

/// Sort key: directories before files, both sorted by name afterwards.
/// Keeps the tree output visually grouped (all subdirs on top).
fn directory_order(entry: &DirEntry) -> u8 {
    if entry.is_dir {
        0
    } else {
        1
    }
}

impl TreeNode {
    /// Count nodes in the subtree rooted here (including self).
    pub fn count_nodes(&self) -> usize {
        1 + self.children.iter().map(Self::count_nodes).sum::<usize>()
    }

    /// Total file size (symlinks counted by their link-entry size).
    pub fn total_size(&self) -> u64 {
        self.size.unwrap_or(0) + self.children.iter().map(Self::total_size).sum::<u64>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempEnvironment;

    #[test]
    fn missing_data_dir_returns_empty_root() {
        let env = TempEnvironment::builder().build();
        // Remove the data dir to simulate a fresh install.
        env.fs.remove_dir_all(&env.data_dir).unwrap();

        let root = collect_data_dir_tree(env.fs.as_ref(), env.paths.as_ref(), 4).unwrap();
        assert_eq!(root.kind, "dir");
        assert!(root.children.is_empty());
    }

    #[test]
    fn depth_zero_returns_root_only_with_truncated_count() {
        let env = TempEnvironment::builder().build();
        // Create a couple of files under data_dir so there's something to truncate.
        env.fs
            .write_file(&env.data_dir.join("a.txt"), b"hi")
            .unwrap();
        env.fs
            .write_file(&env.data_dir.join("b.txt"), b"hi")
            .unwrap();

        let root = collect_data_dir_tree(env.fs.as_ref(), env.paths.as_ref(), 0).unwrap();
        assert_eq!(root.kind, "dir");
        assert!(root.children.is_empty());
        assert!(root.truncated_count.unwrap() >= 2);
    }

    #[test]
    fn files_report_size() {
        let env = TempEnvironment::builder().build();
        env.fs
            .write_file(&env.data_dir.join("hello.txt"), b"hello world")
            .unwrap();

        let root = collect_data_dir_tree(env.fs.as_ref(), env.paths.as_ref(), 1).unwrap();
        let hello = root
            .children
            .iter()
            .find(|c| c.name == "hello.txt")
            .expect("hello.txt node");
        assert_eq!(hello.kind, "file");
        assert_eq!(hello.size, Some(11));
    }

    #[test]
    fn symlinks_carry_target_and_are_not_followed() {
        let env = TempEnvironment::builder().build();
        let target = env.home.join("real.txt");
        env.fs.write_file(&target, b"xx").unwrap();
        env.fs
            .symlink(&target, &env.data_dir.join("link.txt"))
            .unwrap();

        let root = collect_data_dir_tree(env.fs.as_ref(), env.paths.as_ref(), 1).unwrap();
        let link = root
            .children
            .iter()
            .find(|c| c.name == "link.txt")
            .expect("link.txt node");
        assert_eq!(link.kind, "symlink");
        assert_eq!(link.link_target.as_deref(), Some(target.to_str().unwrap()));
    }

    #[test]
    fn directories_before_files_then_alphabetical() {
        let env = TempEnvironment::builder().build();
        env.fs.mkdir_all(&env.data_dir.join("packs")).unwrap();
        env.fs.mkdir_all(&env.data_dir.join("shell")).unwrap();
        env.fs
            .write_file(&env.data_dir.join("deployment-map.tsv"), b"x")
            .unwrap();
        env.fs
            .write_file(&env.data_dir.join("zzz.txt"), b"x")
            .unwrap();

        let root = collect_data_dir_tree(env.fs.as_ref(), env.paths.as_ref(), 1).unwrap();
        let names: Vec<&str> = root.children.iter().map(|c| c.name.as_str()).collect();
        // packs, shell (dirs, alphabetical), then deployment-map.tsv, zzz.txt (files).
        assert_eq!(
            names,
            vec!["packs", "shell", "deployment-map.tsv", "zzz.txt"]
        );
    }

    #[test]
    fn deep_tree_truncates_at_max_depth() {
        let env = TempEnvironment::builder().build();
        let deep = env.data_dir.join("packs").join("vim").join("shell");
        env.fs.mkdir_all(&deep).unwrap();
        env.fs.write_file(&deep.join("aliases.sh"), b"x").unwrap();

        // Depth 2: root -> packs -> vim (truncated, not expanded).
        let root = collect_data_dir_tree(env.fs.as_ref(), env.paths.as_ref(), 2).unwrap();
        let packs = root
            .children
            .iter()
            .find(|c| c.name == "packs")
            .expect("packs node");
        let vim = packs
            .children
            .iter()
            .find(|c| c.name == "vim")
            .expect("vim node");
        assert!(vim.children.is_empty(), "vim should be a truncation leaf");
        assert_eq!(vim.truncated_count, Some(1));
    }

    #[test]
    fn count_and_total_size_helpers_agree() {
        let env = TempEnvironment::builder().build();
        // Start from a clean data_dir so we control the exact shape.
        // (TempEnvironment pre-creates `shell/` and `packs/` — fine for
        // realism, but confuses exact-count assertions.)
        env.fs.remove_dir_all(&env.data_dir).unwrap();
        env.fs.mkdir_all(&env.data_dir).unwrap();

        env.fs.write_file(&env.data_dir.join("a"), b"hi").unwrap(); // 2 bytes
        env.fs
            .write_file(&env.data_dir.join("b"), b"hello")
            .unwrap(); // 5 bytes

        let root = collect_data_dir_tree(env.fs.as_ref(), env.paths.as_ref(), 1).unwrap();
        assert_eq!(root.count_nodes(), 3); // root + 2 children
        assert_eq!(root.total_size(), 7);
    }
}

//! The orientation inventory shown before a user approves an implicit root.
//!
//! Its job is recognition, not prediction: root-wide counts per configured
//! handler category plus at most [`MAX_INVENTORY_PATHS`] example paths across
//! the whole prompt, prioritized so shell and code-execution entries — the
//! ones that can change every future shell — are the ones the user actually
//! sees. Omitted paths are counted, never silently dropped.
//!
//! Explicitly *not* a dry run: building this reads configuration and routing
//! metadata only. It never renders templates, resolves secrets, or inspects
//! candidate pack-entry contents (Spec, "Non-Goals"), which is also why a
//! first-ever run needs no rendered baseline.
//!
//! Behaviour arrives in WS05; this Work Stream fixes the shape.

use std::path::PathBuf;

use super::error::Result;
use super::roots::ResolvedRoot;

/// The most paths the prompt may show, across all categories combined.
///
/// A cap, not a per-category quota: the point is that the confirmation
/// question stays on screen (Spec, story 3).
pub const MAX_INVENTORY_PATHS: usize = 10;

/// A configured-handler category, in prompt priority order.
///
/// The derived [`Ord`] *is* the priority: shell and code execution first
/// because they run or alter program state, then the categories that alter
/// path resolution and user-visible files. Sorting entries by
/// `(category, path)` therefore produces the prompt's order directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InventoryCategory {
    /// Shell startup files added to every future shell session.
    Shell,
    /// Install scripts, package manifests, and other provisioning inputs.
    CodeExecution,
    /// Directories added to `PATH`.
    Path,
    /// External handlers.
    External,
    /// Symlinked files.
    Link,
    /// Everything else the configured handlers recognize.
    Other,
}

impl InventoryCategory {
    /// The categories in the order the prompt lists them.
    pub const PROMPT_ORDER: [InventoryCategory; 6] = [
        InventoryCategory::Shell,
        InventoryCategory::CodeExecution,
        InventoryCategory::Path,
        InventoryCategory::External,
        InventoryCategory::Link,
        InventoryCategory::Other,
    ];

    /// Stable, user-facing name of the category.
    pub fn label(self) -> &'static str {
        match self {
            InventoryCategory::Shell => "shell",
            InventoryCategory::CodeExecution => "code execution",
            InventoryCategory::Path => "path",
            InventoryCategory::External => "external",
            InventoryCategory::Link => "link",
            InventoryCategory::Other => "other",
        }
    }
}

/// One example path shown in the prompt.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct InventoryEntry {
    /// The category this file was recognized as. First sort key, so
    /// high-impact entries survive the path cap.
    pub category: InventoryCategory,
    /// The file's path relative to the root being approved — the prompt names
    /// the root once and stays readable.
    pub relative_path: PathBuf,
}

/// How many files a category recognized, root-wide.
///
/// Counts are unaffected by the path cap: they stay visible even when their
/// example paths were omitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryCount {
    pub category: InventoryCategory,
    pub files: usize,
}

/// The bounded orientation inventory for one root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootInventory {
    /// Root-wide counts per category, in [`InventoryCategory::PROMPT_ORDER`].
    pub counts: Vec<CategoryCount>,
    /// At most [`MAX_INVENTORY_PATHS`] example paths, category-priority first.
    pub sample: Vec<InventoryEntry>,
    /// How many recognized files are not in `sample`.
    pub omitted: usize,
}

impl RootInventory {
    /// Total recognized files across all categories.
    pub fn total_files(&self) -> usize {
        self.counts.iter().map(|count| count.files).sum()
    }
}

/// Build the orientation inventory for the root about to be approved.
pub fn build_inventory(root: &ResolvedRoot) -> Result<RootInventory> {
    let _ = root;
    todo!("WS05: safety decisions and orientation inventory")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The prompt's priority is encoded in the type's ordering rather than a
    /// separate table, so a sort on `(category, path)` cannot drift from the
    /// documented order.
    #[test]
    fn category_ordering_is_the_prompt_priority() {
        let mut shuffled = vec![
            InventoryCategory::Other,
            InventoryCategory::Link,
            InventoryCategory::CodeExecution,
            InventoryCategory::External,
            InventoryCategory::Shell,
            InventoryCategory::Path,
        ];
        shuffled.sort();

        assert_eq!(shuffled, InventoryCategory::PROMPT_ORDER);
    }

    #[test]
    fn entries_sort_by_category_then_relative_path() {
        let mut entries = [
            InventoryEntry {
                category: InventoryCategory::Link,
                relative_path: PathBuf::from("vim/vimrc"),
            },
            InventoryEntry {
                category: InventoryCategory::Shell,
                relative_path: PathBuf::from("zsh/zshrc"),
            },
            InventoryEntry {
                category: InventoryCategory::Shell,
                relative_path: PathBuf::from("bash/bashrc"),
            },
        ];
        entries.sort();

        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.relative_path.to_str().unwrap())
                .collect::<Vec<_>>(),
            ["bash/bashrc", "zsh/zshrc", "vim/vimrc"]
        );
    }

    #[test]
    fn counts_stay_whole_when_paths_are_capped() {
        let inventory = RootInventory {
            counts: vec![
                CategoryCount {
                    category: InventoryCategory::Shell,
                    files: 12,
                },
                CategoryCount {
                    category: InventoryCategory::Link,
                    files: 30,
                },
            ],
            sample: Vec::new(),
            omitted: 42,
        };

        assert_eq!(inventory.total_files(), 42);
        assert!(MAX_INVENTORY_PATHS < inventory.total_files());
    }

    #[test]
    fn category_labels_are_stable() {
        assert_eq!(
            InventoryCategory::PROMPT_ORDER
                .iter()
                .map(|category| category.label())
                .collect::<Vec<_>>(),
            [
                "shell",
                "code execution",
                "path",
                "external",
                "link",
                "other"
            ]
        );
    }
}

//! The confirmation prompt's view.
//!
//! Safety Lock's gate decides ([`authorize`](crate::safety_lock::authorize))
//! and hands back an untrusted root plus the bounded orientation
//! [`RootInventory`]. Turning that into something a user reads is a
//! presentation question, so it lives here with dodot's other view DTOs
//! rather than in the domain module, which stays free of rendering.
//!
//! One flat `Serialize` shape, rendered through the `safety-prompt` template.
//! It goes to **stderr**, never stdout: a root-sensitive command asked for
//! JSON or YAML must still produce a parseable document, so the safety
//! interaction cannot share the structured channel (Spec, story 11).

use serde::Serialize;

use crate::safety_lock::{ResolvedRoot, RootInventory};

/// One configured-handler category's file count, as the prompt shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SafetyPromptCount {
    /// The category's user-facing name — `shell`, `code execution`, `path`,
    /// `external`, `link`, `other`.
    pub category: String,
    /// How many files the category recognized, root-wide. Unaffected by the
    /// path cap, so a category whose examples were omitted still shows what
    /// it found.
    pub files: usize,
}

/// The prompt shown before an implicitly discovered root may drive a
/// root-sensitive mutation.
///
/// Deliberately thin: the root, how it was selected, counts, and a bounded
/// sample. It is a recognition aid — enough to notice a wrong directory — and
/// explicitly not a deployment preview, so nothing here describes what an
/// operation would do to any of these files (Spec, "Non-Goals").
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SafetyPromptView {
    /// The canonical root being approved, in the reversible spelling `dodot
    /// roots list` prints and `dodot roots forget` accepts back. Naming it
    /// the same way everywhere is what makes the approval a user gives here
    /// the one they can later find and revoke.
    pub root: String,
    /// What selected it: `git top-level` or `current directory`. Shown
    /// because the root Dodot chose is often not the directory in the user's
    /// shell prompt (Spec, story 2).
    pub provenance: String,
    /// Per-category counts, in prompt priority order. Categories that
    /// recognized nothing are absent rather than shown as zero.
    pub counts: Vec<SafetyPromptCount>,
    /// At most ten example paths, relative to the root, shell and
    /// code-execution first.
    pub paths: Vec<String>,
    /// How many recognized files are not in `paths`.
    pub omitted: usize,
    /// Total recognized files across every category.
    pub total: usize,
}

impl SafetyPromptView {
    /// Build the prompt for `root` from the inventory the gate carried out
    /// with its answer.
    ///
    /// Takes the inventory rather than building one: an approvable outcome
    /// only exists for a root Dodot could describe, and re-deriving the
    /// description here would let the prompt drift from the decision that
    /// produced it.
    pub fn new(root: &ResolvedRoot, inventory: &RootInventory) -> Self {
        Self {
            root: root.identity().spelling(),
            provenance: root.source().label().to_string(),
            counts: inventory
                .counts
                .iter()
                .map(|count| SafetyPromptCount {
                    category: count.category.label().to_string(),
                    files: count.files,
                })
                .collect(),
            paths: inventory
                .sample
                .iter()
                .map(|entry| entry.relative_path.display().to_string())
                .collect(),
            omitted: inventory.omitted,
            total: inventory.total_files(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::safety_lock::{
        CategoryCount, InventoryCategory, InventoryEntry, RootIdentity, RootSource,
    };

    use super::*;

    fn root() -> ResolvedRoot {
        ResolvedRoot::new(RootIdentity::new("/srv/dots").unwrap(), RootSource::Git)
    }

    fn inventory() -> RootInventory {
        RootInventory {
            counts: vec![
                CategoryCount {
                    category: InventoryCategory::Shell,
                    files: 3,
                },
                CategoryCount {
                    category: InventoryCategory::Link,
                    files: 9,
                },
            ],
            sample: vec![
                InventoryEntry {
                    category: InventoryCategory::Shell,
                    relative_path: PathBuf::from("zsh/aliases.sh"),
                },
                InventoryEntry {
                    category: InventoryCategory::Link,
                    relative_path: PathBuf::from("git/gitconfig"),
                },
            ],
            omitted: 10,
        }
    }

    #[test]
    fn the_view_names_the_root_the_way_roots_list_will() {
        let view = SafetyPromptView::new(&root(), &inventory());

        assert_eq!(
            view.root,
            RootIdentity::new("/srv/dots").unwrap().spelling()
        );
        assert_eq!(view.provenance, "git top-level");
    }

    /// The prompt has to stay short enough that the question it ends with is
    /// still on screen — a wall of blank lines between the root and the
    /// `[y/N]` is the failure mode the ten-path cap exists to prevent.
    #[test]
    fn the_rendered_prompt_names_the_root_its_counts_and_its_sample() {
        let rendered = crate::render::render_safety_prompt(
            &SafetyPromptView::new(&root(), &inventory()),
            standout_render::OutputMode::Text,
        )
        .unwrap();

        for expected in [
            "/srv/dots",
            "selected by git top-level",
            "3 shell",
            "9 link",
            "zsh/aliases.sh",
            "git/gitconfig",
            "... and 10 more",
            "dodot roots forget",
        ] {
            assert!(
                rendered.contains(expected),
                "`{expected}` missing from:\n{rendered}"
            );
        }
        assert!(
            !rendered.contains("\n\n\n"),
            "the prompt padded itself with blank lines:\n{rendered:?}"
        );
    }

    /// A root with nothing recognizable is still worth approving or refusing
    /// — and saying so is more useful than a header over an empty list.
    #[test]
    fn an_empty_root_says_so_instead_of_rendering_an_empty_inventory() {
        let empty = RootInventory {
            counts: Vec::new(),
            sample: Vec::new(),
            omitted: 0,
        };

        let rendered = crate::render::render_safety_prompt(
            &SafetyPromptView::new(&root(), &empty),
            standout_render::OutputMode::Text,
        )
        .unwrap();

        assert!(rendered.contains("recognizes no files"), "{rendered}");
        assert!(
            !rendered.contains("What dodot recognizes there"),
            "{rendered}"
        );
    }

    #[test]
    fn counts_survive_the_path_cap_that_omitted_their_examples() {
        let view = SafetyPromptView::new(&root(), &inventory());

        assert_eq!(view.total, 12);
        assert_eq!(view.paths.len(), 2);
        assert_eq!(view.omitted, 10);
        assert_eq!(
            view.counts,
            vec![
                SafetyPromptCount {
                    category: "shell".into(),
                    files: 3,
                },
                SafetyPromptCount {
                    category: "link".into(),
                    files: 9,
                },
            ]
        );
    }
}

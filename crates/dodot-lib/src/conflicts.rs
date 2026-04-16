//! Cross-pack conflict detection.
//!
//! Detects when multiple packs resolve to the same user-visible target
//! path. Conflicts are checked **after** all intents are collected and
//! target paths are fully resolved — this catches collisions introduced
//! by `[symlink.targets]`, `force_home`, `_home/` prefixes, etc.
//!
//! Only **Link–Link** collisions are detected: two packs produce
//! `HandlerIntent::Link` with the same resolved `user_path`. This is
//! a real filesystem collision where the last `up` silently overwrites
//! the previous pack's symlink.
//!
//! Stage intents (shell/path handlers) are *not* flagged because they
//! are stored in per-pack namespaced directories in the datastore —
//! no filesystem collision occurs. Multiple packs having `aliases.sh`
//! is a legitimate and common pattern.

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

use crate::operations::HandlerIntent;

/// One pack's claim on a target path.
#[derive(Debug, Clone)]
pub struct Claimant {
    pub pack: String,
    pub handler: String,
    pub source: PathBuf,
}

/// A cross-pack conflict: multiple packs claim the same effective target.
#[derive(Debug, Clone)]
pub struct Conflict {
    /// The resolved target path (filesystem path for Link intents,
    /// descriptive label for Stage intents).
    pub target: PathBuf,
    /// Every pack that claims this target.
    pub claimants: Vec<Claimant>,
}

impl fmt::Display for Conflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "  target: {}", self.target.display())?;
        for c in &self.claimants {
            write!(
                f,
                "\n    - pack '{}' ({} handler): {}",
                c.pack,
                c.handler,
                c.source.display()
            )?;
        }
        Ok(())
    }
}

/// Format a list of conflicts for error display.
pub fn format_conflicts(conflicts: &[Conflict]) -> String {
    conflicts
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The effective target for conflict grouping.
///
/// Only `Link` intents produce a target (the resolved `user_path`).
/// `Stage` and `Run` intents don't create user-visible filesystem
/// entries that could collide — they're stored in per-pack namespaced
/// directories in the datastore.
fn effective_target(intent: &HandlerIntent) -> Option<PathBuf> {
    match intent {
        HandlerIntent::Link { user_path, .. } => Some(user_path.clone()),
        HandlerIntent::Stage { .. } | HandlerIntent::Run { .. } => None,
    }
}

/// Detect cross-pack conflicts across all collected intents.
///
/// `pack_intents` is a slice of `(pack_name, intents)` pairs, one per
/// pack. Returns a (possibly empty) list of conflicts where multiple
/// **different** packs claim the same target.
pub fn detect_cross_pack_conflicts(pack_intents: &[(String, Vec<HandlerIntent>)]) -> Vec<Conflict> {
    let mut targets: HashMap<PathBuf, Vec<Claimant>> = HashMap::new();

    for (pack_name, intents) in pack_intents {
        for intent in intents {
            if let Some(target) = effective_target(intent) {
                targets.entry(target).or_default().push(Claimant {
                    pack: pack_name.clone(),
                    handler: intent.handler().to_string(),
                    source: intent_source(intent),
                });
            }
        }
    }

    let mut conflicts: Vec<Conflict> = targets
        .into_iter()
        .filter(|(_, claimants)| {
            // Only flag when at least two *different* packs claim the target.
            // Same-pack "conflicts" are out of scope (a single pack can't
            // have two files with the same name).
            let first = &claimants[0].pack;
            claimants.len() > 1 && claimants.iter().any(|c| c.pack != *first)
        })
        .map(|(target, claimants)| Conflict { target, claimants })
        .collect();

    // Sort for deterministic output
    conflicts.sort_by(|a, b| a.target.cmp(&b.target));
    conflicts
}

fn intent_source(intent: &HandlerIntent) -> PathBuf {
    match intent {
        HandlerIntent::Link { source, .. } => source.clone(),
        HandlerIntent::Stage { source, .. } => source.clone(),
        HandlerIntent::Run { executable, .. } => PathBuf::from(executable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(pack: &str, source: &str, user_path: &str) -> HandlerIntent {
        HandlerIntent::Link {
            pack: pack.into(),
            handler: "symlink".into(),
            source: PathBuf::from(source),
            user_path: PathBuf::from(user_path),
        }
    }

    fn stage(pack: &str, handler: &str, source: &str) -> HandlerIntent {
        HandlerIntent::Stage {
            pack: pack.into(),
            handler: handler.into(),
            source: PathBuf::from(source),
        }
    }

    // ── No conflicts ───────────────────────────────────────────

    #[test]
    fn no_conflicts_when_different_targets() {
        let pack_intents = vec![
            (
                "vim".into(),
                vec![link("vim", "/dot/vim/vimrc", "/home/.vimrc")],
            ),
            (
                "git".into(),
                vec![link("git", "/dot/git/gitconfig", "/home/.gitconfig")],
            ),
        ];
        let conflicts = detect_cross_pack_conflicts(&pack_intents);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn no_conflicts_when_single_pack() {
        let pack_intents = vec![(
            "vim".into(),
            vec![
                link("vim", "/dot/vim/vimrc", "/home/.vimrc"),
                link("vim", "/dot/vim/gvimrc", "/home/.gvimrc"),
            ],
        )];
        let conflicts = detect_cross_pack_conflicts(&pack_intents);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn no_conflicts_when_empty() {
        let conflicts = detect_cross_pack_conflicts(&[]);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn no_conflicts_for_run_intents() {
        let pack_intents = vec![
            (
                "a".into(),
                vec![HandlerIntent::Run {
                    pack: "a".into(),
                    handler: "install".into(),
                    executable: "echo".into(),
                    arguments: vec!["hi".into()],
                    sentinel: "s1".into(),
                }],
            ),
            (
                "b".into(),
                vec![HandlerIntent::Run {
                    pack: "b".into(),
                    handler: "install".into(),
                    executable: "echo".into(),
                    arguments: vec!["hi".into()],
                    sentinel: "s1".into(),
                }],
            ),
        ];
        let conflicts = detect_cross_pack_conflicts(&pack_intents);
        assert!(conflicts.is_empty());
    }

    // ── Link conflicts ─────────────────────────────────────────

    #[test]
    fn detects_link_link_conflict() {
        let pack_intents = vec![
            (
                "pack-a".into(),
                vec![link("pack-a", "/dot/pack-a/aliases", "/home/.aliases")],
            ),
            (
                "pack-b".into(),
                vec![link("pack-b", "/dot/pack-b/aliases", "/home/.aliases")],
            ),
        ];
        let conflicts = detect_cross_pack_conflicts(&pack_intents);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].target, PathBuf::from("/home/.aliases"));
        assert_eq!(conflicts[0].claimants.len(), 2);

        let packs: Vec<&str> = conflicts[0]
            .claimants
            .iter()
            .map(|c| c.pack.as_str())
            .collect();
        assert!(packs.contains(&"pack-a"));
        assert!(packs.contains(&"pack-b"));
    }

    #[test]
    fn detects_multiple_conflicts() {
        let pack_intents = vec![
            (
                "a".into(),
                vec![
                    link("a", "/dot/a/f1", "/home/.f1"),
                    link("a", "/dot/a/f2", "/home/.f2"),
                ],
            ),
            (
                "b".into(),
                vec![
                    link("b", "/dot/b/f1", "/home/.f1"),
                    link("b", "/dot/b/f2", "/home/.f2"),
                ],
            ),
        ];
        let conflicts = detect_cross_pack_conflicts(&pack_intents);
        assert_eq!(conflicts.len(), 2);
    }

    #[test]
    fn three_packs_one_conflict() {
        let pack_intents = vec![
            (
                "a".into(),
                vec![link("a", "/dot/a/conf", "/home/.config/app/conf")],
            ),
            (
                "b".into(),
                vec![link("b", "/dot/b/conf", "/home/.config/app/conf")],
            ),
            (
                "c".into(),
                vec![link("c", "/dot/c/conf", "/home/.config/app/conf")],
            ),
        ];
        let conflicts = detect_cross_pack_conflicts(&pack_intents);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].claimants.len(), 3);
    }

    // ── Stage intents are NOT conflicts ──────────────────────────
    //
    // Shell/path handlers stage files into per-pack namespaced
    // datastore directories — no filesystem collision occurs.

    #[test]
    fn same_name_shell_scripts_are_not_conflicts() {
        let pack_intents = vec![
            (
                "vim".into(),
                vec![stage("vim", "shell", "/dot/vim/aliases.sh")],
            ),
            (
                "git".into(),
                vec![stage("git", "shell", "/dot/git/aliases.sh")],
            ),
        ];
        let conflicts = detect_cross_pack_conflicts(&pack_intents);
        assert!(
            conflicts.is_empty(),
            "same-name shell scripts in different packs are legitimate"
        );
    }

    #[test]
    fn same_name_path_dirs_are_not_conflicts() {
        let pack_intents = vec![
            ("a".into(), vec![stage("a", "path", "/dot/a/bin")]),
            ("b".into(), vec![stage("b", "path", "/dot/b/bin")]),
        ];
        let conflicts = detect_cross_pack_conflicts(&pack_intents);
        assert!(
            conflicts.is_empty(),
            "same-name path dirs in different packs are legitimate"
        );
    }

    #[test]
    fn stage_intents_do_not_conflict_with_link_intents() {
        let pack_intents = vec![
            ("a".into(), vec![link("a", "/dot/a/tool", "/home/bin/tool")]),
            ("b".into(), vec![stage("b", "path", "/dot/b/tool")]),
        ];
        let conflicts = detect_cross_pack_conflicts(&pack_intents);
        assert!(conflicts.is_empty());
    }

    // ── Display ────────────────────────────────────────────────

    #[test]
    fn conflict_display_includes_all_info() {
        let conflict = Conflict {
            target: PathBuf::from("/home/.aliases"),
            claimants: vec![
                Claimant {
                    pack: "pack-a".into(),
                    handler: "symlink".into(),
                    source: PathBuf::from("/dot/pack-a/aliases"),
                },
                Claimant {
                    pack: "pack-b".into(),
                    handler: "symlink".into(),
                    source: PathBuf::from("/dot/pack-b/aliases"),
                },
            ],
        };
        let display = conflict.to_string();
        assert!(display.contains("/home/.aliases"));
        assert!(display.contains("pack-a"));
        assert!(display.contains("pack-b"));
        assert!(display.contains("symlink"));
    }

    #[test]
    fn format_conflicts_combines_multiple() {
        let conflicts = vec![
            Conflict {
                target: PathBuf::from("/home/.a"),
                claimants: vec![
                    Claimant {
                        pack: "x".into(),
                        handler: "symlink".into(),
                        source: PathBuf::from("/dot/x/a"),
                    },
                    Claimant {
                        pack: "y".into(),
                        handler: "symlink".into(),
                        source: PathBuf::from("/dot/y/a"),
                    },
                ],
            },
            Conflict {
                target: PathBuf::from("/home/.b"),
                claimants: vec![
                    Claimant {
                        pack: "x".into(),
                        handler: "symlink".into(),
                        source: PathBuf::from("/dot/x/b"),
                    },
                    Claimant {
                        pack: "y".into(),
                        handler: "symlink".into(),
                        source: PathBuf::from("/dot/y/b"),
                    },
                ],
            },
        ];
        let formatted = format_conflicts(&conflicts);
        assert!(formatted.contains("/home/.a"));
        assert!(formatted.contains("/home/.b"));
    }
}

//! Cross-pack conflict detection.
//!
//! Detects when multiple packs resolve to the same user-visible target
//! path. Conflicts are checked **after** all intents are collected and
//! target paths are fully resolved — this catches collisions introduced
//! by `[symlink.targets]`, `force_home`, `_home/` prefixes, etc.
//!
//! Two kinds of collision are detected:
//!
//! 1. **Symlink target collisions**: two packs produce
//!    `HandlerIntent::Link` with the same resolved `user_path`.
//! 2. **PATH executable shadowing**: two packs stage directories via the
//!    path handler that contain files with the same name — only the
//!    first one in PATH order would be found by the shell.
//!
//! Shell handler Stage intents are *not* flagged because each pack's
//! scripts are sourced independently from per-pack namespaced
//! directories — multiple packs having `aliases.sh` is legitimate.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::fs::Fs;
use crate::handlers::HANDLER_PATH;
use crate::operations::HandlerIntent;

/// One pack's claim on a target path.
#[derive(Debug, Clone)]
pub struct Claimant {
    pub pack: String,
    pub handler: String,
    pub source: PathBuf,
}

/// What kind of collision this conflict represents.
///
/// The two kinds have different display semantics: symlink conflicts
/// have a filesystem target path, while path-executable conflicts have
/// a bare executable name whose location is "somewhere in $PATH".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictKind {
    /// Multiple packs resolve to the same user symlink target.
    SymlinkTarget,
    /// Multiple packs stage a `$PATH` directory that contains files
    /// with the same name — only the first in PATH order would be used.
    PathExecutable,
}

/// A cross-pack conflict: multiple packs claim the same effective target.
#[derive(Debug, Clone)]
pub struct Conflict {
    /// The kind of collision.
    pub kind: ConflictKind,
    /// For [`ConflictKind::SymlinkTarget`]: the resolved filesystem path.
    /// For [`ConflictKind::PathExecutable`]: a sentinel path
    /// `<path-executable>/<name>` — read `.file_name()` for the bare name.
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

/// Detect cross-pack conflicts across all collected intents.
///
/// `pack_intents` is a slice of `(pack_name, intents)` pairs, one per
/// pack. Returns a (possibly empty) list of conflicts where multiple
/// **different** packs claim the same target.
///
/// `fs` is needed to list the contents of path-handler directories
/// for executable name collision detection.
pub fn detect_cross_pack_conflicts(
    pack_intents: &[(String, Vec<HandlerIntent>)],
    fs: &dyn Fs,
) -> Vec<Conflict> {
    let mut targets: HashMap<PathBuf, Vec<Claimant>> = HashMap::new();

    let mut kinds: HashMap<PathBuf, ConflictKind> = HashMap::new();

    for (pack_name, intents) in pack_intents {
        for intent in intents {
            // Symlink target conflicts
            if let HandlerIntent::Link { user_path, .. } = intent {
                kinds.insert(user_path.clone(), ConflictKind::SymlinkTarget);
                targets
                    .entry(user_path.clone())
                    .or_default()
                    .push(Claimant {
                        pack: pack_name.clone(),
                        handler: intent.handler().to_string(),
                        source: intent_source(intent),
                    });
            }

            // PATH executable shadowing: list files inside staged directories
            if let HandlerIntent::Stage {
                handler, source, ..
            } = intent
            {
                if handler == HANDLER_PATH {
                    if let Ok(entries) = fs.read_dir(source) {
                        for entry in entries {
                            if entry.is_file || entry.is_symlink {
                                let key = Path::new("<path-executable>").join(&entry.name);
                                kinds.insert(key.clone(), ConflictKind::PathExecutable);
                                targets.entry(key).or_default().push(Claimant {
                                    pack: pack_name.clone(),
                                    handler: handler.clone(),
                                    source: entry.path.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    let mut conflicts: Vec<Conflict> = targets
        .into_iter()
        .filter(|(_, claimants)| {
            // Only flag when at least two *different* packs claim the target.
            let first = &claimants[0].pack;
            claimants.len() > 1 && claimants.iter().any(|c| c.pack != *first)
        })
        .map(|(target, claimants)| {
            let kind = kinds
                .get(&target)
                .copied()
                .unwrap_or(ConflictKind::SymlinkTarget);
            Conflict {
                kind,
                target,
                claimants,
            }
        })
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
    use crate::testing::TempEnvironment;

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

    /// Helper: create a mock Fs for tests that don't need real filesystem.
    fn dummy_fs() -> std::sync::Arc<crate::fs::OsFs> {
        std::sync::Arc::new(crate::fs::OsFs::new())
    }

    // ── No conflicts ───────────────────────────────────────────

    #[test]
    fn no_conflicts_when_different_targets() {
        let fs = dummy_fs();
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
        let conflicts = detect_cross_pack_conflicts(&pack_intents, fs.as_ref());
        assert!(conflicts.is_empty());
    }

    #[test]
    fn no_conflicts_when_single_pack() {
        let fs = dummy_fs();
        let pack_intents = vec![(
            "vim".into(),
            vec![
                link("vim", "/dot/vim/vimrc", "/home/.vimrc"),
                link("vim", "/dot/vim/gvimrc", "/home/.gvimrc"),
            ],
        )];
        let conflicts = detect_cross_pack_conflicts(&pack_intents, fs.as_ref());
        assert!(conflicts.is_empty());
    }

    #[test]
    fn no_conflicts_when_empty() {
        let fs = dummy_fs();
        let conflicts = detect_cross_pack_conflicts(&[], fs.as_ref());
        assert!(conflicts.is_empty());
    }

    #[test]
    fn no_conflicts_for_run_intents() {
        let fs = dummy_fs();
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
        let conflicts = detect_cross_pack_conflicts(&pack_intents, fs.as_ref());
        assert!(conflicts.is_empty());
    }

    // ── Link conflicts ─────────────────────────────────────────

    #[test]
    fn detects_link_link_conflict() {
        let fs = dummy_fs();
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
        let conflicts = detect_cross_pack_conflicts(&pack_intents, fs.as_ref());
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
        let fs = dummy_fs();
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
        let conflicts = detect_cross_pack_conflicts(&pack_intents, fs.as_ref());
        assert_eq!(conflicts.len(), 2);
    }

    #[test]
    fn three_packs_one_conflict() {
        let fs = dummy_fs();
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
        let conflicts = detect_cross_pack_conflicts(&pack_intents, fs.as_ref());
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].claimants.len(), 3);
    }

    // ── Stage intents ──────────────────────────────────────────

    #[test]
    fn same_name_shell_scripts_are_not_conflicts() {
        let fs = dummy_fs();
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
        let conflicts = detect_cross_pack_conflicts(&pack_intents, fs.as_ref());
        assert!(
            conflicts.is_empty(),
            "same-name shell scripts in different packs are legitimate"
        );
    }

    #[test]
    fn stage_intents_do_not_conflict_with_link_intents() {
        let fs = dummy_fs();
        let pack_intents = vec![
            ("a".into(), vec![link("a", "/dot/a/tool", "/home/bin/tool")]),
            ("b".into(), vec![stage("b", "path", "/nonexistent/dir")]),
        ];
        let conflicts = detect_cross_pack_conflicts(&pack_intents, fs.as_ref());
        assert!(conflicts.is_empty());
    }

    // ── PATH executable shadowing ──────────────────────────────

    #[test]
    fn detects_path_executable_shadowing() {
        // Two packs both have bin/ directories containing a file named `tool`
        let env = TempEnvironment::builder()
            .pack("tools-a")
            .file("bin/tool", "#!/bin/sh\necho a")
            .done()
            .pack("tools-b")
            .file("bin/tool", "#!/bin/sh\necho b")
            .done()
            .build();

        let pack_intents = vec![
            (
                "tools-a".into(),
                vec![stage(
                    "tools-a",
                    "path",
                    &env.dotfiles_root.join("tools-a/bin").to_string_lossy(),
                )],
            ),
            (
                "tools-b".into(),
                vec![stage(
                    "tools-b",
                    "path",
                    &env.dotfiles_root.join("tools-b/bin").to_string_lossy(),
                )],
            ),
        ];
        let conflicts = detect_cross_pack_conflicts(&pack_intents, env.fs.as_ref());
        assert_eq!(conflicts.len(), 1, "should detect shadowed executable");

        let c = &conflicts[0];
        assert!(
            c.target.to_string_lossy().contains("tool"),
            "target should mention the executable name: {}",
            c.target.display()
        );
        assert_eq!(c.claimants.len(), 2);

        let packs: Vec<&str> = c.claimants.iter().map(|cl| cl.pack.as_str()).collect();
        assert!(packs.contains(&"tools-a"));
        assert!(packs.contains(&"tools-b"));
    }

    #[test]
    fn no_path_conflict_when_different_executables() {
        // Two packs with bin/ directories but different file names — no conflict
        let env = TempEnvironment::builder()
            .pack("tools-a")
            .file("bin/tool-a", "#!/bin/sh")
            .done()
            .pack("tools-b")
            .file("bin/tool-b", "#!/bin/sh")
            .done()
            .build();

        let pack_intents = vec![
            (
                "tools-a".into(),
                vec![stage(
                    "tools-a",
                    "path",
                    &env.dotfiles_root.join("tools-a/bin").to_string_lossy(),
                )],
            ),
            (
                "tools-b".into(),
                vec![stage(
                    "tools-b",
                    "path",
                    &env.dotfiles_root.join("tools-b/bin").to_string_lossy(),
                )],
            ),
        ];
        let conflicts = detect_cross_pack_conflicts(&pack_intents, env.fs.as_ref());
        assert!(conflicts.is_empty());
    }

    #[test]
    fn path_executable_conflict_shows_source_files() {
        let env = TempEnvironment::builder()
            .pack("a")
            .file("bin/deploy", "#!/bin/sh\necho a")
            .done()
            .pack("b")
            .file("bin/deploy", "#!/bin/sh\necho b")
            .done()
            .build();

        let pack_intents = vec![
            (
                "a".into(),
                vec![stage(
                    "a",
                    "path",
                    &env.dotfiles_root.join("a/bin").to_string_lossy(),
                )],
            ),
            (
                "b".into(),
                vec![stage(
                    "b",
                    "path",
                    &env.dotfiles_root.join("b/bin").to_string_lossy(),
                )],
            ),
        ];
        let conflicts = detect_cross_pack_conflicts(&pack_intents, env.fs.as_ref());
        assert_eq!(conflicts.len(), 1);

        // Claimant sources should point to the actual files, not the directories
        for claimant in &conflicts[0].claimants {
            assert!(
                claimant.source.to_string_lossy().contains("deploy"),
                "source should be the file, not the directory: {}",
                claimant.source.display()
            );
        }
    }

    #[test]
    fn same_pack_path_executables_are_not_conflicts() {
        // A single pack can't conflict with itself
        let env = TempEnvironment::builder()
            .pack("tools")
            .file("bin/tool", "#!/bin/sh")
            .done()
            .build();

        let pack_intents = vec![(
            "tools".into(),
            vec![stage(
                "tools",
                "path",
                &env.dotfiles_root.join("tools/bin").to_string_lossy(),
            )],
        )];
        let conflicts = detect_cross_pack_conflicts(&pack_intents, env.fs.as_ref());
        assert!(conflicts.is_empty());
    }

    #[test]
    fn detects_path_shadowing_via_symlinks() {
        // bin/ entries that are symlinks (e.g. bin/tool -> ../libexec/tool) must
        // also be detected as potential shadowing conflicts.
        let env = TempEnvironment::builder()
            .pack("tools-a")
            .file("libexec/tool", "#!/bin/sh\necho a")
            .done()
            .pack("tools-b")
            .file("libexec/tool", "#!/bin/sh\necho b")
            .done()
            .build();

        // Create bin/ directories and symlinks inside them
        let bin_a = env.dotfiles_root.join("tools-a/bin");
        let bin_b = env.dotfiles_root.join("tools-b/bin");
        env.fs.mkdir_all(&bin_a).unwrap();
        env.fs.mkdir_all(&bin_b).unwrap();
        env.fs
            .symlink(
                &env.dotfiles_root.join("tools-a/libexec/tool"),
                &bin_a.join("tool"),
            )
            .unwrap();
        env.fs
            .symlink(
                &env.dotfiles_root.join("tools-b/libexec/tool"),
                &bin_b.join("tool"),
            )
            .unwrap();

        let pack_intents = vec![
            (
                "tools-a".into(),
                vec![stage("tools-a", "path", &bin_a.to_string_lossy())],
            ),
            (
                "tools-b".into(),
                vec![stage("tools-b", "path", &bin_b.to_string_lossy())],
            ),
        ];
        let conflicts = detect_cross_pack_conflicts(&pack_intents, env.fs.as_ref());
        assert_eq!(
            conflicts.len(),
            1,
            "symlink executables with the same name should be detected as shadowing"
        );
        let packs: Vec<&str> = conflicts[0]
            .claimants
            .iter()
            .map(|c| c.pack.as_str())
            .collect();
        assert!(packs.contains(&"tools-a"));
        assert!(packs.contains(&"tools-b"));
    }

    // ── Display ────────────────────────────────────────────────

    #[test]
    fn conflict_display_includes_all_info() {
        let conflict = Conflict {
            kind: ConflictKind::SymlinkTarget,
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
                kind: ConflictKind::SymlinkTarget,
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
                kind: ConflictKind::SymlinkTarget,
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

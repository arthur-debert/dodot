//! `Link` intent: deploy a source file by symlink, via the
//! datastore-mediated double-link (source → datastore → user_path).
//!
//! Owns ancestor-cycle detection (refuse to write through a symlink
//! that resolves back into the dodot store), conflict handling (the
//! `--force` / content-equivalence escape hatch), and the dry-run
//! simulation.

use std::path::{Path, PathBuf};

use tracing::{debug, info};

use crate::operations::{HandlerIntent, Operation, OperationResult};
use crate::Result;

use super::Executor;

impl<'a> Executor<'a> {
    pub(super) fn execute_link(&self, intent: &HandlerIntent) -> Result<Vec<OperationResult>> {
        let HandlerIntent::Link {
            pack,
            handler,
            source,
            user_path,
        } = intent
        else {
            unreachable!("execute_link called with non-Link intent");
        };

        debug!(
            pack,
            handler,
            source = %source.display(),
            user_path = %user_path.display(),
            "executing link intent"
        );

        // Refuse to deploy when an ancestor of user_path is a symlink
        // that resolves back into the pack store or dodot data dir.
        // Writing through such an ancestor lands back inside the pack
        // (clobbering source files) or creates a pack↔data-dir cycle.
        if let Some((ancestor, target)) = self.ancestor_cycles_into_store(user_path) {
            let op = Operation::CreateUserLink {
                pack: pack.clone(),
                handler: handler.clone(),
                datastore_path: Default::default(),
                user_path: user_path.clone(),
            };
            return Ok(vec![OperationResult::fail(
                op,
                cycle_message(user_path, &ancestor, &target),
            )]);
        }

        // Pre-check: does a non-symlink file exist at user_path?
        // We check BEFORE creating the data link to avoid leaving
        // dangling state when the user link would fail.
        //
        // #44: if the existing file's content is byte-identical to
        // the source we'd deploy, treat it as safe to replace —
        // the content reaching `user_path` doesn't change, only
        // the storage representation does. No `--force` required.
        if !self.fs.is_symlink(user_path) && self.fs.exists(user_path) {
            let content_equivalent = crate::equivalence::is_equivalent(user_path, source, self.fs);
            if self.force || content_equivalent {
                if content_equivalent {
                    info!(
                        pack,
                        path = %user_path.display(),
                        "auto-replacing content-equivalent file with dodot symlink"
                    );
                } else {
                    info!(
                        pack,
                        path = %user_path.display(),
                        "force-removing existing file"
                    );
                }
                // Remove the existing path before creating the symlink
                if self.fs.is_dir(user_path) {
                    self.fs.remove_dir_all(user_path)?;
                } else {
                    self.fs.remove_file(user_path)?;
                }
            } else {
                info!(
                    pack,
                    path = %user_path.display(),
                    "conflict: file already exists"
                );
                // Return a failed result — non-fatal so other files
                // in the pack can still be processed.
                let op = Operation::CreateUserLink {
                    pack: pack.clone(),
                    handler: handler.clone(),
                    datastore_path: Default::default(),
                    user_path: user_path.clone(),
                };
                return Ok(vec![OperationResult::fail(
                    op,
                    format!(
                        "conflict: {} already exists (use --force to overwrite)",
                        user_path.display()
                    ),
                )]);
            }
        }

        // Step 1: Create data link (source → datastore)
        let datastore_path = self.datastore.create_data_link(pack, handler, source)?;
        debug!(
            pack,
            datastore_path = %datastore_path.display(),
            "created data link"
        );

        // Step 2: Create user link (datastore → user location)
        self.datastore
            .create_user_link(&datastore_path, user_path)?;

        let filename = source.file_name().unwrap_or_default().to_string_lossy();
        info!(
            pack,
            file = %filename,
            target = %user_path.display(),
            "created symlink"
        );

        let op = Operation::CreateUserLink {
            pack: pack.clone(),
            handler: handler.clone(),
            datastore_path: datastore_path.clone(),
            user_path: user_path.clone(),
        };

        Ok(vec![OperationResult::ok(
            op,
            format!("{} → {}", filename, user_path.display()),
        )])
    }

    pub(super) fn simulate_link(&self, intent: &HandlerIntent) -> Vec<OperationResult> {
        let HandlerIntent::Link {
            pack,
            handler,
            source,
            user_path,
        } = intent
        else {
            unreachable!("simulate_link called with non-Link intent");
        };

        // Surface ancestor-into-pack-store cycles in dry-run too so
        // the user sees the problem before committing.
        if let Some((ancestor, target)) = self.ancestor_cycles_into_store(user_path) {
            return vec![OperationResult::fail(
                Operation::CreateUserLink {
                    pack: pack.clone(),
                    handler: handler.clone(),
                    datastore_path: Default::default(),
                    user_path: user_path.clone(),
                },
                cycle_message(user_path, &ancestor, &target),
            )];
        }

        // Check for conflicts even in dry-run
        if !self.fs.is_symlink(user_path) && self.fs.exists(user_path) {
            if self.force {
                return vec![OperationResult::ok(
                    Operation::CreateUserLink {
                        pack: pack.clone(),
                        handler: handler.clone(),
                        datastore_path: Default::default(),
                        user_path: user_path.clone(),
                    },
                    format!(
                        "[dry-run] would overwrite {} → {}",
                        source.file_name().unwrap_or_default().to_string_lossy(),
                        user_path.display()
                    ),
                )];
            } else {
                return vec![OperationResult::fail(
                    Operation::CreateUserLink {
                        pack: pack.clone(),
                        handler: handler.clone(),
                        datastore_path: Default::default(),
                        user_path: user_path.clone(),
                    },
                    format!(
                        "conflict: {} already exists (use --force to overwrite)",
                        user_path.display()
                    ),
                )];
            }
        }

        vec![OperationResult::ok(
            Operation::CreateUserLink {
                pack: pack.clone(),
                handler: handler.clone(),
                datastore_path: Default::default(),
                user_path: user_path.clone(),
            },
            format!(
                "[dry-run] would link {} → {}",
                source.file_name().unwrap_or_default().to_string_lossy(),
                user_path.display()
            ),
        )]
    }

    /// Walk `user_path`'s ancestors. If any is a symlink whose single-hop
    /// resolved target lives under `dotfiles_root` or `data_dir`, return
    /// `(ancestor, resolved_target)`. Writing through such an ancestor
    /// lands back inside the store and is always wrong — either it
    /// clobbers a pack source or builds a pack↔data-dir cycle.
    ///
    /// The resolved target is lexically normalized before the prefix
    /// comparison: relative symlinks like `~/.config/warp -> ../dotfiles/warp`
    /// produce a joined path with `..` segments that would not naively
    /// `starts_with(dotfiles_root)`.
    fn ancestor_cycles_into_store(&self, user_path: &Path) -> Option<(PathBuf, PathBuf)> {
        let dotfiles_root = self.paths.dotfiles_root();
        let data_dir = self.paths.data_dir();
        let mut current = user_path.parent()?;
        loop {
            if self.fs.is_symlink(current) {
                if let Ok(raw_target) = self.fs.readlink(current) {
                    let resolved = crate::equivalence::normalize_path(
                        &crate::equivalence::resolve_symlink_target(current, &raw_target),
                    );
                    if resolved.starts_with(dotfiles_root) || resolved.starts_with(data_dir) {
                        return Some((current.to_path_buf(), resolved));
                    }
                }
            }
            match current.parent() {
                Some(p) if p != current => current = p,
                _ => return None,
            }
        }
    }
}

fn cycle_message(user_path: &Path, ancestor: &Path, target: &Path) -> String {
    format!(
        "cycle: {} is a symlink into the dodot store (-> {}); \
         deploying {} through it would write back into the store. \
         Remove or move {} and re-run.",
        ancestor.display(),
        target.display(),
        user_path.display(),
        ancestor.display(),
    )
}

#[cfg(test)]
mod tests {
    use super::super::test_support::make_datastore;
    use super::super::Executor;
    use crate::fs::Fs;
    use crate::operations::HandlerIntent;
    use crate::testing::TempEnvironment;
    use std::path::Path;

    #[test]
    fn execute_link_creates_double_link() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .done()
            .build();
        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        );

        let source = env.dotfiles_root.join("vim/vimrc");
        let user_path = env.home.join(".vimrc");

        let results = executor
            .execute(vec![HandlerIntent::Link {
                pack: "vim".into(),
                handler: "symlink".into(),
                source: source.clone(),
                user_path: user_path.clone(),
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].success);

        // Verify the double-link chain
        env.assert_double_link("vim", "symlink", "vimrc", &source, &user_path);
    }

    #[test]
    fn execute_link_conflict_returns_failed_result() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .done()
            .home_file(".vimrc", "existing content")
            .build();
        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        );

        let source = env.dotfiles_root.join("vim/vimrc");
        let user_path = env.home.join(".vimrc");

        let results = executor
            .execute(vec![HandlerIntent::Link {
                pack: "vim".into(),
                handler: "symlink".into(),
                source: source.clone(),
                user_path: user_path.clone(),
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(!results[0].success, "should report conflict");
        assert!(
            results[0].message.contains("conflict"),
            "msg: {}",
            results[0].message
        );
        assert!(
            results[0].message.contains("--force"),
            "msg: {}",
            results[0].message
        );

        // Data link should NOT have been created (pre-check prevents it)
        env.assert_no_handler_state("vim", "symlink");

        // Original file should be untouched
        env.assert_file_contents(&user_path, "existing content");
    }

    #[test]
    fn execute_link_force_overwrites_existing_file() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .done()
            .home_file(".vimrc", "existing content")
            .build();
        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            true,
            false,
            true,
        );

        let source = env.dotfiles_root.join("vim/vimrc");
        let user_path = env.home.join(".vimrc");

        let results = executor
            .execute(vec![HandlerIntent::Link {
                pack: "vim".into(),
                handler: "symlink".into(),
                source: source.clone(),
                user_path: user_path.clone(),
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].success, "force should succeed");

        // Verify the double-link chain was created
        env.assert_double_link("vim", "symlink", "vimrc", &source, &user_path);

        // Content should now be from the pack
        let content = env.fs.read_to_string(&user_path).unwrap();
        assert_eq!(content, "set nocompatible");
    }

    #[test]
    fn execute_link_conflict_does_not_block_other_intents() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "set nocompatible")
            .file("gvimrc", "set guifont=Mono")
            .done()
            .home_file(".vimrc", "existing content")
            .build();
        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        );

        let results = executor
            .execute(vec![
                HandlerIntent::Link {
                    pack: "vim".into(),
                    handler: "symlink".into(),
                    source: env.dotfiles_root.join("vim/vimrc"),
                    user_path: env.home.join(".vimrc"),
                },
                HandlerIntent::Link {
                    pack: "vim".into(),
                    handler: "symlink".into(),
                    source: env.dotfiles_root.join("vim/gvimrc"),
                    user_path: env.home.join(".gvimrc"),
                },
            ])
            .unwrap();

        assert_eq!(results.len(), 2);
        // First should fail (conflict)
        assert!(!results[0].success);
        // Second should succeed (no conflict)
        assert!(results[1].success);

        // gvimrc should be deployed despite vimrc conflict
        env.assert_double_link(
            "vim",
            "symlink",
            "gvimrc",
            &env.dotfiles_root.join("vim/gvimrc"),
            &env.home.join(".gvimrc"),
        );
    }

    #[test]
    fn dry_run_detects_conflict() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .home_file(".vimrc", "existing")
            .build();
        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            true,
            false,
            false,
            true,
        );

        let results = executor
            .execute(vec![HandlerIntent::Link {
                pack: "vim".into(),
                handler: "symlink".into(),
                source: env.dotfiles_root.join("vim/vimrc"),
                user_path: env.home.join(".vimrc"),
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(results[0].message.contains("conflict"));
    }

    #[test]
    fn link_refuses_when_user_path_parent_symlinks_into_pack() {
        let env = TempEnvironment::builder()
            .pack("warp")
            .file("keybindings.yaml", "keep me")
            .done()
            .build();
        // Legacy setup: ~/.config/warp is a symlink into the pack itself.
        let pack_dir = env.dotfiles_root.join("warp");
        let config_warp = env.config_home.join("warp");
        env.fs.mkdir_all(&env.config_home).unwrap();
        env.fs.symlink(&pack_dir, &config_warp).unwrap();

        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        );

        let source = pack_dir.join("keybindings.yaml");
        let user_path = config_warp.join("keybindings.yaml");

        let results = executor
            .execute(vec![HandlerIntent::Link {
                pack: "warp".into(),
                handler: "symlink".into(),
                source: source.clone(),
                user_path: user_path.clone(),
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(!results[0].success, "expected failure, got: {:?}", results);
        assert!(
            results[0].message.contains("cycle"),
            "expected cycle message, got: {}",
            results[0].message
        );

        // No data link created, source file untouched.
        env.assert_no_handler_state("warp", "symlink");
        env.assert_file_contents(&source, "keep me");
    }

    /// Same check but the ancestor points into `data_dir`. Writing
    /// through it would land in the datastore and still wedge the
    /// system.
    #[test]
    fn link_refuses_when_user_path_parent_symlinks_into_data_dir() {
        let env = TempEnvironment::builder()
            .pack("warp")
            .file("keybindings.yaml", "keep me")
            .done()
            .build();
        let config_warp = env.config_home.join("warp");
        env.fs.mkdir_all(&env.config_home).unwrap();
        env.fs.mkdir_all(&env.data_dir).unwrap();
        env.fs.symlink(&env.data_dir, &config_warp).unwrap();

        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        );

        let source = env.dotfiles_root.join("warp/keybindings.yaml");
        let user_path = config_warp.join("keybindings.yaml");

        let results = executor
            .execute(vec![HandlerIntent::Link {
                pack: "warp".into(),
                handler: "symlink".into(),
                source: source.clone(),
                user_path: user_path.clone(),
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(results[0].message.contains("cycle"));
        env.assert_no_handler_state("warp", "symlink");
    }

    /// Dry-run must surface the same error, not silently report
    /// "would link".
    #[test]
    fn simulate_link_reports_ancestor_cycle() {
        let env = TempEnvironment::builder()
            .pack("warp")
            .file("keybindings.yaml", "keep me")
            .done()
            .build();
        let pack_dir = env.dotfiles_root.join("warp");
        let config_warp = env.config_home.join("warp");
        env.fs.mkdir_all(&env.config_home).unwrap();
        env.fs.symlink(&pack_dir, &config_warp).unwrap();

        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            true, // dry_run
            false,
            false,
            true,
        );

        let source = pack_dir.join("keybindings.yaml");
        let user_path = config_warp.join("keybindings.yaml");

        let results = executor
            .execute(vec![HandlerIntent::Link {
                pack: "warp".into(),
                handler: "symlink".into(),
                source,
                user_path,
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(
            results[0].message.contains("cycle"),
            "msg: {}",
            results[0].message
        );
    }

    /// --force must NOT bypass the ancestor-cycle check. A cycle can
    /// never be "forced through" — it would corrupt the pack.
    #[test]
    fn force_does_not_bypass_ancestor_cycle_check() {
        let env = TempEnvironment::builder()
            .pack("warp")
            .file("keybindings.yaml", "keep me")
            .done()
            .build();
        let pack_dir = env.dotfiles_root.join("warp");
        let config_warp = env.config_home.join("warp");
        env.fs.mkdir_all(&env.config_home).unwrap();
        env.fs.symlink(&pack_dir, &config_warp).unwrap();

        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            true, // force
            false,
            true,
        );

        let source = pack_dir.join("keybindings.yaml");
        let user_path = config_warp.join("keybindings.yaml");

        let results = executor
            .execute(vec![HandlerIntent::Link {
                pack: "warp".into(),
                handler: "symlink".into(),
                source: source.clone(),
                user_path,
            }])
            .unwrap();

        assert!(!results[0].success, "force must not bypass cycle check");
        env.assert_file_contents(&source, "keep me");
    }

    /// Relative-target ancestor symlinks must also be detected. A link
    /// like `~/.config/warp -> ../../h/dotfiles/warp` joins lexically to
    /// a path containing `..` segments that wouldn't naively pass
    /// `starts_with(dotfiles_root)` — we normalize first.
    #[test]
    fn link_refuses_relative_ancestor_symlink_into_pack() {
        let env = TempEnvironment::builder()
            .pack("warp")
            .file("keybindings.yaml", "keep me")
            .done()
            .build();
        let pack_dir = env.dotfiles_root.join("warp");
        let config_warp = env.config_home.join("warp");
        env.fs.mkdir_all(&env.config_home).unwrap();

        // config_home is home/.config, dotfiles_root is home/dotfiles,
        // so the relative hop is `../dotfiles/warp` — exactly the shape
        // Copilot flagged: contains `..`, joins to a path that would
        // NOT naively `starts_with(dotfiles_root)` without normalization.
        let rel_target = Path::new("../dotfiles/warp");
        env.fs.symlink(rel_target, &config_warp).unwrap();

        let (ds, _) = make_datastore(&env);
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        );

        let source = pack_dir.join("keybindings.yaml");
        let user_path = config_warp.join("keybindings.yaml");

        let results = executor
            .execute(vec![HandlerIntent::Link {
                pack: "warp".into(),
                handler: "symlink".into(),
                source: source.clone(),
                user_path,
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(
            !results[0].success,
            "relative ancestor symlink must still be caught: {:?}",
            results
        );
        assert!(results[0].message.contains("cycle"));
        env.assert_no_handler_state("warp", "symlink");
        env.assert_file_contents(&source, "keep me");
    }
}

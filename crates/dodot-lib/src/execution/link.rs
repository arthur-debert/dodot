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

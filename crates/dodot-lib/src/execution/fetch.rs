//! `Fetch` intent: pull an external resource into the datastore and
//! create the user-visible symlink that exposes it.
//!
//! Sentinel posture mirrors the install handler: the entry's content
//! signature is the sentinel payload.
//!
//! - For `file`, the signature is the user-declared sha256 in
//!   `externals.toml`. Re-running `up` with the same sha256 is a
//!   no-op; bumping it invalidates the old sentinel.
//! - For `git-repo`, the signature is the upstream HEAD SHA returned
//!   by a cheap `git ls-remote`. If the remote SHA matches the local
//!   clone's HEAD, the clone is left alone — even if `up` is run
//!   many times in a session. If the remote has moved, the executor
//!   shells out to `git fetch --depth=1 + reset --hard FETCH_HEAD`.
//!
//! Failure posture:
//! - **Integrity failure** (sha256 mismatch) is fatal — we refuse to
//!   write tampered content into the datastore.
//! - **Network failure** is soft — if a cached copy is present we leave
//!   it in place and report the failure as a non-success result; other
//!   intents still execute. This covers both HTTP and git transports.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

use crate::external::{ArchiveEntry, FetchSpec};
use crate::operations::{HandlerIntent, Operation, OperationResult};
use crate::Result;

/// Parsed archive contents, held in memory between successful parse
/// and (potentially destructive) cleanup of the prior extraction.
enum ArchiveExtracted {
    /// `type = "archive-file"`: a single member extracted by name.
    Single(ArchiveEntry),
    /// `type = "archive"`: the whole archive's entry tree.
    Tree(HashMap<PathBuf, ArchiveEntry>),
}

use super::Executor;

impl<'a> Executor<'a> {
    pub(super) fn execute_fetch(&self, intent: &HandlerIntent) -> Result<Vec<OperationResult>> {
        let HandlerIntent::Fetch {
            pack,
            handler,
            name,
            spec,
            user_path,
        } = intent
        else {
            unreachable!("execute_fetch called with non-Fetch intent");
        };

        match spec {
            FetchSpec::File { url, sha256 } => {
                self.execute_fetch_file(pack, handler, name, url, sha256, user_path)
            }
            FetchSpec::GitRepo {
                url,
                subpath,
                git_ref,
                commit,
            } => self.execute_fetch_git_repo(
                pack,
                handler,
                name,
                url,
                subpath.as_deref(),
                git_ref.as_deref(),
                commit.as_deref(),
                user_path,
            ),
            FetchSpec::Archive {
                url,
                sha256,
                format,
            } => self.execute_fetch_archive(
                pack, handler, name, url, sha256, *format, None, user_path,
            ),
            FetchSpec::ArchiveFile {
                url,
                sha256,
                member,
                format,
            } => self.execute_fetch_archive(
                pack,
                handler,
                name,
                url,
                sha256,
                *format,
                Some(member.as_str()),
                user_path,
            ),
            FetchSpec::Unsupported => Ok(vec![OperationResult::fail(
                fetch_op(pack, handler, name, "<unsupported>"),
                format!(
                    "external '{name}': unsupported type — supported in this release: `file`, `git-repo`, `archive`, `archive-file`"
                ),
            )]),
        }
    }

    pub(super) fn simulate_fetch(&self, intent: &HandlerIntent) -> Vec<OperationResult> {
        let HandlerIntent::Fetch {
            pack,
            handler,
            name,
            spec,
            user_path,
        } = intent
        else {
            unreachable!("simulate_fetch called with non-Fetch intent");
        };

        match spec {
            FetchSpec::File { url, sha256 } => {
                let sentinel = file_sentinel(name, sha256);
                let already = self
                    .datastore
                    .has_sentinel(pack, handler, &sentinel)
                    .unwrap_or(false);
                let msg = if already {
                    format!("[dry-run] {name} fresh (sha256 matches)")
                } else {
                    format!(
                        "[dry-run] would fetch {url} → {} (verify sha256={})",
                        user_path.display(),
                        short(sha256)
                    )
                };
                vec![OperationResult::ok(fetch_op(pack, handler, name, url), msg)]
            }
            FetchSpec::GitRepo {
                url,
                subpath,
                git_ref,
                commit,
            } => {
                let datastore_path = self.paths.handler_data_dir(pack, handler).join(name);
                let already = self.fs.exists(&datastore_path);
                let pin_label = match (git_ref.as_deref(), commit.as_deref()) {
                    (Some(r), _) => format!(" @ ref={r}"),
                    (_, Some(c)) => format!(" @ commit={}", short(c)),
                    _ => String::new(),
                };
                let subpath_label = subpath
                    .as_deref()
                    .map(|p| format!(" subpath={p}"))
                    .unwrap_or_default();
                let msg = if already {
                    if commit.is_some() {
                        format!(
                            "[dry-run] {name} pinned to commit; refresh only when TOML changes{subpath_label}"
                        )
                    } else {
                        format!(
                            "[dry-run] {name} would ls-remote {url}{pin_label} and refresh only if upstream differs{subpath_label}"
                        )
                    }
                } else {
                    format!(
                        "[dry-run] would clone {url}{pin_label} → {}{subpath_label} → {}",
                        datastore_path.display(),
                        user_path.display()
                    )
                };
                vec![OperationResult::ok(fetch_op(pack, handler, name, url), msg)]
            }
            FetchSpec::Archive { url, sha256, .. } => {
                let sentinel = archive_sentinel(name, sha256, None);
                let already = self
                    .datastore
                    .has_sentinel(pack, handler, &sentinel)
                    .unwrap_or(false);
                let msg = if already {
                    format!("[dry-run] {name} fresh (archive sha256 matches)")
                } else {
                    format!(
                        "[dry-run] would download {url}, verify sha256={}, extract → {}",
                        short(sha256),
                        user_path.display()
                    )
                };
                vec![OperationResult::ok(fetch_op(pack, handler, name, url), msg)]
            }
            FetchSpec::ArchiveFile {
                url,
                sha256,
                member,
                ..
            } => {
                let sentinel = archive_sentinel(name, sha256, Some(member));
                let already = self
                    .datastore
                    .has_sentinel(pack, handler, &sentinel)
                    .unwrap_or(false);
                let msg = if already {
                    format!("[dry-run] {name} fresh (archive sha256 + member matches)")
                } else {
                    format!(
                        "[dry-run] would download {url}, verify sha256={}, extract `{member}` → {}",
                        short(sha256),
                        user_path.display()
                    )
                };
                vec![OperationResult::ok(fetch_op(pack, handler, name, url), msg)]
            }
            FetchSpec::Unsupported => vec![OperationResult::fail(
                fetch_op(pack, handler, name, "<unsupported>"),
                format!("[dry-run] external '{name}': unsupported type"),
            )],
        }
    }

    /// Fetch one `type = "file"` external.
    fn execute_fetch_file(
        &self,
        pack: &str,
        handler: &str,
        name: &str,
        url: &str,
        expected_sha256: &str,
        user_path: &Path,
    ) -> Result<Vec<OperationResult>> {
        let sentinel = file_sentinel(name, expected_sha256);
        let op = || fetch_op(pack, handler, name, url);

        // Computing the expected datastore path up-front lets the
        // sentinel-hit branch verify the user-visible symlink is
        // still healthy without re-fetching.
        let filename = filename_for_target(user_path);
        let rel = format!("{name}/{filename}");
        let expected_datastore_path = self
            .paths
            .handler_data_dir(pack, handler)
            .join(name)
            .join(&filename);

        // Sentinel hit: content matching this sha256 has already been
        // fetched. Skip the network round-trip, but make sure the
        // user-visible symlink still exists and points at the right
        // datastore copy — a deleted/broken link should self-repair on
        // the next `up` even without `--force` or a sha change.
        if !self.force && self.datastore.has_sentinel(pack, handler, &sentinel)? {
            debug!(
                pack,
                name, "external sentinel matches; checking deployed link"
            );
            return self.repair_external_link(
                pack,
                handler,
                name,
                &expected_datastore_path,
                user_path,
                op,
            );
        }

        // Pre-check `user_path` for a conflicting non-symlink BEFORE
        // we fetch — mirrors `execute_link`'s posture so conflicts
        // surface as failed OperationResults without burning a network
        // round-trip, without partially-written state, and without
        // aborting the whole run.
        if let Some(conflict) = self.check_external_target_conflict(name, user_path, op) {
            return Ok(conflict);
        }

        let Some(fetcher) = self.fetcher() else {
            return Ok(vec![OperationResult::fail(
                op(),
                format!(
                    "external '{name}': executor has no HTTP fetcher configured; call Executor::with_fetcher() in production wiring"
                ),
            )]);
        };

        info!(pack, name, url, "fetching external");
        let bytes = match fetcher.fetch(url) {
            Ok(b) => b,
            Err(err) if err.is_transient() => {
                // Soft-fail: keep any previously fetched copy and
                // surface a non-success result.
                warn!(pack, name, %err, "external fetch failed (transient)");
                return Ok(vec![OperationResult::fail(
                    op(),
                    format!("{name}: fetch failed ({err}); leaving cached copy in place"),
                )]);
            }
            Err(err) => {
                return Ok(vec![OperationResult::fail(
                    op(),
                    format!("{name}: fetch failed: {err}"),
                )]);
            }
        };

        let actual = sha256_hex(&bytes);
        if !sha256_matches(expected_sha256, &actual) {
            return Ok(vec![OperationResult::fail(
                op(),
                format!(
                    "{name}: sha256 mismatch (configured {}, actual {}); refusing to write",
                    short(expected_sha256),
                    short(&actual)
                ),
            )]);
        }

        // Persist into the datastore: `<handler_data_dir>/<name>/<filename>`.
        let datastore_path = self
            .datastore
            .write_rendered_file(pack, handler, &rel, &bytes)?;
        debug!(pack, name, datastore = %datastore_path.display(), "wrote external to datastore");

        // Symlink the user-visible target → datastore copy. We already
        // pre-checked for non-symlink conflicts above, so this only
        // needs to handle "remove existing dodot symlink and re-create".
        self.create_external_user_link(&datastore_path, user_path)?;

        // Record sentinel so subsequent up's are no-ops.
        self.write_sentinel(pack, handler, &sentinel)?;

        let create_link = Operation::CreateUserLink {
            pack: pack.to_string(),
            handler: handler.to_string(),
            datastore_path: datastore_path.clone(),
            user_path: user_path.to_path_buf(),
        };

        Ok(vec![
            OperationResult::ok(
                op(),
                format!("{name}: fetched {} bytes from {url}", bytes.len()),
            ),
            OperationResult::ok(create_link, format!("{name} → {}", user_path.display())),
        ])
    }

    /// Fetch + extract a `type = "archive"` (whole tree) or
    /// `type = "archive-file"` (single entry) external.
    ///
    /// When `member` is `None`, the whole archive is materialised
    /// under `<handler_data_dir>/<name>/` and the user-visible
    /// symlink points at that directory. When `member = Some(p)`,
    /// only that entry is written, at `<handler_data_dir>/<name>/<basename>`,
    /// and the symlink points at the single file.
    ///
    /// Sentinel: `<name>-archive-<sha-prefix>[-<member-hash>]`. The
    /// sha256 is the archive's hash (already declared by the user);
    /// for archive-file we mix in a short hash of the member path so
    /// changing `member = ...` in the TOML re-extracts.
    #[allow(clippy::too_many_arguments)]
    fn execute_fetch_archive(
        &self,
        pack: &str,
        handler: &str,
        name: &str,
        url: &str,
        expected_sha256: &str,
        format: Option<crate::external::ArchiveFormat>,
        member: Option<&str>,
        user_path: &Path,
    ) -> Result<Vec<OperationResult>> {
        let op = || fetch_op(pack, handler, name, url);
        let sentinel = archive_sentinel(name, expected_sha256, member);

        if !self.force && self.datastore.has_sentinel(pack, handler, &sentinel)? {
            debug!(pack, name, "archive sentinel matches; skipping fetch");
            return Ok(vec![OperationResult::ok(
                op(),
                format!("{name}: fresh (archive sha256 matches)"),
            )]);
        }

        // Resolve format: explicit field wins; otherwise infer from
        // the URL filename.
        let format = match format.or_else(|| crate::external::ArchiveFormat::infer_from_url(url)) {
            Some(f) => f,
            None => {
                return Ok(vec![OperationResult::fail(
                    op(),
                    format!(
                        "{name}: archive format could not be inferred from URL {url}; set `format` explicitly"
                    ),
                )]);
            }
        };

        let Some(fetcher) = self.fetcher() else {
            return Ok(vec![OperationResult::fail(
                op(),
                format!(
                    "external '{name}': executor has no HTTP fetcher configured; call Executor::with_fetcher() in production wiring"
                ),
            )]);
        };

        info!(pack, name, url, ?format, ?member, "downloading archive");
        let bytes = match fetcher.fetch(url) {
            Ok(b) => b,
            Err(err) if err.is_transient() => {
                warn!(pack, name, %err, "archive fetch failed (transient)");
                return Ok(vec![OperationResult::fail(
                    op(),
                    format!("{name}: fetch failed ({err}); leaving cached copy in place"),
                )]);
            }
            Err(err) => {
                return Ok(vec![OperationResult::fail(
                    op(),
                    format!("{name}: fetch failed: {err}"),
                )]);
            }
        };

        let actual = sha256_hex(&bytes);
        if !sha256_matches(expected_sha256, &actual) {
            return Ok(vec![OperationResult::fail(
                op(),
                format!(
                    "{name}: sha256 mismatch (configured {}, actual {}); refusing to extract",
                    short(expected_sha256),
                    short(&actual)
                ),
            )]);
        }

        // Parse + validate the archive BEFORE touching the previous
        // extraction on disk — a corrupt download or an unsafe path
        // must not destroy the cached copy that's already deployed.
        let entry_root = self.paths.handler_data_dir(pack, handler).join(name);
        let archive_parse = if let Some(m) = member {
            match crate::external::read_member(&bytes, format, m) {
                Ok(e) => ArchiveExtracted::Single(e),
                Err(err) => {
                    return Ok(vec![OperationResult::fail(op(), format!("{name}: {err}"))]);
                }
            }
        } else {
            match crate::external::read_all(&bytes, format) {
                Ok(es) => ArchiveExtracted::Tree(es),
                Err(err) => {
                    return Ok(vec![OperationResult::fail(op(), format!("{name}: {err}"))]);
                }
            }
        };

        // Now safe to wipe the previous extraction. We only get here
        // once we've already parsed the new archive successfully.
        if self.fs.exists(&entry_root) {
            // For archive-file we wrote a single file; for archive we
            // wrote a directory tree. Handle both.
            if self.fs.is_dir(&entry_root) {
                self.fs.remove_dir_all(&entry_root)?;
            } else {
                self.fs.remove_file(&entry_root)?;
            }
        }

        let (symlink_target, ops) = match archive_parse {
            ArchiveExtracted::Single(entry) => {
                let m = member.expect("Single variant only produced for member fetches");
                // Land the single file at `<name>/<basename-of-member>`.
                // basename so users get a stable path inside the
                // datastore even when the archive uses a deep member.
                let basename = std::path::Path::new(m)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "content".into());
                let rel = format!("{name}/{basename}");
                let dst = self.write_archive_entry(pack, handler, &rel, &entry)?;
                (
                    dst,
                    vec![format!(
                        "{name}: extracted `{m}` ({} bytes)",
                        entry.bytes.len()
                    )],
                )
            }
            ArchiveExtracted::Tree(entries) => {
                let mut written = 0usize;
                for (rel_path, entry) in &entries {
                    let rel_str = rel_path.to_string_lossy();
                    let rel_full = format!("{name}/{rel_str}");
                    if entry.is_dir {
                        self.datastore
                            .write_rendered_dir(pack, handler, &rel_full)?;
                    } else {
                        self.write_archive_entry(pack, handler, &rel_full, entry)?;
                        written += 1;
                    }
                }
                (
                    entry_root.clone(),
                    vec![format!(
                        "{name}: extracted {} files into {}",
                        written,
                        entry_root.display()
                    )],
                )
            }
        };

        // User-visible symlink.
        self.create_external_user_link(&symlink_target, user_path)?;
        self.write_sentinel(pack, handler, &sentinel)?;

        let create_link = Operation::CreateUserLink {
            pack: pack.to_string(),
            handler: handler.to_string(),
            datastore_path: symlink_target.clone(),
            user_path: user_path.to_path_buf(),
        };
        let mut results = Vec::new();
        for msg in ops {
            results.push(OperationResult::ok(op(), msg));
        }
        results.push(OperationResult::ok(
            create_link,
            format!("{name} → {}", user_path.display()),
        ));
        Ok(results)
    }

    /// Write one archive entry to the datastore, preserving the
    /// archive's mode bits when present so executable files stay
    /// executable after `up`. Falls back to `write_rendered_file`
    /// (umask-default mode) when the entry didn't carry a mode.
    fn write_archive_entry(
        &self,
        pack: &str,
        handler: &str,
        rel: &str,
        entry: &crate::external::ArchiveEntry,
    ) -> Result<std::path::PathBuf> {
        match entry.mode {
            Some(mode) if mode != 0 => {
                self.datastore
                    .write_rendered_file_with_mode(pack, handler, rel, &entry.bytes, mode)
            }
            _ => self
                .datastore
                .write_rendered_file(pack, handler, rel, &entry.bytes),
        }
    }

    /// Fetch one `type = "git-repo"` external.
    ///
    /// Freshness model:
    /// - **Unpinned** (`git_ref = None`, `commit = None`):
    ///   `git ls-remote HEAD`; refresh when upstream moves.
    /// - **`ref = "v1"`**: `git ls-remote v1`; refresh when that
    ///   reference's SHA changes (rare for tags, possible for
    ///   branch refs).
    /// - **`commit = "<sha>"`**: no ls-remote at all; check the
    ///   local clone against the pinned SHA and refresh only if
    ///   they differ (which happens when the user edits the TOML).
    ///
    /// `subpath` triggers sparse-checkout on the initial clone; the
    /// user-visible symlink then targets that subpath inside the
    /// clone.
    ///
    /// Network failures (ls-remote, fetch, even initial clone) are
    /// soft: the cached clone (if any) stays put and the result
    /// surfaces as a non-success.
    #[allow(clippy::too_many_arguments)]
    fn execute_fetch_git_repo(
        &self,
        pack: &str,
        handler: &str,
        name: &str,
        url: &str,
        subpath: Option<&str>,
        git_ref: Option<&str>,
        commit: Option<&str>,
        user_path: &Path,
    ) -> Result<Vec<OperationResult>> {
        let op = || fetch_op(pack, handler, name, url);

        let Some(git) = self.git() else {
            return Ok(vec![OperationResult::fail(
                op(),
                format!(
                    "external '{name}': executor has no git runner configured; call Executor::with_git() in production wiring"
                ),
            )]);
        };

        // The clone always lands at <handler_data_dir>/<name>/. When
        // subpath is set, the user-visible symlink points one level
        // deeper.
        let clone_path = self.paths.handler_data_dir(pack, handler).join(name);
        let already_cloned = self.fs.exists(&clone_path);
        let symlink_target = match subpath {
            Some(p) => clone_path.join(p),
            None => clone_path.clone(),
        };

        // Reference we ask git about. `commit` pins skip ls-remote
        // entirely; otherwise we use the configured ref or HEAD.
        let tracking_ref = git_ref.unwrap_or("HEAD");

        if !already_cloned {
            // Fresh clone path.
            if let Some(parent) = clone_path.parent() {
                self.fs.mkdir_all(parent)?;
            }
            info!(
                pack,
                name,
                url,
                ?git_ref,
                ?subpath,
                "shallow-cloning external"
            );
            let cloned_sha = match git.shallow_clone(url, &clone_path, git_ref, subpath) {
                Ok(s) => s,
                Err(err) => {
                    warn!(pack, name, %err, "git clone failed");
                    return Ok(vec![OperationResult::fail(
                        op(),
                        format!("{name}: clone failed: {err}"),
                    )]);
                }
            };
            // Pinned to a specific commit: snap HEAD to that commit
            // after the clone so the local SHA matches the pin.
            //
            // A shallow clone (--depth=1) only ships the tip object,
            // so a non-tip commit won't be reachable yet — fetch it
            // explicitly first, then check out.
            let final_sha = if let Some(c) = commit {
                if !cloned_sha.eq_ignore_ascii_case(c) {
                    if let Err(err) = git.fetch_and_reset(&clone_path, c) {
                        return Ok(vec![OperationResult::fail(
                            op(),
                            format!("{name}: fetch {} after clone failed: {err}", short(c)),
                        )]);
                    }
                    if let Err(err) = git.checkout(&clone_path, c) {
                        return Ok(vec![OperationResult::fail(
                            op(),
                            format!("{name}: checkout {} after clone failed: {err}", short(c)),
                        )]);
                    }
                }
                c.to_string()
            } else {
                cloned_sha
            };
            return self.finish_git_repo(
                pack,
                handler,
                name,
                url,
                &clone_path,
                &symlink_target,
                user_path,
                &final_sha,
            );
        }

        // Existing clone — read local first so a corrupted clone is
        // surfaced as a hard failure regardless of whether upstream is
        // reachable.
        let local = match git.local_head(&clone_path) {
            Ok(s) => s,
            Err(err) => {
                return Ok(vec![OperationResult::fail(
                    op(),
                    format!(
                        "{name}: existing clone at {} is unreadable: {err}",
                        clone_path.display()
                    ),
                )]);
            }
        };

        // Commit pin: compare local against the pin directly; no
        // ls-remote needed since the spec pins a fixed SHA.
        if let Some(c) = commit {
            if local.eq_ignore_ascii_case(c) && !self.force {
                return self
                    .finish_git_repo(
                        pack,
                        handler,
                        name,
                        url,
                        &clone_path,
                        &symlink_target,
                        user_path,
                        &local,
                    )
                    .map(|mut results| {
                        if let Some(r) = results.first_mut() {
                            *r = OperationResult::ok(
                                op(),
                                format!("{name}: pinned to commit {}", short(c)),
                            );
                        }
                        results
                    });
            }
            // Pin moved or --force: fetch the pin, then checkout.
            match git.fetch_and_reset(&clone_path, c) {
                Ok(_) => {}
                Err(err) if err.is_transient() => {
                    return Ok(vec![OperationResult::fail(
                        op(),
                        format!(
                            "{name}: fetch {} failed ({err}); cached clone at {} stays in place",
                            short(c),
                            short(&local)
                        ),
                    )]);
                }
                Err(err) => {
                    return Ok(vec![OperationResult::fail(
                        op(),
                        format!("{name}: fetch {} failed: {err}", short(c)),
                    )]);
                }
            }
            // After fetch+reset, local HEAD should be at `c`. The
            // explicit checkout is belt-and-braces in case the user
            // pinned a non-tip commit and FETCH_HEAD landed on a
            // different ref. Checkout failure is a hard fail rather
            // than a warning — leaving the tree in a mismatched state
            // would silently mislead `dodot status`.
            if let Err(err) = git.checkout(&clone_path, c) {
                return Ok(vec![OperationResult::fail(
                    op(),
                    format!("{name}: checkout {} after fetch failed: {err}", short(c)),
                )]);
            }
            return self.finish_git_repo(
                pack,
                handler,
                name,
                url,
                &clone_path,
                &symlink_target,
                user_path,
                c,
            );
        }

        // Unpinned or ref-pinned: ls-remote the tracking reference.
        // Transient ls-remote failures don't abort the run, but they
        // DO surface as non-success — claiming "fresh" when we
        // couldn't actually verify upstream would mislead. The cached
        // clone (already symlinked from the prior successful run)
        // stays put.
        let upstream = match git.ls_remote(url, tracking_ref) {
            Ok(s) => s,
            Err(err) if err.is_transient() => {
                warn!(pack, name, %err, "ls-remote failed (transient); using cached clone");
                return Ok(vec![OperationResult::fail(
                    op(),
                    format!(
                        "{name}: ls-remote {tracking_ref} failed ({err}); using cached clone at {}",
                        short(&local)
                    ),
                )]);
            }
            Err(err) => {
                return Ok(vec![OperationResult::fail(
                    op(),
                    format!("{name}: ls-remote {tracking_ref} failed: {err}"),
                )]);
            }
        };

        // Both upstream and local are known SHAs from here on.
        let (final_sha, was_refreshed) = if upstream == self.force_refresh_target(&local) {
            (local, false)
        } else {
            info!(
                pack, name,
                local = %short(&local),
                remote = %short(&upstream),
                "upstream moved; fetching + reset"
            );
            match git.fetch_and_reset(&clone_path, tracking_ref) {
                Ok(s) => (s, true),
                Err(err) if err.is_transient() => {
                    warn!(pack, name, %err, "fetch+reset failed (transient); keeping cached");
                    return Ok(vec![OperationResult::fail(
                        op(),
                        format!(
                            "{name}: fetch failed ({err}); cached clone at {} stays in place",
                            short(&local)
                        ),
                    )]);
                }
                Err(err) => {
                    return Ok(vec![OperationResult::fail(
                        op(),
                        format!("{name}: fetch failed: {err}"),
                    )]);
                }
            }
        };

        let mut results = self.finish_git_repo(
            pack,
            handler,
            name,
            url,
            &clone_path,
            &symlink_target,
            user_path,
            &final_sha,
        )?;
        if !was_refreshed && self.fetcher_message_overridable(&results) {
            // Only rewrite to "fresh (== upstream)" when we actually
            // know upstream matches local — i.e. ls-remote succeeded
            // and the SHAs lined up. The transient-failure paths
            // already returned above.
            results[0] = OperationResult::ok(
                op(),
                format!("{name}: fresh ({} == upstream)", short(&final_sha)),
            );
        }
        Ok(results)
    }

    /// Forced refresh shim: when `--force` is set we want to refresh
    /// even if local == remote, so flip the comparison target to
    /// guarantee a mismatch.
    fn force_refresh_target(&self, local: &str) -> String {
        if self.force {
            format!("force-refresh-{local}")
        } else {
            local.to_string()
        }
    }

    /// True when the first result is the "fetched bytes" message that
    /// the caller can safely replace with a "fresh" message.
    fn fetcher_message_overridable(&self, results: &[OperationResult]) -> bool {
        results.first().is_some_and(|r| r.success)
    }

    /// Symlink + sentinel finalize for git-repo. Shared by initial
    /// clone and refresh paths.
    ///
    /// `clone_path` is the on-disk clone root (always
    /// `<handler_data_dir>/<name>`); `symlink_target` is where the
    /// user-visible symlink should point — equal to `clone_path` when
    /// no subpath is configured, deeper otherwise.
    #[allow(clippy::too_many_arguments)]
    fn finish_git_repo(
        &self,
        pack: &str,
        handler: &str,
        name: &str,
        url: &str,
        clone_path: &Path,
        symlink_target: &Path,
        user_path: &Path,
        sha: &str,
    ) -> Result<Vec<OperationResult>> {
        self.create_external_user_link(symlink_target, user_path)?;
        let sentinel = git_repo_sentinel(name, sha);
        self.write_sentinel(pack, handler, &sentinel)?;

        let create_link = Operation::CreateUserLink {
            pack: pack.to_string(),
            handler: handler.to_string(),
            datastore_path: symlink_target.to_path_buf(),
            user_path: user_path.to_path_buf(),
        };
        Ok(vec![
            OperationResult::ok(
                fetch_op(pack, handler, name, url),
                format!("{name}: HEAD={} at {}", short(sha), clone_path.display()),
            ),
            OperationResult::ok(create_link, format!("{name} → {}", user_path.display())),
        ])
    }

    /// Sentinel-hit path: confirm the deployed symlink still resolves
    /// to the expected datastore copy. Restores the link if it's
    /// missing or pointing somewhere stale, surfaces a conflict result
    /// if a non-symlink occupies the target.
    fn repair_external_link(
        &self,
        pack: &str,
        handler: &str,
        name: &str,
        expected_datastore_path: &Path,
        user_path: &Path,
        op: impl Fn() -> Operation,
    ) -> Result<Vec<OperationResult>> {
        let needs_relink = if self.fs.is_symlink(user_path) {
            // Existing symlink — only re-link if it points elsewhere.
            self.fs
                .readlink(user_path)
                .map(|t| t != expected_datastore_path)
                .unwrap_or(true)
        } else if self.fs.exists(user_path) {
            // Non-symlink at the target. Without --force we have to
            // surface the conflict the same way the fetch path does.
            if let Some(conflict) = self.check_external_target_conflict(name, user_path, &op) {
                return Ok(conflict);
            }
            true
        } else {
            // No file there at all — restore the link.
            true
        };

        if !needs_relink {
            return Ok(vec![OperationResult::ok(
                op(),
                format!("{name}: fresh (sha256 matches)"),
            )]);
        }

        // The datastore file may have been wiped externally even
        // though the sentinel survived. Refuse to dangle the symlink
        // in that case — surface as a failed result so `up` re-fetches
        // on the next run (the user can clear the sentinel manually
        // if they want immediate repair).
        if !self.fs.exists(expected_datastore_path) {
            return Ok(vec![OperationResult::fail(
                op(),
                format!(
                    "{name}: sentinel present but datastore copy missing at {}; run `dodot up --force` to refetch",
                    expected_datastore_path.display()
                ),
            )]);
        }

        self.create_external_user_link(expected_datastore_path, user_path)?;
        let create_link = Operation::CreateUserLink {
            pack: pack.to_string(),
            handler: handler.to_string(),
            datastore_path: expected_datastore_path.to_path_buf(),
            user_path: user_path.to_path_buf(),
        };
        Ok(vec![
            OperationResult::ok(op(), format!("{name}: fresh (sha256 matches)")),
            OperationResult::ok(
                create_link,
                format!("{name} → {} (repaired)", user_path.display()),
            ),
        ])
    }

    /// Pre-check whether `user_path` has a non-symlink occupant that
    /// would block deploying an external. Returns `Some(results)` when
    /// there is a conflict the caller should propagate; `None` when
    /// it's safe to proceed.
    fn check_external_target_conflict(
        &self,
        name: &str,
        user_path: &Path,
        op: impl Fn() -> Operation,
    ) -> Option<Vec<OperationResult>> {
        if self.fs.is_symlink(user_path) || !self.fs.exists(user_path) {
            return None;
        }
        if self.force {
            return None;
        }
        Some(vec![OperationResult::fail(
            op(),
            format!(
                "{name}: conflict — {} already exists and is not a dodot symlink (use --force to overwrite)",
                user_path.display()
            ),
        )])
    }

    /// Symlink leg of a Fetch: the "source" is already in the
    /// datastore (we just wrote it), so only the user-visible leg is
    /// needed. Non-symlink conflicts must be pre-checked via
    /// [`Self::check_external_target_conflict`] before calling — this
    /// helper assumes the caller has confirmed it's safe to overwrite.
    fn create_external_user_link(&self, datastore_path: &Path, user_path: &Path) -> Result<()> {
        if !self.fs.is_symlink(user_path) && self.fs.exists(user_path) {
            // Caller is supposed to have pre-checked. `--force` is the
            // only way to land here; consume the conflicting path.
            if self.fs.is_dir(user_path) {
                self.fs.remove_dir_all(user_path)?;
            } else {
                self.fs.remove_file(user_path)?;
            }
        }
        // `create_user_link` is idempotent against an existing dodot
        // symlink — replaces it if the target differs, no-ops if it
        // already points at the right datastore path.
        self.datastore.create_user_link(datastore_path, user_path)
    }

    fn write_sentinel(&self, pack: &str, handler: &str, sentinel: &str) -> Result<()> {
        // Sentinels live alongside other handler state. The
        // `write_rendered_file` path conveniently creates parent dirs
        // and accepts arbitrary content.
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let content = format!("completed|{timestamp}");
        self.datastore
            .write_rendered_file(pack, handler, sentinel, content.as_bytes())?;
        Ok(())
    }
}

/// Build the sentinel filename for a `type = "file"` entry.
fn file_sentinel(name: &str, sha256: &str) -> String {
    format!("{name}-{}", short(sha256))
}

/// Sentinel filename for a `type = "git-repo"` entry. The SHA prefix
/// is the upstream HEAD commit we deployed; bumping upstream changes
/// the sentinel, so `dodot status` can tell at a glance which commit
/// is live.
fn git_repo_sentinel(name: &str, sha: &str) -> String {
    format!("{name}-git-{}", short(sha))
}

/// Sentinel filename for `archive` / `archive-file` entries.
///
/// The archive sha256 prefix is the primary key. For archive-file we
/// also mix in a short hash of the member path so changing the
/// `member` field in the TOML (without changing the archive itself)
/// still invalidates the sentinel.
fn archive_sentinel(name: &str, sha256: &str, member: Option<&str>) -> String {
    match member {
        Some(m) => {
            let member_hash = sha256_hex(m.as_bytes());
            format!("{name}-archive-{}-{}", short(sha256), short(&member_hash))
        }
        None => format!("{name}-archive-{}", short(sha256)),
    }
}

/// Derive the on-disk filename inside the datastore subdir from the
/// target path. Falls back to "content" when the target ends in `/`.
fn filename_for_target(target: &Path) -> String {
    target
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "content".into())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let digest = h.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Constant-time-ish hex compare: case-insensitive, but the timing
/// difference doesn't matter for a content-addressed sentinel that
/// the user already declared in their checked-in TOML.
fn sha256_matches(expected: &str, actual_hex: &str) -> bool {
    expected.eq_ignore_ascii_case(actual_hex)
}

/// First 16 hex chars of a sha256 — used for sentinel filenames so
/// the datastore directory listing stays readable. Cryptographically
/// the full 256 bits live in the user's TOML; the sentinel just keys
/// off the prefix.
fn short(sha: &str) -> String {
    sha.chars().take(16).collect()
}

fn fetch_op(pack: &str, handler: &str, name: &str, url: &str) -> Operation {
    Operation::FetchExternal {
        pack: pack.to_string(),
        handler: handler.to_string(),
        name: name.to_string(),
        url: url.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::make_datastore;
    use super::super::Executor;
    use crate::external::{FetchSpec, HttpFetchError, HttpFetcher};
    use crate::fs::Fs;
    use crate::operations::HandlerIntent;
    use crate::paths::Pather;
    use crate::testing::TempEnvironment;
    use std::sync::Mutex;

    /// Mock fetcher returning pre-canned bodies per URL.
    struct MockFetcher {
        responses:
            Mutex<std::collections::HashMap<String, std::result::Result<Vec<u8>, HttpFetchError>>>,
        calls: Mutex<Vec<String>>,
    }

    impl MockFetcher {
        fn new() -> Self {
            Self {
                responses: Mutex::new(Default::default()),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn with(self, url: &str, body: &[u8]) -> Self {
            self.responses
                .lock()
                .unwrap()
                .insert(url.into(), Ok(body.to_vec()));
            self
        }

        fn with_error(self, url: &str, err: HttpFetchError) -> Self {
            self.responses.lock().unwrap().insert(url.into(), Err(err));
            self
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl HttpFetcher for MockFetcher {
        fn fetch(&self, url: &str) -> std::result::Result<Vec<u8>, HttpFetchError> {
            self.calls.lock().unwrap().push(url.into());
            match self.responses.lock().unwrap().remove(url) {
                Some(r) => r,
                None => Err(HttpFetchError::InvalidUrl(format!(
                    "mock: no response configured for {url}"
                ))),
            }
        }
    }

    fn known_body() -> &'static [u8] {
        b"#!/bin/sh\nexport SHARED=1\n"
    }

    fn known_sha() -> String {
        super::sha256_hex(known_body())
    }

    #[test]
    fn execute_fetch_writes_datastore_and_symlinks() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);
        let fetcher = MockFetcher::new().with("https://example.com/aliases.sh", known_body());

        let user_path = env.home.join(".config/shared/aliases.sh");
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        )
        .with_fetcher(&fetcher);

        let results = executor
            .execute(vec![HandlerIntent::Fetch {
                pack: "shared".into(),
                handler: "external".into(),
                name: "aliases".into(),
                spec: FetchSpec::File {
                    url: "https://example.com/aliases.sh".into(),
                    sha256: known_sha(),
                },
                user_path: user_path.clone(),
            }])
            .unwrap();

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.success), "{results:#?}");
        assert_eq!(fetcher.calls(), vec!["https://example.com/aliases.sh"]);

        // The user-visible link exists and resolves to the bytes we fed in.
        assert!(env.fs.is_symlink(&user_path));
        let content = env.fs.read_to_string(&user_path).unwrap();
        assert_eq!(content.as_bytes(), known_body());

        // Sentinel was recorded.
        let sentinel = super::file_sentinel("aliases", &known_sha());
        env.assert_sentinel("shared", "external", &sentinel);
    }

    #[test]
    fn execute_fetch_is_idempotent_via_sentinel() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);
        let fetcher = MockFetcher::new().with("https://example.com/aliases.sh", known_body());

        let user_path = env.home.join(".config/shared/aliases.sh");
        let intent = HandlerIntent::Fetch {
            pack: "shared".into(),
            handler: "external".into(),
            name: "aliases".into(),
            spec: FetchSpec::File {
                url: "https://example.com/aliases.sh".into(),
                sha256: known_sha(),
            },
            user_path: user_path.clone(),
        };

        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        )
        .with_fetcher(&fetcher);
        let _ = executor.execute(vec![intent.clone()]).unwrap();

        // Second run: no calls because sentinel matches.
        let results = executor.execute(vec![intent]).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert!(
            results[0].message.contains("fresh"),
            "msg: {}",
            results[0].message
        );
        // Mock pops responses on use; only the first execute consumed it.
        assert_eq!(fetcher.calls(), vec!["https://example.com/aliases.sh"]);
    }

    #[test]
    fn sentinel_hit_repairs_deleted_user_symlink() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);
        let fetcher = MockFetcher::new().with("https://example.com/aliases.sh", known_body());
        let user_path = env.home.join(".config/shared/aliases.sh");
        let intent = HandlerIntent::Fetch {
            pack: "shared".into(),
            handler: "external".into(),
            name: "aliases".into(),
            spec: FetchSpec::File {
                url: "https://example.com/aliases.sh".into(),
                sha256: known_sha(),
            },
            user_path: user_path.clone(),
        };

        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        )
        .with_fetcher(&fetcher);

        // Initial deploy: fetch + link.
        executor.execute(vec![intent.clone()]).unwrap();
        assert!(env.fs.is_symlink(&user_path));

        // User (or a stray `rm`) deletes the deployed symlink.
        env.fs.remove_file(&user_path).unwrap();
        assert!(!env.fs.exists(&user_path));

        // Re-running `up` with the sentinel still present must
        // restore the symlink even though the sha hasn't changed and
        // --force is off. The fetcher's only canned response was
        // already consumed — so a regression here would surface as
        // "no response configured for ...".
        let results = executor.execute(vec![intent]).unwrap();
        assert!(
            results.iter().all(|r| r.success),
            "repair should succeed: {results:#?}"
        );
        assert!(
            env.fs.is_symlink(&user_path),
            "user-visible symlink should be restored"
        );
        assert_eq!(
            env.fs.read_to_string(&user_path).unwrap(),
            "#!/bin/sh\nexport SHARED=1\n"
        );
        // The repair must not re-fetch from upstream.
        assert_eq!(fetcher.calls(), vec!["https://example.com/aliases.sh"]);
    }

    #[test]
    fn non_symlink_at_target_returns_failed_result_not_error() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);
        let fetcher = MockFetcher::new().with("https://example.com/aliases.sh", known_body());

        // Place a regular file at the target path before deploying.
        let user_path = env.home.join(".config/shared/aliases.sh");
        env.fs.mkdir_all(user_path.parent().unwrap()).unwrap();
        env.fs
            .write_file(&user_path, b"hand-written by the user")
            .unwrap();

        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false, // not --force
            false,
            true,
        )
        .with_fetcher(&fetcher);

        // Without --force, this must surface as a failed
        // OperationResult, NOT as an Err that propagates out of
        // execute() — same posture as execute_link's conflict path.
        let results = executor
            .execute(vec![HandlerIntent::Fetch {
                pack: "shared".into(),
                handler: "external".into(),
                name: "aliases".into(),
                spec: FetchSpec::File {
                    url: "https://example.com/aliases.sh".into(),
                    sha256: known_sha(),
                },
                user_path: user_path.clone(),
            }])
            .unwrap();
        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(
            results[0].message.contains("conflict"),
            "msg: {}",
            results[0].message
        );
        // Original file untouched.
        assert_eq!(
            env.fs.read_to_string(&user_path).unwrap(),
            "hand-written by the user"
        );
        // No fetch happened (we pre-checked the conflict).
        assert!(fetcher.calls().is_empty());
    }

    #[test]
    fn execute_fetch_rejects_sha256_mismatch() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);
        let fetcher = MockFetcher::new().with("https://example.com/aliases.sh", b"tampered");

        let user_path = env.home.join(".config/shared/aliases.sh");
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        )
        .with_fetcher(&fetcher);

        let results = executor
            .execute(vec![HandlerIntent::Fetch {
                pack: "shared".into(),
                handler: "external".into(),
                name: "aliases".into(),
                spec: FetchSpec::File {
                    url: "https://example.com/aliases.sh".into(),
                    sha256: known_sha(),
                },
                user_path: user_path.clone(),
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(
            results[0].message.contains("sha256 mismatch"),
            "msg: {}",
            results[0].message
        );
        // Nothing was written.
        assert!(!env.fs.exists(&user_path));
        env.assert_no_handler_state("shared", "external");
    }

    #[test]
    fn transient_network_failure_is_non_fatal() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);
        let fetcher = MockFetcher::new().with_error(
            "https://example.com/aliases.sh",
            HttpFetchError::Network {
                url: "https://example.com/aliases.sh".into(),
                source: "simulated".into(),
            },
        );

        let user_path = env.home.join(".config/shared/aliases.sh");
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        )
        .with_fetcher(&fetcher);

        let results = executor
            .execute(vec![HandlerIntent::Fetch {
                pack: "shared".into(),
                handler: "external".into(),
                name: "aliases".into(),
                spec: FetchSpec::File {
                    url: "https://example.com/aliases.sh".into(),
                    sha256: known_sha(),
                },
                user_path: user_path.clone(),
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(
            results[0].message.contains("fetch failed"),
            "msg: {}",
            results[0].message
        );
    }

    #[test]
    fn unsupported_type_fails_cleanly() {
        let env = TempEnvironment::builder().build();
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
            .execute(vec![HandlerIntent::Fetch {
                pack: "shared".into(),
                handler: "external".into(),
                name: "omz".into(),
                spec: FetchSpec::Unsupported,
                user_path: env.home.join(".oh-my-zsh"),
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(
            results[0].message.contains("unsupported type"),
            "msg: {}",
            results[0].message
        );
    }

    #[test]
    fn dry_run_does_not_fetch() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);
        let fetcher = MockFetcher::new().with("https://example.com/aliases.sh", known_body());

        let user_path = env.home.join(".config/shared/aliases.sh");
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            true,
            false,
            false,
            true,
        )
        .with_fetcher(&fetcher);

        let results = executor
            .execute(vec![HandlerIntent::Fetch {
                pack: "shared".into(),
                handler: "external".into(),
                name: "aliases".into(),
                spec: FetchSpec::File {
                    url: "https://example.com/aliases.sh".into(),
                    sha256: known_sha(),
                },
                user_path: user_path.clone(),
            }])
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert!(
            results[0].message.contains("[dry-run]"),
            "msg: {}",
            results[0].message
        );
        // No fetch call, no symlink, no sentinel.
        assert!(fetcher.calls().is_empty());
        assert!(!env.fs.exists(&user_path));
        env.assert_no_handler_state("shared", "external");
    }

    // ── git-repo ───────────────────────────────────────────────

    use crate::external::MockGitRunner;

    fn git_intent(name: &str, url: &str, user_path: std::path::PathBuf) -> HandlerIntent {
        HandlerIntent::Fetch {
            pack: "frameworks".into(),
            handler: "external".into(),
            name: name.into(),
            spec: FetchSpec::GitRepo {
                url: url.into(),
                subpath: None,
                git_ref: None,
                commit: None,
            },
            user_path,
        }
    }

    #[test]
    fn git_repo_fresh_clone_runs_once_then_idempotent() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);
        let git = MockGitRunner::new("a".repeat(40).as_str(), b"# omz\n");
        let user_path = env.home.join(".oh-my-zsh");
        let intent = git_intent("omz", "https://x/omz.git", user_path.clone());

        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        )
        .with_git(&git);

        // First run: clone happens.
        let first = executor.execute(vec![intent.clone()]).unwrap();
        assert_eq!(first.len(), 2);
        assert!(first.iter().all(|r| r.success), "{first:#?}");
        assert!(
            git.calls().iter().any(|c| c.starts_with("clone ")),
            "{:?}",
            git.calls()
        );
        assert!(env.fs.is_symlink(&user_path));

        // Second run: clone exists, ls-remote matches local → no fetch.
        let calls_before = git.calls().len();
        let second = executor.execute(vec![intent]).unwrap();
        assert!(second.iter().all(|r| r.success), "{second:#?}");
        assert!(
            second[0].message.contains("fresh"),
            "msg: {}",
            second[0].message
        );
        let calls_after = git.calls();
        // The second run should add ls-remote + local_head, but no
        // fetch / clone.
        let added = &calls_after[calls_before..];
        assert!(
            added.iter().any(|c| c.starts_with("ls-remote ")),
            "{added:?}"
        );
        assert!(added.iter().all(|c| !c.starts_with("clone ")), "{added:?}");
        assert!(
            added.iter().all(|c| !c.starts_with("fetch+reset ")),
            "{added:?}"
        );
    }

    #[test]
    fn git_repo_refreshes_when_upstream_moves() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);
        let git = MockGitRunner::new(&"a".repeat(40), b"v1");
        let user_path = env.home.join(".oh-my-zsh");
        let intent = git_intent("omz", "https://x/omz.git", user_path.clone());

        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        )
        .with_git(&git);

        // Initial clone.
        executor.execute(vec![intent.clone()]).unwrap();

        // Upstream moves; next run must fetch+reset.
        git.set_upstream_sha(&"b".repeat(40));
        let results = executor.execute(vec![intent]).unwrap();
        assert!(results.iter().all(|r| r.success), "{results:#?}");
        assert!(
            git.calls().iter().any(|c| c.starts_with("fetch+reset ")),
            "{:?}",
            git.calls()
        );
        // The marker file should reflect the refresh.
        let datastore_path = env
            .paths
            .handler_data_dir("frameworks", "external")
            .join("omz");
        let content = std::fs::read_to_string(datastore_path.join("README.md")).unwrap();
        assert!(content.contains("# refreshed"), "got: {content:?}");
    }

    #[test]
    fn git_repo_offline_ls_remote_surfaces_failure_keeps_clone() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);
        let git = MockGitRunner::new(&"a".repeat(40), b"# omz\n");
        let user_path = env.home.join(".oh-my-zsh");
        let intent = git_intent("omz", "https://x/omz.git", user_path.clone());

        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        )
        .with_git(&git);

        // Initial clone succeeds.
        executor.execute(vec![intent.clone()]).unwrap();

        // Network goes down. ls-remote fails transiently; we must
        // SURFACE that as a failed OperationResult (claiming "fresh"
        // would lie about an upstream check we couldn't perform), but
        // the cached clone and its symlink stay healthy.
        git.set_ls_remote_offline(true);
        let results = executor.execute(vec![intent]).unwrap();
        assert_eq!(results.len(), 1);
        assert!(!results[0].success, "{results:#?}");
        assert!(
            results[0].message.contains("ls-remote failed"),
            "msg: {}",
            results[0].message
        );
        assert!(
            results[0].message.contains("cached clone"),
            "msg: {}",
            results[0].message
        );
        // Symlink still points at the clone — the previous successful
        // run left it in place and we didn't disturb it.
        assert!(env.fs.is_symlink(&user_path));
    }

    #[test]
    fn git_repo_offline_fetch_after_upstream_move_soft_fails() {
        // Exercise the post-ls-remote, mid-refresh failure path:
        // upstream moved, ls-remote returned a new SHA, but fetch+reset
        // hits a transient error. The cached clone (at the old SHA)
        // stays in place and the result surfaces as non-success.
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);
        let git = MockGitRunner::new(&"a".repeat(40), b"# omz\n");
        let user_path = env.home.join(".oh-my-zsh");
        let intent = git_intent("omz", "https://x/omz.git", user_path.clone());

        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        )
        .with_git(&git);

        // First run clones.
        executor.execute(vec![intent.clone()]).unwrap();

        // Move upstream; fetch fails transiently.
        git.set_upstream_sha(&"b".repeat(40));
        git.set_fetch_offline(true);
        let results = executor.execute(vec![intent]).unwrap();
        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(
            results[0].message.contains("fetch failed") || results[0].message.contains("cached"),
            "msg: {}",
            results[0].message
        );
    }

    fn git_intent_with(
        name: &str,
        url: &str,
        subpath: Option<&str>,
        git_ref: Option<&str>,
        commit: Option<&str>,
        user_path: std::path::PathBuf,
    ) -> HandlerIntent {
        HandlerIntent::Fetch {
            pack: "frameworks".into(),
            handler: "external".into(),
            name: name.into(),
            spec: FetchSpec::GitRepo {
                url: url.into(),
                subpath: subpath.map(String::from),
                git_ref: git_ref.map(String::from),
                commit: commit.map(String::from),
            },
            user_path,
        }
    }

    #[test]
    fn git_repo_subpath_targets_subdirectory() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);
        let git = MockGitRunner::new(&"a".repeat(40), b"# themes\n");
        let user_path = env.home.join(".config/zsh/themes/p10k");
        let intent = git_intent_with(
            "p10k",
            "https://x/p10k.git",
            Some("themes"),
            None,
            None,
            user_path.clone(),
        );

        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        )
        .with_git(&git);

        let results = executor.execute(vec![intent]).unwrap();
        assert!(results.iter().all(|r| r.success), "{results:#?}");

        // The symlink resolves through `themes/` inside the clone.
        assert!(env.fs.is_symlink(&user_path));
        let clone_root = env
            .paths
            .handler_data_dir("frameworks", "external")
            .join("p10k");
        let resolved = env.fs.readlink(&user_path).unwrap();
        assert_eq!(resolved, clone_root.join("themes"));
        // And the marker file (which the mock writes under subpath)
        // is reachable through the symlink.
        let content = env.fs.read_to_string(&user_path.join("README.md")).unwrap();
        assert!(content.contains("# themes"));
    }

    #[test]
    fn git_repo_ref_pin_uses_named_reference() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);
        let git = MockGitRunner::new(&"a".repeat(40), b"# tagged\n");
        let user_path = env.home.join(".oh-my-zsh");
        let intent = git_intent_with(
            "omz",
            "https://x/omz.git",
            None,
            Some("v1.20.0"),
            None,
            user_path.clone(),
        );

        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        )
        .with_git(&git);

        // First run clones --branch v1.20.0.
        executor.execute(vec![intent.clone()]).unwrap();
        assert!(
            git.calls()
                .iter()
                .any(|c| c.contains("clone") && c.contains("ref=v1.20.0")),
            "{:?}",
            git.calls()
        );

        // Second run: ls-remote uses the configured ref, not HEAD.
        let calls_before = git.calls().len();
        executor.execute(vec![intent]).unwrap();
        let new_calls = &git.calls()[calls_before..];
        assert!(
            new_calls
                .iter()
                .any(|c| c.contains("ls-remote") && c.ends_with(" v1.20.0")),
            "{new_calls:?}"
        );
    }

    #[test]
    fn git_repo_commit_pin_skips_ls_remote_when_local_matches() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);
        let commit = "deadbeef1234567890abcdef1234567890abcdef".to_string();
        let git = MockGitRunner::new(&commit, b"# frozen\n");
        let user_path = env.home.join(".oh-my-zsh");
        let intent = git_intent_with(
            "omz",
            "https://x/omz.git",
            None,
            None,
            Some(&commit),
            user_path.clone(),
        );

        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        )
        .with_git(&git);

        // First run clones. When the cloned tip already matches the
        // pin (as the mock arranges via `MockGitRunner::new(&commit)`),
        // the executor's commit-pin shortcut avoids an unnecessary
        // checkout — confirm the clone happened.
        executor.execute(vec![intent.clone()]).unwrap();
        assert!(
            git.calls().iter().any(|c| c.starts_with("clone ")),
            "{:?}",
            git.calls()
        );

        // Second run: local SHA matches pin → no ls-remote, no fetch.
        let calls_before = git.calls().len();
        let results = executor.execute(vec![intent]).unwrap();
        let new_calls = &git.calls()[calls_before..];
        assert!(
            new_calls.iter().all(|c| !c.contains("ls-remote")),
            "commit pin must not poll upstream: {new_calls:?}"
        );
        assert!(
            new_calls.iter().all(|c| !c.contains("fetch+reset")),
            "{new_calls:?}"
        );
        assert!(
            results[0].message.contains("pinned to commit"),
            "msg: {}",
            results[0].message
        );
    }

    #[test]
    fn git_repo_dry_run_does_not_touch_filesystem() {
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);
        let git = MockGitRunner::new(&"a".repeat(40), b"# omz\n");
        let user_path = env.home.join(".oh-my-zsh");
        let intent = git_intent("omz", "https://x/omz.git", user_path.clone());

        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            true,
            false,
            false,
            true,
        )
        .with_git(&git);

        let results = executor.execute(vec![intent]).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert!(
            results[0].message.contains("[dry-run]"),
            "msg: {}",
            results[0].message
        );
        // No clone calls, no symlink, no datastore tree.
        assert!(git.calls().is_empty());
        assert!(!env.fs.exists(&user_path));
        env.assert_no_handler_state("frameworks", "external");
    }

    // ── archive / archive-file ─────────────────────────────────

    fn make_tar_gz_two_files() -> Vec<u8> {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;

        let mut tar_buf: Vec<u8> = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_buf);

            let mut header = tar::Header::new_gnu();
            let body = b"# alpha theme\n";
            header.set_path("themes/alpha.zsh").unwrap();
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, &body[..]).unwrap();

            let mut header = tar::Header::new_gnu();
            let body = b"#!/bin/sh\necho setup\n";
            header.set_path("scripts/setup.sh").unwrap();
            header.set_size(body.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append(&header, &body[..]).unwrap();
            builder.finish().unwrap();
        }
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        gz.write_all(&tar_buf).unwrap();
        gz.finish().unwrap()
    }

    fn archive_intent(
        name: &str,
        url: &str,
        sha256: String,
        user_path: std::path::PathBuf,
    ) -> HandlerIntent {
        HandlerIntent::Fetch {
            pack: "themes".into(),
            handler: "external".into(),
            name: name.into(),
            spec: FetchSpec::Archive {
                url: url.into(),
                sha256,
                format: None,
            },
            user_path,
        }
    }

    fn archive_file_intent(
        name: &str,
        url: &str,
        sha256: String,
        member: &str,
        user_path: std::path::PathBuf,
    ) -> HandlerIntent {
        HandlerIntent::Fetch {
            pack: "themes".into(),
            handler: "external".into(),
            name: name.into(),
            spec: FetchSpec::ArchiveFile {
                url: url.into(),
                sha256,
                member: member.into(),
                format: None,
            },
            user_path,
        }
    }

    #[test]
    fn archive_full_extracts_tree_and_symlinks() {
        let bytes = make_tar_gz_two_files();
        let sha = super::sha256_hex(&bytes);
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);
        let fetcher = MockFetcher::new().with("https://x/p10k.tar.gz", &bytes);
        let user_path = env.home.join(".config/themes/p10k");
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        )
        .with_fetcher(&fetcher);

        let results = executor
            .execute(vec![archive_intent(
                "p10k",
                "https://x/p10k.tar.gz",
                sha,
                user_path.clone(),
            )])
            .unwrap();
        assert!(results.iter().all(|r| r.success), "{results:#?}");
        assert!(env.fs.is_symlink(&user_path));
        // Two files materialised under the entry dir.
        let theme = user_path.join("themes/alpha.zsh");
        let script = user_path.join("scripts/setup.sh");
        assert!(env.fs.exists(&theme));
        assert!(env.fs.exists(&script));
        assert_eq!(env.fs.read_to_string(&theme).unwrap(), "# alpha theme\n");
    }

    #[test]
    fn archive_idempotent_via_sentinel() {
        let bytes = make_tar_gz_two_files();
        let sha = super::sha256_hex(&bytes);
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);
        let fetcher = MockFetcher::new().with("https://x/p10k.tar.gz", &bytes);
        let user_path = env.home.join(".config/themes/p10k");
        let intent = archive_intent("p10k", "https://x/p10k.tar.gz", sha.clone(), user_path);
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        )
        .with_fetcher(&fetcher);

        executor.execute(vec![intent.clone()]).unwrap();
        // Second run: mock only had one canned response, so a second
        // fetch attempt would fail. Sentinel must prevent the fetch.
        let second = executor.execute(vec![intent]).unwrap();
        assert!(second.iter().all(|r| r.success), "{second:#?}");
        assert!(
            second[0].message.contains("fresh"),
            "msg: {}",
            second[0].message
        );
    }

    #[test]
    fn archive_sha256_mismatch_refuses_to_extract() {
        let bytes = make_tar_gz_two_files();
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);
        let fetcher = MockFetcher::new().with("https://x/p10k.tar.gz", &bytes);
        let user_path = env.home.join(".config/themes/p10k");
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        )
        .with_fetcher(&fetcher);

        let results = executor
            .execute(vec![archive_intent(
                "p10k",
                "https://x/p10k.tar.gz",
                "wrong".repeat(13), // 65 chars
                user_path.clone(),
            )])
            .unwrap();
        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(
            results[0].message.contains("sha256 mismatch"),
            "msg: {}",
            results[0].message
        );
        assert!(!env.fs.exists(&user_path));
    }

    #[test]
    fn archive_file_extracts_single_member() {
        let bytes = make_tar_gz_two_files();
        let sha = super::sha256_hex(&bytes);
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);
        let fetcher = MockFetcher::new().with("https://x/p10k.tar.gz", &bytes);
        let user_path = env.home.join("setup.sh");
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        )
        .with_fetcher(&fetcher);

        let results = executor
            .execute(vec![archive_file_intent(
                "setup",
                "https://x/p10k.tar.gz",
                sha,
                "scripts/setup.sh",
                user_path.clone(),
            )])
            .unwrap();
        assert!(results.iter().all(|r| r.success), "{results:#?}");
        assert!(env.fs.is_symlink(&user_path));
        // The deployed file's content matches the archive member.
        assert_eq!(
            env.fs.read_to_string(&user_path).unwrap(),
            "#!/bin/sh\necho setup\n"
        );
    }

    #[test]
    fn archive_file_missing_member_fails_clearly() {
        let bytes = make_tar_gz_two_files();
        let sha = super::sha256_hex(&bytes);
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);
        let fetcher = MockFetcher::new().with("https://x/p10k.tar.gz", &bytes);
        let user_path = env.home.join("doesnt-matter");
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        )
        .with_fetcher(&fetcher);

        let results = executor
            .execute(vec![archive_file_intent(
                "missing",
                "https://x/p10k.tar.gz",
                sha,
                "no/such/path.sh",
                user_path,
            )])
            .unwrap();
        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(
            results[0].message.contains("does not contain member"),
            "msg: {}",
            results[0].message
        );
    }

    #[test]
    fn archive_unknown_format_url_fails_helpfully() {
        let bytes = b"some-bytes".to_vec();
        let sha = super::sha256_hex(&bytes);
        let env = TempEnvironment::builder().build();
        let (ds, _) = make_datastore(&env);
        let fetcher = MockFetcher::new().with("https://x/no-extension", &bytes);
        let user_path = env.home.join("x");
        let executor = Executor::new(
            &ds,
            env.fs.as_ref(),
            env.paths.as_ref(),
            false,
            false,
            false,
            true,
        )
        .with_fetcher(&fetcher);

        let results = executor
            .execute(vec![archive_intent(
                "weird",
                "https://x/no-extension",
                sha,
                user_path,
            )])
            .unwrap();
        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(
            results[0].message.contains("format could not be inferred"),
            "msg: {}",
            results[0].message
        );
    }
}

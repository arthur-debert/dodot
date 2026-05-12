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

use std::path::Path;

use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

use crate::external::FetchSpec;
use crate::operations::{HandlerIntent, Operation, OperationResult};
use crate::Result;

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
            FetchSpec::GitRepo { url } => {
                self.execute_fetch_git_repo(pack, handler, name, url, user_path)
            }
            FetchSpec::Unsupported => Ok(vec![OperationResult::fail(
                fetch_op(pack, handler, name, "<unsupported>"),
                format!(
                    "external '{name}': unsupported type — supported in this release: `file`, `git-repo`"
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
            FetchSpec::GitRepo { url } => {
                let datastore_path = self.paths.handler_data_dir(pack, handler).join(name);
                let already = self.fs.exists(&datastore_path);
                let msg = if already {
                    format!("[dry-run] {name} would ls-remote {url} and refresh only if upstream differs")
                } else {
                    format!(
                        "[dry-run] would clone {url} → {} → {}",
                        datastore_path.display(),
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

    /// Fetch one `type = "git-repo"` external.
    ///
    /// Upstream HEAD is the freshness oracle. Flow:
    /// 1. Compute the datastore path: `<handler_data_dir>/<name>`.
    /// 2. If the path doesn't exist → shallow-clone fresh.
    /// 3. Otherwise → `git ls-remote HEAD` on the upstream URL. If
    ///    it matches the local clone's HEAD, no-op. If it differs,
    ///    fetch + reset --hard.
    /// 4. Make sure the user-visible symlink target → datastore path.
    ///
    /// Network failures (ls-remote, fetch, even initial clone) are
    /// soft: the cached clone (if any) stays put and the result
    /// surfaces as a non-success — `up` continues with the rest of
    /// the pack.
    fn execute_fetch_git_repo(
        &self,
        pack: &str,
        handler: &str,
        name: &str,
        url: &str,
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

        let datastore_path = self.paths.handler_data_dir(pack, handler).join(name);
        let already_cloned = self.fs.exists(&datastore_path);

        if !already_cloned {
            // Fresh clone path. The datastore dir's parent must exist.
            if let Some(parent) = datastore_path.parent() {
                self.fs.mkdir_all(parent)?;
            }
            info!(pack, name, url, "shallow-cloning external");
            let sha = match git.shallow_clone(url, &datastore_path) {
                Ok(s) => s,
                Err(err) => {
                    warn!(pack, name, %err, "git clone failed");
                    return Ok(vec![OperationResult::fail(
                        op(),
                        format!("{name}: clone failed: {err}"),
                    )]);
                }
            };
            return self.finish_git_repo(
                pack,
                handler,
                name,
                url,
                &datastore_path,
                user_path,
                &sha,
            );
        }

        // Existing clone path. We need the local HEAD regardless of
        // whether ls-remote succeeds, so read it first — a corrupted
        // local clone is a hard error that should surface even if
        // upstream happens to be reachable.
        let local = match git.local_head(&datastore_path) {
            Ok(s) => s,
            Err(err) => {
                // The clone exists but git can't read it — corrupted
                // state. Better to surface than to silently re-clone
                // (which could mask a deeper problem).
                return Ok(vec![OperationResult::fail(
                    op(),
                    format!(
                        "{name}: existing clone at {} is unreadable: {err}",
                        datastore_path.display()
                    ),
                )]);
            }
        };

        // Cheap freshness check. Transient ls-remote failures don't
        // abort the run, but they DO surface as non-success results —
        // claiming "fresh" when we couldn't actually verify upstream
        // would be misleading. The cached clone is already symlinked
        // from a prior successful run; nothing more needed on disk.
        let upstream = match git.ls_remote_head(url) {
            Ok(s) => s,
            Err(err) if err.is_transient() => {
                warn!(pack, name, %err, "ls-remote failed (transient); using cached clone");
                return Ok(vec![OperationResult::fail(
                    op(),
                    format!(
                        "{name}: ls-remote failed ({err}); using cached clone at {}",
                        short(&local)
                    ),
                )]);
            }
            Err(err) => {
                return Ok(vec![OperationResult::fail(
                    op(),
                    format!("{name}: ls-remote failed: {err}"),
                )]);
            }
        };

        // From here on, upstream and local are both known SHAs.
        let (final_sha, was_refreshed) = if upstream == self.force_refresh_target(&local) {
            (local, false)
        } else {
            info!(
                pack, name,
                local = %short(&local),
                remote = %short(&upstream),
                "upstream moved; fetching + reset"
            );
            match git.fetch_and_reset(&datastore_path) {
                Ok(s) => (s, true),
                Err(err) if err.is_transient() => {
                    warn!(pack, name, %err, "fetch+reset failed (transient); keeping cached");
                    // Cached SHA stands; we tried to refresh but the
                    // network blipped after ls-remote succeeded.
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

        // Refresh sentinel + symlink either way (idempotent).
        let mut results = self.finish_git_repo(
            pack,
            handler,
            name,
            url,
            &datastore_path,
            user_path,
            &final_sha,
        )?;
        if !was_refreshed && self.fetcher_message_overridable(&results) {
            // Only rewrite the "fetched" message when we actually
            // know upstream matches local — i.e. ls-remote succeeded
            // and the SHAs lined up. The transient-failure path
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
    #[allow(clippy::too_many_arguments)]
    fn finish_git_repo(
        &self,
        pack: &str,
        handler: &str,
        name: &str,
        url: &str,
        datastore_path: &Path,
        user_path: &Path,
        sha: &str,
    ) -> Result<Vec<OperationResult>> {
        self.create_external_user_link(datastore_path, user_path)?;
        let sentinel = git_repo_sentinel(name, sha);
        self.write_sentinel(pack, handler, &sentinel)?;

        let create_link = Operation::CreateUserLink {
            pack: pack.to_string(),
            handler: handler.to_string(),
            datastore_path: datastore_path.to_path_buf(),
            user_path: user_path.to_path_buf(),
        };
        Ok(vec![
            OperationResult::ok(
                fetch_op(pack, handler, name, url),
                format!(
                    "{name}: HEAD={} at {}",
                    short(sha),
                    datastore_path.display()
                ),
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
            spec: FetchSpec::GitRepo { url: url.into() },
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
}

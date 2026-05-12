//! `Fetch` intent: pull an external resource into the datastore and
//! create the user-visible symlink that exposes it.
//!
//! Sentinel posture mirrors the install handler: the entry's content
//! signature (for `file`, the configured sha256) is the sentinel
//! payload. Re-running `up` with the same sha256 is a no-op. Bumping
//! the sha256 in `externals.toml` invalidates the old sentinel, so the
//! file is re-fetched and re-verified.
//!
//! Failure posture:
//! - **Integrity failure** (sha256 mismatch) is fatal — we refuse to
//!   write tampered content into the datastore.
//! - **Network failure** is soft — if a cached copy is present we leave
//!   it in place and report the failure as a non-success result; other
//!   intents still execute.

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
            FetchSpec::Unsupported => Ok(vec![OperationResult::fail(
                fetch_op(pack, handler, name, "<unsupported>"),
                format!(
                    "external '{name}': unsupported type — only `type = \"file\"` is implemented in this release"
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

        // Sentinel hit: content matching this sha256 has already been
        // fetched and deployed. Mirror the install-handler posture and
        // no-op silently unless --force is set.
        if !self.force && self.datastore.has_sentinel(pack, handler, &sentinel)? {
            debug!(pack, name, "external sentinel matches; skipping fetch");
            return Ok(vec![OperationResult::ok(
                op(),
                format!("{name}: fresh (sha256 matches)"),
            )]);
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
        let filename = filename_for_target(user_path);
        let rel = format!("{name}/{filename}");
        let datastore_path = self
            .datastore
            .write_rendered_file(pack, handler, &rel, &bytes)?;
        debug!(pack, name, datastore = %datastore_path.display(), "wrote external to datastore");

        // Symlink the user-visible target → datastore copy.
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

    /// Symlink leg of a Fetch: similar to `execute_link` but the
    /// "source" is already in the datastore (we just wrote it), so
    /// only the user-visible leg is needed.
    fn create_external_user_link(&self, datastore_path: &Path, user_path: &Path) -> Result<()> {
        if self.fs.is_symlink(user_path) {
            // Idempotent: refresh to point at the (possibly new)
            // datastore path. `create_user_link` already handles
            // wrong-target replacement.
            self.datastore.create_user_link(datastore_path, user_path)
        } else if self.fs.exists(user_path) {
            if self.force {
                if self.fs.is_dir(user_path) {
                    self.fs.remove_dir_all(user_path)?;
                } else {
                    self.fs.remove_file(user_path)?;
                }
                self.datastore.create_user_link(datastore_path, user_path)
            } else {
                Err(crate::DodotError::SymlinkConflict {
                    path: user_path.to_path_buf(),
                })
            }
        } else {
            self.datastore.create_user_link(datastore_path, user_path)
        }
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
}

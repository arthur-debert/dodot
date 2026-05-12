//! Git operations for `type = "git-repo"` externals.
//!
//! Shell-out to the user's `git` binary is the simplest design that
//! supports shallow clones (`--depth=1 --filter=blob:none`) and
//! sparse-tree fetch — both features the issue explicitly requires
//! and that the pure-Rust gitoxide crates don't yet expose at a
//! porcelain level. The dependency on a system `git` is a reasonable
//! prerequisite for dodot users (they're managing dotfiles, after all).
//!
//! The trait abstraction exists so tests don't have to network out to
//! real repos. Tests use [`MockGitRunner`] which records calls and
//! returns canned SHAs.

use std::path::Path;
use std::process::Command;

/// Error category returned by a git runner.
///
/// The executor distinguishes transient (network / `ls-remote` /
/// fetch) failures from misconfiguration so a temporary network blip
/// doesn't kill the whole `up` invocation.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// Could not invoke `git` at all — probably missing from PATH.
    #[error("`git` not found on PATH: {0}")]
    NotFound(String),
    /// `git` ran but failed. `stderr` carries the actual message.
    #[error("git {operation} failed (exit {exit_code}): {stderr}")]
    CommandFailed {
        operation: String,
        exit_code: i32,
        stderr: String,
    },
    /// Output was structurally not what we expected (e.g. ls-remote
    /// returned 0 rows for HEAD).
    #[error("git {operation} produced unexpected output: {detail}")]
    BadOutput { operation: String, detail: String },
}

impl GitError {
    /// Is this a network-style failure that should soft-fail with a
    /// cached copy rather than abort `up`?
    pub fn is_transient(&self) -> bool {
        match self {
            // `git` itself is missing → user has to fix it; never soft-fail.
            Self::NotFound(_) => false,
            // Command failures during network ops are almost always
            // transient (DNS, auth blip, server flake). Misuse of git
            // is the runner's bug, not the user's, so we still surface
            // it but treat it as transient — the cached clone stays.
            Self::CommandFailed { .. } => true,
            // ls-remote returning nothing usually means upstream is
            // unreachable or moved; treat as transient.
            Self::BadOutput { .. } => true,
        }
    }
}

/// Abstraction over the small handful of git operations dodot needs.
///
/// Each method is a porcelain-level verb: implementations shell out
/// to `git` with the right flags. Tests mock this trait.
pub trait GitRunner: Send + Sync {
    /// `git ls-remote <url> <reference>` — return the upstream SHA
    /// for a reference without fetching any objects. Use `"HEAD"`
    /// for the default branch.
    fn ls_remote(&self, url: &str, reference: &str) -> std::result::Result<String, GitError>;

    /// `git clone --depth=1 --filter=blob:none [--branch <ref>] <url> <dest>`,
    /// optionally followed by a sparse-checkout setup if `subpath` is
    /// `Some(p)`:
    /// ```text
    /// git -C <dest> sparse-checkout init --cone
    /// git -C <dest> sparse-checkout set <p>
    /// ```
    /// Returns the cloned HEAD SHA.
    fn shallow_clone(
        &self,
        url: &str,
        dest: &Path,
        reference: Option<&str>,
        subpath: Option<&str>,
    ) -> std::result::Result<String, GitError>;

    /// `git -C <repo> fetch --depth=1 origin <reference>` followed
    /// by `git -C <repo> reset --hard FETCH_HEAD`. Returns the new
    /// HEAD SHA.
    fn fetch_and_reset(
        &self,
        repo: &Path,
        reference: &str,
    ) -> std::result::Result<String, GitError>;

    /// `git -C <repo> checkout <target>`. Used for commit pinning
    /// when the local clone already has the object.
    fn checkout(&self, repo: &Path, target: &str) -> std::result::Result<(), GitError>;

    /// `git -C <repo> rev-parse HEAD` — local HEAD SHA.
    fn local_head(&self, repo: &Path) -> std::result::Result<String, GitError>;

    /// `git -C <repo> status --porcelain`. Empty output means the
    /// working tree is clean; any non-empty body means the tree
    /// diverged from HEAD. Used by drift detection.
    fn status_porcelain(&self, repo: &Path) -> std::result::Result<String, GitError>;
}

/// Production `git` runner: actually shells out.
pub struct ShellGitRunner;

impl ShellGitRunner {
    pub fn new() -> Self {
        Self
    }

    fn run(operation: &str, cmd: &mut Command) -> std::result::Result<String, GitError> {
        let output = cmd.output().map_err(|e| match e.kind() {
            // Only "no such binary" gets the NotFound diagnostic —
            // permission-denied, ENOMEM, etc. shouldn't masquerade as
            // "user has to install git". Map those to CommandFailed
            // with the kernel's reason so `is_transient()` can still
            // route them through the soft-fail path.
            std::io::ErrorKind::NotFound => GitError::NotFound(e.to_string()),
            _ => GitError::CommandFailed {
                operation: operation.to_string(),
                exit_code: -1,
                stderr: format!("spawn failed: {e}"),
            },
        })?;
        if !output.status.success() {
            return Err(GitError::CommandFailed {
                operation: operation.to_string(),
                exit_code: output.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

impl Default for ShellGitRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl GitRunner for ShellGitRunner {
    fn ls_remote(&self, url: &str, reference: &str) -> std::result::Result<String, GitError> {
        let stdout = Self::run(
            "ls-remote",
            Command::new("git").args(["ls-remote", "--exit-code", url, reference]),
        )?;
        // Output is one `<sha>\t<ref>` row per matching ref. For an
        // annotated tag, `git ls-remote <tag>` returns two rows:
        //   <tag-object-sha>     refs/tags/<tag>
        //   <commit-sha>         refs/tags/<tag>^{}
        // The dereferenced (`^{}`) row is the commit we actually want
        // to track; the tag-object SHA changes whenever the tag's
        // message/signer changes, which would cause spurious refresh
        // loops. Prefer `^{}` when present, fall back to the first
        // row otherwise.
        let rows: Vec<(&str, &str)> = stdout
            .lines()
            .filter_map(|line| {
                let mut parts = line.split_whitespace();
                let sha = parts.next()?;
                let refname = parts.next().unwrap_or("");
                Some((sha, refname))
            })
            .collect();
        if rows.is_empty() {
            return Err(GitError::BadOutput {
                operation: "ls-remote".into(),
                detail: format!("no rows for {reference} on {url}"),
            });
        }
        let chosen = rows
            .iter()
            .find(|(_, refname)| refname.ends_with("^{}"))
            .or_else(|| rows.first())
            .expect("rows is non-empty");
        let sha = chosen.0.to_string();
        if sha.len() < 7 {
            return Err(GitError::BadOutput {
                operation: "ls-remote".into(),
                detail: format!("sha too short: {sha:?}"),
            });
        }
        Ok(sha)
    }

    fn shallow_clone(
        &self,
        url: &str,
        dest: &Path,
        reference: Option<&str>,
        subpath: Option<&str>,
    ) -> std::result::Result<String, GitError> {
        // Build the clone invocation. Non-path args go through `.args`
        // as `&str`; `dest` is appended via `.arg(&Path)` so non-UTF-8
        // paths survive verbatim (`to_string_lossy` would corrupt them).
        let mut cmd = Command::new("git");
        cmd.args(["clone", "--depth=1", "--filter=blob:none"]);
        if let Some(r) = reference {
            // --single-branch is implied by --depth=1, but be explicit.
            cmd.args(["--branch", r, "--single-branch"]);
        }
        // `--no-checkout` only when a sparse pattern follows so we
        // don't materialise the whole tree before narrowing it.
        if subpath.is_some() {
            cmd.arg("--no-checkout");
        }
        cmd.arg("--").arg(url).arg(dest);
        Self::run("clone", &mut cmd)?;

        if let Some(pattern) = subpath {
            Self::run(
                "sparse-checkout init",
                Command::new("git")
                    .arg("-C")
                    .arg(dest)
                    .args(["sparse-checkout", "init", "--cone"]),
            )?;
            // `--` keeps a pattern that starts with `-` (e.g.
            // `-experimental`) from being interpreted as an option.
            Self::run(
                "sparse-checkout set",
                Command::new("git").arg("-C").arg(dest).args([
                    "sparse-checkout",
                    "set",
                    "--",
                    pattern,
                ]),
            )?;
            Self::run(
                "checkout",
                Command::new("git").arg("-C").arg(dest).args(["checkout"]),
            )?;
        }
        self.local_head(dest)
    }

    fn fetch_and_reset(
        &self,
        repo: &Path,
        reference: &str,
    ) -> std::result::Result<String, GitError> {
        Self::run(
            "fetch",
            Command::new("git").arg("-C").arg(repo).args([
                "fetch",
                "--depth=1",
                "origin",
                reference,
            ]),
        )?;
        Self::run(
            "reset",
            Command::new("git")
                .arg("-C")
                .arg(repo)
                .args(["reset", "--hard", "FETCH_HEAD"]),
        )?;
        self.local_head(repo)
    }

    fn checkout(&self, repo: &Path, target: &str) -> std::result::Result<(), GitError> {
        Self::run(
            "checkout",
            Command::new("git")
                .arg("-C")
                .arg(repo)
                .args(["checkout", target]),
        )?;
        Ok(())
    }

    fn local_head(&self, repo: &Path) -> std::result::Result<String, GitError> {
        Self::run(
            "rev-parse",
            Command::new("git")
                .arg("-C")
                .arg(repo)
                .args(["rev-parse", "HEAD"]),
        )
    }

    fn status_porcelain(&self, repo: &Path) -> std::result::Result<String, GitError> {
        Self::run(
            "status",
            Command::new("git")
                .arg("-C")
                .arg(repo)
                .args(["status", "--porcelain"]),
        )
    }
}

/// Mock GitRunner for tests. Records call sites and returns canned
/// SHAs. Exposed so tests in sibling modules (`execution::fetch`) can
/// reuse it without duplication.
#[cfg(any(test, feature = "test-utils"))]
pub struct MockGitRunner {
    inner: std::sync::Mutex<MockGitInner>,
}

#[cfg(any(test, feature = "test-utils"))]
struct MockGitInner {
    /// Upstream SHA returned by `ls_remote_head`. None = error.
    pub ls_remote_sha: Option<String>,
    /// SHA recorded by the last clone / fetch+reset / used by
    /// `local_head` to answer subsequent queries.
    pub local_sha: Option<String>,
    /// Whether ls_remote_head should fail with a transient error
    /// to exercise the offline-tolerant path.
    pub ls_remote_offline: bool,
    /// Whether fetch_and_reset should fail.
    pub fetch_offline: bool,
    /// Canned `git status --porcelain` output. Empty = clean.
    /// Used by drift-detection tests so they don't have to drive
    /// real git invocations.
    pub status_porcelain: String,
    pub calls: Vec<String>,
    /// Per-clone marker file written into the destination so tests
    /// can confirm the executor actually "produced" a clone tree.
    pub clone_marker_content: Vec<u8>,
}

#[cfg(any(test, feature = "test-utils"))]
impl MockGitRunner {
    pub fn new(upstream_sha: &str, clone_marker: &[u8]) -> Self {
        Self {
            inner: std::sync::Mutex::new(MockGitInner {
                ls_remote_sha: Some(upstream_sha.into()),
                local_sha: None,
                ls_remote_offline: false,
                fetch_offline: false,
                status_porcelain: String::new(),
                calls: Vec::new(),
                clone_marker_content: clone_marker.to_vec(),
            }),
        }
    }

    /// Plant canned `git status --porcelain` output so drift tests
    /// can simulate modified / clean working trees without driving
    /// real git.
    pub fn set_status_porcelain(&self, output: &str) {
        let mut g = self.inner.lock().unwrap();
        g.status_porcelain = output.into();
    }

    /// Replace the upstream SHA — used to simulate a remote update
    /// between two `up` runs.
    pub fn set_upstream_sha(&self, sha: &str) {
        let mut g = self.inner.lock().unwrap();
        g.ls_remote_sha = Some(sha.into());
    }

    /// Force `ls_remote_head` to fail transiently (network down).
    pub fn set_ls_remote_offline(&self, offline: bool) {
        let mut g = self.inner.lock().unwrap();
        g.ls_remote_offline = offline;
    }

    /// Force `fetch_and_reset` to fail transiently.
    pub fn set_fetch_offline(&self, offline: bool) {
        let mut g = self.inner.lock().unwrap();
        g.fetch_offline = offline;
    }

    pub fn calls(&self) -> Vec<String> {
        self.inner.lock().unwrap().calls.clone()
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl GitRunner for MockGitRunner {
    fn ls_remote(&self, url: &str, reference: &str) -> std::result::Result<String, GitError> {
        let mut g = self.inner.lock().unwrap();
        g.calls.push(format!("ls-remote {url} {reference}"));
        if g.ls_remote_offline {
            return Err(GitError::CommandFailed {
                operation: "ls-remote".into(),
                exit_code: 1,
                stderr: "simulated offline".into(),
            });
        }
        g.ls_remote_sha.clone().ok_or_else(|| GitError::BadOutput {
            operation: "ls-remote".into(),
            detail: "mock returned no SHA".into(),
        })
    }

    fn shallow_clone(
        &self,
        url: &str,
        dest: &Path,
        reference: Option<&str>,
        subpath: Option<&str>,
    ) -> std::result::Result<String, GitError> {
        let mut g = self.inner.lock().unwrap();
        g.calls.push(format!(
            "clone {url} ref={} subpath={} -> {}",
            reference.unwrap_or("HEAD"),
            subpath.unwrap_or("-"),
            dest.display()
        ));
        std::fs::create_dir_all(dest).map_err(|e| GitError::CommandFailed {
            operation: "clone".into(),
            exit_code: -1,
            stderr: e.to_string(),
        })?;
        // When subpath is set, materialise the marker inside that
        // subdir so symlink-into-subpath cases are exercised end-to-end.
        let marker_dir = match subpath {
            Some(p) => {
                let d = dest.join(p);
                std::fs::create_dir_all(&d).map_err(|e| GitError::CommandFailed {
                    operation: "sparse-checkout".into(),
                    exit_code: -1,
                    stderr: e.to_string(),
                })?;
                d
            }
            None => dest.to_path_buf(),
        };
        let marker = marker_dir.join("README.md");
        std::fs::write(&marker, &g.clone_marker_content).map_err(|e| GitError::CommandFailed {
            operation: "clone".into(),
            exit_code: -1,
            stderr: e.to_string(),
        })?;
        let sha = g
            .ls_remote_sha
            .clone()
            .unwrap_or_else(|| "0000000000000000000000000000000000000000".into());
        g.local_sha = Some(sha.clone());
        Ok(sha)
    }

    fn fetch_and_reset(
        &self,
        repo: &Path,
        reference: &str,
    ) -> std::result::Result<String, GitError> {
        let mut g = self.inner.lock().unwrap();
        g.calls
            .push(format!("fetch+reset {} ref={reference}", repo.display()));
        if g.fetch_offline {
            return Err(GitError::CommandFailed {
                operation: "fetch".into(),
                exit_code: 1,
                stderr: "simulated offline".into(),
            });
        }
        // Bump the marker file so callers can verify the tree
        // actually changed after a refresh.
        let marker = repo.join("README.md");
        let mut buf = g.clone_marker_content.clone();
        buf.extend_from_slice(b"\n# refreshed");
        let _ = std::fs::write(&marker, &buf);
        let sha = g
            .ls_remote_sha
            .clone()
            .unwrap_or_else(|| "1111111111111111111111111111111111111111".into());
        g.local_sha = Some(sha.clone());
        Ok(sha)
    }

    fn checkout(&self, repo: &Path, target: &str) -> std::result::Result<(), GitError> {
        let mut g = self.inner.lock().unwrap();
        g.calls
            .push(format!("checkout {} {target}", repo.display()));
        g.local_sha = Some(target.into());
        Ok(())
    }

    fn local_head(&self, _repo: &Path) -> std::result::Result<String, GitError> {
        let mut g = self.inner.lock().unwrap();
        g.calls.push("rev-parse".into());
        g.local_sha.clone().ok_or_else(|| GitError::BadOutput {
            operation: "rev-parse".into(),
            detail: "mock has no local sha (clone wasn't called)".into(),
        })
    }

    fn status_porcelain(&self, repo: &Path) -> std::result::Result<String, GitError> {
        let mut g = self.inner.lock().unwrap();
        g.calls.push(format!("status {}", repo.display()));
        Ok(g.status_porcelain.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_classification() {
        assert!(GitError::CommandFailed {
            operation: "fetch".into(),
            exit_code: 128,
            stderr: "dns failure".into(),
        }
        .is_transient());
        assert!(GitError::BadOutput {
            operation: "ls-remote".into(),
            detail: "x".into(),
        }
        .is_transient());
        assert!(!GitError::NotFound("missing".into()).is_transient());
    }

    #[test]
    fn mock_clone_writes_marker_and_records_sha() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("clone");
        let mock = MockGitRunner::new("abc123def456", b"# hello\n");
        let sha = mock
            .shallow_clone("https://example.com/repo.git", &dest, None, None)
            .unwrap();
        assert_eq!(sha, "abc123def456");
        assert!(dest.join("README.md").exists());
        assert_eq!(mock.local_head(&dest).unwrap(), "abc123def456");
    }

    #[test]
    fn mock_clone_subpath_places_marker_inside_subpath() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("clone");
        let mock = MockGitRunner::new("abc", b"# theme\n");
        mock.shallow_clone("https://x/r.git", &dest, None, Some("themes"))
            .unwrap();
        // Marker is inside the subpath, not at the repo root —
        // confirms sparse-checkout was honoured by the mock.
        assert!(dest.join("themes/README.md").exists());
        assert!(!dest.join("README.md").exists());
    }

    #[test]
    fn mock_fetch_updates_marker_and_sha() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("clone");
        let mock = MockGitRunner::new("aaa", b"# v1\n");
        mock.shallow_clone("https://x/r.git", &dest, None, None)
            .unwrap();
        mock.set_upstream_sha("bbb");
        let new_sha = mock.fetch_and_reset(&dest, "HEAD").unwrap();
        assert_eq!(new_sha, "bbb");
        let body = std::fs::read_to_string(dest.join("README.md")).unwrap();
        assert!(body.contains("# refreshed"));
    }

    #[test]
    fn mock_checkout_updates_local_sha() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("clone");
        let mock = MockGitRunner::new("abc", b"");
        mock.shallow_clone("https://x/r.git", &dest, None, None)
            .unwrap();
        mock.checkout(&dest, "frozen-commit-sha").unwrap();
        assert_eq!(mock.local_head(&dest).unwrap(), "frozen-commit-sha");
    }

    #[test]
    fn mock_offline_ls_remote() {
        let mock = MockGitRunner::new("aaa", b"");
        mock.set_ls_remote_offline(true);
        let err = mock.ls_remote("https://x/r.git", "HEAD").unwrap_err();
        assert!(err.is_transient());
    }
}

//! Drift detection for externals.
//!
//! Drift answers a different question than upstream-freshness: it
//! asks "did the user edit the deployed copy?" rather than "did
//! upstream move?" The deployed location is a symlink into the
//! datastore, so an edit lands on the datastore copy — which the
//! handler will clobber on the next refresh.
//!
//! Drift detection is opt-in (`dodot status --check-drift`) because
//! a thorough check involves hashing every deployed file, which can
//! be expensive for big trees (think `oh-my-zsh`).
//!
//! Per-type strategy (PR 5):
//! - **file** — compute sha256 of the datastore copy and compare
//!   against the entry's configured sha256. Fast.
//! - **git-repo** — `git -C <clone> status --porcelain`. Any output
//!   means the working tree diverged from HEAD.
//! - **archive** / **archive-file** — drift check deferred. The
//!   report surfaces a "not implemented" note rather than a false
//!   negative, so users know the silence isn't a clean bill of
//!   health.

use std::path::PathBuf;

use crate::external::{FetchSpec, GitRunner};
use crate::fs::Fs;
use crate::paths::Pather;
use crate::Result;

/// One row of `--check-drift` output.
#[derive(Debug, Clone)]
pub struct DriftReport {
    pub pack: String,
    pub entry_name: String,
    pub kind: DriftKind,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriftKind {
    /// No drift detected.
    Clean,
    /// Deployed content differs from what dodot wrote.
    Drifted,
    /// Drift detection isn't implemented for this entry type yet.
    /// Surfaced explicitly so the user knows the absence of a
    /// Drifted row is not the same as "definitely clean."
    NotImplemented,
    /// The deployed copy is gone (user deleted the file/clone).
    Missing,
    /// We tried to assess drift but the check itself errored —
    /// e.g. `git status --porcelain` failed because the runner
    /// said `git` isn't on PATH, or a tarball became unreadable.
    /// Distinct from `Missing` (which means the artifact is gone)
    /// and from `NotImplemented` (which means we never tried).
    CheckFailed,
}

impl DriftReport {
    pub fn is_drifted(&self) -> bool {
        matches!(self.kind, DriftKind::Drifted)
    }
}

/// Build a `DriftReport` for every entry declared in `externals.toml`
/// for the given pack. Pure read-only; safe to call from `status`.
///
/// `git` is optional — without it, `git-repo` entries report
/// [`DriftKind::NotImplemented`] (matches what the executor does when
/// no GitRunner is configured).
pub fn detect_drift_for_pack(
    pack: &str,
    externals_toml_bytes: &[u8],
    paths: &dyn Pather,
    fs: &dyn Fs,
    git: Option<&dyn GitRunner>,
) -> Result<Vec<DriftReport>> {
    let parsed = crate::external::parse_externals_toml(externals_toml_bytes)?;
    let mut reports = Vec::with_capacity(parsed.entries.len());
    let handler_dir = paths.handler_data_dir(pack, crate::handlers::HANDLER_EXTERNAL);

    for (name, entry) in parsed.entries {
        let datastore_entry = handler_dir.join(&name);
        let report = match &entry.spec {
            FetchSpec::File { sha256, .. } => {
                check_file_drift(pack, &name, &datastore_entry, sha256, &entry.target, fs)
            }
            FetchSpec::GitRepo { .. } => match git {
                Some(g) => check_git_drift(pack, &name, &datastore_entry, g, fs),
                None => DriftReport {
                    pack: pack.into(),
                    entry_name: name.clone(),
                    kind: DriftKind::NotImplemented,
                    detail: "git runner not configured".into(),
                },
            },
            FetchSpec::Archive { .. } | FetchSpec::ArchiveFile { .. } => DriftReport {
                pack: pack.into(),
                entry_name: name.clone(),
                kind: DriftKind::NotImplemented,
                detail: "drift detection for archive entries lands in a later release".into(),
            },
            FetchSpec::Unsupported => DriftReport {
                pack: pack.into(),
                entry_name: name.clone(),
                kind: DriftKind::NotImplemented,
                detail: "unsupported entry type".into(),
            },
        };
        reports.push(report);
    }
    Ok(reports)
}

/// Derive the datastore basename for a `type = "file"` entry from its
/// `target = "~/..."` field. Mirrors `filename_for_target` in
/// `crate::execution::fetch` so the check looks at the exact path the
/// executor wrote — picking the first file in the entry directory is
/// nondeterministic when stale siblings linger.
fn target_basename(target: &str) -> String {
    std::path::Path::new(target)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "content".into())
}

fn check_file_drift(
    pack: &str,
    name: &str,
    datastore_entry_dir: &std::path::Path,
    expected_sha256: &str,
    target: &str,
    fs: &dyn Fs,
) -> DriftReport {
    // The file lives at `<entry-dir>/<basename-of-target>`. Resolve
    // the exact path the executor would have written rather than
    // scanning the dir (which can hold stale siblings and produce a
    // nondeterministic answer).
    let basename = target_basename(target);
    let file_path: PathBuf = datastore_entry_dir.join(&basename);
    if !fs.exists(&file_path) {
        return DriftReport {
            pack: pack.into(),
            entry_name: name.into(),
            kind: DriftKind::Missing,
            detail: format!("no deployed file at {}", file_path.display()),
        };
    }
    let bytes = match fs.read_file(&file_path) {
        Ok(b) => b,
        Err(e) => {
            return DriftReport {
                pack: pack.into(),
                entry_name: name.into(),
                kind: DriftKind::CheckFailed,
                detail: format!("cannot read {}: {e}", file_path.display()),
            };
        }
    };
    let actual = hex_sha256(&bytes);
    if actual.eq_ignore_ascii_case(expected_sha256) {
        DriftReport {
            pack: pack.into(),
            entry_name: name.into(),
            kind: DriftKind::Clean,
            detail: format!("sha256 matches ({})", short(&actual)),
        }
    } else {
        DriftReport {
            pack: pack.into(),
            entry_name: name.into(),
            kind: DriftKind::Drifted,
            detail: format!(
                "deployed sha256 {} ≠ configured {}",
                short(&actual),
                short(expected_sha256)
            ),
        }
    }
}

fn check_git_drift(
    pack: &str,
    name: &str,
    clone_path: &std::path::Path,
    git: &dyn GitRunner,
    fs: &dyn Fs,
) -> DriftReport {
    if !fs.exists(clone_path) {
        return DriftReport {
            pack: pack.into(),
            entry_name: name.into(),
            kind: DriftKind::Missing,
            detail: format!("clone missing at {}", clone_path.display()),
        };
    }
    // `git status --porcelain` is the drift oracle. We route through
    // the GitRunner trait so tests can mock it and so missing-git
    // errors flow through the same classification as the rest of the
    // git path.
    match git.status_porcelain(clone_path) {
        Ok(out) if out.is_empty() => DriftReport {
            pack: pack.into(),
            entry_name: name.into(),
            kind: DriftKind::Clean,
            detail: "no local modifications".into(),
        },
        Ok(out) => DriftReport {
            pack: pack.into(),
            entry_name: name.into(),
            kind: DriftKind::Drifted,
            detail: format!("{} modified path(s)", out.lines().count()),
        },
        Err(e) => DriftReport {
            pack: pack.into(),
            entry_name: name.into(),
            kind: DriftKind::CheckFailed,
            detail: format!("git status failed: {e}"),
        },
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    h.update(bytes);
    let digest = h.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn short(sha: &str) -> String {
    sha.chars().take(16).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempEnvironment;

    fn write_datastore_file(
        env: &TempEnvironment,
        pack: &str,
        entry: &str,
        basename: &str,
        body: &[u8],
    ) -> PathBuf {
        let dir = env
            .paths
            .handler_data_dir(pack, crate::handlers::HANDLER_EXTERNAL)
            .join(entry);
        env.fs.mkdir_all(&dir).unwrap();
        let path = dir.join(basename);
        env.fs.write_file(&path, body).unwrap();
        path
    }

    fn file_externals_toml(name: &str, sha256: &str) -> String {
        // Target basename matches `name` so the deployed file the
        // tests write at `<entry>/<name>` is what drift inspects.
        format!(
            r#"
[{name}]
type   = "file"
url    = "https://example.com/x"
target = "~/{name}"
sha256 = "{sha256}"
"#
        )
    }

    #[test]
    fn file_drift_clean_when_sha_matches() {
        let env = TempEnvironment::builder().build();
        let body = b"hello world";
        let sha = hex_sha256(body);
        write_datastore_file(&env, "p", "x", "x", body);
        let toml = file_externals_toml("x", &sha);
        let reports = detect_drift_for_pack(
            "p",
            toml.as_bytes(),
            env.paths.as_ref(),
            env.fs.as_ref(),
            None,
        )
        .unwrap();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].kind, DriftKind::Clean);
    }

    #[test]
    fn file_drift_detected_when_user_edits_deployed_copy() {
        let env = TempEnvironment::builder().build();
        let configured_sha = hex_sha256(b"original content");
        // User overwrote the deployed copy with something else.
        write_datastore_file(&env, "p", "x", "x", b"tampered content");
        let toml = file_externals_toml("x", &configured_sha);
        let reports = detect_drift_for_pack(
            "p",
            toml.as_bytes(),
            env.paths.as_ref(),
            env.fs.as_ref(),
            None,
        )
        .unwrap();
        assert_eq!(reports[0].kind, DriftKind::Drifted);
        assert!(reports[0].detail.contains("deployed sha256"));
    }

    #[test]
    fn file_drift_ignores_stale_siblings_in_entry_dir() {
        // A previous deploy left a sibling file in the entry dir
        // (target rename, manual cleanup gone wrong). Drift must
        // inspect the entry whose basename matches the *current*
        // target — not whichever file happens to come first in the
        // directory listing.
        let env = TempEnvironment::builder().build();
        let body = b"current content";
        let sha = hex_sha256(body);
        // Sibling that would lexically sort first and previously
        // confused the check.
        write_datastore_file(&env, "p", "x", "0-old-name", b"stale");
        write_datastore_file(&env, "p", "x", "x", body);
        let toml = file_externals_toml("x", &sha);
        let reports = detect_drift_for_pack(
            "p",
            toml.as_bytes(),
            env.paths.as_ref(),
            env.fs.as_ref(),
            None,
        )
        .unwrap();
        assert_eq!(reports[0].kind, DriftKind::Clean);
    }

    #[test]
    fn missing_deployed_file_reports_missing() {
        let env = TempEnvironment::builder().build();
        let toml = file_externals_toml("x", "deadbeef");
        let reports = detect_drift_for_pack(
            "p",
            toml.as_bytes(),
            env.paths.as_ref(),
            env.fs.as_ref(),
            None,
        )
        .unwrap();
        assert_eq!(reports[0].kind, DriftKind::Missing);
    }

    #[test]
    fn archive_entry_reports_not_implemented() {
        let env = TempEnvironment::builder().build();
        let toml = r#"
[arc]
type   = "archive"
url    = "https://example.com/x.tar.gz"
target = "~/x"
sha256 = "abc"
"#;
        let reports = detect_drift_for_pack(
            "p",
            toml.as_bytes(),
            env.paths.as_ref(),
            env.fs.as_ref(),
            None,
        )
        .unwrap();
        assert_eq!(reports[0].kind, DriftKind::NotImplemented);
    }

    #[test]
    fn git_drift_uses_runner_status_porcelain() {
        // Drive drift detection through the trait — confirms the
        // mock runner is actually consulted and that drift kind
        // tracks porcelain output.
        let env = TempEnvironment::builder().build();
        let mock = crate::external::MockGitRunner::new(&"a".repeat(40), b"");
        // Fake the clone directory existing.
        let clone_dir = env
            .paths
            .handler_data_dir("p", crate::handlers::HANDLER_EXTERNAL)
            .join("omz");
        env.fs.mkdir_all(&clone_dir).unwrap();

        let toml = r#"
[omz]
type   = "git-repo"
url    = "https://x/omz.git"
target = "~/.oh-my-zsh"
"#;

        mock.set_status_porcelain("");
        let reports = detect_drift_for_pack(
            "p",
            toml.as_bytes(),
            env.paths.as_ref(),
            env.fs.as_ref(),
            Some(&mock),
        )
        .unwrap();
        assert_eq!(reports[0].kind, DriftKind::Clean);

        mock.set_status_porcelain(" M themes/agnoster.zsh-theme\n");
        let reports = detect_drift_for_pack(
            "p",
            toml.as_bytes(),
            env.paths.as_ref(),
            env.fs.as_ref(),
            Some(&mock),
        )
        .unwrap();
        assert_eq!(reports[0].kind, DriftKind::Drifted);
        assert!(reports[0].detail.contains("1 modified"));
    }

    #[test]
    fn report_is_drifted_helper_only_true_for_drifted_kind() {
        let r = DriftReport {
            pack: "p".into(),
            entry_name: "e".into(),
            kind: DriftKind::Drifted,
            detail: "".into(),
        };
        assert!(r.is_drifted());
        let r = DriftReport {
            kind: DriftKind::Clean,
            ..r
        };
        assert!(!r.is_drifted());
    }
}

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
    /// Couldn't read deployed state at all (deleted by user, etc.).
    Missing,
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
                check_file_drift(pack, &name, &datastore_entry, sha256, fs)
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

fn check_file_drift(
    pack: &str,
    name: &str,
    datastore_entry_dir: &std::path::Path,
    expected_sha256: &str,
    fs: &dyn Fs,
) -> DriftReport {
    // The file lives at `<entry-dir>/<basename>`. We don't know the
    // basename without reading the dir.
    let Some(file_path) = first_file_in_dir(datastore_entry_dir, fs) else {
        return DriftReport {
            pack: pack.into(),
            entry_name: name.into(),
            kind: DriftKind::Missing,
            detail: "no deployed file in datastore".into(),
        };
    };
    let bytes = match fs.read_file(&file_path) {
        Ok(b) => b,
        Err(e) => {
            return DriftReport {
                pack: pack.into(),
                entry_name: name.into(),
                kind: DriftKind::Missing,
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
    // Use `git status --porcelain` as the drift oracle. We expose
    // that on the GitRunner trait indirectly via the helper below
    // so the trait stays narrow.
    match git_porcelain(git, clone_path) {
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
            kind: DriftKind::Missing,
            detail: format!("git status failed: {e}"),
        },
    }
}

/// Shell-out helper that bypasses the `GitRunner` trait's narrow
/// porcelain-verb surface. Drift is the only thing dodot does that
/// needs `git status --porcelain`, so we keep it co-located here
/// rather than bloating the trait with another method that exists
/// only for this PR.
fn git_porcelain(_git: &dyn GitRunner, repo: &std::path::Path) -> Result<String> {
    use std::process::Command;
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["status", "--porcelain"])
        .output()
        .map_err(|e| crate::DodotError::Other(format!("git status failed: {e}")))?;
    if !output.status.success() {
        return Err(crate::DodotError::Other(format!(
            "git status exited {}: {}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn first_file_in_dir(dir: &std::path::Path, fs: &dyn Fs) -> Option<PathBuf> {
    if !fs.exists(dir) {
        return None;
    }
    let entries = fs.read_dir(dir).ok()?;
    entries
        .into_iter()
        .filter(|e| e.is_file)
        .map(|e| dir.join(e.name))
        .next()
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
    use std::path::Path;

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
        format!(
            r#"
[{name}]
type   = "file"
url    = "https://example.com/x"
target = "~/x"
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

    // Silence unused-import warning when git_porcelain isn't exercised
    // by tests (it shells out to real git).
    #[allow(dead_code)]
    fn _unused(_p: &Path) {}
}

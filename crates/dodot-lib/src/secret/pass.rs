//! `pass` provider — password-store integration.
//!
//! Reference shape: `pass:path/to/entry` (single-colon, no slashes
//! after the colon for the scheme prefix; provider sees the literal
//! path, e.g. `path/to/entry`).
//!
//! Resolution: shells out to `pass show <path>`. By convention,
//! `pass` entries' first line is the password; subsequent lines are
//! arbitrary metadata. dodot follows this convention — `resolve()`
//! returns the first line, stripped of trailing `\n`.
//!
//! `pass` itself defers crypto to GPG; the gpg-agent dance handles
//! interactive auth (passphrase prompts, smartcard touches) before
//! `pass show` returns. Our `probe()` verifies that the binary is on
//! PATH and that `$PASSWORD_STORE_DIR` (or `~/.password-store`) is
//! initialised; deeper auth-state checks (gpg key access) are
//! deferred to resolve-time because probing them would mean
//! triggering the very prompt we're trying to gate behind probe().
//!
//! See `secrets.lex` §5.2 for the spec table.

use std::path::PathBuf;
use std::sync::Arc;

use crate::datastore::CommandRunner;
use crate::secret::provider::{ProbeResult, SecretProvider};
use crate::secret::secret_string::SecretString;
use crate::{DodotError, Result};

/// `SecretProvider` impl for password-store.
pub struct PassProvider {
    runner: Arc<dyn CommandRunner>,
    /// Directory to check for store initialisation. Defaults to
    /// `$PASSWORD_STORE_DIR` if set, falling back to `~/.password-store`.
    /// Tests inject a hermetic path here.
    store_dir: PathBuf,
}

impl PassProvider {
    /// Construct with a runner and explicit store directory. Tests
    /// use this directly; production code uses [`Self::from_env`].
    pub fn new(runner: Arc<dyn CommandRunner>, store_dir: PathBuf) -> Self {
        Self { runner, store_dir }
    }

    /// Construct from environment: respects `$PASSWORD_STORE_DIR`,
    /// falls back to `$HOME/.password-store`. If `$HOME` is unset
    /// (deeply unusual; suggests a test or a daemon context),
    /// returns a provider rooted at `/.password-store` — `probe()`
    /// will surface `Misconfigured` because that path won't exist.
    pub fn from_env(runner: Arc<dyn CommandRunner>) -> Self {
        let store_dir = std::env::var_os("PASSWORD_STORE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let mut p = std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("/"));
                p.push(".password-store");
                p
            });
        Self::new(runner, store_dir)
    }

    /// Validate the reference shape before shelling out. Empty
    /// references and references whose path segments include `..`
    /// (path traversal) are rejected up-front. We don't try to be
    /// clever about shell-quoting because `CommandRunner::run` takes
    /// argv as a slice — there's no shell interpolation in play.
    ///
    /// We check segment-equality instead of a substring match on
    /// `..` so that legitimate entry names containing two
    /// consecutive dots (e.g. `service-foo..staging`) pass through.
    /// Pass entries are stored as files on disk; only `..` as its
    /// own segment escapes the store root.
    fn validate_reference(reference: &str) -> Result<()> {
        if reference.is_empty() {
            return Err(DodotError::Other(
                "pass reference is empty. Expected `pass:path/to/entry`.".into(),
            ));
        }
        if reference.split('/').any(|seg| seg == "..") {
            return Err(DodotError::Other(format!(
                "pass reference `{reference}` contains a `..` path segment — \
                 path-traversal references are refused for safety. \
                 Use the literal entry path under the store root."
            )));
        }
        Ok(())
    }
}

impl SecretProvider for PassProvider {
    fn scheme(&self) -> &str {
        "pass"
    }

    fn probe(&self) -> ProbeResult {
        // Cheap binary-on-PATH check: `pass version` returns 0 with
        // a banner. `pass --help` would also work; `version` is the
        // canonical "I'm here" probe across most tool conventions.
        match self.runner.run("pass", &["version".into()]) {
            Ok(out) if out.exit_code == 0 => {}
            Ok(_) => {
                return ProbeResult::ProbeFailed {
                    details: "`pass version` returned a non-zero exit code; the \
                              binary is on PATH but not behaving as expected"
                        .into(),
                };
            }
            Err(_) => {
                return ProbeResult::NotInstalled {
                    hint: "install pass: https://www.passwordstore.org/ \
                           (e.g. `apt install pass`, `brew install pass`)"
                        .into(),
                };
            }
        }
        // Initialised-store check: a `.gpg-id` file at the store
        // root is `pass init`'s canonical artifact.
        let gpg_id = self.store_dir.join(".gpg-id");
        if !gpg_id.exists() {
            return ProbeResult::Misconfigured {
                hint: format!(
                    "password store not initialised at {} \
                     (no .gpg-id found). \
                     Run `pass init <gpg-key-id>`, or set \
                     $PASSWORD_STORE_DIR to point at an existing store.",
                    self.store_dir.display()
                ),
            };
        }
        ProbeResult::Ok
    }

    fn resolve(&self, reference: &str) -> Result<SecretString> {
        Self::validate_reference(reference)?;
        let out = self
            .runner
            .run("pass", &["show".into(), reference.into()])?;
        if out.exit_code != 0 {
            // Map the most common "entry not found" pattern to a
            // sharper error. `pass show missing/path` exits 1 and
            // prints `Error: missing/path is not in the password
            // store.` to stderr. Detect by exit + a stable phrase.
            let stderr = out.stderr.trim();
            let err_msg = if stderr.contains("not in the password store") {
                format!(
                    "secret `pass:{reference}` not found in the password store. \
                     Verify the entry: `pass ls {}`",
                    parent_path(reference).unwrap_or("/")
                )
            } else if stderr.is_empty() {
                format!("`pass show {reference}` exited with code {}", out.exit_code)
            } else {
                // Provider stderr can carry gpg-agent diagnostics or
                // similar. Surface verbatim — but `pass` doesn't echo
                // the password to stderr, so this is safe to include.
                format!(
                    "`pass show {reference}` failed (exit {}): {stderr}",
                    out.exit_code
                )
            };
            return Err(DodotError::Other(err_msg));
        }
        // pass show emits: <password>\n[optional metadata lines]\n
        // The first line is the password. Strip trailing `\n` only;
        // a multi-line value (subsequent lines) is metadata, not
        // password content.
        let first_line = out.stdout.split('\n').next().unwrap_or("");
        Ok(SecretString::new(first_line.to_string()))
    }
}

/// Return everything before the last `/` in `reference`, or `None`
/// if the reference is at the store root.
fn parent_path(reference: &str) -> Option<&str> {
    let idx = reference.rfind('/')?;
    Some(&reference[..idx])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datastore::CommandOutput;
    use std::sync::Mutex;

    /// `(executable, args, response)` tuple used by `ScriptedRunner`.
    type ScriptedResponse = (
        String,
        Vec<String>,
        std::result::Result<CommandOutput, String>,
    );

    /// Test runner: maps `(executable, args)` to a canned outcome.
    /// Records every call so probe-then-resolve flows can be
    /// asserted against.
    struct ScriptedRunner {
        responses: Mutex<Vec<ScriptedResponse>>,
        calls: Mutex<Vec<(String, Vec<String>)>>,
    }

    impl ScriptedRunner {
        fn new() -> Self {
            Self {
                responses: Mutex::new(Vec::new()),
                calls: Mutex::new(Vec::new()),
            }
        }
        fn expect(
            self,
            exe: impl Into<String>,
            args: Vec<String>,
            response: std::result::Result<CommandOutput, String>,
        ) -> Self {
            self.responses
                .lock()
                .unwrap()
                .push((exe.into(), args, response));
            self
        }
        fn calls(&self) -> Vec<(String, Vec<String>)> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl CommandRunner for ScriptedRunner {
        fn run(&self, exe: &str, args: &[String]) -> Result<CommandOutput> {
            self.calls
                .lock()
                .unwrap()
                .push((exe.to_string(), args.to_vec()));
            let mut responses = self.responses.lock().unwrap();
            if responses.is_empty() {
                return Err(DodotError::Other(format!(
                    "ScriptedRunner: unexpected call to `{exe} {args:?}` — no responses queued"
                )));
            }
            let (expected_exe, expected_args, response) = responses.remove(0);
            assert_eq!(exe, expected_exe, "executable mismatch");
            assert_eq!(args, expected_args.as_slice(), "args mismatch");
            response.map_err(DodotError::Other)
        }
    }

    fn ok(stdout: &str) -> std::result::Result<CommandOutput, String> {
        Ok(CommandOutput {
            exit_code: 0,
            stdout: stdout.into(),
            stderr: String::new(),
        })
    }

    fn err(exit: i32, stderr: &str) -> std::result::Result<CommandOutput, String> {
        Ok(CommandOutput {
            exit_code: exit,
            stdout: String::new(),
            stderr: stderr.into(),
        })
    }

    fn make_store_dir(initialised: bool) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        if initialised {
            std::fs::write(dir.path().join(".gpg-id"), "test@example.invalid\n").unwrap();
        }
        dir
    }

    #[test]
    fn scheme_is_pass() {
        let dir = make_store_dir(true);
        let p = PassProvider::new(Arc::new(ScriptedRunner::new()), dir.path().into());
        assert_eq!(p.scheme(), "pass");
    }

    #[test]
    fn resolve_returns_first_line_of_pass_show_output() {
        let dir = make_store_dir(true);
        let runner = Arc::new(ScriptedRunner::new().expect(
            "pass",
            vec!["show".into(), "personal/db".into()],
            ok("hunter2\nuser: alice\nurl: https://db.example\n"),
        ));
        let p = PassProvider::new(runner, dir.path().into());
        let s = p.resolve("personal/db").unwrap();
        assert_eq!(s.expose().unwrap(), "hunter2");
    }

    #[test]
    fn resolve_handles_value_without_trailing_newline() {
        let dir = make_store_dir(true);
        let runner = Arc::new(ScriptedRunner::new().expect(
            "pass",
            vec!["show".into(), "k".into()],
            ok("no-newline-at-end"),
        ));
        let p = PassProvider::new(runner, dir.path().into());
        assert_eq!(
            p.resolve("k").unwrap().expose().unwrap(),
            "no-newline-at-end"
        );
    }

    #[test]
    fn resolve_maps_not_in_store_to_actionable_error() {
        let dir = make_store_dir(true);
        let runner = Arc::new(ScriptedRunner::new().expect(
            "pass",
            vec!["show".into(), "missing/k".into()],
            err(1, "Error: missing/k is not in the password store."),
        ));
        let p = PassProvider::new(runner, dir.path().into());
        let e = p.resolve("missing/k").unwrap_err().to_string();
        assert!(e.contains("`pass:missing/k` not found"));
        // Lists the parent path so the user can run `pass ls <parent>`
        // to spot a typo.
        assert!(e.contains("`pass ls missing`"));
    }

    #[test]
    fn resolve_other_failures_include_stderr_verbatim() {
        let dir = make_store_dir(true);
        let runner = Arc::new(ScriptedRunner::new().expect(
            "pass",
            vec!["show".into(), "k".into()],
            err(2, "gpg: decryption failed: No secret key"),
        ));
        let p = PassProvider::new(runner, dir.path().into());
        let e = p.resolve("k").unwrap_err().to_string();
        assert!(e.contains("decryption failed"));
        assert!(e.contains("(exit 2)"));
    }

    #[test]
    fn resolve_rejects_empty_reference() {
        let dir = make_store_dir(true);
        let p = PassProvider::new(Arc::new(ScriptedRunner::new()), dir.path().into());
        let e = p.resolve("").unwrap_err().to_string();
        assert!(e.contains("empty"));
    }

    #[test]
    fn resolve_rejects_dotdot_reference() {
        let dir = make_store_dir(true);
        let p = PassProvider::new(Arc::new(ScriptedRunner::new()), dir.path().into());
        let e = p.resolve("../escape").unwrap_err().to_string();
        assert!(e.contains("path-traversal"));
    }

    #[test]
    fn resolve_rejects_dotdot_in_middle_segment() {
        let dir = make_store_dir(true);
        let p = PassProvider::new(Arc::new(ScriptedRunner::new()), dir.path().into());
        let e = p.resolve("foo/../escape").unwrap_err().to_string();
        assert!(e.contains("path-traversal"));
    }

    #[test]
    fn resolve_accepts_double_dot_inside_a_segment() {
        // `foo..bar` is a legitimate pass entry name (two consecutive
        // dots within one segment, not a `..` path segment). The
        // validator must let it through; the runner answers with the
        // entry's value.
        let dir = make_store_dir(true);
        let runner = Arc::new(ScriptedRunner::new().expect(
            "pass",
            vec!["show".into(), "service-foo..staging".into()],
            ok("hunter2\n"),
        ));
        let p = PassProvider::new(runner, dir.path().into());
        let v = p.resolve("service-foo..staging").unwrap();
        assert_eq!(v.expose().unwrap(), "hunter2");
    }

    #[test]
    fn probe_ok_when_binary_present_and_store_initialised() {
        let dir = make_store_dir(true);
        let runner = Arc::new(ScriptedRunner::new().expect(
            "pass",
            vec!["version".into()],
            ok("=============================================\n= pass: the standard unix password manager =\n"),
        ));
        let p = PassProvider::new(runner.clone(), dir.path().into());
        assert!(matches!(p.probe(), ProbeResult::Ok));
        assert_eq!(runner.calls().len(), 1);
    }

    #[test]
    fn probe_not_installed_when_runner_errors() {
        let dir = make_store_dir(true);
        let runner = Arc::new(ScriptedRunner::new().expect(
            "pass",
            vec!["version".into()],
            Err("command not found: pass".into()),
        ));
        let p = PassProvider::new(runner, dir.path().into());
        match p.probe() {
            ProbeResult::NotInstalled { hint } => {
                assert!(hint.contains("install pass"));
                assert!(hint.contains("apt install"));
                assert!(hint.contains("brew install"));
            }
            other => panic!("expected NotInstalled, got {other:?}"),
        }
    }

    #[test]
    fn probe_misconfigured_when_store_uninitialised() {
        let dir = make_store_dir(false); // no .gpg-id
        let runner = Arc::new(ScriptedRunner::new().expect(
            "pass",
            vec!["version".into()],
            ok("pass v1.7\n"),
        ));
        let p = PassProvider::new(runner, dir.path().into());
        match p.probe() {
            ProbeResult::Misconfigured { hint } => {
                assert!(hint.contains("not initialised"));
                assert!(hint.contains("pass init"));
                assert!(hint.contains("PASSWORD_STORE_DIR"));
            }
            other => panic!("expected Misconfigured, got {other:?}"),
        }
    }

    #[test]
    fn probe_failed_on_nonzero_version_exit() {
        let dir = make_store_dir(true);
        let runner =
            Arc::new(ScriptedRunner::new().expect("pass", vec!["version".into()], err(127, "")));
        let p = PassProvider::new(runner, dir.path().into());
        match p.probe() {
            ProbeResult::ProbeFailed { details } => {
                assert!(details.contains("non-zero exit"));
            }
            other => panic!("expected ProbeFailed, got {other:?}"),
        }
    }

    #[test]
    fn parent_path_strips_last_segment() {
        assert_eq!(parent_path("a/b/c"), Some("a/b"));
        assert_eq!(parent_path("a/b"), Some("a"));
        assert_eq!(parent_path("a"), None);
        assert_eq!(parent_path(""), None);
    }
}

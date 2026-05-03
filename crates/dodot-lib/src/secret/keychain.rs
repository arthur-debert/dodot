//! `keychain` provider — macOS Keychain integration via the
//! `security` command (`/usr/bin/security`, ships with macOS).
//!
//! Reference shape: `keychain:<service>` or
//! `keychain:<service>/<account>`.
//!
//! - `keychain:GitHub` → first item whose service name matches
//!   `GitHub`, regardless of account. Useful when the keychain
//!   item name is unique across all your stored credentials.
//! - `keychain:GitHub/alice` → exact (service, account) pair.
//!   Use this shape when you have multiple credentials per
//!   service (work + personal accounts).
//!
//! Resolution: `security find-generic-password -s <service>
//! [-a <account>] -w` — `-w` makes `security` print only the
//! password on stdout (no surrounding metadata), exit 0 on
//! success, exit 44 with a `SecKeychainSearchCopyNext` stderr
//! line when the item isn't found.
//!
//! Auth model: the user's *login keychain* is unlocked by
//! default at session start. dodot does NOT call `security
//! unlock-keychain` — that would either need the user's password
//! or skip the prompt entirely (security risk). When the
//! keychain is locked, `security find-generic-password` returns
//! exit 51 with a `User interaction is not allowed` stderr line
//! and we surface that as a `NotAuthenticated` probe.
//!
//! Platform: macOS only. On Linux / WSL the `security` binary
//! isn't on PATH and `probe()` returns `NotInstalled` with a
//! "use secret-tool instead" pointer.
//!
//! See `secrets.lex` §5.2 / §5.4 (provider table + error UX) and
//! §S4 (OS-level providers).

use std::sync::Arc;

use crate::datastore::CommandRunner;
use crate::secret::provider::{ProbeResult, SecretProvider};
use crate::secret::secret_string::SecretString;
use crate::{DodotError, Result};

/// `SecretProvider` impl for the macOS Keychain via the
/// `security` command. Holds a `CommandRunner` for subprocess
/// invocations; tests substitute a `ScriptedRunner` to mock
/// `security` without touching the real keychain.
pub struct KeychainProvider {
    runner: Arc<dyn CommandRunner>,
}

impl KeychainProvider {
    pub fn new(runner: Arc<dyn CommandRunner>) -> Self {
        Self { runner }
    }

    pub fn from_env(runner: Arc<dyn CommandRunner>) -> Self {
        Self::new(runner)
    }

    /// Parse the post-prefix reference into `(service,
    /// Option<account>)`. The registry has already stripped
    /// `keychain:`. Empty references and trailing slashes are
    /// rejected up front.
    fn parse_reference(suffix: &str) -> Result<(&str, Option<&str>)> {
        if suffix.is_empty() {
            return Err(DodotError::Other(
                "keychain reference is empty. Expected `keychain:<service>[/<account>]`.".into(),
            ));
        }
        let (service, account) = match suffix.split_once('/') {
            Some((s, a)) => (s, Some(a)),
            None => (suffix, None),
        };
        if service.is_empty() {
            return Err(DodotError::Other(format!(
                "keychain reference `keychain:{suffix}` has an empty service name."
            )));
        }
        if let Some(a) = account {
            if a.is_empty() {
                return Err(DodotError::Other(format!(
                    "keychain reference `keychain:{suffix}` has an empty account name. \
                     Either drop the trailing `/` (use `keychain:<service>` for a \
                     service-only lookup) or supply an account: `keychain:<service>/<account>`."
                )));
            }
        }
        Ok((service, account))
    }
}

impl SecretProvider for KeychainProvider {
    fn scheme(&self) -> &str {
        "keychain"
    }

    fn probe(&self) -> ProbeResult {
        // Step 1: binary on PATH? On macOS `/usr/bin/security`
        // is part of the base system, so a missing binary is
        // almost always "this is a non-macOS host".
        match self.runner.run("security", &["-h".into()]) {
            Ok(_) => {}
            Err(_) => {
                return ProbeResult::NotInstalled {
                    hint: "the `security` command is macOS-only. \
                           On Linux / WSL, use the `secret-tool` provider instead \
                           (`[secret.providers.secret-tool] enabled = true`)."
                        .into(),
                };
            }
        }
        // Step 2: keychain accessibility. We don't have a known
        // item to look up at probe time, so we use a lightweight
        // sanity check: `security default-keychain` returns the
        // user's default keychain path on success and a
        // diagnostic on failure. Doesn't unlock anything, doesn't
        // require any pre-existing items.
        match self.runner.run("security", &["default-keychain".into()]) {
            Ok(out) if out.exit_code == 0 => ProbeResult::Ok,
            Ok(_) => ProbeResult::ProbeFailed {
                details: "`security default-keychain` returned non-zero — \
                          the binary is on PATH but no default keychain is \
                          configured. Run `security login-keychain` to inspect."
                    .into(),
            },
            Err(_) => ProbeResult::ProbeFailed {
                details: "could not run `security default-keychain` after a \
                          successful `security -h`; intermittent subprocess failure"
                    .into(),
            },
        }
    }

    fn resolve(&self, reference: &str) -> Result<SecretString> {
        let (service, account) = Self::parse_reference(reference)?;
        let mut args: Vec<String> =
            vec!["find-generic-password".into(), "-s".into(), service.into()];
        if let Some(a) = account {
            args.push("-a".into());
            args.push(a.into());
        }
        // `-w` prints just the password on stdout — without it
        // `security` dumps the full attribute table.
        args.push("-w".into());

        let out = self.runner.run("security", &args)?;
        if out.exit_code != 0 {
            let stderr = out.stderr.trim();
            // `security`'s exit codes for find-generic-password
            // are stable: 44 = not found, 51 = user interaction
            // not allowed (typically a locked keychain in a
            // non-interactive context).
            let err_msg = if out.exit_code == 44 || stderr.contains("could not be found") {
                let qualifier = match account {
                    Some(a) => format!("(service `{service}`, account `{a}`)"),
                    None => format!("(service `{service}`)"),
                };
                format!(
                    "secret `keychain:{reference}` not found in the keychain {qualifier}. \
                     Verify with `security find-generic-password -s '{service}'`{} \
                     -- or add the item via Keychain Access.app / \
                     `security add-generic-password -s '{service}' [-a '<account>'] -w '<password>'`.",
                    account
                        .map(|a| format!(" -a '{a}'"))
                        .unwrap_or_default(),
                )
            } else if out.exit_code == 51
                || stderr.contains("User interaction is not allowed")
                || stderr.contains("locked")
            {
                format!(
                    "secret resolution for `keychain:{reference}` failed: \
                     the keychain is locked or interaction is not allowed. \
                     Unlock the login keychain (e.g. by signing in / opening \
                     Keychain Access.app) and re-run dodot."
                )
            } else if stderr.is_empty() {
                format!(
                    "`security find-generic-password` exited with code {} \
                     (no diagnostic output)",
                    out.exit_code
                )
            } else {
                // `security` prints diagnostics to stderr; the
                // password (`-w` mode) only goes to stdout, so
                // surfacing stderr verbatim is safe.
                format!(
                    "`security find-generic-password` failed (exit {}): {stderr}",
                    out.exit_code
                )
            };
            return Err(DodotError::Other(err_msg));
        }
        // `security -w` emits the password followed by a single
        // trailing newline. Strip exactly one — same contract as
        // the other providers (op / bw / pass).
        let mut value = out.stdout;
        if value.ends_with('\n') {
            value.pop();
        }
        Ok(SecretString::new(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datastore::CommandOutput;
    use std::sync::Mutex;

    type ScriptedResponse = (
        String,
        Vec<String>,
        std::result::Result<CommandOutput, String>,
    );

    struct ScriptedRunner {
        responses: Mutex<Vec<ScriptedResponse>>,
    }
    impl ScriptedRunner {
        fn new() -> Self {
            Self {
                responses: Mutex::new(Vec::new()),
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
    }
    impl CommandRunner for ScriptedRunner {
        fn run(&self, exe: &str, args: &[String]) -> Result<CommandOutput> {
            let mut r = self.responses.lock().unwrap();
            if r.is_empty() {
                return Err(DodotError::Other(format!(
                    "ScriptedRunner: unexpected `{exe} {args:?}`"
                )));
            }
            let (e, a, out) = r.remove(0);
            assert_eq!(exe, e);
            assert_eq!(args, a.as_slice());
            out.map_err(DodotError::Other)
        }
    }
    fn ok(stdout: &str) -> std::result::Result<CommandOutput, String> {
        Ok(CommandOutput {
            exit_code: 0,
            stdout: stdout.into(),
            stderr: String::new(),
        })
    }
    fn err_out(exit: i32, stderr: &str) -> std::result::Result<CommandOutput, String> {
        Ok(CommandOutput {
            exit_code: exit,
            stdout: String::new(),
            stderr: stderr.into(),
        })
    }

    // ── parse_reference ─────────────────────────────────────────

    #[test]
    fn parse_reference_service_only() {
        let (s, a) = KeychainProvider::parse_reference("GitHub").unwrap();
        assert_eq!(s, "GitHub");
        assert_eq!(a, None);
    }

    #[test]
    fn parse_reference_service_and_account() {
        let (s, a) = KeychainProvider::parse_reference("GitHub/alice").unwrap();
        assert_eq!(s, "GitHub");
        assert_eq!(a, Some("alice"));
    }

    #[test]
    fn parse_reference_rejects_empty_suffix() {
        let e = KeychainProvider::parse_reference("")
            .unwrap_err()
            .to_string();
        assert!(e.contains("empty"));
    }

    #[test]
    fn parse_reference_rejects_empty_service() {
        let e = KeychainProvider::parse_reference("/alice")
            .unwrap_err()
            .to_string();
        assert!(e.contains("empty service"));
    }

    #[test]
    fn parse_reference_rejects_trailing_slash() {
        // Trailing slash with no account is a typo — guide the
        // user back to the service-only shape.
        let e = KeychainProvider::parse_reference("GitHub/")
            .unwrap_err()
            .to_string();
        assert!(e.contains("empty account"));
        assert!(e.contains("drop the trailing"));
    }

    // ── probe ───────────────────────────────────────────────────

    #[test]
    fn probe_ok_when_security_present_and_default_keychain_resolves() {
        let runner = Arc::new(
            ScriptedRunner::new()
                .expect("security", vec!["-h".into()], ok(""))
                .expect(
                    "security",
                    vec!["default-keychain".into()],
                    ok("    \"/Users/x/Library/Keychains/login.keychain-db\"\n"),
                ),
        );
        let p = KeychainProvider::new(runner);
        assert!(matches!(p.probe(), ProbeResult::Ok));
    }

    #[test]
    fn probe_not_installed_when_runner_errors() {
        // On Linux / WSL `security` isn't on PATH at all;
        // ShellCommandRunner returns Err("command not found").
        let runner = Arc::new(ScriptedRunner::new().expect(
            "security",
            vec!["-h".into()],
            Err("command not found: security".into()),
        ));
        let p = KeychainProvider::new(runner);
        match p.probe() {
            ProbeResult::NotInstalled { hint } => {
                assert!(hint.contains("macOS-only"));
                assert!(hint.contains("secret-tool"));
            }
            other => panic!("expected NotInstalled, got {other:?}"),
        }
    }

    #[test]
    fn probe_failed_when_default_keychain_returns_nonzero() {
        let runner = Arc::new(
            ScriptedRunner::new()
                .expect("security", vec!["-h".into()], ok(""))
                .expect(
                    "security",
                    vec!["default-keychain".into()],
                    err_out(50, "no default keychain"),
                ),
        );
        let p = KeychainProvider::new(runner);
        assert!(matches!(p.probe(), ProbeResult::ProbeFailed { .. }));
    }

    // ── resolve ─────────────────────────────────────────────────

    #[test]
    fn resolve_service_only_invokes_find_generic_password_correctly() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "security",
            vec![
                "find-generic-password".into(),
                "-s".into(),
                "GitHub".into(),
                "-w".into(),
            ],
            ok("ghp_abc123\n"),
        ));
        let p = KeychainProvider::new(runner);
        let v = p.resolve("GitHub").unwrap();
        assert_eq!(v.expose().unwrap(), "ghp_abc123");
    }

    #[test]
    fn resolve_with_account_threads_account_into_args() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "security",
            vec![
                "find-generic-password".into(),
                "-s".into(),
                "GitHub".into(),
                "-a".into(),
                "alice".into(),
                "-w".into(),
            ],
            ok("alice-token\n"),
        ));
        let p = KeychainProvider::new(runner);
        let v = p.resolve("GitHub/alice").unwrap();
        assert_eq!(v.expose().unwrap(), "alice-token");
    }

    #[test]
    fn resolve_maps_exit_44_to_not_found_with_actionable_hint() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "security",
            vec![
                "find-generic-password".into(),
                "-s".into(),
                "missing".into(),
                "-w".into(),
            ],
            err_out(
                44,
                "security: SecKeychainSearchCopyNext: The specified item could not be found in the keychain.",
            ),
        ));
        let p = KeychainProvider::new(runner);
        let e = p.resolve("missing").unwrap_err().to_string();
        assert!(e.contains("not found"));
        assert!(e.contains("`missing`"));
        assert!(e.contains("security add-generic-password"));
    }

    #[test]
    fn resolve_not_found_qualifier_includes_account_when_provided() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "security",
            vec![
                "find-generic-password".into(),
                "-s".into(),
                "GitHub".into(),
                "-a".into(),
                "missing".into(),
                "-w".into(),
            ],
            err_out(44, "could not be found"),
        ));
        let p = KeychainProvider::new(runner);
        let e = p.resolve("GitHub/missing").unwrap_err().to_string();
        assert!(e.contains("`GitHub`"));
        assert!(e.contains("`missing`"));
        assert!(e.contains("-a 'missing'"));
    }

    #[test]
    fn resolve_maps_exit_51_to_locked_keychain_diagnostic() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "security",
            vec![
                "find-generic-password".into(),
                "-s".into(),
                "GitHub".into(),
                "-w".into(),
            ],
            err_out(51, "security: User interaction is not allowed."),
        ));
        let p = KeychainProvider::new(runner);
        let e = p.resolve("GitHub").unwrap_err().to_string();
        assert!(e.contains("locked or interaction is not allowed"));
        assert!(e.contains("Unlock"));
    }

    #[test]
    fn resolve_passes_through_unrecognized_stderr() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "security",
            vec![
                "find-generic-password".into(),
                "-s".into(),
                "GitHub".into(),
                "-w".into(),
            ],
            err_out(1, "weird internal failure"),
        ));
        let p = KeychainProvider::new(runner);
        let e = p.resolve("GitHub").unwrap_err().to_string();
        assert!(e.contains("weird internal failure"));
        assert!(e.contains("exit 1"));
    }

    #[test]
    fn resolve_strips_exactly_one_trailing_newline() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "security",
            vec![
                "find-generic-password".into(),
                "-s".into(),
                "k".into(),
                "-w".into(),
            ],
            ok("value-with-trailing-blank\n\n"),
        ));
        let p = KeychainProvider::new(runner);
        let v = p.resolve("k").unwrap();
        assert_eq!(v.expose().unwrap(), "value-with-trailing-blank\n");
    }
}

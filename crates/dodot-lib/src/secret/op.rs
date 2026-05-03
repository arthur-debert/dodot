//! `op` provider — 1Password CLI integration.
//!
//! Reference shape: `op://Vault/Item/Field`. The registry strips the
//! `op:` prefix and hands `//Vault/Item/Field` to the provider; we
//! hand the full `op://Vault/Item/Field` form to the `op read`
//! subcommand because that's the canonical input shape the CLI
//! expects.
//!
//! Resolution: `op read op://Vault/Item/Field` — emits the field's
//! string value on stdout, exit 0 on success, non-zero on missing
//! reference / auth failure.
//!
//! Auth: prefers `OP_SERVICE_ACCOUNT_TOKEN` (non-interactive,
//! CI-friendly). Falls back to interactive desktop-app integration
//! if no token is set; in that case `probe()` flags
//! `NotAuthenticated` rather than letting `op read` block on a UI
//! prompt.
//!
//! See `secrets.lex` §5.2 (provider table) and §5.4 (error UX).

use std::sync::Arc;

use crate::datastore::CommandRunner;
use crate::secret::provider::{ProbeResult, SecretProvider};
use crate::secret::secret_string::SecretString;
use crate::{DodotError, Result};

/// `SecretProvider` impl for the 1Password CLI (`op`).
pub struct OpProvider {
    runner: Arc<dyn CommandRunner>,
    /// Whether `OP_SERVICE_ACCOUNT_TOKEN` is set in the environment.
    /// Captured at construction so probe() / resolve() don't
    /// re-read the env on every call (and so tests can inject the
    /// answer without touching the real env).
    has_service_token: bool,
}

impl OpProvider {
    /// Construct with explicit `has_service_token`. Tests use this.
    pub fn new(runner: Arc<dyn CommandRunner>, has_service_token: bool) -> Self {
        Self {
            runner,
            has_service_token,
        }
    }

    /// Construct from environment: reads `OP_SERVICE_ACCOUNT_TOKEN`.
    pub fn from_env(runner: Arc<dyn CommandRunner>) -> Self {
        let has_service_token = std::env::var("OP_SERVICE_ACCOUNT_TOKEN")
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        Self::new(runner, has_service_token)
    }

    /// Validate the post-prefix reference shape. The registry hands
    /// us `//Vault/Item/Field`; we expect at least three path
    /// segments after `//`.
    fn validate_reference(reference: &str) -> Result<()> {
        let stripped = reference.strip_prefix("//").ok_or_else(|| {
            DodotError::Other(format!(
                "op reference suffix `{reference}` is not in `//Vault/Item/Field` form. \
                 Expected the full URI shape `op://Vault/Item/Field`."
            ))
        })?;
        let segs: Vec<&str> = stripped.split('/').filter(|s| !s.is_empty()).collect();
        if segs.len() < 3 {
            return Err(DodotError::Other(format!(
                "op reference `op:{reference}` is missing path segments. \
                 Expected `op://<vault>/<item>/<field>`; got {} segment(s).",
                segs.len()
            )));
        }
        Ok(())
    }
}

impl SecretProvider for OpProvider {
    fn scheme(&self) -> &str {
        "op"
    }

    fn probe(&self) -> ProbeResult {
        // Step 1: binary on PATH? `op --version` is fast and
        // doesn't hit the network or unlock anything.
        match self.runner.run("op", &["--version".into()]) {
            Ok(out) if out.exit_code == 0 => {}
            Ok(_) => {
                return ProbeResult::ProbeFailed {
                    details: "`op --version` returned non-zero — the binary is on PATH \
                              but not behaving as expected"
                        .into(),
                };
            }
            Err(_) => {
                return ProbeResult::NotInstalled {
                    hint: "install 1Password CLI: \
                           https://1password.com/downloads/command-line \
                           (e.g. `brew install --cask 1password-cli`)"
                        .into(),
                };
            }
        }

        // Step 2: auth state. We require the non-interactive path
        // (`OP_SERVICE_ACCOUNT_TOKEN`) because resolve()-time
        // blocking on a desktop-app modal is exactly the
        // auth-fatigue failure mode §7.4 was written to avoid.
        // If the user prefers desktop-app integration on their
        // workstation, they should still set the env var for dodot
        // runs — the spec is unambiguous about that.
        if !self.has_service_token {
            return ProbeResult::NotAuthenticated {
                hint: "set OP_SERVICE_ACCOUNT_TOKEN \
                       (https://developer.1password.com/docs/service-accounts/) \
                       so dodot can resolve secrets without interactive prompts"
                    .into(),
            };
        }

        // Step 3: light service-account validity check. `op whoami`
        // returns 0 when the token can authenticate, non-zero
        // otherwise. Cheap, no vault reads, no items returned —
        // safe to run on every probe.
        match self.runner.run("op", &["whoami".into()]) {
            Ok(out) if out.exit_code == 0 => ProbeResult::Ok,
            Ok(_) => ProbeResult::NotAuthenticated {
                hint: "OP_SERVICE_ACCOUNT_TOKEN is set but `op whoami` failed; \
                       check that the token is valid and not expired"
                    .into(),
            },
            Err(_) => ProbeResult::ProbeFailed {
                details: "could not run `op whoami` after a successful `op --version`; \
                          intermittent subprocess failure"
                    .into(),
            },
        }
    }

    fn resolve(&self, reference: &str) -> Result<SecretString> {
        Self::validate_reference(reference)?;
        // Reconstruct the full URI form the CLI expects (op://...).
        let full = format!("op:{reference}");
        let out = self.runner.run("op", &["read".into(), full.clone()])?;
        if out.exit_code != 0 {
            let stderr = out.stderr.trim();
            // Common shapes:
            //   "[ERROR] 2025/05/02 12:34:56 \"X\" isn't an item in the \"Y\" vault."
            //   "[ERROR] 2025/05/02 12:34:56 vault \"Y\" not found."
            //   "[ERROR] 2025/05/02 12:34:56 missing field..."
            let err_msg = if stderr.contains("isn't an item") || stderr.contains("not found") {
                format!(
                    "secret `{full}` not found. \
                     Verify with `op item get \"<item>\" --vault \"<vault>\"`."
                )
            } else if stderr.contains("authentication") || stderr.contains("token") {
                format!(
                    "secret resolution for `{full}` failed authentication. \
                     Check OP_SERVICE_ACCOUNT_TOKEN and the SA's vault access."
                )
            } else if stderr.is_empty() {
                format!("`op read {full}` exited with code {}", out.exit_code)
            } else {
                // op's stderr does NOT echo the secret value; it
                // emits structured error lines. Surfacing verbatim
                // is safe and aids diagnosis.
                format!("`op read {full}` failed (exit {}): {stderr}", out.exit_code)
            };
            return Err(DodotError::Other(err_msg));
        }
        // op read emits the value with a trailing newline. Strip
        // exactly one trailing `\n` so values that legitimately end
        // in `\n\n` keep one of them; in practice secret values
        // shouldn't have either, but the principled move is to
        // trim the CLI's added newline rather than `trim_end_matches`.
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

    #[test]
    fn scheme_is_op() {
        let p = OpProvider::new(Arc::new(ScriptedRunner::new()), true);
        assert_eq!(p.scheme(), "op");
    }

    #[test]
    fn resolve_strips_one_trailing_newline_from_stdout() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "op",
            vec!["read".into(), "op://V/I/F".into()],
            ok("the-value\n"),
        ));
        let p = OpProvider::new(runner, true);
        let s = p.resolve("//V/I/F").unwrap();
        assert_eq!(s.expose().unwrap(), "the-value");
    }

    #[test]
    fn resolve_handles_value_without_trailing_newline() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "op",
            vec!["read".into(), "op://V/I/F".into()],
            ok("no-newline"),
        ));
        let p = OpProvider::new(runner, true);
        assert_eq!(
            p.resolve("//V/I/F").unwrap().expose().unwrap(),
            "no-newline"
        );
    }

    #[test]
    fn resolve_maps_isnt_an_item_to_actionable_error() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "op",
            vec!["read".into(), "op://V/I/F".into()],
            err_out(
                1,
                "[ERROR] 2026/05/02 14:00:00 \"I\" isn't an item in the \"V\" vault.",
            ),
        ));
        let p = OpProvider::new(runner, true);
        let e = p.resolve("//V/I/F").unwrap_err().to_string();
        assert!(e.contains("`op://V/I/F` not found"));
        assert!(e.contains("op item get"));
    }

    #[test]
    fn resolve_maps_authentication_failures_to_token_hint() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "op",
            vec!["read".into(), "op://V/I/F".into()],
            err_out(1, "[ERROR] authentication required"),
        ));
        let p = OpProvider::new(runner, true);
        let e = p.resolve("//V/I/F").unwrap_err().to_string();
        assert!(e.contains("failed authentication"));
        assert!(e.contains("OP_SERVICE_ACCOUNT_TOKEN"));
    }

    #[test]
    fn resolve_other_failures_include_stderr() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "op",
            vec!["read".into(), "op://V/I/F".into()],
            err_out(2, "rate limited; try again later"),
        ));
        let p = OpProvider::new(runner, true);
        let e = p.resolve("//V/I/F").unwrap_err().to_string();
        assert!(e.contains("rate limited"));
        assert!(e.contains("(exit 2)"));
    }

    #[test]
    fn resolve_rejects_reference_without_double_slash() {
        let p = OpProvider::new(Arc::new(ScriptedRunner::new()), true);
        // The registry would never produce this — split_scheme on
        // `op:foo` yields suffix `foo`. We refuse defensively so the
        // user sees a clear shape hint, not a downstream `op` CLI error.
        let e = p.resolve("Vault/Item/Field").unwrap_err().to_string();
        assert!(e.contains("not in `//Vault/Item/Field` form"));
    }

    #[test]
    fn resolve_rejects_too_few_segments() {
        let p = OpProvider::new(Arc::new(ScriptedRunner::new()), true);
        let e = p.resolve("//Vault/Item").unwrap_err().to_string();
        assert!(e.contains("missing path segments"));
        assert!(e.contains("got 2 segment(s)"));
    }

    #[test]
    fn probe_not_installed_when_op_binary_missing() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "op",
            vec!["--version".into()],
            Err("command not found: op".into()),
        ));
        let p = OpProvider::new(runner, true);
        match p.probe() {
            ProbeResult::NotInstalled { hint } => {
                assert!(hint.contains("1Password CLI"));
                assert!(hint.contains("brew install"));
            }
            other => panic!("expected NotInstalled, got {other:?}"),
        }
    }

    #[test]
    fn probe_not_authenticated_when_no_service_token_in_env() {
        let runner =
            Arc::new(ScriptedRunner::new().expect("op", vec!["--version".into()], ok("2.34.0\n")));
        let p = OpProvider::new(runner, /*has_service_token=*/ false);
        match p.probe() {
            ProbeResult::NotAuthenticated { hint } => {
                assert!(hint.contains("OP_SERVICE_ACCOUNT_TOKEN"));
                assert!(hint.contains("service-accounts"));
            }
            other => panic!("expected NotAuthenticated, got {other:?}"),
        }
    }

    #[test]
    fn probe_not_authenticated_when_whoami_fails_despite_token() {
        let runner = Arc::new(
            ScriptedRunner::new()
                .expect("op", vec!["--version".into()], ok("2.34.0\n"))
                .expect(
                    "op",
                    vec!["whoami".into()],
                    err_out(1, "[ERROR] token rejected"),
                ),
        );
        let p = OpProvider::new(runner, true);
        match p.probe() {
            ProbeResult::NotAuthenticated { hint } => {
                assert!(hint.contains("token is valid"));
            }
            other => panic!("expected NotAuthenticated, got {other:?}"),
        }
    }

    #[test]
    fn probe_ok_when_binary_present_token_set_and_whoami_succeeds() {
        let runner = Arc::new(
            ScriptedRunner::new()
                .expect("op", vec!["--version".into()], ok("2.34.0\n"))
                .expect(
                    "op",
                    vec!["whoami".into()],
                    ok("URL: https://my.1password.com\n"),
                ),
        );
        let p = OpProvider::new(runner, true);
        assert!(matches!(p.probe(), ProbeResult::Ok));
    }
}

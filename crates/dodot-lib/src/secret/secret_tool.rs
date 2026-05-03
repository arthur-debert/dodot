//! `secret-tool` provider — freedesktop Secret Service via the
//! `secret-tool` command-line client (typically shipped by
//! `libsecret-tools` / `libsecret` on most Linux distributions).
//!
//! Reference shape: `secret-tool:<service>` or
//! `secret-tool:<service>/<account>`.
//!
//! - `secret-tool:GitHub` → first item with attribute
//!   `service=GitHub`, regardless of other attributes.
//! - `secret-tool:GitHub/alice` → item with both `service=GitHub`
//!   and `account=alice`.
//!
//! Resolution: `secret-tool lookup service <service>
//! [account <account>]` — emits the secret on stdout, exit 0
//! on success, exit 1 on miss with no stderr text (the canonical
//! freedesktop "no result" shape).
//!
//! Auth model: the user's session daemon (gnome-keyring,
//! keepassxc with the SecretService plugin, KDE Wallet, etc.)
//! is unlocked at session start by D-Bus activation. dodot does
//! NOT call `secret-tool unlock` (no such command anyway —
//! unlocking is daemon-driven). When the keyring is locked, the
//! daemon prompts the user via the desktop session; in
//! non-interactive contexts the lookup fails and we surface a
//! "keyring locked" diagnostic.
//!
//! Platform: Linux primarily. macOS users should reach for the
//! `keychain` provider instead. On hosts without `secret-tool`
//! installed, `probe()` returns `NotInstalled` with the typical
//! `apt install libsecret-tools` / `dnf install libsecret`
//! pointer.
//!
//! Why we don't expose generic attribute pairs in the reference
//! syntax (e.g. `secret-tool:k1=v1,k2=v2`): the most common
//! libsecret schema is the GNOME default with `service` and
//! `account` attributes. Power users with custom schemas can
//! still adopt secrets storage, but they lose the per-reference
//! attribute flexibility — accept the simplification for now;
//! a future extension can layer attribute pairs on top without
//! breaking the existing shape.
//!
//! See `secrets.lex` §5.2 / §5.4 (provider table + error UX) and
//! §S4 (OS-level providers).

use std::sync::Arc;

use crate::datastore::CommandRunner;
use crate::secret::provider::{ProbeResult, SecretProvider};
use crate::secret::secret_string::SecretString;
use crate::{DodotError, Result};

pub struct SecretToolProvider {
    runner: Arc<dyn CommandRunner>,
}

impl SecretToolProvider {
    pub fn new(runner: Arc<dyn CommandRunner>) -> Self {
        Self { runner }
    }

    pub fn from_env(runner: Arc<dyn CommandRunner>) -> Self {
        Self::new(runner)
    }

    /// Same shape as the keychain provider: `<service>` or
    /// `<service>/<account>`. Empty-segment cases all reject up
    /// front with hints pointing at the canonical syntax.
    fn parse_reference(suffix: &str) -> Result<(&str, Option<&str>)> {
        if suffix.is_empty() {
            return Err(DodotError::Other(
                "secret-tool reference is empty. Expected \
                 `secret-tool:<service>[/<account>]`."
                    .into(),
            ));
        }
        let (service, account) = match suffix.split_once('/') {
            Some((s, a)) => (s, Some(a)),
            None => (suffix, None),
        };
        if service.is_empty() {
            return Err(DodotError::Other(format!(
                "secret-tool reference `secret-tool:{suffix}` \
                 has an empty service name."
            )));
        }
        if let Some(a) = account {
            if a.is_empty() {
                return Err(DodotError::Other(format!(
                    "secret-tool reference `secret-tool:{suffix}` \
                     has an empty account name. Either drop the trailing `/` \
                     (use `secret-tool:<service>` for a service-only lookup) \
                     or supply an account: `secret-tool:<service>/<account>`."
                )));
            }
        }
        Ok((service, account))
    }
}

impl SecretProvider for SecretToolProvider {
    fn scheme(&self) -> &str {
        "secret-tool"
    }

    fn probe(&self) -> ProbeResult {
        // Step 1: binary on PATH. `secret-tool --version` is
        // cheap and doesn't touch the daemon.
        match self.runner.run("secret-tool", &["--version".into()]) {
            Ok(out) if out.exit_code == 0 => {}
            Ok(_) => {
                return ProbeResult::ProbeFailed {
                    details: "`secret-tool --version` returned non-zero — \
                              the binary is on PATH but not behaving as \
                              expected"
                        .into(),
                };
            }
            Err(_) => {
                return ProbeResult::NotInstalled {
                    hint: "install secret-tool: \
                           `apt install libsecret-tools` (Debian/Ubuntu), \
                           `dnf install libsecret` (Fedora), \
                           `pacman -S libsecret` (Arch). \
                           On macOS, use the `keychain` provider instead \
                           (`[secret.providers.keychain] enabled = true`)."
                        .into(),
                };
            }
        }
        // We deliberately don't probe the daemon's lock state —
        // there's no read-only `secret-tool` subcommand that
        // probes daemon health without a real lookup. Auth /
        // lock failures surface at resolve time with the daemon's
        // own error text.
        ProbeResult::Ok
    }

    fn resolve(&self, reference: &str) -> Result<SecretString> {
        let (service, account) = Self::parse_reference(reference)?;
        let mut args: Vec<String> = vec!["lookup".into(), "service".into(), service.into()];
        if let Some(a) = account {
            args.push("account".into());
            args.push(a.into());
        }
        let out = self.runner.run("secret-tool", &args)?;
        if out.exit_code != 0 {
            let stderr = out.stderr.trim();
            // `secret-tool lookup` exits 1 with empty stdout AND
            // empty stderr when the item simply isn't found —
            // canonical freedesktop "no result" shape. Locked-
            // keyring errors come through with a daemon-emitted
            // diagnostic on stderr ("The name is not activatable",
            // "Keyring is locked", etc., depending on backend).
            let err_msg =
                if (out.exit_code == 1 && stderr.is_empty()) || stderr.contains("not found") {
                    let qualifier = match account {
                        Some(a) => format!("(service `{service}`, account `{a}`)"),
                        None => format!("(service `{service}`)"),
                    };
                    format!(
                        "secret `secret-tool:{reference}` not found in the keyring {qualifier}. \
                     Verify with `secret-tool search service '{service}'`{}; \
                     add via `secret-tool store --label='<label>' service '{service}' \
                     {}` (you'll be prompted for the value).",
                        account
                            .map(|a| format!(" account '{a}'"))
                            .unwrap_or_default(),
                        account
                            .map(|a| format!("account '{a}'"))
                            .unwrap_or_else(|| "<key> <value>".into()),
                    )
                } else if stderr.contains("locked")
                    || stderr.contains("not activatable")
                    || stderr.contains("session bus")
                    || stderr.contains("Could not connect")
                    || stderr.contains("autolaunch D-Bus")
                    || stderr.contains("DBUS_SESSION_BUS_ADDRESS")
                {
                    format!(
                        "secret resolution for `secret-tool:{reference}` failed: \
                     the keyring is locked or no Secret Service daemon is \
                     responding ({stderr}). Unlock the keyring via your \
                     desktop session, or start a daemon (e.g. \
                     `gnome-keyring-daemon --start --components=secrets`)."
                    )
                } else if stderr.is_empty() {
                    format!(
                        "`secret-tool lookup` exited with code {} \
                     (no diagnostic output)",
                        out.exit_code
                    )
                } else {
                    format!(
                        "`secret-tool lookup` failed (exit {}): {stderr}",
                        out.exit_code
                    )
                };
            return Err(DodotError::Other(err_msg));
        }
        // `secret-tool lookup` does NOT add a trailing newline
        // (unlike `pass` / `op` / `bw` / `security`), but we
        // still strip one if present for symmetry with the other
        // providers — better to be lenient.
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
        let (s, a) = SecretToolProvider::parse_reference("GitHub").unwrap();
        assert_eq!(s, "GitHub");
        assert_eq!(a, None);
    }

    #[test]
    fn parse_reference_service_and_account() {
        let (s, a) = SecretToolProvider::parse_reference("GitHub/alice").unwrap();
        assert_eq!(s, "GitHub");
        assert_eq!(a, Some("alice"));
    }

    #[test]
    fn parse_reference_rejects_empty_suffix() {
        let e = SecretToolProvider::parse_reference("")
            .unwrap_err()
            .to_string();
        assert!(e.contains("empty"));
    }

    #[test]
    fn parse_reference_rejects_empty_service() {
        let e = SecretToolProvider::parse_reference("/alice")
            .unwrap_err()
            .to_string();
        assert!(e.contains("empty service"));
    }

    #[test]
    fn parse_reference_rejects_trailing_slash() {
        let e = SecretToolProvider::parse_reference("GitHub/")
            .unwrap_err()
            .to_string();
        assert!(e.contains("empty account"));
        assert!(e.contains("drop the trailing"));
    }

    // ── probe ───────────────────────────────────────────────────

    #[test]
    fn probe_ok_when_binary_present() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "secret-tool",
            vec!["--version".into()],
            ok("secret-tool 0.21.4\n"),
        ));
        let p = SecretToolProvider::new(runner);
        assert!(matches!(p.probe(), ProbeResult::Ok));
    }

    #[test]
    fn probe_not_installed_when_runner_errors() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "secret-tool",
            vec!["--version".into()],
            Err("command not found: secret-tool".into()),
        ));
        let p = SecretToolProvider::new(runner);
        match p.probe() {
            ProbeResult::NotInstalled { hint } => {
                assert!(hint.contains("apt install libsecret-tools"));
                assert!(hint.contains("On macOS, use the `keychain` provider"));
            }
            other => panic!("expected NotInstalled, got {other:?}"),
        }
    }

    #[test]
    fn probe_failed_when_version_returns_nonzero() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "secret-tool",
            vec!["--version".into()],
            err_out(1, "broken"),
        ));
        let p = SecretToolProvider::new(runner);
        assert!(matches!(p.probe(), ProbeResult::ProbeFailed { .. }));
    }

    // ── resolve ─────────────────────────────────────────────────

    #[test]
    fn resolve_service_only_invokes_lookup_correctly() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "secret-tool",
            vec!["lookup".into(), "service".into(), "GitHub".into()],
            ok("ghp_abc123"),
        ));
        let p = SecretToolProvider::new(runner);
        let v = p.resolve("GitHub").unwrap();
        assert_eq!(v.expose().unwrap(), "ghp_abc123");
    }

    #[test]
    fn resolve_with_account_threads_account_attribute() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "secret-tool",
            vec![
                "lookup".into(),
                "service".into(),
                "GitHub".into(),
                "account".into(),
                "alice".into(),
            ],
            ok("alice-token"),
        ));
        let p = SecretToolProvider::new(runner);
        let v = p.resolve("GitHub/alice").unwrap();
        assert_eq!(v.expose().unwrap(), "alice-token");
    }

    #[test]
    fn resolve_maps_exit_1_empty_stderr_to_not_found() {
        // Canonical freedesktop "no result" shape: exit 1, both
        // stdout and stderr empty.
        let runner = Arc::new(ScriptedRunner::new().expect(
            "secret-tool",
            vec!["lookup".into(), "service".into(), "missing".into()],
            err_out(1, ""),
        ));
        let p = SecretToolProvider::new(runner);
        let e = p.resolve("missing").unwrap_err().to_string();
        assert!(e.contains("not found"));
        assert!(e.contains("`missing`"));
        assert!(e.contains("secret-tool store"));
    }

    #[test]
    fn resolve_not_found_qualifier_includes_account_when_provided() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "secret-tool",
            vec![
                "lookup".into(),
                "service".into(),
                "GitHub".into(),
                "account".into(),
                "missing".into(),
            ],
            err_out(1, ""),
        ));
        let p = SecretToolProvider::new(runner);
        let e = p.resolve("GitHub/missing").unwrap_err().to_string();
        assert!(e.contains("`GitHub`"));
        assert!(e.contains("`missing`"));
        assert!(e.contains("account 'missing'"));
    }

    #[test]
    fn resolve_maps_locked_keyring_to_actionable_diagnostic() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "secret-tool",
            vec!["lookup".into(), "service".into(), "GitHub".into()],
            err_out(1, "Cannot autolaunch D-Bus without X11 $DISPLAY"),
        ));
        let p = SecretToolProvider::new(runner);
        let e = p.resolve("GitHub").unwrap_err().to_string();
        assert!(e.contains("locked or no Secret Service daemon"));
        assert!(e.contains("gnome-keyring-daemon"));
    }

    #[test]
    fn resolve_passes_through_unrecognized_stderr() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "secret-tool",
            vec!["lookup".into(), "service".into(), "GitHub".into()],
            err_out(2, "weird internal failure"),
        ));
        let p = SecretToolProvider::new(runner);
        let e = p.resolve("GitHub").unwrap_err().to_string();
        assert!(e.contains("weird internal failure"));
        assert!(e.contains("exit 2"));
    }

    #[test]
    fn resolve_strips_one_trailing_newline_when_present() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "secret-tool",
            vec!["lookup".into(), "service".into(), "k".into()],
            ok("value-with-trailing-blank\n\n"),
        ));
        let p = SecretToolProvider::new(runner);
        let v = p.resolve("k").unwrap();
        assert_eq!(v.expose().unwrap(), "value-with-trailing-blank\n");
    }

    #[test]
    fn resolve_keeps_value_intact_when_no_trailing_newline() {
        // Unlike most CLIs, secret-tool does not append a newline.
        // Pin that we don't strip anything when the bytes already
        // end without one.
        let runner = Arc::new(ScriptedRunner::new().expect(
            "secret-tool",
            vec!["lookup".into(), "service".into(), "k".into()],
            ok("exact-bytes-no-trailing-newline"),
        ));
        let p = SecretToolProvider::new(runner);
        let v = p.resolve("k").unwrap();
        assert_eq!(v.expose().unwrap(), "exact-bytes-no-trailing-newline");
    }
}

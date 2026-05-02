//! `bw` provider — Bitwarden CLI integration.
//!
//! Reference shape: `bw:<item>` or `bw:<item>#<field>`.
//!
//! - `bw:gh-token` → resolves the password field of the item named
//!   `gh-token` (the most common shape — Bitwarden items default to
//!   storing a password).
//! - `bw:gh-token#username` → resolves a different first-class field
//!   (`username`, `password`, `notes`, `totp`, `uri`).
//!
//! Custom (user-defined) fields are out of scope for Phase S2; they
//! require parsing the item JSON and walking the `fields` array, and
//! the design intentionally keeps the Phase S2 surface narrow. A
//! later phase can add `bw:<item>#field.<custom>` without breaking
//! anything here.
//!
//! Resolution: `bw get <field> <item>` → emits the value on stdout,
//! exit 0 on success, non-zero with diagnostic text on stderr for
//! "item not found", "vault locked", or "not logged in".
//!
//! Auth model: bw needs an unlocked vault. `bw status` reports one
//! of `unauthenticated` / `locked` / `unlocked` as JSON; we map the
//! first two to `NotAuthenticated` with the corresponding fix-it
//! hint, and only return Ok when the vault is unlocked. The CLI
//! reads the session key from `BW_SESSION` env var when set.
//!
//! See `secrets.lex` §5.2 (provider table) and §5.4 (error UX).

use std::sync::Arc;

use crate::datastore::CommandRunner;
use crate::secret::provider::{ProbeResult, SecretProvider};
use crate::secret::secret_string::SecretString;
use crate::{DodotError, Result};

/// First-class fields the Bitwarden CLI exposes via `bw get <field>`.
/// Listed in the same order as the `bw get --help` output.
const FIRST_CLASS_FIELDS: &[&str] = &["password", "username", "notes", "totp", "uri"];

/// Default field when the reference omits `#field`. Bitwarden items
/// are predominantly used for credentials, so `password` is the
/// principled default.
const DEFAULT_FIELD: &str = "password";

/// `SecretProvider` impl for the Bitwarden CLI (`bw`).
pub struct BwProvider {
    runner: Arc<dyn CommandRunner>,
}

impl BwProvider {
    pub fn new(runner: Arc<dyn CommandRunner>) -> Self {
        Self { runner }
    }

    /// Construct from the process environment. No env vars are read
    /// at construction (auth comes from `BW_SESSION` which the bw
    /// binary reads itself); the function exists for symmetry with
    /// `OpProvider::from_env` and to keep the call-site shape
    /// uniform across providers.
    pub fn from_env(runner: Arc<dyn CommandRunner>) -> Self {
        Self::new(runner)
    }

    /// Parse the suffix the registry hands us into `(item, field)`.
    ///
    /// The registry has already stripped the `bw:` scheme prefix.
    /// `<item>` is everything before the optional `#`, `<field>` is
    /// everything after — must be one of [`FIRST_CLASS_FIELDS`] or
    /// the parser refuses with an actionable error so the user
    /// catches typos at render time rather than getting a generic
    /// "item not found" from the CLI.
    fn parse_reference(suffix: &str) -> Result<(&str, &str)> {
        if suffix.is_empty() {
            return Err(DodotError::Other(
                "bw reference is empty. Expected `bw:<item>[#<field>]`.".into(),
            ));
        }
        let (item, field) = match suffix.split_once('#') {
            Some((i, f)) => (i, f),
            None => (suffix, DEFAULT_FIELD),
        };
        if item.is_empty() {
            return Err(DodotError::Other(format!(
                "bw reference `bw:{suffix}` has an empty item name. \
                 Expected `bw:<item>[#<field>]`."
            )));
        }
        if !FIRST_CLASS_FIELDS.contains(&field) {
            return Err(DodotError::Other(format!(
                "bw reference `bw:{suffix}` requests field `{field}`, \
                 which is not a first-class Bitwarden field. \
                 Supported fields are: {}.",
                FIRST_CLASS_FIELDS.join(", ")
            )));
        }
        Ok((item, field))
    }
}

impl SecretProvider for BwProvider {
    fn scheme(&self) -> &str {
        "bw"
    }

    fn probe(&self) -> ProbeResult {
        // Step 1: binary on PATH? `bw --version` doesn't hit the
        // network and doesn't unlock anything.
        match self.runner.run("bw", &["--version".into()]) {
            Ok(out) if out.exit_code == 0 => {}
            Ok(_) => {
                return ProbeResult::ProbeFailed {
                    details: "`bw --version` returned non-zero — the binary is on PATH \
                              but not behaving as expected"
                        .into(),
                };
            }
            Err(_) => {
                return ProbeResult::NotInstalled {
                    hint: "install Bitwarden CLI: \
                           https://bitwarden.com/help/cli/ \
                           (e.g. `brew install bitwarden-cli`, `npm install -g @bitwarden/cli`)"
                        .into(),
                };
            }
        }

        // Step 2: vault state. `bw status` emits a JSON document
        // with a `status` field of `unauthenticated` / `locked` /
        // `unlocked`. We pattern-match on the substring rather than
        // pulling in a JSON parser for this single field — the
        // status enum is tiny and stable across bw versions.
        match self.runner.run("bw", &["status".into()]) {
            Ok(out) if out.exit_code == 0 => {
                let s = out.stdout.as_str();
                if s.contains(r#""status":"unlocked""#) || s.contains("\"status\": \"unlocked\"") {
                    ProbeResult::Ok
                } else if s.contains(r#""status":"locked""#) || s.contains("\"status\": \"locked\"")
                {
                    ProbeResult::NotAuthenticated {
                        hint: "Bitwarden vault is locked. Run `bw unlock` and export \
                               the returned BW_SESSION token, then re-run dodot."
                            .into(),
                    }
                } else if s.contains(r#""status":"unauthenticated""#)
                    || s.contains("\"status\": \"unauthenticated\"")
                {
                    ProbeResult::NotAuthenticated {
                        hint: "Bitwarden CLI is not logged in. Run `bw login` (or \
                               `bw login --apikey` for a service account), then \
                               `bw unlock` to mint a session token."
                            .into(),
                    }
                } else {
                    ProbeResult::ProbeFailed {
                        details: format!(
                            "`bw status` returned exit 0 but the output didn't contain \
                             a recognized status field. Output: {}",
                            s.trim()
                        ),
                    }
                }
            }
            Ok(out) => ProbeResult::ProbeFailed {
                details: format!(
                    "`bw status` exited with code {}: {}",
                    out.exit_code,
                    out.stderr.trim()
                ),
            },
            Err(_) => ProbeResult::ProbeFailed {
                details: "could not run `bw status` after a successful `bw --version`; \
                          intermittent subprocess failure"
                    .into(),
            },
        }
    }

    fn resolve(&self, reference: &str) -> Result<SecretString> {
        let (item, field) = Self::parse_reference(reference)?;
        let out = self
            .runner
            .run("bw", &["get".into(), field.into(), item.into()])?;
        if out.exit_code != 0 {
            let stderr = out.stderr.trim();
            let err_msg = if stderr.contains("Not found") || stderr.contains("More than one result")
            {
                format!(
                    "secret `bw:{reference}` not found in the vault \
                     (or matched multiple items). \
                     Verify with `bw list items --search '<item>'`; \
                     use the item id if the search is ambiguous."
                )
            } else if stderr.contains("not logged in")
                || stderr.contains("vault is locked")
                || stderr.contains("Vault is locked")
            {
                format!(
                    "secret resolution for `bw:{reference}` failed: \
                     Bitwarden vault is locked or unauthenticated. \
                     Run `bw unlock` and export the returned BW_SESSION token."
                )
            } else if stderr.is_empty() {
                format!("`bw get {field} {item}` exited with code {}", out.exit_code)
            } else {
                format!(
                    "`bw get {field} {item}` failed (exit {}): {stderr}",
                    out.exit_code
                )
            };
            return Err(DodotError::Other(err_msg));
        }
        // bw emits the value to stdout and (depending on the bw
        // version) sometimes adds a trailing newline. Strip one
        // trailing '\n' if present — same shape as op.
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
    fn parse_reference_defaults_to_password_when_no_fragment() {
        let (item, field) = BwProvider::parse_reference("gh-token").unwrap();
        assert_eq!(item, "gh-token");
        assert_eq!(field, "password");
    }

    #[test]
    fn parse_reference_extracts_explicit_field() {
        let (item, field) = BwProvider::parse_reference("gh-token#username").unwrap();
        assert_eq!(item, "gh-token");
        assert_eq!(field, "username");
    }

    #[test]
    fn parse_reference_rejects_empty_suffix() {
        let e = BwProvider::parse_reference("").unwrap_err().to_string();
        assert!(e.contains("empty"));
    }

    #[test]
    fn parse_reference_rejects_empty_item() {
        let e = BwProvider::parse_reference("#password")
            .unwrap_err()
            .to_string();
        assert!(e.contains("empty item name"));
    }

    #[test]
    fn parse_reference_rejects_unsupported_field() {
        let e = BwProvider::parse_reference("gh-token#fingerprint")
            .unwrap_err()
            .to_string();
        assert!(e.contains("not a first-class"));
        // Surfaces the supported list so the user knows what they can
        // pick.
        for f in FIRST_CLASS_FIELDS {
            assert!(e.contains(f), "error message missing supported field {f}");
        }
    }

    // ── probe ───────────────────────────────────────────────────

    #[test]
    fn probe_ok_when_binary_present_and_vault_unlocked() {
        let runner = Arc::new(
            ScriptedRunner::new()
                .expect("bw", vec!["--version".into()], ok("2026.4.1\n"))
                .expect(
                    "bw",
                    vec!["status".into()],
                    ok(r#"{"serverUrl":null,"status":"unlocked"}"#),
                ),
        );
        let p = BwProvider::new(runner);
        assert!(matches!(p.probe(), ProbeResult::Ok));
    }

    #[test]
    fn probe_not_installed_when_runner_errors() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "bw",
            vec!["--version".into()],
            Err("command not found: bw".into()),
        ));
        let p = BwProvider::new(runner);
        match p.probe() {
            ProbeResult::NotInstalled { hint } => {
                assert!(hint.contains("install Bitwarden CLI"));
                assert!(hint.contains("brew install bitwarden-cli"));
            }
            other => panic!("expected NotInstalled, got {other:?}"),
        }
    }

    #[test]
    fn probe_not_authenticated_when_vault_locked() {
        let runner = Arc::new(
            ScriptedRunner::new()
                .expect("bw", vec!["--version".into()], ok("2026.4.1\n"))
                .expect(
                    "bw",
                    vec!["status".into()],
                    ok(r#"{"serverUrl":null,"status":"locked"}"#),
                ),
        );
        let p = BwProvider::new(runner);
        match p.probe() {
            ProbeResult::NotAuthenticated { hint } => {
                assert!(hint.contains("locked"));
                assert!(hint.contains("bw unlock"));
                assert!(hint.contains("BW_SESSION"));
            }
            other => panic!("expected NotAuthenticated, got {other:?}"),
        }
    }

    #[test]
    fn probe_not_authenticated_when_unauthenticated() {
        let runner = Arc::new(
            ScriptedRunner::new()
                .expect("bw", vec!["--version".into()], ok("2026.4.1\n"))
                .expect(
                    "bw",
                    vec!["status".into()],
                    ok(r#"{"status":"unauthenticated"}"#),
                ),
        );
        let p = BwProvider::new(runner);
        match p.probe() {
            ProbeResult::NotAuthenticated { hint } => {
                assert!(hint.contains("not logged in"));
                assert!(hint.contains("bw login"));
            }
            other => panic!("expected NotAuthenticated, got {other:?}"),
        }
    }

    #[test]
    fn probe_failed_when_status_output_unrecognized() {
        let runner = Arc::new(
            ScriptedRunner::new()
                .expect("bw", vec!["--version".into()], ok("2026.4.1\n"))
                .expect("bw", vec!["status".into()], ok("not json")),
        );
        let p = BwProvider::new(runner);
        assert!(matches!(p.probe(), ProbeResult::ProbeFailed { .. }));
    }

    // ── resolve ─────────────────────────────────────────────────

    #[test]
    fn resolve_default_field_invokes_bw_get_password() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "bw",
            vec!["get".into(), "password".into(), "gh-token".into()],
            ok("ghp_abc123\n"),
        ));
        let p = BwProvider::new(runner);
        let v = p.resolve("gh-token").unwrap();
        assert_eq!(v.expose().unwrap(), "ghp_abc123");
    }

    #[test]
    fn resolve_explicit_field_routes_to_correct_subcommand() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "bw",
            vec!["get".into(), "username".into(), "gh-token".into()],
            ok("debert+dodot\n"),
        ));
        let p = BwProvider::new(runner);
        let v = p.resolve("gh-token#username").unwrap();
        assert_eq!(v.expose().unwrap(), "debert+dodot");
    }

    #[test]
    fn resolve_maps_not_found_to_actionable_message() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "bw",
            vec!["get".into(), "password".into(), "missing".into()],
            err_out(1, "Not found."),
        ));
        let p = BwProvider::new(runner);
        let e = p.resolve("missing").unwrap_err().to_string();
        assert!(e.contains("not found"));
        assert!(e.contains("bw list items"));
    }

    #[test]
    fn resolve_maps_locked_vault_to_lock_diagnostic() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "bw",
            vec!["get".into(), "password".into(), "gh-token".into()],
            err_out(1, "Vault is locked."),
        ));
        let p = BwProvider::new(runner);
        let e = p.resolve("gh-token").unwrap_err().to_string();
        assert!(e.contains("locked or unauthenticated"));
    }

    #[test]
    fn resolve_passes_through_unrecognized_stderr() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "bw",
            vec!["get".into(), "password".into(), "gh-token".into()],
            err_out(1, "some other failure"),
        ));
        let p = BwProvider::new(runner);
        let e = p.resolve("gh-token").unwrap_err().to_string();
        assert!(e.contains("some other failure"));
        assert!(e.contains("exit 1"));
    }

    #[test]
    fn resolve_strips_exactly_one_trailing_newline() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "bw",
            vec!["get".into(), "password".into(), "k".into()],
            ok("value-with-trailing-blank\n\n"),
        ));
        let p = BwProvider::new(runner);
        let v = p.resolve("k").unwrap();
        // Two newlines in stdout → one stays in the resolved value
        // (the "trim exactly one CLI-added newline" contract).
        assert_eq!(v.expose().unwrap(), "value-with-trailing-blank\n");
    }
}

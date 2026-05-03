//! `sops` provider — Mozilla SOPS integration.
//!
//! Reference shape: `sops:<file>#<dot.path>`.
//!
//! - `sops:secrets.yaml#database.password` → decrypts `secrets.yaml`
//!   relative to the dotfiles root, extracts the value at the
//!   nested key path `database.password`.
//! - `sops:/abs/path/secrets.yaml#api.key` → absolute paths bypass
//!   the dotfiles-root anchor.
//!
//! The dot-separated key path is translated to SOPS's bracketed
//! `--extract` syntax (`["database"]["password"]`). Array indexing
//! (`[0]`) is not supported in this phase; nested map keys cover
//! the dotfile use case fully.
//!
//! Auth: SOPS picks up its identity from the in-tree `.sops.yaml` +
//! whichever key source it's configured to use (age, gpg, cloud
//! KMS). dodot doesn't introspect that — `probe()` only checks the
//! binary; misconfigured key sources surface at resolve time as a
//! decrypt failure with the actual SOPS error text passed through
//! verbatim.
//!
//! See `secrets.lex` §5.2 (provider table) and §5.4 (error UX).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::datastore::CommandRunner;
use crate::secret::provider::{ProbeResult, SecretProvider};
use crate::secret::secret_string::SecretString;
use crate::{DodotError, Result};

/// `SecretProvider` impl for SOPS.
pub struct SopsProvider {
    runner: Arc<dyn CommandRunner>,
    /// Anchor for relative file paths in references. Built from the
    /// pather's `dotfiles_root()` at registry construction.
    root: PathBuf,
}

impl SopsProvider {
    pub fn new(runner: Arc<dyn CommandRunner>, root: PathBuf) -> Self {
        Self { runner, root }
    }

    /// Parse the post-prefix reference into `(absolute_file_path,
    /// dot_path, extract_argument)`. The registry has already
    /// stripped the `sops:` scheme prefix; we expect
    /// `<file>#<dot.path>`.
    ///
    /// `extract_argument` is the SOPS `--extract` syntax
    /// (`["a"]["b"]["c"]`), built from the dot-separated path.
    /// Empty segments are rejected up-front (an `..` in the path
    /// would otherwise produce an empty bracket pair, which SOPS
    /// rejects with an opaque error).
    ///
    /// Each segment is escaped before being wrapped in quotes:
    /// `\` → `\\`, `"` → `\"`. SOPS's `--extract` uses a
    /// JSON-string-like syntax for each bracket key, so segments
    /// containing literal quotes or backslashes (legal YAML/JSON
    /// keys) need the same escaping the language requires.
    /// Without this, a key like `db."backup"` would land as
    /// `["db.\"backup\""]` invalidly closed and SOPS would reject
    /// the call with an opaque parse error.
    ///
    /// `dot_path` is the original user-facing path returned
    /// alongside `extract` so error messages can reference what
    /// the user actually wrote (`a.b.c`) instead of the internal
    /// bracket form (`["a"]["b"]["c"]`).
    fn parse_reference(&self, suffix: &str) -> Result<(PathBuf, String, String)> {
        let (file, path) = suffix.split_once('#').ok_or_else(|| {
            DodotError::Other(format!(
                "sops reference `sops:{suffix}` is missing the `#path.to.key` fragment. \
                 Expected `sops:<file>#<dot.path>` — for example \
                 `sops:secrets.yaml#database.password`."
            ))
        })?;
        if file.is_empty() {
            return Err(DodotError::Other(format!(
                "sops reference `sops:{suffix}` has an empty file name."
            )));
        }
        if path.is_empty() {
            return Err(DodotError::Other(format!(
                "sops reference `sops:{suffix}` has an empty key path. \
                 Expected at least one segment after `#`."
            )));
        }
        let segments: Vec<&str> = path.split('.').collect();
        if segments.iter().any(|s| s.is_empty()) {
            return Err(DodotError::Other(format!(
                "sops reference `sops:{suffix}` has an empty key segment. \
                 Use a single dot between segments (`a.b.c`), not `a..c`."
            )));
        }
        let mut extract = String::with_capacity(path.len() + segments.len() * 4);
        for seg in &segments {
            extract.push('[');
            extract.push('"');
            for ch in seg.chars() {
                match ch {
                    '\\' => extract.push_str(r"\\"),
                    '"' => extract.push_str(r#"\""#),
                    other => extract.push(other),
                }
            }
            extract.push('"');
            extract.push(']');
        }
        let file_path = if Path::new(file).is_absolute() {
            PathBuf::from(file)
        } else {
            self.root.join(file)
        };
        Ok((file_path, path.to_string(), extract))
    }
}

impl SecretProvider for SopsProvider {
    fn scheme(&self) -> &str {
        "sops"
    }

    fn probe(&self) -> ProbeResult {
        match self.runner.run("sops", &["--version".into()]) {
            Ok(out) if out.exit_code == 0 => ProbeResult::Ok,
            Ok(_) => ProbeResult::ProbeFailed {
                details: "`sops --version` returned non-zero — the binary is on PATH \
                          but not behaving as expected"
                    .into(),
            },
            Err(_) => ProbeResult::NotInstalled {
                hint: "install SOPS: https://github.com/getsops/sops/releases \
                       (e.g. `brew install sops`, or download a release tarball)"
                    .into(),
            },
        }
        // Note: we deliberately don't try to probe the key state
        // (age key, gpg-agent, KMS creds). SOPS auth depends on the
        // in-tree `.sops.yaml` + whichever key backend it's configured
        // for, and a meaningful auth probe would require an actual
        // decrypt of a known file — which we don't have. Decryption
        // failures surface at resolve time with the SOPS error text
        // passed through verbatim, which is more diagnostic than any
        // probe message we could synthesize.
    }

    fn resolve(&self, reference: &str) -> Result<SecretString> {
        let (file_path, dot_path, extract) = self.parse_reference(reference)?;
        let out = self.runner.run(
            "sops",
            &[
                "--decrypt".into(),
                "--extract".into(),
                extract.clone(),
                file_path.to_string_lossy().to_string(),
            ],
        )?;
        if out.exit_code != 0 {
            let stderr = out.stderr.trim();
            let err_msg = if stderr.contains("no such file")
                || stderr.contains("does not exist")
                || stderr.contains("no such file or directory")
            {
                format!(
                    "secret `sops:{reference}` references a file that doesn't exist: \
                     {}. Verify the path (relative paths are anchored at the dotfiles root).",
                    file_path.display()
                )
            } else if stderr.contains("not a sops") || stderr.contains("metadata not found") {
                format!(
                    "file `{}` is not a SOPS-encrypted document. \
                     Encrypt it with `sops --encrypt --in-place {}` first.",
                    file_path.display(),
                    file_path.display()
                )
            } else if stderr.contains("could not decrypt")
                || stderr.contains("decryption failed")
                || stderr.contains("MAC failure")
            {
                format!(
                    "decryption of `{}` failed: {stderr}. \
                     Check that the configured key (.sops.yaml) is available — \
                     SOPS_AGE_KEY_FILE for age, gpg-agent for gpg, AWS creds for KMS.",
                    file_path.display()
                )
            } else if stderr.contains("Key not found")
                || stderr.contains("no value at this path")
                || stderr.contains("invalid path")
            {
                // Report the user-facing dot path (what they wrote
                // after `#`), not the bracketed `--extract` argument
                // we hand to SOPS — the user shouldn't have to map
                // back from internal syntax to figure out which
                // reference is broken.
                format!(
                    "key path `{dot_path}` not found in `{}`. \
                     Verify with `sops --decrypt {} | yq` (or `jq` for JSON files).",
                    file_path.display(),
                    file_path.display()
                )
            } else if stderr.is_empty() {
                format!(
                    "`sops --decrypt --extract` exited with code {}",
                    out.exit_code
                )
            } else {
                format!(
                    "`sops --decrypt --extract '{extract}' {}` failed (exit {}): {stderr}",
                    file_path.display(),
                    out.exit_code
                )
            };
            return Err(DodotError::Other(err_msg));
        }
        // sops emits the value to stdout. Strip exactly one trailing
        // newline (same contract as op / bw / pass).
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
    fn root() -> PathBuf {
        PathBuf::from("/dotfiles")
    }

    // ── parse_reference ─────────────────────────────────────────

    #[test]
    fn parse_reference_translates_dot_path_to_bracket_extract() {
        let p = SopsProvider::new(Arc::new(ScriptedRunner::new()), root());
        let (file, dot_path, extract) = p.parse_reference("secrets.yaml#a.b.c").unwrap();
        assert_eq!(file, PathBuf::from("/dotfiles/secrets.yaml"));
        assert_eq!(dot_path, "a.b.c");
        assert_eq!(extract, r#"["a"]["b"]["c"]"#);
    }

    #[test]
    fn parse_reference_keeps_absolute_paths_unchanged() {
        let p = SopsProvider::new(Arc::new(ScriptedRunner::new()), root());
        let (file, _, _) = p.parse_reference("/etc/secrets.yaml#k").unwrap();
        assert_eq!(file, PathBuf::from("/etc/secrets.yaml"));
    }

    #[test]
    fn parse_reference_anchors_relative_paths_at_dotfiles_root() {
        let p = SopsProvider::new(Arc::new(ScriptedRunner::new()), root());
        let (file, _, _) = p.parse_reference("nested/secrets.yaml#k").unwrap();
        assert_eq!(file, PathBuf::from("/dotfiles/nested/secrets.yaml"));
    }

    #[test]
    fn parse_reference_escapes_quotes_and_backslashes_in_segments() {
        // SOPS's bracket-key syntax is JSON-string-shaped. A
        // literal `"` in a segment must escape to `\"`, and a
        // literal `\` must escape to `\\`. Without this, the
        // extract argument would be syntactically broken (or worse,
        // produce a different key than intended) — the user would
        // get an opaque SOPS error and no way to map it back.
        let p = SopsProvider::new(Arc::new(ScriptedRunner::new()), root());
        let (_, _, extract) = p
            .parse_reference(r#"s.yaml#has"quote.has\backslash"#)
            .unwrap();
        assert_eq!(extract, r#"["has\"quote"]["has\\backslash"]"#);
    }

    #[test]
    fn parse_reference_rejects_missing_fragment() {
        let p = SopsProvider::new(Arc::new(ScriptedRunner::new()), root());
        let e = p.parse_reference("secrets.yaml").unwrap_err().to_string();
        assert!(e.contains("missing the `#path.to.key` fragment"));
        assert!(e.contains("sops:secrets.yaml#database.password"));
    }

    #[test]
    fn parse_reference_rejects_empty_key_path() {
        let p = SopsProvider::new(Arc::new(ScriptedRunner::new()), root());
        let e = p.parse_reference("secrets.yaml#").unwrap_err().to_string();
        assert!(e.contains("empty key path"));
    }

    #[test]
    fn parse_reference_rejects_empty_segment() {
        let p = SopsProvider::new(Arc::new(ScriptedRunner::new()), root());
        let e = p.parse_reference("s.yaml#a..b").unwrap_err().to_string();
        assert!(e.contains("empty key segment"));
        assert!(e.contains("a..c"));
    }

    #[test]
    fn parse_reference_rejects_empty_file_name() {
        let p = SopsProvider::new(Arc::new(ScriptedRunner::new()), root());
        let e = p.parse_reference("#k").unwrap_err().to_string();
        assert!(e.contains("empty file name"));
    }

    // ── probe ───────────────────────────────────────────────────

    #[test]
    fn probe_ok_when_binary_present() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "sops",
            vec!["--version".into()],
            ok("sops 3.10.2 (latest)\n"),
        ));
        let p = SopsProvider::new(runner, root());
        assert!(matches!(p.probe(), ProbeResult::Ok));
    }

    #[test]
    fn probe_not_installed_when_runner_errors() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "sops",
            vec!["--version".into()],
            Err("command not found: sops".into()),
        ));
        let p = SopsProvider::new(runner, root());
        match p.probe() {
            ProbeResult::NotInstalled { hint } => {
                assert!(hint.contains("install SOPS"));
                assert!(hint.contains("brew install sops"));
            }
            other => panic!("expected NotInstalled, got {other:?}"),
        }
    }

    #[test]
    fn probe_failed_when_version_returns_nonzero() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "sops",
            vec!["--version".into()],
            err_out(1, "internal error"),
        ));
        let p = SopsProvider::new(runner, root());
        assert!(matches!(p.probe(), ProbeResult::ProbeFailed { .. }));
    }

    // ── resolve ─────────────────────────────────────────────────

    #[test]
    fn resolve_dispatches_decrypt_extract_with_correct_args() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "sops",
            vec![
                "--decrypt".into(),
                "--extract".into(),
                r#"["database"]["password"]"#.into(),
                "/dotfiles/secrets.yaml".into(),
            ],
            ok("hunter2\n"),
        ));
        let p = SopsProvider::new(runner, root());
        let v = p.resolve("secrets.yaml#database.password").unwrap();
        assert_eq!(v.expose().unwrap(), "hunter2");
    }

    #[test]
    fn resolve_maps_missing_file_to_actionable_message() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "sops",
            vec![
                "--decrypt".into(),
                "--extract".into(),
                r#"["k"]"#.into(),
                "/dotfiles/missing.yaml".into(),
            ],
            err_out(1, "open /dotfiles/missing.yaml: no such file or directory"),
        ));
        let p = SopsProvider::new(runner, root());
        let e = p.resolve("missing.yaml#k").unwrap_err().to_string();
        assert!(e.contains("doesn't exist"));
        assert!(e.contains("/dotfiles/missing.yaml"));
        assert!(e.contains("relative paths"));
    }

    #[test]
    fn resolve_maps_unencrypted_file_to_encrypt_in_place_hint() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "sops",
            vec![
                "--decrypt".into(),
                "--extract".into(),
                r#"["k"]"#.into(),
                "/dotfiles/plain.yaml".into(),
            ],
            err_out(203, "sops metadata not found"),
        ));
        let p = SopsProvider::new(runner, root());
        let e = p.resolve("plain.yaml#k").unwrap_err().to_string();
        assert!(e.contains("not a SOPS-encrypted document"));
        assert!(e.contains("--encrypt --in-place"));
    }

    #[test]
    fn resolve_maps_decrypt_failure_to_key_diagnostic() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "sops",
            vec![
                "--decrypt".into(),
                "--extract".into(),
                r#"["k"]"#.into(),
                "/dotfiles/s.yaml".into(),
            ],
            err_out(128, "could not decrypt: MAC failure"),
        ));
        let p = SopsProvider::new(runner, root());
        let e = p.resolve("s.yaml#k").unwrap_err().to_string();
        assert!(e.contains("decryption of"));
        assert!(e.contains("SOPS_AGE_KEY_FILE"));
        assert!(e.contains("MAC failure"));
    }

    #[test]
    fn resolve_maps_missing_key_path_to_yq_hint() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "sops",
            vec![
                "--decrypt".into(),
                "--extract".into(),
                r#"["a"]["b"]"#.into(),
                "/dotfiles/s.yaml".into(),
            ],
            err_out(91, "no value at this path: [\"a\"][\"b\"]"),
        ));
        let p = SopsProvider::new(runner, root());
        let e = p.resolve("s.yaml#a.b").unwrap_err().to_string();
        assert!(e.contains("not found in"));
        assert!(e.contains("yq"));
        // The user-facing error reports the dot path they wrote
        // (`a.b`), not the bracketed `--extract` shape (`["a"]["b"]`).
        // Otherwise users would have to mentally translate from
        // internal syntax back to the reference they wrote, which
        // is what makes a "missing key" message annoying.
        assert!(
            e.contains("`a.b`"),
            "expected user-facing dot path in error, got: {e}"
        );
        assert!(
            !e.contains("[\"a\"]"),
            "expected bracket form to not leak into user-facing error: {e}"
        );
    }

    #[test]
    fn resolve_passes_through_unrecognized_stderr_with_command_context() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "sops",
            vec![
                "--decrypt".into(),
                "--extract".into(),
                r#"["k"]"#.into(),
                "/dotfiles/s.yaml".into(),
            ],
            err_out(1, "weird internal failure"),
        ));
        let p = SopsProvider::new(runner, root());
        let e = p.resolve("s.yaml#k").unwrap_err().to_string();
        assert!(e.contains("weird internal failure"));
        assert!(e.contains("--decrypt --extract"));
        assert!(e.contains("exit 1"));
    }

    #[test]
    fn resolve_strips_exactly_one_trailing_newline() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "sops",
            vec![
                "--decrypt".into(),
                "--extract".into(),
                r#"["k"]"#.into(),
                "/dotfiles/s.yaml".into(),
            ],
            ok("multi-line-value\n\n"),
        ));
        let p = SopsProvider::new(runner, root());
        let v = p.resolve("s.yaml#k").unwrap();
        // Two newlines in stdout → one stays in resolved value.
        assert_eq!(v.expose().unwrap(), "multi-line-value\n");
    }
}

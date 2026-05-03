//! `age` whole-file preprocessor — decrypts `*.age` files at deploy
//! time.
//!
//! Matches files ending in `.age`, runs `age --decrypt --identity
//! <id_path> <source>`, and emits the plaintext as an
//! [`ExpandedFile`] with `deploy_mode = Some(0o600)` so the pipeline
//! chmods the rendered datastore file before the symlink lands at
//! the user's home. No template expansion happens — this is a pure
//! decrypt-and-emit operation.
//!
//! Reference flow (from `secrets.lex` §4.2):
//!
//!     1. Scan finds `ssh/id_ed25519.age`
//!     2. AgePreprocessor strips `.age` → expanded filename `id_ed25519`
//!     3. expand() shells out to age, captures plaintext
//!     4. Pipeline writes the bytes to the datastore + chmods 0600
//!     5. Symlink handler links it to `~/.ssh/id_ed25519`
//!
//! `age` reads its identity from the path passed via `--identity`.
//! When the config doesn't set one explicitly, we fall back to
//! `$AGE_IDENTITY` env var, then to `~/.config/age/identity.txt`
//! (the conventional default the age docs use). When none of those
//! exist, the preprocessor still attempts the call — `age` itself
//! emits a clear "no identity" error which we forward verbatim.
//!
//! See `secrets.lex` §4.1–§4.3 (supported formats, deployment flow,
//! mode 0600 enforcement) and `preprocessing-pipeline.lex` §2.3
//! (Opaque transform semantics).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::datastore::CommandRunner;
use crate::fs::Fs;
use crate::preprocessing::{ExpandedFile, Preprocessor, TransformType};
use crate::{DodotError, Result};

/// `age` decryption preprocessor. Constructed from
/// `[preprocessor.age]` config + the shared `CommandRunner`.
///
/// Holds the identity path resolved at construction so every
/// `expand()` call uses the same identity file (no re-reading of
/// env vars per file). The path is **not** validated to exist at
/// construction; `age` validates at decrypt time and emits a
/// diagnostic we surface verbatim if the file is missing.
pub struct AgePreprocessor {
    runner: Arc<dyn CommandRunner>,
    identity: PathBuf,
    /// Configured extensions (default `["age"]`). Stored without
    /// leading dots; `matches_extension` requires a literal `.`
    /// before the extension to avoid `id.age` matching `id`.
    extensions: Vec<String>,
}

impl AgePreprocessor {
    pub fn new(runner: Arc<dyn CommandRunner>, identity: PathBuf, extensions: Vec<String>) -> Self {
        let extensions: Vec<String> = extensions
            .into_iter()
            .map(|e| e.trim_start_matches('.').to_string())
            .collect();
        Self {
            runner,
            identity,
            extensions,
        }
    }

    /// Construct with the canonical `~/.config/age/identity.txt`
    /// default identity and the default `["age"]` extension set —
    /// matches what most users have installed via `age-keygen`.
    pub fn from_env(runner: Arc<dyn CommandRunner>) -> Self {
        let identity = std::env::var("AGE_IDENTITY")
            .map(PathBuf::from)
            .ok()
            .or_else(|| {
                std::env::var("HOME").ok().map(|h| {
                    let mut p = PathBuf::from(h);
                    p.push(".config/age/identity.txt");
                    p
                })
            })
            .unwrap_or_else(|| PathBuf::from("identity.txt"));
        Self::new(runner, identity, vec!["age".to_string()])
    }
}

impl Preprocessor for AgePreprocessor {
    fn name(&self) -> &str {
        "age"
    }

    fn transform_type(&self) -> TransformType {
        TransformType::Opaque
    }

    fn matches_extension(&self, filename: &str) -> bool {
        self.extensions.iter().any(|ext| {
            filename
                .strip_suffix(ext.as_str())
                .is_some_and(|prefix| prefix.ends_with('.'))
        })
    }

    fn stripped_name(&self, filename: &str) -> String {
        // Prefer the longest matching extension so an `.age.bak`
        // override (unlikely but possible per config) wins over a
        // bare `.age`. Same shape as TemplatePreprocessor.
        self.extensions
            .iter()
            .filter_map(|ext| {
                filename
                    .strip_suffix(ext.as_str())
                    .and_then(|prefix| prefix.strip_suffix('.'))
                    .map(|stripped| (ext.len(), stripped))
            })
            .max_by_key(|(len, _)| *len)
            .map(|(_, stripped)| stripped.to_string())
            .unwrap_or_else(|| filename.to_string())
    }

    fn expand(&self, source: &Path, _fs: &dyn Fs) -> Result<Vec<ExpandedFile>> {
        // `run_bytes` (not `run`) so binary plaintext (raw key
        // blobs, X.509 DER certs) round-trips verbatim. The
        // `String::from_utf8_lossy` decode in the line-buffered
        // `run` path corrupts non-UTF-8 bytes — fatal for
        // whole-file secrets.
        let out = self.runner.run_bytes(
            "age",
            &[
                "--decrypt".into(),
                "--identity".into(),
                self.identity.to_string_lossy().to_string(),
                source.to_string_lossy().to_string(),
            ],
        )?;
        if out.exit_code != 0 {
            let stderr = out.stderr.trim();
            // Map the most common diagnostic shapes to actionable
            // hints. age's stderr is short and stable; we surface
            // verbatim text where mapping isn't clear.
            let msg = if stderr.contains("no identity matched") {
                format!(
                    "age: no identity matched any of the recipients for `{}`. \
                     The decryption key in `{}` doesn't match the recipient \
                     this file was encrypted to. Re-encrypt the file with the \
                     correct recipient (`age -r <pubkey> -e ...`) or point \
                     `[preprocessor.age] identity` at the right key file.",
                    source.display(),
                    self.identity.display()
                )
            } else if stderr.contains("no such file")
                || stderr.contains("identity") && stderr.contains("does not exist")
            {
                format!(
                    "age: identity file `{}` not found. \
                     Generate one with `age-keygen -o {}`, or set \
                     `[preprocessor.age] identity` to point at an existing key.",
                    self.identity.display(),
                    self.identity.display()
                )
            } else if stderr.is_empty() {
                format!(
                    "age decryption of `{}` exited {} (no diagnostic output)",
                    source.display(),
                    out.exit_code
                )
            } else {
                // age's stderr does not echo plaintext; surfacing it
                // verbatim is safe and aids diagnosis.
                format!(
                    "age decryption of `{}` failed (exit {}): {stderr}",
                    source.display(),
                    out.exit_code
                )
            };
            return Err(DodotError::PreprocessorError {
                preprocessor: "age".into(),
                source_file: source.to_path_buf(),
                message: msg,
            });
        }
        let filename = source
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let stripped = self.stripped_name(&filename);
        Ok(vec![ExpandedFile {
            relative_path: PathBuf::from(stripped),
            content: out.stdout,
            is_dir: false,
            tracked_render: None,
            context_hash: None,
            secret_line_ranges: Vec::new(),
            // Per `secrets.lex` §4.3: rendered whole-file secrets
            // are 0600 regardless of the source file's mode.
            deploy_mode: Some(0o600),
        }])
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

    /// A scripted runner whose `run_bytes` returns canned raw-byte
    /// responses (so binary tests can pin verbatim round-trip).
    /// `run` is unimplemented — the only call site we exercise is
    /// `run_bytes`.
    type ScriptedBytesResponse = (
        String,
        Vec<String>,
        std::result::Result<crate::datastore::CommandOutputBytes, String>,
    );
    struct ScriptedBytesRunner {
        responses: Mutex<Vec<ScriptedBytesResponse>>,
    }
    impl ScriptedBytesRunner {
        fn new() -> Self {
            Self {
                responses: Mutex::new(Vec::new()),
            }
        }
        fn expect(
            self,
            exe: impl Into<String>,
            args: Vec<String>,
            response: std::result::Result<crate::datastore::CommandOutputBytes, String>,
        ) -> Self {
            self.responses
                .lock()
                .unwrap()
                .push((exe.into(), args, response));
            self
        }
    }
    impl CommandRunner for ScriptedBytesRunner {
        fn run(&self, _exe: &str, _args: &[String]) -> Result<CommandOutput> {
            unreachable!("ScriptedBytesRunner only supports run_bytes")
        }
        fn run_bytes(
            &self,
            exe: &str,
            args: &[String],
        ) -> Result<crate::datastore::CommandOutputBytes> {
            let mut r = self.responses.lock().unwrap();
            if r.is_empty() {
                return Err(DodotError::Other(format!(
                    "ScriptedBytesRunner: unexpected `{exe} {args:?}`"
                )));
            }
            let (e, a, out) = r.remove(0);
            assert_eq!(exe, e);
            assert_eq!(args, a.as_slice());
            out.map_err(DodotError::Other)
        }
    }
    fn ok_bytes(
        stdout: &[u8],
    ) -> std::result::Result<crate::datastore::CommandOutputBytes, String> {
        Ok(crate::datastore::CommandOutputBytes {
            exit_code: 0,
            stdout: stdout.to_vec(),
            stderr: String::new(),
        })
    }
    fn make_pp(runner: Arc<dyn CommandRunner>) -> AgePreprocessor {
        AgePreprocessor::new(runner, PathBuf::from("/k/id.txt"), vec!["age".into()])
    }
    fn null_fs() -> crate::fs::OsFs {
        crate::fs::OsFs::new()
    }

    // ── matches / stripped_name ─────────────────────────────────

    #[test]
    fn matches_extension_only_when_dot_age_is_a_real_suffix() {
        let p = make_pp(Arc::new(ScriptedRunner::new()));
        assert!(p.matches_extension("id_ed25519.age"));
        assert!(!p.matches_extension("foo.age.bak"));
        // `idage` is not `id.age`.
        assert!(!p.matches_extension("idage"));
    }

    #[test]
    fn stripped_name_drops_age_suffix() {
        let p = make_pp(Arc::new(ScriptedRunner::new()));
        assert_eq!(p.stripped_name("id_ed25519.age"), "id_ed25519");
    }

    // ── expand ──────────────────────────────────────────────────

    #[test]
    fn expand_invokes_age_with_decrypt_and_identity_args() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "age",
            vec![
                "--decrypt".into(),
                "--identity".into(),
                "/k/id.txt".into(),
                "/pack/secret.age".into(),
            ],
            ok("PLAINTEXT BYTES\n"),
        ));
        let p = make_pp(runner);
        let out = p.expand(Path::new("/pack/secret.age"), &null_fs()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].relative_path, PathBuf::from("secret"));
        assert_eq!(out[0].content, b"PLAINTEXT BYTES\n");
        assert_eq!(out[0].deploy_mode, Some(0o600));
        // Opaque preprocessors don't produce a tracked render or
        // context hash — the baseline cache won't try to reverse-
        // merge against them.
        assert!(out[0].tracked_render.is_none());
        assert!(out[0].context_hash.is_none());
    }

    #[test]
    fn expand_preserves_binary_plaintext_verbatim_via_run_bytes() {
        // Non-UTF-8 plaintext (a raw binary key blob) flows
        // through intact via `run_bytes`. The earlier `run` path
        // would have decoded stdout via `String::from_utf8_lossy`
        // and replaced 0xff / 0xfe with U+FFFD, corrupting
        // round-tripped bytes. Pin that the preprocessor goes
        // through `run_bytes` and the bytes survive verbatim.
        let raw = vec![0u8, 1, 2, 0xff, 0xfe, b'\n', 0x80, 0xc0];
        let runner = Arc::new(ScriptedBytesRunner::new().expect(
            "age",
            vec![
                "--decrypt".into(),
                "--identity".into(),
                "/k/id.txt".into(),
                "/pack/key.age".into(),
            ],
            ok_bytes(&raw),
        ));
        let p = make_pp(runner);
        let out = p.expand(Path::new("/pack/key.age"), &null_fs()).unwrap();
        assert_eq!(out[0].deploy_mode, Some(0o600));
        assert_eq!(out[0].relative_path, PathBuf::from("key"));
        assert_eq!(out[0].content, raw, "raw bytes must round-trip verbatim");
    }

    #[test]
    fn expand_maps_no_identity_match_to_recipient_diagnostic() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "age",
            vec![
                "--decrypt".into(),
                "--identity".into(),
                "/k/id.txt".into(),
                "/pack/x.age".into(),
            ],
            err_out(1, "age: error: no identity matched any of the recipients"),
        ));
        let p = make_pp(runner);
        let e = p
            .expand(Path::new("/pack/x.age"), &null_fs())
            .unwrap_err()
            .to_string();
        assert!(e.contains("no identity matched"));
        assert!(e.contains("Re-encrypt"));
        assert!(e.contains("/k/id.txt"));
    }

    #[test]
    fn expand_maps_missing_identity_file_to_generate_hint() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "age",
            vec![
                "--decrypt".into(),
                "--identity".into(),
                "/k/id.txt".into(),
                "/pack/x.age".into(),
            ],
            err_out(1, "age: error: identity file does not exist: /k/id.txt"),
        ));
        let p = make_pp(runner);
        let e = p
            .expand(Path::new("/pack/x.age"), &null_fs())
            .unwrap_err()
            .to_string();
        assert!(e.contains("identity file"));
        assert!(e.contains("not found"));
        assert!(e.contains("age-keygen"));
    }

    #[test]
    fn expand_passes_unrecognized_stderr_through_with_command_context() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "age",
            vec![
                "--decrypt".into(),
                "--identity".into(),
                "/k/id.txt".into(),
                "/pack/x.age".into(),
            ],
            err_out(1, "age: error: weird internal failure"),
        ));
        let p = make_pp(runner);
        let e = p
            .expand(Path::new("/pack/x.age"), &null_fs())
            .unwrap_err()
            .to_string();
        assert!(e.contains("weird internal failure"));
        assert!(e.contains("age decryption"));
        assert!(e.contains("exit 1"));
    }

    #[test]
    fn expand_handles_empty_stderr_failure() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "age",
            vec![
                "--decrypt".into(),
                "--identity".into(),
                "/k/id.txt".into(),
                "/pack/x.age".into(),
            ],
            err_out(2, ""),
        ));
        let p = make_pp(runner);
        let e = p
            .expand(Path::new("/pack/x.age"), &null_fs())
            .unwrap_err()
            .to_string();
        assert!(e.contains("exited 2"));
        assert!(e.contains("no diagnostic output"));
    }

    #[test]
    fn expand_propagates_runner_error_when_subprocess_fails_to_spawn() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "age",
            vec![
                "--decrypt".into(),
                "--identity".into(),
                "/k/id.txt".into(),
                "/pack/x.age".into(),
            ],
            Err("command not found: age".into()),
        ));
        let p = make_pp(runner);
        let e = p
            .expand(Path::new("/pack/x.age"), &null_fs())
            .unwrap_err()
            .to_string();
        // The CommandRunner-level error surfaces; users get told
        // age isn't installed without the preprocessor having to
        // probe at construction.
        assert!(e.contains("command not found: age"));
    }
}

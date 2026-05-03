//! `gpg` whole-file preprocessor — decrypts `*.gpg` (and optionally
//! `*.asc`) files at deploy time.
//!
//! Same shape as the age preprocessor: matches the configured
//! extensions, runs `gpg --decrypt --quiet --batch <source>`,
//! captures plaintext on stdout, and emits an [`ExpandedFile`]
//! with `deploy_mode = Some(0o600)` per `secrets.lex` §4.3.
//! `TransformType::Opaque` — no reverse path.
//!
//! Auth model differs from age: gpg picks up its identity from
//! `gpg-agent` rather than an explicit identity-file argument. For
//! a passphrase-protected key, the agent prompts (or pulls cached
//! credentials); for a YubiKey-backed key, the smartcard daemon
//! handles it. dodot doesn't introspect any of that — `--batch`
//! makes the call non-interactive at dodot's end so we don't block
//! a `dodot up` on a TTY-only prompt; if the agent isn't ready,
//! `gpg` exits with a clear "gpg-agent" diagnostic which we
//! surface.
//!
//! See `secrets.lex` §4.1–§4.3 and `preprocessing-pipeline.lex`
//! §2.3 (Opaque transform semantics).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::datastore::CommandRunner;
use crate::fs::Fs;
use crate::preprocessing::{ExpandedFile, Preprocessor, TransformType};
use crate::{DodotError, Result};

/// `gpg` decryption preprocessor. Constructed from
/// `[preprocessor.gpg]` config + the shared `CommandRunner`.
///
/// Configurable extensions (default `["gpg", "asc"]`) cover both
/// the binary-armored form (`.gpg`) and the ASCII-armored form
/// (`.asc`); the same `gpg --decrypt` call handles both.
pub struct GpgPreprocessor {
    runner: Arc<dyn CommandRunner>,
    extensions: Vec<String>,
}

impl GpgPreprocessor {
    pub fn new(runner: Arc<dyn CommandRunner>, extensions: Vec<String>) -> Self {
        let extensions: Vec<String> = extensions
            .into_iter()
            .map(|e| e.trim_start_matches('.').to_string())
            .collect();
        Self { runner, extensions }
    }

    pub fn from_env(runner: Arc<dyn CommandRunner>) -> Self {
        Self::new(runner, vec!["gpg".into(), "asc".into()])
    }
}

impl Preprocessor for GpgPreprocessor {
    fn name(&self) -> &str {
        "gpg"
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
        // `--batch` keeps gpg non-interactive at our end. `--quiet`
        // suppresses the "encrypted with N MB key, ID ..." banner
        // so stderr stays focused on real failures. The default
        // homedir / agent socket is used; the user's normal gpg
        // configuration applies.
        let out = self.runner.run(
            "gpg",
            &[
                "--decrypt".into(),
                "--quiet".into(),
                "--batch".into(),
                source.to_string_lossy().to_string(),
            ],
        )?;
        if out.exit_code != 0 {
            let stderr = out.stderr.trim();
            let msg = if stderr.contains("decryption failed") && stderr.contains("No secret key") {
                format!(
                    "gpg: no secret key for `{}`. \
                     The recipient this file was encrypted to isn't in your \
                     keyring. Import the matching private key (`gpg --import`) \
                     or re-encrypt with `gpg --encrypt --recipient <id>`.",
                    source.display()
                )
            } else if stderr.contains("gpg-agent") || stderr.contains("agent_genkey failed") {
                format!(
                    "gpg: gpg-agent isn't responsive for `{}`. \
                     Start it with `gpgconf --launch gpg-agent`, or check \
                     `~/.gnupg/gpg-agent.conf` and restart your session.",
                    source.display()
                )
            } else if stderr.contains("Bad session key") || stderr.contains("Bad passphrase") {
                format!(
                    "gpg: bad passphrase / session key for `{}`. \
                     gpg's `--batch` mode does not prompt; cache the \
                     passphrase in gpg-agent first (e.g. by decrypting \
                     interactively once) and retry.",
                    source.display()
                )
            } else if stderr.contains("No such file") || stderr.contains("can't open") {
                format!(
                    "gpg: source file `{}` not found or not readable.",
                    source.display()
                )
            } else if stderr.is_empty() {
                format!(
                    "gpg decryption of `{}` exited {} (no diagnostic output)",
                    source.display(),
                    out.exit_code
                )
            } else {
                format!(
                    "gpg decryption of `{}` failed (exit {}): {stderr}",
                    source.display(),
                    out.exit_code
                )
            };
            return Err(DodotError::PreprocessorError {
                preprocessor: "gpg".into(),
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
            content: out.stdout.into_bytes(),
            is_dir: false,
            tracked_render: None,
            context_hash: None,
            secret_line_ranges: Vec::new(),
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
    fn make_pp(runner: Arc<dyn CommandRunner>) -> GpgPreprocessor {
        GpgPreprocessor::new(runner, vec!["gpg".into(), "asc".into()])
    }
    fn null_fs() -> crate::fs::OsFs {
        crate::fs::OsFs::new()
    }

    // ── matches / stripped_name ─────────────────────────────────

    #[test]
    fn matches_extension_handles_both_gpg_and_asc() {
        let p = make_pp(Arc::new(ScriptedRunner::new()));
        assert!(p.matches_extension("Brewfile.gpg"));
        assert!(p.matches_extension("notes.txt.asc"));
        assert!(!p.matches_extension("plain.txt"));
        assert!(!p.matches_extension("foogpg"));
    }

    #[test]
    fn stripped_name_drops_either_extension() {
        let p = make_pp(Arc::new(ScriptedRunner::new()));
        assert_eq!(p.stripped_name("Brewfile.gpg"), "Brewfile");
        assert_eq!(p.stripped_name("notes.txt.asc"), "notes.txt");
    }

    // ── expand ──────────────────────────────────────────────────

    #[test]
    fn expand_invokes_gpg_with_decrypt_quiet_batch() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "gpg",
            vec![
                "--decrypt".into(),
                "--quiet".into(),
                "--batch".into(),
                "/pack/Brewfile.gpg".into(),
            ],
            ok("brew \"ripgrep\"\n"),
        ));
        let p = make_pp(runner);
        let out = p
            .expand(Path::new("/pack/Brewfile.gpg"), &null_fs())
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].relative_path, PathBuf::from("Brewfile"));
        assert_eq!(out[0].content, b"brew \"ripgrep\"\n");
        assert_eq!(out[0].deploy_mode, Some(0o600));
        assert!(out[0].tracked_render.is_none());
    }

    #[test]
    fn expand_strips_asc_extension_when_used() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "gpg",
            vec![
                "--decrypt".into(),
                "--quiet".into(),
                "--batch".into(),
                "/pack/notes.txt.asc".into(),
            ],
            ok("private notes\n"),
        ));
        let p = make_pp(runner);
        let out = p
            .expand(Path::new("/pack/notes.txt.asc"), &null_fs())
            .unwrap();
        assert_eq!(out[0].relative_path, PathBuf::from("notes.txt"));
    }

    #[test]
    fn expand_maps_no_secret_key_to_keyring_diagnostic() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "gpg",
            vec![
                "--decrypt".into(),
                "--quiet".into(),
                "--batch".into(),
                "/pack/x.gpg".into(),
            ],
            err_out(2, "gpg: decryption failed: No secret key"),
        ));
        let p = make_pp(runner);
        let e = p
            .expand(Path::new("/pack/x.gpg"), &null_fs())
            .unwrap_err()
            .to_string();
        assert!(e.contains("no secret key"));
        assert!(e.contains("gpg --import"));
    }

    #[test]
    fn expand_maps_agent_failure_to_agent_diagnostic() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "gpg",
            vec![
                "--decrypt".into(),
                "--quiet".into(),
                "--batch".into(),
                "/pack/x.gpg".into(),
            ],
            err_out(2, "gpg: agent_genkey failed: end of file"),
        ));
        let p = make_pp(runner);
        let e = p
            .expand(Path::new("/pack/x.gpg"), &null_fs())
            .unwrap_err()
            .to_string();
        assert!(e.contains("gpg-agent"));
        assert!(e.contains("gpgconf --launch"));
    }

    #[test]
    fn expand_maps_bad_passphrase_to_batch_caching_hint() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "gpg",
            vec![
                "--decrypt".into(),
                "--quiet".into(),
                "--batch".into(),
                "/pack/x.gpg".into(),
            ],
            err_out(2, "gpg: public key decryption failed: Bad passphrase"),
        ));
        let p = make_pp(runner);
        let e = p
            .expand(Path::new("/pack/x.gpg"), &null_fs())
            .unwrap_err()
            .to_string();
        assert!(e.contains("bad passphrase"));
        assert!(e.contains("--batch"));
        assert!(e.contains("cache the passphrase"));
    }

    #[test]
    fn expand_maps_missing_source_to_file_diagnostic() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "gpg",
            vec![
                "--decrypt".into(),
                "--quiet".into(),
                "--batch".into(),
                "/pack/missing.gpg".into(),
            ],
            err_out(
                2,
                "gpg: can't open '/pack/missing.gpg': No such file or directory",
            ),
        ));
        let p = make_pp(runner);
        let e = p
            .expand(Path::new("/pack/missing.gpg"), &null_fs())
            .unwrap_err()
            .to_string();
        assert!(e.contains("source file"));
        assert!(e.contains("not found"));
    }

    #[test]
    fn expand_passes_unrecognized_stderr_through_with_command_context() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "gpg",
            vec![
                "--decrypt".into(),
                "--quiet".into(),
                "--batch".into(),
                "/pack/x.gpg".into(),
            ],
            err_out(2, "gpg: weird internal failure"),
        ));
        let p = make_pp(runner);
        let e = p
            .expand(Path::new("/pack/x.gpg"), &null_fs())
            .unwrap_err()
            .to_string();
        assert!(e.contains("weird internal failure"));
        assert!(e.contains("gpg decryption"));
        assert!(e.contains("exit 2"));
    }

    #[test]
    fn expand_handles_empty_stderr_failure() {
        let runner = Arc::new(ScriptedRunner::new().expect(
            "gpg",
            vec![
                "--decrypt".into(),
                "--quiet".into(),
                "--batch".into(),
                "/pack/x.gpg".into(),
            ],
            err_out(2, ""),
        ));
        let p = make_pp(runner);
        let e = p
            .expand(Path::new("/pack/x.gpg"), &null_fs())
            .unwrap_err()
            .to_string();
        assert!(e.contains("exited 2"));
    }
}

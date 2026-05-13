//! `RunOnceCommand` + `RunOnceHandler<C>` — the shared shape behind
//! the run-once provisioning handlers (`install`, `homebrew`, and the
//! forthcoming `nix`).
//!
//! All three of these handlers do the same job: run a program on a
//! user-provided file, hash the file, write a sentinel so we know not
//! to run again unnecessarily. This module owns that logic once.
//! Per-handler specialization (program name, argument shape, optional
//! pre-flight validation, status copy) lives in a small
//! [`RunOnceCommand`] trait, with [`RunOnceHandler`] handling the
//! rest.
//!
//! This is PR A of the work tracked in #169. Subsequent PRs retrofit
//! the existing `install` and `homebrew` handlers onto this shape
//! (PR B), then flip the run-once policy to notify-don't-rerun on
//! content change (PR C). For PR A, this module is **pure addition**:
//! it exists, is unit-tested, and is not yet wired into the registry.

use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::datastore::DataStore;
use crate::fs::Fs;
use crate::handlers::{ExecutionPhase, Handler, HandlerConfig, HandlerStatus};
use crate::operations::HandlerIntent;
use crate::paths::Pather;
use crate::rules::RuleMatch;
use crate::Result;

/// Per-handler specialization for a run-once handler.
///
/// Implementations declare the handler's identity (name, phase) and
/// how a matched file becomes a command invocation. They may
/// optionally provide a pre-flight check ([`Self::validate`]) and
/// customize status messages. Everything else is shared via
/// [`RunOnceHandler`].
pub trait RunOnceCommand: Send + Sync {
    /// Unique handler name (e.g. `"install"`, `"homebrew"`, `"nix"`).
    fn handler_name(&self) -> &str;

    /// Execution phase for this handler.
    fn phase(&self) -> ExecutionPhase;

    /// Build the `(executable, arguments)` tuple for invoking the
    /// command against `path`.
    fn command_for(&self, path: &Path) -> (String, Vec<String>);

    /// Optional pre-flight check. Default: no-op.
    ///
    /// Returning `Err` from this method aborts intent generation for
    /// the matched file and propagates the error. The default
    /// implementation passes any file through unchanged.
    ///
    /// Note: the current signature is filesystem-only. Handlers that
    /// need subprocess access for validation (e.g. invoking `nix
    /// eval` to check manifest shape) will need a richer mechanism;
    /// see #169 for the design discussion.
    fn validate(&self, _fs: &dyn Fs, _path: &Path) -> Result<()> {
        Ok(())
    }

    /// Human-readable status message when a current-hash sentinel
    /// exists. Default: `"ran"`. Override for per-handler copy
    /// (e.g. `"brew packages installed"`).
    fn status_deployed(&self) -> &str {
        "ran"
    }

    /// Human-readable status message when no sentinel exists.
    /// Default: `"never ran"`.
    fn status_pending(&self) -> &str {
        "never ran"
    }
}

/// The shared body for run-once handlers.
///
/// Holds a borrow of [`Fs`] and an instance of some
/// [`RunOnceCommand`]. Implements [`Handler`] by routing per-handler
/// concerns to the command and keeping the shared logic — checksum,
/// sentinel, intent construction, status lookup — in one place.
pub struct RunOnceHandler<'a, C: RunOnceCommand> {
    fs: &'a dyn Fs,
    cmd: C,
}

impl<'a, C: RunOnceCommand> RunOnceHandler<'a, C> {
    pub fn new(fs: &'a dyn Fs, cmd: C) -> Self {
        Self { fs, cmd }
    }

    /// Access the underlying command (useful in tests).
    pub fn command(&self) -> &C {
        &self.cmd
    }
}

impl<C: RunOnceCommand> Handler for RunOnceHandler<'_, C> {
    fn name(&self) -> &str {
        self.cmd.handler_name()
    }

    fn phase(&self) -> ExecutionPhase {
        self.cmd.phase()
    }

    fn to_intents(
        &self,
        matches: &[RuleMatch],
        _config: &HandlerConfig,
        _paths: &dyn Pather,
        _fs: &dyn Fs,
    ) -> Result<Vec<HandlerIntent>> {
        let mut intents = Vec::new();

        for m in matches {
            if m.is_dir {
                continue;
            }

            // First-time-pack passive case: a templated file with no
            // baseline yet lands here as a placeholder match (no
            // bytes, no file on disk). We can't compute a sentinel
            // without rendering, and rendering is the §7.4 violation
            // we refuse. Skip intent generation for this match —
            // status / dry-run will report the file as pending via
            // the symlink chain instead, and the next real `dodot
            // up` plans the Run intent normally. See issue #121.
            //
            // This skip runs *before* validation: a placeholder has
            // no content for a validator to inspect, and we shouldn't
            // surface a validation error for a file that may not
            // exist yet in any meaningful sense.
            let has_rendered = m.rendered_bytes.is_some();
            let has_disk = self.fs.exists(&m.absolute_path);
            if !has_rendered && !has_disk {
                tracing::debug!(
                    pack = %m.pack,
                    file = %m.absolute_path.display(),
                    handler = self.cmd.handler_name(),
                    "skipping run-once intent — no rendered bytes and no on-disk file \
                     (first-time-pack passive placeholder)"
                );
                continue;
            }

            // We have content. Validate first, then hash.
            self.cmd.validate(self.fs, &m.absolute_path)?;

            // Sentinel hashing prefers in-memory rendered bytes when
            // they're available (preprocessor-produced files); falls
            // back to a disk read for plain on-disk files. The
            // in-memory path is what lets `dodot status` and `up
            // --dry-run` compute correct sentinels for templated
            // files without writing the rendered file to disk.
            let checksum = match m.rendered_bytes.as_deref() {
                Some(bytes) => file_checksum_bytes(bytes),
                None => file_checksum(self.fs, &m.absolute_path)?,
            };

            let filename = m
                .relative_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let sentinel = format!("{filename}-{checksum}");

            let (executable, arguments) = self.cmd.command_for(&m.absolute_path);

            intents.push(HandlerIntent::Run {
                pack: m.pack.clone(),
                handler: self.cmd.handler_name().into(),
                executable,
                arguments,
                sentinel,
                filename,
                content_hash: checksum,
            });
        }

        Ok(intents)
    }

    fn check_status(
        &self,
        file: &Path,
        pack: &str,
        datastore: &dyn DataStore,
    ) -> Result<HandlerStatus> {
        let checksum = file_checksum(self.fs, file)?;
        let filename = file.file_name().unwrap_or_default().to_string_lossy();
        let sentinel = format!("{filename}-{checksum}");
        let has_sentinel = datastore.has_sentinel(pack, self.cmd.handler_name(), &sentinel)?;

        Ok(HandlerStatus {
            file: file.to_string_lossy().into_owned(),
            handler: self.cmd.handler_name().into(),
            deployed: has_sentinel,
            message: if has_sentinel {
                self.cmd.status_deployed().into()
            } else {
                self.cmd.status_pending().into()
            },
        })
    }
}

/// Compute a short SHA-256 hex digest of a file's contents.
///
/// Returns the first 8 bytes of the SHA-256 hash as 16 hex chars —
/// unique enough for sentinel-name disambiguation, short enough to
/// keep on-disk paths readable.
///
/// Internal helper used by [`RunOnceHandler`] and (in PR B) by the
/// retrofitted `install` / `homebrew` handlers. Crate-scoped to keep
/// it out of dodot-lib's public API surface.
pub(crate) fn file_checksum(fs: &dyn Fs, path: &Path) -> Result<String> {
    let mut reader = fs.open_read(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf).map_err(|e| crate::DodotError::Fs {
            path: path.to_path_buf(),
            source: e,
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let hash = hasher.finalize();
    Ok(hex_encode(&hash[..8]))
}

/// Same digest format as [`file_checksum`], but over an in-memory
/// byte slice — used when the rendered content is available without
/// a disk read.
pub(crate) fn file_checksum_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let hash = hasher.finalize();
    hex_encode(&hash[..8])
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datastore::{CommandOutput, CommandRunner, FilesystemDataStore};
    use crate::testing::TempEnvironment;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    // Compile-time check: RunOnceCommand is object-safe.
    #[allow(dead_code)]
    fn assert_object_safe(_: &dyn RunOnceCommand) {}

    struct NoopRunner;
    impl CommandRunner for NoopRunner {
        fn run(&self, _: &str, _: &[String]) -> Result<CommandOutput> {
            Ok(CommandOutput {
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    fn make_datastore(env: &TempEnvironment) -> FilesystemDataStore {
        FilesystemDataStore::new(env.fs.clone(), env.paths.clone(), Arc::new(NoopRunner))
    }

    /// Test double — a minimal `RunOnceCommand` implementation.
    struct FakeCommand {
        name: &'static str,
        phase: ExecutionPhase,
        executable: String,
        args_template: Vec<String>,
        validate_fails: bool,
        deployed_msg: &'static str,
        pending_msg: &'static str,
    }

    impl FakeCommand {
        fn new(name: &'static str) -> Self {
            Self {
                name,
                phase: ExecutionPhase::Setup,
                executable: "test-cmd".into(),
                args_template: Vec::new(),
                validate_fails: false,
                deployed_msg: "ran",
                pending_msg: "never ran",
            }
        }
    }

    impl RunOnceCommand for FakeCommand {
        fn handler_name(&self) -> &str {
            self.name
        }
        fn phase(&self) -> ExecutionPhase {
            self.phase
        }
        fn command_for(&self, path: &Path) -> (String, Vec<String>) {
            let mut args = self.args_template.clone();
            args.push(path.to_string_lossy().into_owned());
            (self.executable.clone(), args)
        }
        fn validate(&self, _fs: &dyn Fs, path: &Path) -> Result<()> {
            if self.validate_fails {
                Err(crate::DodotError::Fs {
                    path: path.to_path_buf(),
                    source: std::io::Error::other("synthetic validation failure"),
                })
            } else {
                Ok(())
            }
        }
        fn status_deployed(&self) -> &str {
            self.deployed_msg
        }
        fn status_pending(&self) -> &str {
            self.pending_msg
        }
    }

    fn make_match(
        pack: &str,
        relative: &str,
        absolute: PathBuf,
        rendered: Option<Vec<u8>>,
    ) -> RuleMatch {
        RuleMatch {
            relative_path: relative.into(),
            absolute_path: absolute,
            pack: pack.into(),
            handler: "fake".into(),
            is_dir: false,
            options: HashMap::new(),
            preprocessor_source: None,
            rendered_bytes: rendered.map(Arc::from),
        }
    }

    fn pather(env: &TempEnvironment) -> crate::paths::XdgPather {
        crate::paths::XdgPather::builder()
            .home(&env.home)
            .dotfiles_root(&env.dotfiles_root)
            .build()
            .unwrap()
    }

    #[test]
    fn handler_exposes_command_identity() {
        let env = TempEnvironment::builder().build();
        let handler = RunOnceHandler::new(
            env.fs.as_ref(),
            FakeCommand {
                phase: ExecutionPhase::Provision,
                ..FakeCommand::new("widget")
            },
        );
        assert_eq!(handler.name(), "widget");
        assert_eq!(handler.phase(), ExecutionPhase::Provision);
    }

    #[test]
    fn to_intents_emits_run_with_shared_shape() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("setup.sh", "echo hi")
            .done()
            .build();

        let cmd = FakeCommand {
            executable: "bash".into(),
            args_template: vec!["--".into()],
            ..FakeCommand::new("fake")
        };
        let handler = RunOnceHandler::new(env.fs.as_ref(), cmd);

        let abs = env.dotfiles_root.join("vim/setup.sh");
        let matches = vec![make_match("vim", "setup.sh", abs.clone(), None)];
        let intents = handler
            .to_intents(
                &matches,
                &HandlerConfig::default(),
                &pather(&env),
                env.fs.as_ref(),
            )
            .unwrap();

        assert_eq!(intents.len(), 1);
        match &intents[0] {
            HandlerIntent::Run {
                pack,
                handler: h,
                executable,
                arguments,
                sentinel,
                filename,
                content_hash,
            } => {
                assert_eq!(pack, "vim");
                assert_eq!(h, "fake");
                assert_eq!(executable, "bash");
                // Args template + appended path.
                assert_eq!(arguments[0], "--");
                assert!(arguments[1].ends_with("vim/setup.sh"));
                // Sentinel shape: "<filename>-<16 hex chars>".
                assert!(sentinel.starts_with("setup.sh-"));
                assert_eq!(sentinel.len(), "setup.sh-".len() + 16);
                assert_eq!(filename, "setup.sh");
                assert_eq!(content_hash.len(), 16);
                assert_eq!(*sentinel, format!("{filename}-{content_hash}"));
            }
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn to_intents_prefers_rendered_bytes_over_disk_read() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("setup.sh", "on-disk content")
            .done()
            .build();
        let abs = env.dotfiles_root.join("vim/setup.sh");

        let handler = RunOnceHandler::new(env.fs.as_ref(), FakeCommand::new("fake"));

        let rendered = b"rendered content".to_vec();
        let expected_checksum = file_checksum_bytes(&rendered);
        let matches = vec![make_match("vim", "setup.sh", abs.clone(), Some(rendered))];
        let intents = handler
            .to_intents(
                &matches,
                &HandlerConfig::default(),
                &pather(&env),
                env.fs.as_ref(),
            )
            .unwrap();

        match &intents[0] {
            HandlerIntent::Run { sentinel, .. } => {
                assert_eq!(*sentinel, format!("setup.sh-{expected_checksum}"));
            }
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn to_intents_falls_back_to_disk_when_no_rendered_bytes() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("setup.sh", "disk content")
            .done()
            .build();
        let abs = env.dotfiles_root.join("vim/setup.sh");

        let handler = RunOnceHandler::new(env.fs.as_ref(), FakeCommand::new("fake"));

        let expected_checksum = file_checksum(env.fs.as_ref(), &abs).unwrap();
        let matches = vec![make_match("vim", "setup.sh", abs, None)];
        let intents = handler
            .to_intents(
                &matches,
                &HandlerConfig::default(),
                &pather(&env),
                env.fs.as_ref(),
            )
            .unwrap();

        match &intents[0] {
            HandlerIntent::Run { sentinel, .. } => {
                assert_eq!(*sentinel, format!("setup.sh-{expected_checksum}"));
            }
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn validate_does_not_fire_on_placeholder_match() {
        // A validator that always errors must NOT be called for a
        // first-time-pack passive placeholder (no rendered bytes, no
        // on-disk file). Regression test for Copilot review on #170 —
        // the earlier draft validated before checking for content.
        let env = TempEnvironment::builder().build();
        let cmd = FakeCommand {
            validate_fails: true,
            ..FakeCommand::new("fake")
        };
        let handler = RunOnceHandler::new(env.fs.as_ref(), cmd);

        let ghost = env.dotfiles_root.join("ghost/install.sh"); // never written
        let matches = vec![make_match("ghost", "install.sh", ghost, None)];
        let intents = handler
            .to_intents(
                &matches,
                &HandlerConfig::default(),
                &pather(&env),
                env.fs.as_ref(),
            )
            .expect("placeholder match should skip cleanly without invoking validate");
        assert!(intents.is_empty());
    }

    #[test]
    fn to_intents_skips_first_time_pack_passive_placeholder() {
        // No rendered_bytes, file doesn't exist on disk → skip (no
        // intent), don't error.
        let env = TempEnvironment::builder().build();
        let handler = RunOnceHandler::new(env.fs.as_ref(), FakeCommand::new("fake"));

        let ghost = env.dotfiles_root.join("ghost/install.sh"); // never written
        let matches = vec![make_match("ghost", "install.sh", ghost, None)];
        let intents = handler
            .to_intents(
                &matches,
                &HandlerConfig::default(),
                &pather(&env),
                env.fs.as_ref(),
            )
            .unwrap();

        assert!(intents.is_empty());
    }

    #[test]
    fn to_intents_propagates_validation_error() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("setup.sh", "content")
            .done()
            .build();
        let abs = env.dotfiles_root.join("vim/setup.sh");

        let cmd = FakeCommand {
            validate_fails: true,
            ..FakeCommand::new("fake")
        };
        let handler = RunOnceHandler::new(env.fs.as_ref(), cmd);

        let matches = vec![make_match("vim", "setup.sh", abs, None)];
        let result = handler.to_intents(
            &matches,
            &HandlerConfig::default(),
            &pather(&env),
            env.fs.as_ref(),
        );
        assert!(
            result.is_err(),
            "expected validate failure to propagate, got {result:?}"
        );
    }

    #[test]
    fn to_intents_skips_directories() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("scripts/run", "x")
            .done()
            .build();
        let handler = RunOnceHandler::new(env.fs.as_ref(), FakeCommand::new("fake"));

        let dir_match = RuleMatch {
            is_dir: true,
            ..make_match(
                "vim",
                "scripts",
                env.dotfiles_root.join("vim/scripts"),
                None,
            )
        };
        let intents = handler
            .to_intents(
                &[dir_match],
                &HandlerConfig::default(),
                &pather(&env),
                env.fs.as_ref(),
            )
            .unwrap();
        assert!(intents.is_empty());
    }

    #[test]
    fn to_intents_emits_one_intent_per_match() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("a.sh", "alpha")
            .file("b.sh", "beta")
            .done()
            .build();
        let handler = RunOnceHandler::new(env.fs.as_ref(), FakeCommand::new("fake"));

        let matches = vec![
            make_match("vim", "a.sh", env.dotfiles_root.join("vim/a.sh"), None),
            make_match("vim", "b.sh", env.dotfiles_root.join("vim/b.sh"), None),
        ];
        let intents = handler
            .to_intents(
                &matches,
                &HandlerConfig::default(),
                &pather(&env),
                env.fs.as_ref(),
            )
            .unwrap();
        assert_eq!(intents.len(), 2);
    }

    #[test]
    fn check_status_reports_deployed_when_sentinel_exists() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("setup.sh", "content")
            .done()
            .build();
        let abs = env.dotfiles_root.join("vim/setup.sh");
        let checksum = file_checksum(env.fs.as_ref(), &abs).unwrap();
        let sentinel = format!("setup.sh-{checksum}");

        // Pre-create the sentinel on disk so check_status finds it.
        let sentinel_dir = env.paths.handler_data_dir("vim", "fake");
        env.fs.mkdir_all(&sentinel_dir).unwrap();
        env.fs
            .write_file(&sentinel_dir.join(&sentinel), b"completed|0")
            .unwrap();

        let datastore = make_datastore(&env);
        let cmd = FakeCommand {
            deployed_msg: "all set",
            ..FakeCommand::new("fake")
        };
        let handler = RunOnceHandler::new(env.fs.as_ref(), cmd);

        let status = handler.check_status(&abs, "vim", &datastore).unwrap();
        assert!(status.deployed);
        assert_eq!(status.message, "all set");
        assert_eq!(status.handler, "fake");
    }

    #[test]
    fn check_status_reports_pending_when_no_sentinel() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("setup.sh", "content")
            .done()
            .build();
        let abs = env.dotfiles_root.join("vim/setup.sh");

        let datastore = make_datastore(&env);
        let cmd = FakeCommand {
            pending_msg: "needs attention",
            ..FakeCommand::new("fake")
        };
        let handler = RunOnceHandler::new(env.fs.as_ref(), cmd);

        let status = handler.check_status(&abs, "vim", &datastore).unwrap();
        assert!(!status.deployed);
        assert_eq!(status.message, "needs attention");
    }

    #[test]
    fn file_checksum_and_bytes_agree() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("file.txt", "consistent content")
            .done()
            .build();
        let abs = env.dotfiles_root.join("vim/file.txt");
        let disk = file_checksum(env.fs.as_ref(), &abs).unwrap();
        let in_mem = file_checksum_bytes(b"consistent content");
        assert_eq!(disk, in_mem);
        assert_eq!(disk.len(), 16);
    }

    #[test]
    fn file_checksum_changes_with_content() {
        let a = file_checksum_bytes(b"version 1");
        let b = file_checksum_bytes(b"version 2");
        assert_ne!(a, b);
    }
}

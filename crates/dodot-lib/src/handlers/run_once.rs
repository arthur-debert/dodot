//! `RunOnceCommand` + `RunOnceHandler<C>` — the shared shape behind
//! the run-once provisioning handlers (`install`, `homebrew`, `nix`).
//!
//! All three of these handlers do the same job: run a program on a
//! user-provided file, hash the file, write a sentinel so we know not
//! to run again unnecessarily. This module owns that logic once.
//! Per-handler specialization (program name, argument shape, status
//! copy) lives in a small [`RunOnceCommand`] trait, with
//! [`RunOnceHandler`] handling the rest.
//!
//! # Three-state run-once semantics
//!
//! Run-once handlers consult [`DataStore::did_run`](crate::datastore::DataStore::did_run)
//! to classify a matched file into one of three states:
//!
//! 1. [`NeverRan`](crate::datastore::DidRunStatus::NeverRan) — no
//!    sentinel exists; emit a [`HandlerIntent::Run`] for the file.
//! 2. [`RanCurrent`](crate::datastore::DidRunStatus::RanCurrent) — a
//!    sentinel exists whose recorded hash matches the current
//!    content; skip silently.
//! 3. [`RanDifferent`](crate::datastore::DidRunStatus::RanDifferent) — a
//!    sentinel exists but for a *different* content hash; skip with
//!    notice. The user opts in to re-running via
//!    `dodot up --provision-rerun` (the `provision_rerun` flag —
//!    distinct from `--force`, which only overwrites pre-existing
//!    files at symlink target paths).
//!
//! `dodot status` renders this three-way result as `pending` /
//! `deployed` / `older version (N lines added, M removed)` rows; the
//! `--diff` flag dumps the underlying snapshot-vs-current unified
//! diff for the third state. The third row also carries a footnote
//! naming `dodot up --provision-rerun`, since the label alone states
//! the condition and leaves the user nothing to act on. A fourth row
//! comes from outside this model: `--no-provision` drops the handler
//! before it is consulted at all, and those files render as
//! `skipped (--no-provision)`. A fifth comes from the machine rather
//! than from the user — an absent manager, which the planner skips
//! before any of this is consulted
//! ([`crate::provisioners::availability`]).
//!
//! # Snapshots
//!
//! [`DataStore::run_and_record`](crate::datastore::DataStore::run_and_record)
//! writes a `<sentinel>.snapshot` sibling capturing the script's bytes
//! at the moment of a successful run. Snapshots are the data behind
//! the `(N+ M-)` summary in status and the body of
//! `dodot status --diff`. Sentinels written before snapshots existed
//! have no snapshot sibling — those rows surface as `older version
//! (no diff data)` and are excluded from `--diff` output.
//!
//! Snapshots live at
//! `<datastore>/packs/<pack>/<handler>/<filename>-<hash>.snapshot`;
//! users who want to manage state directly can delete sentinel +
//! snapshot pairs to roll a file back to `never ran` — all of the
//! pairs for that filename, since earlier runs' sentinels are kept
//! and any one of them still reads as `older version`.

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
/// Implementations declare the handler's identity (name, phase), how
/// a matched file becomes a command invocation, and the status
/// messages for the three-state run-once model described at the
/// module level — `pending` /
/// `deployed` / `ran older version`. Everything else (hashing,
/// sentinel construction, `did_run` lookup, intent emission) is
/// shared via [`RunOnceHandler`].
///
/// # Lifecycle invariant
///
/// Every `RunOnceCommand` implementation shares an **identical
/// has-run / which-version-has-run / will-run lifecycle**. The
/// shared [`RunOnceHandler`] consults
/// [`DataStore::did_run`](crate::datastore::DataStore::did_run) and
/// renders the three states (`NeverRan` / `RanCurrent` /
/// `RanDifferent`) the same way for every command. Permitted
/// specializations are limited to:
///
/// - the executable + arguments to run ([`Self::command_for`]),
/// - status-message copy ([`Self::status_pending`] etc.).
///
/// # Where the "is the tool there?" check lives
///
/// Not here. This trait once carried a `validate` hook documented for
/// exactly that, with no implementor. It could not have worked: a
/// finding raised inside intent generation dies with the intent, and
/// `commands/status.rs::run_once_health` derives its row from the
/// source file and the datastore without ever calling the handler,
/// so an absent manager would have rendered as `never ran`. The check
/// is now [`crate::provisioners::availability`], read by the pack
/// planner and by status from one shared module — which is also the
/// only place a skip can be *reported*, since `to_intents` returns
/// intents and nothing else.
///
/// **Per-content gatekeeping at planning time is explicitly out
/// of scope.** Content errors (malformed manifest, syntax error,
/// unsupported shape) must surface at apply time, the same way a
/// broken `Brewfile` errors out of `brew bundle` or a broken
/// `install.sh` errors out of `bash`. A validator that rejects
/// per content breaks the lifecycle invariant: a previously-run
/// file the user later edits into a broken state would fail
/// planning here instead of reaching the `RanDifferent`
/// "older version" notice the run-once policy promises. That
/// asymmetry across commands is the bug, not the feature.
pub trait RunOnceCommand: Send + Sync {
    /// Unique handler name (e.g. `"install"`, `"homebrew"`, `"nix"`).
    fn handler_name(&self) -> &str;

    /// Execution phase for this handler.
    fn phase(&self) -> ExecutionPhase;

    /// Build the `(executable, arguments)` tuple for invoking the
    /// command against `path`.
    ///
    /// Name the executable the way a user would — `brew`, `nix`,
    /// `bash`. For a handler dodot locates itself, the planner
    /// substitutes the absolute path the availability probe found
    /// before the intent leaves it (see
    /// [`crate::provisioners::availability`]), so this stays a pure
    /// function of `path` and never consults the environment.
    fn command_for(&self, path: &Path) -> (String, Vec<String>);

    /// Environment variables the command is spawned with, layered
    /// onto the environment dodot itself runs with — the child still
    /// inherits everything else.
    ///
    /// The rows come from the handler's descriptor in
    /// [`crate::provisioners`], so what a provisioner needs in its
    /// environment is stated next to the argv it qualifies rather
    /// than branched on at the spawn site. A handler with no rows
    /// (and any command with no descriptor at all) returns an empty
    /// vector and spawns exactly as it would with no seam here.
    ///
    /// Duplicate names are applied in order, so a later row wins.
    fn environment(&self) -> Vec<(String, String)> {
        crate::provisioners::environment_for(self.handler_name())
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect()
    }

    /// Human-readable status message when a current-hash sentinel
    /// exists. Default: `"ran"`. Override for per-handler copy
    /// (e.g. `"installed"`).
    fn status_deployed(&self) -> &str {
        "ran"
    }

    /// Human-readable status message when no sentinel exists.
    /// Default: `"never ran"`.
    fn status_pending(&self) -> &str {
        "never ran"
    }

    /// Human-readable status message when a sentinel exists but for
    /// a *different* content hash — the file has been edited since
    /// the last successful run, but the conservative
    /// notify-don't-rerun policy leaves the prior state in place
    /// until the user opts in via `--provision-rerun`.
    ///
    /// Default: `"older version"`. Overridden per-handler for
    /// readability — e.g. `"brew packages older version"` for
    /// homebrew. `dodot status` further annotates this label with a
    /// `(N lines added, M removed)` summary when a snapshot of the
    /// previously-run content is available, or with `(no diff data)`
    /// for sentinels written before snapshots were introduced.
    fn status_ran_different(&self) -> &str {
        "older version"
    }
}

/// The shared body for run-once handlers.
///
/// Holds a borrow of [`Fs`] and an instance of some
/// [`RunOnceCommand`]. Implements [`Handler`] by routing per-handler
/// concerns to the command and keeping the shared logic — checksum,
/// sentinel, intent construction, status lookup — in one place.
///
/// No [`CommandRunner`](crate::datastore::CommandRunner): nothing on
/// this path spawns. The command a run-once handler builds is
/// executed by the executor, and the "is the manager installed?"
/// question belongs to the planner (see the trait docs).
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

            // Two names for the same file, and they are not
            // interchangeable. The sentinel is keyed by the basename
            // (`install.sh-<hash>`) because that is the datastore's
            // on-disk shape; the intent carries the pack-relative
            // path because that is what status rows are keyed by and
            // what a failure has to be reported against.
            let relative_path = m.relative_path.to_string_lossy().into_owned();
            let basename = m
                .relative_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let sentinel = format!("{basename}-{checksum}");

            let (executable, arguments) = self.cmd.command_for(&m.absolute_path);

            intents.push(HandlerIntent::Run {
                pack: m.pack.clone(),
                handler: self.cmd.handler_name().into(),
                executable,
                arguments,
                environment: self.cmd.environment(),
                sentinel,
                relative_path,
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
        let filename = file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();

        let status = datastore.did_run(pack, self.cmd.handler_name(), &filename, &checksum)?;

        // Map the three-way did_run result to the binary
        // deployed/not-deployed status with a descriptive message.
        // `RanDifferent` is marked deployed=true (the script HAS run,
        // just not for the current content) so callers that filter by
        // `deployed` don't miss the entry; the message disambiguates
        // current-version vs. older-version. This is the surface that
        // `dodot status` and `dodot up`'s post-execute rendering both
        // read from, so the "ran older version" notice reaches the
        // user even though it bypasses the OperationResult flow.
        let (deployed, message) = match status {
            crate::datastore::DidRunStatus::NeverRan => (false, self.cmd.status_pending().into()),
            crate::datastore::DidRunStatus::RanCurrent => (true, self.cmd.status_deployed().into()),
            crate::datastore::DidRunStatus::RanDifferent { .. } => (
                true,
                format!(
                    "{} (older version — run `dodot up --provision-rerun` to apply current)",
                    self.cmd.status_deployed()
                ),
            ),
        };

        Ok(HandlerStatus {
            file: file.to_string_lossy().into_owned(),
            handler: self.cmd.handler_name().into(),
            deployed,
            message,
        })
    }
}

/// Canonical run-state copy for a [`RunOnceCommand`] — used by
/// `dodot status` to render the three-state row label without
/// needing a `Box<dyn Handler>` registered with the right `Fs`.
pub struct RunOnceStatusMessages {
    /// Label when no sentinel exists (`DidRunStatus::NeverRan`).
    pub pending: String,
    /// Label when a sentinel matches the current content
    /// (`DidRunStatus::RanCurrent`).
    pub deployed: String,
    /// Label when a sentinel exists for a *different* content hash
    /// (`DidRunStatus::RanDifferent`).
    pub ran_different: String,
}

/// Snapshot the three trait-defined messages off a concrete
/// [`RunOnceCommand`] into owned strings so callers don't have to
/// keep the command instance alive to read them.
pub fn status_messages_for<C: RunOnceCommand>(cmd: &C) -> RunOnceStatusMessages {
    RunOnceStatusMessages {
        pending: cmd.status_pending().to_string(),
        deployed: cmd.status_deployed().to_string(),
        ran_different: cmd.status_ran_different().to_string(),
    }
}

/// Look up the canonical run-state copy for a built-in run-once
/// handler by name. Unknown handler names fall back to the trait
/// defaults.
pub fn run_once_status_messages(handler: &str) -> RunOnceStatusMessages {
    use crate::handlers::{HANDLER_HOMEBREW, HANDLER_INSTALL, HANDLER_NIX};
    if handler == HANDLER_INSTALL {
        return status_messages_for(&crate::handlers::install::InstallCommand);
    }
    if handler == HANDLER_HOMEBREW {
        return status_messages_for(&crate::handlers::homebrew::BrewfileCommand);
    }
    if handler == HANDLER_NIX {
        return status_messages_for(&crate::handlers::nix::NixCommand);
    }
    RunOnceStatusMessages {
        pending: "never ran".into(),
        deployed: "ran".into(),
        ran_different: "older version".into(),
    }
}

/// Compute a short SHA-256 hex digest of a file's contents.
///
/// Returns the first 8 bytes of the SHA-256 hash as 16 hex chars —
/// unique enough for sentinel-name disambiguation, short enough to
/// keep on-disk paths readable.
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
    use crate::datastore::CommandSpec;
    use crate::datastore::{CommandOutput, CommandRunner, FilesystemDataStore};
    use crate::testing::TempEnvironment;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[allow(dead_code)]
    fn assert_object_safe(_: &dyn RunOnceCommand) {}

    struct NoopRunner;
    impl CommandRunner for NoopRunner {
        fn run(&self, _command: CommandSpec<'_>) -> Result<CommandOutput> {
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

    struct FakeCommand {
        name: &'static str,
        phase: ExecutionPhase,
        executable: String,
        args_template: Vec<String>,
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
                environment,
                sentinel,
                relative_path,
                content_hash,
            } => {
                assert_eq!(pack, "vim");
                assert_eq!(h, "fake");
                assert_eq!(executable, "bash");
                // No descriptor row for a handler named "fake", so
                // the shared default declares nothing.
                assert!(environment.is_empty(), "got: {environment:?}");
                assert_eq!(arguments[0], "--");
                assert!(arguments[1].ends_with("vim/setup.sh"));
                assert!(sentinel.starts_with("setup.sh-"));
                assert_eq!(sentinel.len(), "setup.sh-".len() + 16);
                assert_eq!(relative_path, "setup.sh");
                assert_eq!(content_hash.len(), 16);
                assert_eq!(*sentinel, format!("{relative_path}-{content_hash}"));
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
    fn to_intents_skips_first_time_pack_passive_placeholder() {
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
    fn check_status_reports_older_version_when_hash_differs() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("setup.sh", "new content")
            .done()
            .build();
        let abs = env.dotfiles_root.join("vim/setup.sh");

        let sentinel_dir = env.paths.handler_data_dir("vim", "fake");
        env.fs.mkdir_all(&sentinel_dir).unwrap();
        env.fs
            .write_file(
                &sentinel_dir.join("setup.sh-aaaaaaaaaaaaaaaa"),
                b"completed|100",
            )
            .unwrap();

        let datastore = make_datastore(&env);
        let cmd = FakeCommand {
            deployed_msg: "ran",
            ..FakeCommand::new("fake")
        };
        let handler = RunOnceHandler::new(env.fs.as_ref(), cmd);

        let status = handler.check_status(&abs, "vim", &datastore).unwrap();
        assert!(status.deployed, "older version still counts as deployed");
        assert!(
            status.message.contains("older version"),
            "message should flag older version, got: {}",
            status.message
        );
        assert!(
            status.message.contains("--provision-rerun"),
            "message should mention --provision-rerun, got: {}",
            status.message
        );
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

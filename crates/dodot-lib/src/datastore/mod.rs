//! State management for dodot.
//!
//! The [`DataStore`] trait defines dodot's 8-method storage API.
//! [`FilesystemDataStore`] implements it using symlinks and sentinel
//! files on a real (or test) filesystem via the [`Fs`](crate::fs::Fs) trait.

mod filesystem;

pub use filesystem::FilesystemDataStore;

use std::path::{Path, PathBuf};

use crate::Result;

/// Three-way result of [`DataStore::did_run`] — whether a file has
/// been run by a handler, and if so, whether the recorded run matches
/// the current file content.
///
/// Mirrors the spec in #169: run-once handlers consult this to decide
/// between *first-time-run* (NeverRan → execute), *already up to
/// date* (RanCurrent → skip silently), and *file edited since last
/// run* (RanDifferent → skip with notice, user runs `--force` to
/// apply).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DidRunStatus {
    /// No sentinel exists for this filename in this pack/handler.
    NeverRan,
    /// A sentinel matching the current file's content hash exists.
    RanCurrent,
    /// A sentinel exists for a *different* content hash — the file
    /// has changed since the last successful run. `previous_hash` is
    /// the hex hash recorded in the existing sentinel; if the
    /// `<sentinel>.snapshot` sibling file exists (created on or after
    /// PR C of #169) its raw bytes are returned in `previous_snapshot`
    /// so callers can render a diff.
    RanDifferent {
        previous_hash: String,
        previous_snapshot: Option<Vec<u8>>,
    },
}

/// Dodot's storage interface.
///
/// State is represented entirely by symlinks and sentinel files in the
/// filesystem — no database, no lock files. Methods break into three
/// groups:
///
/// **Mutations** — modify state:
/// - [`create_data_link`](DataStore::create_data_link)
/// - [`create_user_link`](DataStore::create_user_link)
/// - [`run_and_record`](DataStore::run_and_record)
/// - [`remove_state`](DataStore::remove_state)
///
/// **Queries** — read state:
/// - [`has_sentinel`](DataStore::has_sentinel)
/// - [`did_run`](DataStore::did_run) — three-way classification used
///   by the run-once handlers
/// - [`has_handler_state`](DataStore::has_handler_state)
/// - [`list_pack_handlers`](DataStore::list_pack_handlers)
/// - [`list_handler_sentinels`](DataStore::list_handler_sentinels)
pub trait DataStore: Send + Sync {
    /// Creates an intermediate symlink in the datastore:
    /// `handler_data_dir(pack, handler) / filename -> source_file`
    ///
    /// Returns the absolute path of the created datastore link.
    /// Idempotent: if the link exists and already points to the correct
    /// source, this is a no-op.
    fn create_data_link(&self, pack: &str, handler: &str, source_file: &Path) -> Result<PathBuf>;

    /// Creates a user-visible symlink:
    /// `user_path -> datastore_path`
    ///
    /// This is the second leg of the double-link architecture.
    /// Creates parent directories as needed.
    fn create_user_link(&self, datastore_path: &Path, user_path: &Path) -> Result<()>;

    /// Executes `command` via shell and records a sentinel on success.
    ///
    /// Idempotent: if the sentinel already exists, the command is not
    /// re-run. The sentinel file stores `completed|{timestamp}`.
    ///
    /// **Edge case**: if the command succeeds but the sentinel write
    /// fails, a subsequent call will re-run the command. This is by
    /// design — re-running is safer than falsely marking as complete.
    /// Install scripts should be idempotent to handle this.
    fn run_and_record(
        &self,
        pack: &str,
        handler: &str,
        executable: &str,
        arguments: &[String],
        sentinel: &str,
        force: bool,
    ) -> Result<()>;

    /// Checks whether a sentinel exists for this pack/handler.
    fn has_sentinel(&self, pack: &str, handler: &str, sentinel: &str) -> Result<bool>;

    /// Three-way "has this file been run, and is it current?" lookup
    /// used by the run-once handlers (`install`, `homebrew`, `nix`).
    ///
    /// Lists sentinel files in the handler data dir matching
    /// `<filename>-<16 hex chars>` (regardless of which hash), then:
    ///
    /// - Empty result → [`DidRunStatus::NeverRan`].
    /// - Any sentinel name's hash matches `current_hash` →
    ///   [`DidRunStatus::RanCurrent`].
    /// - Otherwise → [`DidRunStatus::RanDifferent`] with the prior
    ///   hash and (when available) the snapshot of the file as it
    ///   was at the time of that last run.
    ///
    /// Tie-break for multiple non-matching sentinels: most recently
    /// completed run wins, as recorded by the `completed|<unix-ts>`
    /// payload [`run_and_record`](DataStore::run_and_record) writes
    /// to each sentinel. Sentinels whose payload doesn't parse fall
    /// to the bottom; ties on timestamp break by lexical order on
    /// the sentinel filename for determinism.
    fn did_run(
        &self,
        pack: &str,
        handler: &str,
        filename: &str,
        current_hash: &str,
    ) -> Result<DidRunStatus>;

    /// Removes all state for a pack/handler pair.
    ///
    /// Deletes the handler data directory and everything in it.
    fn remove_state(&self, pack: &str, handler: &str) -> Result<()>;

    /// Checks if any state exists for a pack/handler pair.
    fn has_handler_state(&self, pack: &str, handler: &str) -> Result<bool>;

    /// Lists handler names that have state for a pack.
    fn list_pack_handlers(&self, pack: &str) -> Result<Vec<String>>;

    /// Lists sentinel file names for a pack/handler.
    fn list_handler_sentinels(&self, pack: &str, handler: &str) -> Result<Vec<String>>;

    /// Writes a regular file (not a symlink) into the datastore.
    ///
    /// Used for preprocessor-expanded files where the datastore holds
    /// rendered content rather than a symlink to the source.
    /// Returns the absolute path of the written file.
    /// Idempotent: overwrites if the file already exists.
    ///
    /// `filename` must be a safe relative path — no absolute paths, no
    /// `..` components. Callers (typically the preprocessing pipeline)
    /// are expected to validate before calling. Implementations should
    /// also reject unsafe paths as defense-in-depth.
    fn write_rendered_file(
        &self,
        pack: &str,
        handler: &str,
        filename: &str,
        content: &[u8],
    ) -> Result<PathBuf>;

    /// Like [`write_rendered_file`], but applies `mode` atomically
    /// at file-creation time so the rendered bytes never live on
    /// disk under a more permissive mode (per `secrets.lex` §4.3
    /// for whole-file `age` / `gpg` plaintext). Default impl
    /// falls back to `write_rendered_file` followed by an
    /// `Fs::set_permissions` chmod — semantically equivalent but
    /// briefly leaves the file at the umask-default mode; real
    /// impls should override with the atomic
    /// `Fs::write_file_with_mode` path.
    fn write_rendered_file_with_mode(
        &self,
        pack: &str,
        handler: &str,
        filename: &str,
        content: &[u8],
        mode: u32,
    ) -> Result<PathBuf>;

    /// Creates a directory (mkdir -p) inside the datastore and returns
    /// its absolute path. Used for preprocessor-expanded directory
    /// entries (e.g. directory markers from tar archives).
    ///
    /// Same path-safety constraints as [`write_rendered_file`].
    fn write_rendered_dir(&self, pack: &str, handler: &str, relative: &str) -> Result<PathBuf>;

    /// Returns the absolute path where a sentinel file would be stored.
    fn sentinel_path(&self, pack: &str, handler: &str, sentinel: &str) -> std::path::PathBuf;
}

/// Abstraction over process execution.
///
/// [`FilesystemDataStore`] uses this to run commands in
/// [`run_and_record`](DataStore::run_and_record). Tests can provide a
/// mock that records calls without spawning processes.
pub trait CommandRunner: Send + Sync {
    fn run(&self, executable: &str, arguments: &[String]) -> Result<CommandOutput>;

    /// Variant of [`Self::run`] that returns stdout as raw bytes.
    /// Required for callers that decrypt binary payloads through a
    /// subprocess (whole-file `age` / `gpg` preprocessors per
    /// `secrets.lex` §4) — `String::from_utf8_lossy` on the
    /// `run` path corrupts non-UTF-8 plaintext, so SSH binary
    /// keys / X.509 DER certs / kubeconfig blobs would round-trip
    /// to disk with replacement characters.
    ///
    /// Stderr stays a `String` because diagnostic text is
    /// human-readable in every shipped provider; if a future
    /// caller emits non-UTF-8 stderr we'll add a bytes variant
    /// then.
    ///
    /// Default impl converts a `run()` result by re-encoding the
    /// `String` stdout as bytes — that's safe (UTF-8 is a strict
    /// subset of bytes) but does *not* recover bytes lost to
    /// `from_utf8_lossy` upstream. Real impls (`ShellCommandRunner`)
    /// must override and read stdout as raw bytes from the start.
    fn run_bytes(&self, executable: &str, arguments: &[String]) -> Result<CommandOutputBytes> {
        let out = self.run(executable, arguments)?;
        Ok(CommandOutputBytes {
            exit_code: out.exit_code,
            stdout: out.stdout.into_bytes(),
            stderr: out.stderr,
        })
    }
}

/// [`CommandRunner`] that succeeds without spawning anything.
///
/// Useful for callsites that need a `CommandRunner` for type reasons
/// but never actually invoke commands: status-only registry walks,
/// handler-name enumeration, and tests that exercise non-subprocess
/// code paths. A `run()` call returns `exit_code: 0` with empty
/// stdout/stderr — the same shape `MockCommandRunner` returns by
/// default, minus the call-recording.
#[derive(Debug, Default)]
pub struct NoopCommandRunner;

impl CommandRunner for NoopCommandRunner {
    fn run(&self, _executable: &str, _arguments: &[String]) -> Result<CommandOutput> {
        Ok(CommandOutput {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

/// Output from a command execution.
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Output from a command execution where stdout is held as raw
/// bytes — used by [`CommandRunner::run_bytes`] for callers that
/// must preserve binary payloads (whole-file decryption via age /
/// gpg, etc.).
#[derive(Debug, Clone)]
pub struct CommandOutputBytes {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

/// [`CommandRunner`] that spawns a real shell process.
///
/// `verbose` controls whether the script's raw stdout/stderr is streamed
/// through to the user's terminal. Regardless of the flag, lines matching
/// the `# status:` convention on stdout are always surfaced as live progress
/// markers, and captured output is returned via [`CommandOutput`] for
/// callers that want it.
pub struct ShellCommandRunner {
    verbose: bool,
}

impl ShellCommandRunner {
    pub fn new(verbose: bool) -> Self {
        Self { verbose }
    }
}

pub(crate) fn format_command_for_display(executable: &str, arguments: &[String]) -> String {
    if arguments.is_empty() {
        return executable.to_string();
    }

    let args = arguments
        .iter()
        .map(|arg| {
            if arg.is_empty()
                || arg.chars().any(char::is_whitespace)
                || arg.contains('"')
                || arg.contains('\'')
            {
                format!("{arg:?}")
            } else {
                arg.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!("{executable} {args}")
}

/// Strip the `# status:` prefix from a script line, returning the
/// trimmed message if present.
///
/// Matches `#status:`, `# status:`, and any leading whitespace before
/// the `#`. Designed to be tool-agnostic — a script using this convention
/// is still valid and meaningful when run manually outside dodot.
pub(crate) fn parse_status_line(line: &str) -> Option<&str> {
    let s = line.trim_start();
    let rest = s.strip_prefix('#')?;
    let rest = rest.trim_start();
    let msg = rest.strip_prefix("status:")?;
    Some(msg.trim())
}

impl CommandRunner for ShellCommandRunner {
    fn run(&self, executable: &str, arguments: &[String]) -> Result<CommandOutput> {
        use std::io::{BufRead, BufReader, IsTerminal, Write};
        use std::process::{Command, Stdio};
        use std::sync::{Arc, Mutex};
        use std::thread;

        let mut child = Command::new(executable)
            .args(arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| crate::DodotError::CommandFailed {
                command: format_command_for_display(executable, arguments),
                exit_code: -1,
                stderr: e.to_string(),
            })?;

        let stdout_pipe = child
            .stdout
            .take()
            .expect("piped stdout missing after spawn");
        let stderr_pipe = child
            .stderr
            .take()
            .expect("piped stderr missing after spawn");

        // ANSI dim only if the user's stdout is a TTY — keeps colour
        // codes out of pipes/log files.
        let tty = std::io::stdout().is_terminal();
        let dim = if tty { "\x1b[2m" } else { "" };
        let reset = if tty { "\x1b[0m" } else { "" };
        let arrow = if tty { "→" } else { "->" };

        let verbose = self.verbose;
        let stderr_buf = Arc::new(Mutex::new(String::new()));

        // Read raw bytes (not `BufRead::lines()`) so non-UTF-8 output
        // doesn't stop draining mid-stream — a stalled drain would
        // deadlock the child once the pipe buffer fills. Decode each
        // line lossily for display/capture; binary garbage becomes U+FFFD
        // rather than aborting the read.
        fn pop_eol(buf: &mut Vec<u8>) {
            if buf.last() == Some(&b'\n') {
                buf.pop();
            }
            if buf.last() == Some(&b'\r') {
                buf.pop();
            }
        }

        // Drain stderr in a worker thread to avoid pipe-buffer deadlock
        // (a chatty stderr can block the child if no one's reading).
        let stderr_thread = {
            let buf = stderr_buf.clone();
            thread::spawn(move || {
                let mut reader = BufReader::new(stderr_pipe);
                let host_stderr = std::io::stderr();
                let mut bytes = Vec::new();
                loop {
                    bytes.clear();
                    match reader.read_until(b'\n', &mut bytes) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            pop_eol(&mut bytes);
                            let line = String::from_utf8_lossy(&bytes);
                            {
                                let mut guard = buf.lock().expect("stderr buf poisoned");
                                guard.push_str(&line);
                                guard.push('\n');
                            }
                            if verbose {
                                let mut h = host_stderr.lock();
                                let _ = writeln!(h, "{line}");
                            }
                        }
                    }
                }
            })
        };

        // Read stdout on the main thread: capture, scan for `# status:`,
        // optionally passthrough.
        let mut stdout_buf = String::new();
        {
            let mut reader = BufReader::new(stdout_pipe);
            let host_stdout = std::io::stdout();
            let mut bytes = Vec::new();
            loop {
                bytes.clear();
                match reader.read_until(b'\n', &mut bytes) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        pop_eol(&mut bytes);
                        let line = String::from_utf8_lossy(&bytes);
                        stdout_buf.push_str(&line);
                        stdout_buf.push('\n');

                        if let Some(msg) = parse_status_line(&line) {
                            let mut h = host_stdout.lock();
                            let _ = writeln!(h, "{dim}{arrow}{reset} {msg}");
                        }
                        if verbose {
                            let mut h = host_stdout.lock();
                            let _ = writeln!(h, "{line}");
                        }
                    }
                }
            }
        }

        let _ = stderr_thread.join();
        let stderr_text = stderr_buf.lock().expect("stderr buf poisoned").clone();

        let status = child.wait().map_err(|e| crate::DodotError::CommandFailed {
            command: format_command_for_display(executable, arguments),
            exit_code: -1,
            stderr: e.to_string(),
        })?;
        let exit_code = status.code().unwrap_or(-1);

        if !status.success() {
            // When not verbose, the user hasn't seen any of the script's
            // stderr — surface it now so a failure is debuggable.
            if !verbose && !stderr_text.is_empty() {
                let host_stderr = std::io::stderr();
                let mut h = host_stderr.lock();
                let _ = h.write_all(stderr_text.as_bytes());
                if !stderr_text.ends_with('\n') {
                    let _ = writeln!(h);
                }
            }
            return Err(crate::DodotError::CommandFailed {
                command: format_command_for_display(executable, arguments),
                exit_code,
                stderr: stderr_text,
            });
        }

        Ok(CommandOutput {
            exit_code,
            stdout: stdout_buf,
            stderr: stderr_text,
        })
    }

    /// Override of the default trait impl: reads stdout as raw
    /// bytes (no `from_utf8_lossy` decode), so binary payloads
    /// from age / gpg whole-file decryption survive verbatim.
    /// Stderr is still buffered as text via the same drainer
    /// pattern `run` uses — gpg and age both emit
    /// human-readable diagnostics.
    fn run_bytes(&self, executable: &str, arguments: &[String]) -> Result<CommandOutputBytes> {
        use std::io::{Read, Write};
        use std::process::{Command, Stdio};
        use std::sync::{Arc, Mutex};
        use std::thread;

        let mut child = Command::new(executable)
            .args(arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| crate::DodotError::CommandFailed {
                command: format_command_for_display(executable, arguments),
                exit_code: -1,
                stderr: e.to_string(),
            })?;

        let mut stdout_pipe = child
            .stdout
            .take()
            .expect("piped stdout missing after spawn");
        let stderr_pipe = child
            .stderr
            .take()
            .expect("piped stderr missing after spawn");

        let stderr_buf = Arc::new(Mutex::new(String::new()));
        let stderr_thread = {
            let buf = stderr_buf.clone();
            thread::spawn(move || {
                let mut s = String::new();
                let mut reader = std::io::BufReader::new(stderr_pipe);
                let _ = std::io::Read::read_to_string(&mut reader, &mut s);
                if let Ok(mut guard) = buf.lock() {
                    guard.push_str(&s);
                }
            })
        };

        // Read stdout as raw bytes on the main thread. No
        // line-by-line passthrough / status-line parsing here —
        // those are install-script ergonomics and have no place in
        // a binary-payload pipe.
        let mut stdout_buf: Vec<u8> = Vec::new();
        if let Err(e) = stdout_pipe.read_to_end(&mut stdout_buf) {
            // Surface the IO error, but still wait for the child
            // so we don't leak a zombie.
            let _ = child.wait();
            let _ = stderr_thread.join();
            return Err(crate::DodotError::CommandFailed {
                command: format_command_for_display(executable, arguments),
                exit_code: -1,
                stderr: e.to_string(),
            });
        }

        let _ = stderr_thread.join();
        let stderr_text = stderr_buf.lock().expect("stderr buf poisoned").clone();

        let status = child.wait().map_err(|e| crate::DodotError::CommandFailed {
            command: format_command_for_display(executable, arguments),
            exit_code: -1,
            stderr: e.to_string(),
        })?;
        let exit_code = status.code().unwrap_or(-1);

        if !status.success() && !stderr_text.is_empty() && !self.verbose {
            // Mirror `run`'s "surface stderr on failure when not
            // verbose" pattern so a quiet failure is still
            // debuggable.
            let host_stderr = std::io::stderr();
            let mut h = host_stderr.lock();
            let _ = h.write_all(stderr_text.as_bytes());
            if !stderr_text.ends_with('\n') {
                let _ = writeln!(h);
            }
        }

        // Note: unlike `run`, we don't return `Err` on non-zero
        // exit. Whole-file preprocessors do their own exit-code
        // mapping (see `age.rs` / `gpg.rs`) and need to inspect
        // exit_code + stderr to surface actionable hints.
        Ok(CommandOutputBytes {
            exit_code,
            stdout: stdout_buf,
            stderr: stderr_text,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_status_line_matches_no_space() {
        assert_eq!(parse_status_line("#status: building"), Some("building"));
    }

    #[test]
    fn parse_status_line_matches_one_space() {
        assert_eq!(
            parse_status_line("# status: downloading installer"),
            Some("downloading installer")
        );
    }

    #[test]
    fn parse_status_line_matches_extra_whitespace() {
        assert_eq!(
            parse_status_line("   #   status:   compiling   "),
            Some("compiling")
        );
    }

    #[test]
    fn parse_status_line_rejects_plain_comment() {
        assert_eq!(parse_status_line("# just a comment"), None);
    }

    #[test]
    fn parse_status_line_rejects_non_comment() {
        assert_eq!(parse_status_line("echo status: foo"), None);
    }

    #[test]
    fn parse_status_line_rejects_shebang() {
        // `#!/usr/bin/env bash` doesn't start the magic word — ignored.
        assert_eq!(parse_status_line("#!/bin/bash"), None);
    }

    #[test]
    fn parse_status_line_returns_empty_message() {
        // Empty status: still matches (script chose to print a blank progress).
        assert_eq!(parse_status_line("# status:"), Some(""));
    }

    #[test]
    fn shell_runner_streams_and_captures_real_script() {
        // Smoke-test the real spawn/streaming path. We assert on the
        // captured CommandOutput; live host-stdout assertions would
        // require redirecting process-wide stdout and aren't worth the
        // complexity here.
        let runner = ShellCommandRunner::new(false);
        let script = "echo starting; \
            echo '# status: phase one'; \
            echo middle; \
            echo '# status: phase two'; \
            echo done";
        let out = runner
            .run("bash", &["-c".into(), script.into()])
            .expect("script should succeed");
        assert!(out.stdout.contains("starting"));
        assert!(out.stdout.contains("# status: phase one"));
        assert!(out.stdout.contains("middle"));
        assert!(out.stdout.contains("# status: phase two"));
        assert!(out.stdout.contains("done"));
        assert_eq!(out.exit_code, 0);
    }

    #[test]
    fn shell_runner_returns_error_on_nonzero_exit() {
        let runner = ShellCommandRunner::new(false);
        let result = runner.run("bash", &["-c".into(), "exit 7".into()]);
        match result {
            Err(crate::DodotError::CommandFailed { exit_code, .. }) => {
                assert_eq!(exit_code, 7);
            }
            other => panic!("expected CommandFailed, got {other:?}"),
        }
    }

    #[test]
    fn shell_runner_captures_stderr_in_command_output() {
        let runner = ShellCommandRunner::new(false);
        let out = runner
            .run("bash", &["-c".into(), "echo hello >&2; echo world".into()])
            .expect("script should succeed");
        assert!(out.stderr.contains("hello"));
        assert!(out.stdout.contains("world"));
    }
}

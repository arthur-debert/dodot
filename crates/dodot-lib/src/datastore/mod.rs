//! State management for dodot.
//!
//! The [`DataStore`] trait defines dodot's 8-method storage API.
//! [`FilesystemDataStore`] implements it using symlinks and sentinel
//! files on a real (or test) filesystem via the [`Fs`](crate::fs::Fs) trait.

mod filesystem;

pub use filesystem::FilesystemDataStore;

use std::path::{Path, PathBuf};

use crate::Result;

/// Dodot's storage interface.
///
/// State is represented entirely by symlinks and sentinel files in the
/// filesystem — no database, no lock files. The 8 methods break into
/// three groups:
///
/// **Mutations** — modify state:
/// - [`create_data_link`](DataStore::create_data_link)
/// - [`create_user_link`](DataStore::create_user_link)
/// - [`run_and_record`](DataStore::run_and_record)
/// - [`remove_state`](DataStore::remove_state)
///
/// **Queries** — read state:
/// - [`has_sentinel`](DataStore::has_sentinel)
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
}

/// Output from a command execution.
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub exit_code: i32,
    pub stdout: String,
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
    pub verbose: bool,
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

        // Drain stderr in a worker thread to avoid pipe-buffer deadlock
        // (a chatty stderr can block the child if no one's reading).
        let stderr_thread = {
            let buf = stderr_buf.clone();
            thread::spawn(move || {
                let reader = BufReader::new(stderr_pipe);
                let host_stderr = std::io::stderr();
                for line in reader.lines().map_while(std::io::Result::ok) {
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
            })
        };

        // Read stdout on the main thread: capture, scan for `# status:`,
        // optionally passthrough.
        let mut stdout_buf = String::new();
        {
            let reader = BufReader::new(stdout_pipe);
            let host_stdout = std::io::stdout();
            for line in reader.lines().map_while(std::io::Result::ok) {
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

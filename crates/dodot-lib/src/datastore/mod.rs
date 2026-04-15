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
pub struct ShellCommandRunner;

fn format_command_for_display(executable: &str, arguments: &[String]) -> String {
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

impl CommandRunner for ShellCommandRunner {
    fn run(&self, executable: &str, arguments: &[String]) -> Result<CommandOutput> {
        let output = std::process::Command::new(executable)
            .args(arguments)
            .output()
            .map_err(|e| crate::DodotError::CommandFailed {
                command: format_command_for_display(executable, arguments),
                exit_code: -1,
                stderr: e.to_string(),
            })?;

        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        if !output.status.success() {
            return Err(crate::DodotError::CommandFailed {
                command: format_command_for_display(executable, arguments),
                exit_code,
                stderr,
            });
        }

        Ok(CommandOutput {
            exit_code,
            stdout,
            stderr,
        })
    }
}

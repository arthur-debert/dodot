use std::path::PathBuf;
use thiserror::Error;

/// The single error type for all dodot operations.
///
/// Each variant carries enough context to produce a useful error message
/// without needing to inspect the source chain.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum DodotError {
    #[error("filesystem error at {path}: {source}")]
    Fs {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("symlink conflict: {path} already exists and is not managed by dodot")]
    SymlinkConflict { path: PathBuf },

    #[error("protected path: {path} cannot be symlinked")]
    ProtectedPath { path: PathBuf },

    #[error("pack not found: {name}")]
    PackNotFound { name: String },

    #[error("pack is invalid: {name}: {reason}")]
    PackInvalid { name: String, reason: String },

    #[error("handler not found: {name}")]
    HandlerNotFound { name: String },

    #[error("config error: {0}")]
    Config(String),

    #[error("command failed: {command} (exit code {exit_code})\n{stderr}")]
    CommandFailed {
        command: String,
        exit_code: i32,
        stderr: String,
    },

    #[error("invalid pattern {pattern}: {reason}")]
    InvalidPattern { pattern: String, reason: String },

    #[error("cross-pack deployment conflict detected (--force does not override this):\n{}", crate::conflicts::format_conflicts(.conflicts))]
    CrossPackConflict {
        conflicts: Vec<crate::conflicts::Conflict>,
    },

    #[error("preprocessing failed for {source_file} ({preprocessor}): {message}")]
    PreprocessorError {
        preprocessor: String,
        source_file: PathBuf,
        message: String,
    },

    #[error("preprocessing collision in pack \"{pack}\": {source_file} expands to {expanded_name}, which already exists as a regular file")]
    PreprocessorCollision {
        pack: String,
        source_file: String,
        expanded_name: String,
    },

    #[error("{0}")]
    Other(String),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, DodotError>;

/// Helper to wrap an `io::Error` with the path that caused it.
pub(crate) fn fs_err(path: impl Into<PathBuf>, source: std::io::Error) -> DodotError {
    DodotError::Fs {
        path: path.into(),
        source,
    }
}

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

    #[error("pack ordering collision: display name `{display_name}` resolves to multiple packs:\n{}", .paths.iter().map(|p| format!("  - {}", p.display())).collect::<Vec<_>>().join("\n"))]
    PackOrderingCollision {
        display_name: String,
        paths: Vec<PathBuf>,
    },

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

    #[error("preprocessing collision in pack \"{pack}\": {source_file} expands to {expanded_name}, which conflicts with an existing pack file or another preprocessor's output")]
    PreprocessorCollision {
        pack: String,
        source_file: String,
        expanded_name: String,
    },

    #[error("template render failed for {}:\n  {message}", source_file.display())]
    TemplateRender {
        source_file: PathBuf,
        message: String,
    },

    #[error("template variable name \"{name}\" is reserved (dodot and env are built-in namespaces); choose a different name in [preprocessor.template.vars]")]
    TemplateReservedVar { name: String },

    // Hint uses `git diff -- '<path>'`: the `--` separator defangs paths
    // that start with a dash, and single-quoting handles whitespace and
    // shell metacharacters. Paths containing literal single quotes are
    // not auto-escaped — vanishingly rare for dotfile sources, and the
    // user can adjust the command manually in that edge case.
    #[error("unresolved dodot-conflict markers in {} at line{} {}\n  resolve the conflict block(s) with `git diff -- '{}'` and remove the dodot-conflict marker lines, then re-run.", source_file.display(), if line_numbers.len() == 1 { "" } else { "s" }, line_numbers.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", "), source_file.display())]
    UnresolvedConflictMarker {
        source_file: PathBuf,
        line_numbers: Vec<usize>,
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

//! Operation types — the atomic units of work dodot performs.
//!
//! dodot only does four things. Everything else is orchestration.

use std::path::PathBuf;

use serde::Serialize;

/// The four atomic operations dodot performs.
///
/// Uses enum variants with associated data so that each variant carries
/// exactly the fields it needs — impossible states are unrepresentable.
#[derive(Debug, Clone, Serialize)]
pub enum Operation {
    /// Link a source file into the datastore.
    /// `handler_data_dir(pack, handler) / filename -> source`
    CreateDataLink {
        pack: String,
        handler: String,
        source: PathBuf,
    },

    /// Create a user-visible symlink.
    /// `user_path -> datastore_path`
    CreateUserLink {
        pack: String,
        handler: String,
        datastore_path: PathBuf,
        user_path: PathBuf,
    },

    /// Execute a command and record a sentinel on success.
    RunCommand {
        pack: String,
        handler: String,
        executable: String,
        arguments: Vec<String>,
        sentinel: String,
    },

    /// Check whether a sentinel exists (query, not mutation).
    CheckSentinel {
        pack: String,
        handler: String,
        sentinel: String,
    },
}

impl Operation {
    pub fn pack(&self) -> &str {
        match self {
            Self::CreateDataLink { pack, .. }
            | Self::CreateUserLink { pack, .. }
            | Self::RunCommand { pack, .. }
            | Self::CheckSentinel { pack, .. } => pack,
        }
    }

    pub fn handler(&self) -> &str {
        match self {
            Self::CreateDataLink { handler, .. }
            | Self::CreateUserLink { handler, .. }
            | Self::RunCommand { handler, .. }
            | Self::CheckSentinel { handler, .. } => handler,
        }
    }

    /// Human-readable label for the operation type.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::CreateDataLink { .. } => "CreateDataLink",
            Self::CreateUserLink { .. } => "CreateUserLink",
            Self::RunCommand { .. } => "RunCommand",
            Self::CheckSentinel { .. } => "CheckSentinel",
        }
    }
}

/// Higher-level intent produced by handlers.
///
/// Handlers declare *what* they want, not *how* to do it. The executor
/// converts intents into [`Operation`]s and [`DataStore`](crate::datastore::DataStore) calls.
///
/// This avoids the awkward pattern where `CreateUserLink` would need a
/// placeholder datastore path that the executor fills later — instead
/// `Link` carries the full intent and the executor splits it into two
/// atomic operations.
#[derive(Debug, Clone, Serialize)]
pub enum HandlerIntent {
    /// Symlink handler: create both legs of the double-link.
    /// Executor splits this into CreateDataLink + CreateUserLink.
    Link {
        pack: String,
        handler: String,
        source: PathBuf,
        user_path: PathBuf,
    },

    /// Shell/path handlers: stage a file in the datastore.
    /// Shell init reads it from there.
    Stage {
        pack: String,
        handler: String,
        source: PathBuf,
    },

    /// Install/homebrew handlers: run a command with sentinel tracking.
    Run {
        pack: String,
        handler: String,
        executable: String,
        arguments: Vec<String>,
        sentinel: String,
    },
}

impl HandlerIntent {
    pub fn pack(&self) -> &str {
        match self {
            Self::Link { pack, .. } | Self::Stage { pack, .. } | Self::Run { pack, .. } => pack,
        }
    }

    pub fn handler(&self) -> &str {
        match self {
            Self::Link { handler, .. }
            | Self::Stage { handler, .. }
            | Self::Run { handler, .. } => handler,
        }
    }
}

/// The outcome of executing a single operation.
#[derive(Debug, Clone, Serialize)]
pub struct OperationResult {
    pub operation: Operation,
    pub success: bool,
    pub message: String,
}

impl OperationResult {
    pub fn ok(operation: Operation, message: impl Into<String>) -> Self {
        Self {
            operation,
            success: true,
            message: message.into(),
        }
    }

    pub fn fail(operation: Operation, message: impl Into<String>) -> Self {
        Self {
            operation,
            success: false,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_accessors() {
        let op = Operation::CreateDataLink {
            pack: "vim".into(),
            handler: "symlink".into(),
            source: PathBuf::from("/src/vimrc"),
        };
        assert_eq!(op.pack(), "vim");
        assert_eq!(op.handler(), "symlink");
        assert_eq!(op.kind(), "CreateDataLink");
    }

    #[test]
    fn handler_intent_accessors() {
        let intent = HandlerIntent::Link {
            pack: "git".into(),
            handler: "symlink".into(),
            source: PathBuf::from("/src/gitconfig"),
            user_path: PathBuf::from("/home/.gitconfig"),
        };
        assert_eq!(intent.pack(), "git");
        assert_eq!(intent.handler(), "symlink");
    }

    #[test]
    fn operation_result_constructors() {
        let op = Operation::CheckSentinel {
            pack: "vim".into(),
            handler: "install".into(),
            sentinel: "abc".into(),
        };
        let ok = OperationResult::ok(op.clone(), "done");
        assert!(ok.success);

        let fail = OperationResult::fail(op, "oops");
        assert!(!fail.success);
    }

    #[test]
    fn operation_serializes() {
        let op = Operation::RunCommand {
            pack: "vim".into(),
            handler: "install".into(),
            executable: "echo".into(),
            arguments: vec!["hi".into()],
            sentinel: "s1".into(),
        };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("RunCommand"));
        assert!(json.contains("echo"));
        assert!(json.contains("hi"));
    }
}

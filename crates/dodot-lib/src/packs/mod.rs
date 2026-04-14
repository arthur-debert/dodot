//! Pack types and discovery.
//!
//! A pack is a directory of related dotfiles (e.g. `vim/`, `git/`, `zsh/`).
//! It is the unit of organisation, deployment, and removal.

use std::path::PathBuf;

use serde::Serialize;

use crate::handlers::HandlerConfig;

/// A dotfile pack — a directory of related configuration files.
#[derive(Debug, Clone, Serialize)]
pub struct Pack {
    /// Directory name (e.g. `"vim"`).
    pub name: String,

    /// Absolute path to the pack directory.
    pub path: PathBuf,

    /// Handler-relevant configuration for this pack (merged from
    /// app defaults + root config + pack config).
    pub config: HandlerConfig,
}

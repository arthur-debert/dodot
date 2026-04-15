//! `fill` command — add placeholder files to an existing pack.
//!
//! Creates template files for each configured handler pattern so the
//! user has a starting point. Files that already exist are skipped.

use serde::Serialize;

use crate::packs::orchestration::ExecutionContext;
use crate::{DodotError, Result};

#[derive(Debug, Clone, Serialize)]
pub struct FillResult {
    pub message: String,
    pub details: Vec<String>,
}

/// Handler template content keyed by the filename to create.
struct FillTemplate {
    filename: &'static str,
    content: &'static str,
}

const TEMPLATES: &[FillTemplate] = &[
    FillTemplate {
        filename: "install.sh",
        content: r#"#!/usr/bin/env bash
# Install script for PACK_NAME
#
# Runs ONCE during `dodot up`. Re-runs only if this file changes
# (tracked by content checksum). Should be idempotent.

set -euo pipefail

echo "Installing PACK_NAME..."

# Add your installation commands below
# Examples:
# mkdir -p "$HOME/.config/PACK_NAME"
# curl -fsSL https://example.com/install.sh | bash
"#,
    },
    FillTemplate {
        filename: "aliases.sh",
        content: r#"#!/usr/bin/env sh
# Shell aliases for PACK_NAME
#
# Sourced into your shell on every session via dodot-init.sh.
# Changes take effect in new shells or after `dodot up`.

# Add your aliases below
# Examples:
# alias ll='ls -la'
# alias g='git'
"#,
    },
    FillTemplate {
        filename: "Brewfile",
        content: r#"# Homebrew dependencies for PACK_NAME
#
# Processed during `dodot up`. Re-runs only if this file changes.
# Uses standard Brewfile syntax:
# https://github.com/Homebrew/homebrew-bundle

# Examples:
# brew 'git'
# brew 'tmux'
# cask 'firefox'
"#,
    },
];

/// Add placeholder files to an existing pack.
///
/// Creates template files for install.sh, aliases.sh, and Brewfile.
/// Skips files that already exist. Replaces `PACK_NAME` in templates
/// with the actual pack name.
pub fn fill(pack_name: &str, ctx: &ExecutionContext) -> Result<FillResult> {
    let pack_path = ctx.paths.dotfiles_root().join(pack_name);

    if !ctx.fs.exists(&pack_path) {
        return Err(DodotError::PackNotFound {
            name: pack_name.into(),
        });
    }

    let mut details = Vec::new();
    let mut created = 0;

    for template in TEMPLATES {
        let file_path = pack_path.join(template.filename);

        if ctx.fs.exists(&file_path) {
            details.push(format!("  {} (exists, skipped)", template.filename));
            continue;
        }

        let content = template.content.replace("PACK_NAME", pack_name);
        ctx.fs.write_file(&file_path, content.as_bytes())?;

        // Make install.sh executable
        if template.filename.ends_with(".sh") {
            ctx.fs.set_permissions(&file_path, 0o755)?;
        }

        details.push(format!("  {} (created)", template.filename));
        created += 1;
    }

    let message = if created == 0 {
        format!("Pack '{pack_name}' already has all template files.")
    } else {
        format!("Added {created} template file(s) to '{pack_name}'.")
    };

    Ok(FillResult { message, details })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigManager;
    use crate::datastore::{CommandOutput, CommandRunner, FilesystemDataStore};
    use crate::fs::Fs;
    use crate::paths::Pather;
    use crate::testing::TempEnvironment;
    use std::sync::Arc;

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

    fn make_ctx(env: &TempEnvironment) -> ExecutionContext {
        let runner = Arc::new(NoopRunner);
        let datastore = Arc::new(FilesystemDataStore::new(
            env.fs.clone(),
            env.paths.clone(),
            runner,
        ));
        let config_manager = Arc::new(ConfigManager::new(&env.dotfiles_root).unwrap());
        ExecutionContext {
            fs: env.fs.clone() as Arc<dyn Fs>,
            datastore,
            paths: env.paths.clone() as Arc<dyn Pather>,
            config_manager,
            dry_run: false,
            no_provision: false,
            provision_rerun: false,
        }
    }

    #[test]
    fn fill_creates_template_files() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .build();
        let ctx = make_ctx(&env);

        let result = fill("vim", &ctx).unwrap();
        assert!(result.message.contains("3 template"));

        env.assert_exists(&env.dotfiles_root.join("vim/install.sh"));
        env.assert_exists(&env.dotfiles_root.join("vim/aliases.sh"));
        env.assert_exists(&env.dotfiles_root.join("vim/Brewfile"));

        // install.sh should contain pack name
        let content = env
            .fs
            .read_to_string(&env.dotfiles_root.join("vim/install.sh"))
            .unwrap();
        assert!(content.contains("vim"), "should contain pack name");
        assert!(!content.contains("PACK_NAME"), "should replace placeholder");
    }

    #[test]
    fn fill_skips_existing_files() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("install.sh", "#!/bin/sh\nmy custom script")
            .done()
            .build();
        let ctx = make_ctx(&env);

        let result = fill("vim", &ctx).unwrap();
        // Only 2 created (aliases.sh + Brewfile), install.sh skipped
        assert!(result.message.contains("2 template"));
        assert!(result
            .details
            .iter()
            .any(|d| d.contains("install.sh") && d.contains("skipped")));

        // Original content preserved
        let content = env
            .fs
            .read_to_string(&env.dotfiles_root.join("vim/install.sh"))
            .unwrap();
        assert_eq!(content, "#!/bin/sh\nmy custom script");
    }

    #[test]
    fn fill_all_existing_reports_correctly() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("install.sh", "x")
            .file("aliases.sh", "x")
            .file("Brewfile", "x")
            .done()
            .build();
        let ctx = make_ctx(&env);

        let result = fill("vim", &ctx).unwrap();
        assert!(result.message.contains("already has all"));
    }

    #[test]
    fn fill_nonexistent_pack_errors() {
        let env = TempEnvironment::builder().build();
        let ctx = make_ctx(&env);

        let err = fill("nonexistent", &ctx).unwrap_err();
        assert!(matches!(err, DodotError::PackNotFound { .. }));
    }
}

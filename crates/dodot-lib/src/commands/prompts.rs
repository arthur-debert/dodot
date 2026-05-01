//! `dodot prompts` — inspect and reset dismissed-prompt state.
//!
//! Thin wrapper over [`crate::prompts::PromptRegistry`]. Two operations:
//!
//! - `prompts list` — show every known prompt key with its current
//!   dismissed/active state and a human-readable description.
//! - `prompts reset` — clear one key (so the prompt fires again next
//!   time) or all keys.

use serde::Serialize;

use crate::commands::MessageResult;
use crate::packs::orchestration::ExecutionContext;
use crate::prompts::{catalog, PromptRegistry};
use crate::Result;

/// One row in `dodot prompts list`.
#[derive(Debug, Clone, Serialize)]
pub struct PromptRow {
    pub key: String,
    pub description: String,
    /// `"dismissed"` or `"active"`. Strings rather than bools so the
    /// template can colour them via existing status-style tags.
    pub status: String,
    /// Unix timestamp (seconds) of dismissal, or `None` if active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dismissed_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptsListResult {
    pub rows: Vec<PromptRow>,
    /// Backing file path for the registry (helpful in diagnostics).
    pub registry_path: String,
}

/// `dodot prompts list` — show every known prompt with its state.
///
/// Returns rows for every key in [`catalog::KNOWN_PROMPTS`] plus any
/// dismissed key not in the catalog (so a stale entry from a prior
/// dodot version is still visible and resettable).
pub fn list(ctx: &ExecutionContext) -> Result<PromptsListResult> {
    let path = ctx.paths.prompts_path();
    let registry = PromptRegistry::load(ctx.fs.as_ref(), path.clone())?;

    let mut rows: Vec<PromptRow> = catalog::KNOWN_PROMPTS
        .iter()
        .map(|d| {
            let dismissed_at = registry
                .dismissed()
                .into_iter()
                .find(|(k, _)| *k == d.key)
                .map(|(_, r)| r.dismissed_at);
            PromptRow {
                key: d.key.to_string(),
                description: d.description.to_string(),
                status: if dismissed_at.is_some() {
                    "dismissed".into()
                } else {
                    "active".into()
                },
                dismissed_at,
            }
        })
        .collect();

    // Surface any unknown keys lurking in the registry so they can be
    // reset. Catalog absence is not an error — older dodot versions
    // may have written them.
    for (key, record) in registry.dismissed() {
        if catalog::lookup(key).is_none() {
            rows.push(PromptRow {
                key: key.to_string(),
                description: "(no catalog description; key from a prior dodot version)".into(),
                status: "dismissed".into(),
                dismissed_at: Some(record.dismissed_at),
            });
        }
    }

    rows.sort_by(|a, b| a.key.cmp(&b.key));

    Ok(PromptsListResult {
        rows,
        registry_path: path.display().to_string(),
    })
}

/// `dodot prompts reset` — clear dismissals so the prompt fires again.
///
/// `key = None` means reset every dismissed prompt.
pub fn reset(key: Option<&str>, ctx: &ExecutionContext) -> Result<MessageResult> {
    let path = ctx.paths.prompts_path();
    let mut registry = PromptRegistry::load(ctx.fs.as_ref(), path)?;

    let (message, details) = match key {
        Some(k) => {
            if registry.reset(k) {
                (
                    format!("Reset prompt `{k}`."),
                    vec![
                        "The prompt will fire again next time the relevant condition is met."
                            .into(),
                    ],
                )
            } else {
                // Not an error — just nothing to do.
                (
                    format!("Prompt `{k}` was already active (or never dismissed)."),
                    vec![],
                )
            }
        }
        None => {
            let n = registry.reset_all();
            if n == 0 {
                ("No dismissed prompts to reset.".into(), vec![])
            } else {
                (
                    format!("Reset {n} dismissed prompt(s)."),
                    vec!["All previously dismissed prompts will fire again next time their condition is met.".into()],
                )
            }
        }
    };

    registry.save(ctx.fs.as_ref())?;

    Ok(MessageResult { message, details })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::config::ConfigManager;
    use crate::datastore::{CommandOutput, CommandRunner, FilesystemDataStore};
    use crate::fs::Fs;
    use crate::paths::Pather;
    use crate::testing::TempEnvironment;

    struct NoopRunner;
    impl CommandRunner for NoopRunner {
        fn run(&self, _executable: &str, _arguments: &[String]) -> Result<CommandOutput> {
            Ok(CommandOutput {
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    fn ctx(env: &TempEnvironment) -> ExecutionContext {
        let runner: Arc<dyn CommandRunner> = Arc::new(NoopRunner);
        let datastore = Arc::new(FilesystemDataStore::new(
            env.fs.clone(),
            env.paths.clone(),
            runner.clone(),
        ));
        let config_manager = Arc::new(ConfigManager::new(&env.dotfiles_root).unwrap());
        ExecutionContext {
            fs: env.fs.clone() as Arc<dyn Fs>,
            datastore,
            paths: env.paths.clone() as Arc<dyn Pather>,
            config_manager,
            syntax_checker: Arc::new(crate::shell::NoopSyntaxChecker),
            command_runner: runner,
            dry_run: false,
            no_provision: true,
            provision_rerun: false,
            force: false,
            view_mode: crate::commands::ViewMode::Full,
            group_mode: crate::commands::GroupMode::Name,
            verbose: false,
        }
    }

    #[test]
    fn list_shows_every_known_prompt_as_active_initially() {
        let env = TempEnvironment::builder().build();
        let result = list(&ctx(&env)).expect("list");
        assert_eq!(result.rows.len(), catalog::KNOWN_PROMPTS.len());
        for row in &result.rows {
            assert_eq!(row.status, "active");
            assert!(row.dismissed_at.is_none());
        }
    }

    #[test]
    fn list_shows_dismissed_after_dismissal() {
        let env = TempEnvironment::builder().build();
        let path = ctx(&env).paths.prompts_path();
        let mut registry = PromptRegistry::load(env.fs.as_ref(), path).unwrap();
        registry.dismiss_at("plist.install_filters", 1714557600);
        registry.save(env.fs.as_ref()).unwrap();

        let result = list(&ctx(&env)).expect("list");
        let row = result
            .rows
            .iter()
            .find(|r| r.key == "plist.install_filters")
            .expect("row");
        assert_eq!(row.status, "dismissed");
        assert_eq!(row.dismissed_at, Some(1714557600));
    }

    #[test]
    fn list_surfaces_unknown_keys_so_they_can_be_reset() {
        let env = TempEnvironment::builder().build();
        let path = ctx(&env).paths.prompts_path();
        let mut registry = PromptRegistry::load(env.fs.as_ref(), path).unwrap();
        registry.dismiss_at("legacy.key.from.older.dodot", 1714557600);
        registry.save(env.fs.as_ref()).unwrap();

        let result = list(&ctx(&env)).expect("list");
        let row = result
            .rows
            .iter()
            .find(|r| r.key == "legacy.key.from.older.dodot")
            .expect("legacy row should appear");
        assert_eq!(row.status, "dismissed");
        assert!(row.description.contains("prior dodot version"));
    }

    #[test]
    fn reset_one_persists() {
        let env = TempEnvironment::builder().build();
        let path = ctx(&env).paths.prompts_path();
        let mut registry = PromptRegistry::load(env.fs.as_ref(), path.clone()).unwrap();
        registry.dismiss_at("plist.install_filters", 1714557600);
        registry.save(env.fs.as_ref()).unwrap();

        let r = reset(Some("plist.install_filters"), &ctx(&env)).expect("reset");
        assert!(r.message.contains("Reset prompt"));

        let registry = PromptRegistry::load(env.fs.as_ref(), path).unwrap();
        assert!(!registry.is_dismissed("plist.install_filters"));
    }

    #[test]
    fn reset_unknown_key_succeeds_without_error() {
        let env = TempEnvironment::builder().build();
        let r = reset(Some("never-dismissed"), &ctx(&env)).expect("reset");
        assert!(r.message.contains("already active"));
    }

    #[test]
    fn reset_all_clears_everything() {
        let env = TempEnvironment::builder().build();
        let path = ctx(&env).paths.prompts_path();
        let mut registry = PromptRegistry::load(env.fs.as_ref(), path.clone()).unwrap();
        registry.dismiss_at("a", 1);
        registry.dismiss_at("b", 2);
        registry.save(env.fs.as_ref()).unwrap();

        let r = reset(None, &ctx(&env)).expect("reset all");
        assert!(r.message.contains("Reset 2"));

        let registry = PromptRegistry::load(env.fs.as_ref(), path).unwrap();
        assert!(registry.dismissed().is_empty());
    }
}

//! Homebrew handler — runs `brew bundle` with checksum-based sentinel
//! tracking, via the shared [`crate::handlers::run_once`] machinery.
//!
//! The bulk of the behavior lives in
//! [`crate::handlers::run_once::RunOnceHandler`]. This module supplies
//! the [`BrewfileCommand`] specialization.
//!
//! Two of brew's defaults are turned off here, because both make a
//! `Brewfile` run do work the user did not ask for:
//!
//! - `--no-upgrade` — `brew bundle` upgrades every outdated formula
//!   it encounters by default, so "install what this Brewfile
//!   declares" would otherwise be a long mutating upgrade of packages
//!   that have nothing to do with the file.
//! - `HOMEBREW_NO_AUTO_UPDATE` — declared in the handler's
//!   [`crate::provisioners`] row and applied by the shared spawn
//!   path, it stops the first brew of the day from running a `brew
//!   update` first.
//!
//! They interact: with auto-update suppressed a stale brew stays
//! stale, so nothing downstream may assume a brew recent enough for
//! the newer `Brewfile` entry types. Checking brew's own version
//! before the bundle runs is a separate workstream's job and is not
//! implemented here: this handler still does no pre-flight
//! validation, and a too-old brew surfaces as brew's own parse error.

use std::path::Path;

use crate::handlers::run_once::RunOnceCommand;
use crate::handlers::{ExecutionPhase, HANDLER_HOMEBREW};

/// [`RunOnceCommand`] for the `homebrew` handler.
///
/// Invokes `brew bundle --no-upgrade --file <abs path>`, with
/// `HOMEBREW_NO_AUTO_UPDATE` set from the handler's descriptor row.
/// See the module docs for why both. No pre-flight validation —
/// `brew` itself surfaces parse errors clearly when the Brewfile is
/// malformed. See the
/// [`RunOnceCommand`](crate::handlers::run_once::RunOnceCommand)
/// lifecycle invariant.
pub struct BrewfileCommand;

impl RunOnceCommand for BrewfileCommand {
    fn handler_name(&self) -> &str {
        HANDLER_HOMEBREW
    }

    fn phase(&self) -> ExecutionPhase {
        ExecutionPhase::Provision
    }

    fn command_for(&self, path: &Path) -> (String, Vec<String>) {
        (
            "brew".to_string(),
            vec![
                "bundle".into(),
                // Install what the Brewfile declares; leave every
                // other outdated formula on the machine alone.
                "--no-upgrade".into(),
                "--file".into(),
                path.to_string_lossy().into_owned(),
            ],
        )
    }

    fn status_deployed(&self) -> &str {
        "installed"
    }

    fn status_pending(&self) -> &str {
        "brew packages not installed"
    }

    fn status_ran_different(&self) -> &str {
        "brew packages older version"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::Fs;
    use crate::handlers::run_once::RunOnceHandler;
    use crate::handlers::{Handler, HandlerConfig};
    use crate::operations::HandlerIntent;
    use crate::rules::RuleMatch;
    use crate::testing::TempEnvironment;
    use std::collections::HashMap;

    #[test]
    fn brewfile_command_suppresses_auto_update() {
        // The variable is declared in the descriptor registry and
        // reaches the command through the shared default, so this
        // fails if the two ever disagree.
        assert_eq!(
            BrewfileCommand.environment(),
            vec![("HOMEBREW_NO_AUTO_UPDATE".to_string(), "1".to_string())]
        );
    }

    #[test]
    fn brewfile_command_does_not_upgrade_unrelated_formulae() {
        let (executable, arguments) = BrewfileCommand.command_for(Path::new("/packs/dev/Brewfile"));
        assert_eq!(executable, "brew");
        assert_eq!(
            arguments,
            vec!["bundle", "--no-upgrade", "--file", "/packs/dev/Brewfile"]
        );
    }

    #[test]
    fn brewfile_command_identity() {
        assert_eq!(BrewfileCommand.handler_name(), HANDLER_HOMEBREW);
        assert_eq!(BrewfileCommand.phase(), ExecutionPhase::Provision);
        assert_eq!(BrewfileCommand.status_deployed(), "installed");
        assert_eq!(
            BrewfileCommand.status_pending(),
            "brew packages not installed"
        );
    }

    #[test]
    fn brewfile_command_emits_run_intent_with_expected_shape() {
        let env = TempEnvironment::builder()
            .pack("dev")
            .file("Brewfile", "brew \"ripgrep\"")
            .done()
            .build();

        let runner = crate::datastore::NoopCommandRunner;
        let handler = RunOnceHandler::new(env.fs.as_ref(), &runner, BrewfileCommand);
        let matches = vec![RuleMatch {
            relative_path: "Brewfile".into(),
            absolute_path: env.dotfiles_root.join("dev/Brewfile"),
            pack: "dev".into(),
            handler: "homebrew".into(),
            is_dir: false,
            options: HashMap::new(),
            preprocessor_source: None,
            rendered_bytes: None,
        }];

        let pather = crate::paths::XdgPather::builder()
            .home(&env.home)
            .dotfiles_root(&env.dotfiles_root)
            .build()
            .unwrap();

        let intents = handler
            .to_intents(
                &matches,
                &HandlerConfig::default(),
                &pather,
                env.fs.as_ref() as &dyn Fs,
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
                assert_eq!(pack, "dev");
                assert_eq!(h, HANDLER_HOMEBREW);
                assert_eq!(executable, "brew");
                assert_eq!(arguments[0], "bundle");
                assert_eq!(arguments[1], "--no-upgrade");
                assert_eq!(arguments[2], "--file");
                assert!(arguments[3].ends_with("Brewfile"));
                assert_eq!(
                    environment.as_slice(),
                    &[("HOMEBREW_NO_AUTO_UPDATE".to_string(), "1".to_string())]
                );
                assert!(sentinel.starts_with("Brewfile-"));
                assert_eq!(sentinel.len(), "Brewfile-".len() + 16);
                assert_eq!(relative_path, "Brewfile");
                assert_eq!(content_hash.len(), 16);
                assert_eq!(*sentinel, format!("{relative_path}-{content_hash}"));
            }
            other => panic!("expected Run, got {other:?}"),
        }
    }
}

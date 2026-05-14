//! Homebrew handler — runs `brew bundle` with checksum-based sentinel
//! tracking, via the shared [`crate::handlers::run_once`] machinery.
//!
//! The bulk of the behavior lives in
//! [`crate::handlers::run_once::RunOnceHandler`]. This module supplies
//! the [`BrewfileCommand`] specialization: program name (`brew`) and
//! argument shape (`bundle --file <path>`).

use std::path::Path;

use crate::handlers::run_once::RunOnceCommand;
use crate::handlers::{ExecutionPhase, HANDLER_HOMEBREW};

/// [`RunOnceCommand`] for the `homebrew` handler.
///
/// Invokes `brew bundle --file <abs path>`. No pre-flight validation —
/// `brew` itself surfaces parse errors clearly when the Brewfile is
/// malformed.
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
                "--file".into(),
                path.to_string_lossy().into_owned(),
            ],
        )
    }

    fn status_deployed(&self) -> &str {
        "brew packages installed"
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
    fn brewfile_command_identity() {
        assert_eq!(BrewfileCommand.handler_name(), HANDLER_HOMEBREW);
        assert_eq!(BrewfileCommand.phase(), ExecutionPhase::Provision);
        assert_eq!(BrewfileCommand.status_deployed(), "brew packages installed");
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

        let handler = RunOnceHandler::new(env.fs.as_ref(), BrewfileCommand);
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
                sentinel,
                filename,
                content_hash,
            } => {
                assert_eq!(pack, "dev");
                assert_eq!(h, HANDLER_HOMEBREW);
                assert_eq!(executable, "brew");
                assert_eq!(arguments[0], "bundle");
                assert_eq!(arguments[1], "--file");
                assert!(arguments[2].ends_with("Brewfile"));
                assert!(sentinel.starts_with("Brewfile-"));
                assert_eq!(sentinel.len(), "Brewfile-".len() + 16);
                assert_eq!(filename, "Brewfile");
                assert_eq!(content_hash.len(), 16);
                assert_eq!(*sentinel, format!("{filename}-{content_hash}"));
            }
            other => panic!("expected Run, got {other:?}"),
        }
    }
}

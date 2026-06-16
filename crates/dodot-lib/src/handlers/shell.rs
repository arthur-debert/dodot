//! Shell handler — stages shell scripts for sourcing via dodot-init.sh.

use std::path::Path;

use crate::datastore::DataStore;
use crate::fs::Fs;
use crate::handlers::{ExecutionPhase, Handler, HandlerConfig, HandlerStatus, HANDLER_SHELL};
use crate::operations::HandlerIntent;
use crate::paths::Pather;
use crate::rules::RuleMatch;
use crate::Result;

pub struct ShellHandler;

impl Handler for ShellHandler {
    fn name(&self) -> &str {
        HANDLER_SHELL
    }

    fn phase(&self) -> ExecutionPhase {
        ExecutionPhase::ShellInit
    }

    fn to_intents(
        &self,
        matches: &[RuleMatch],
        _config: &HandlerConfig,
        _paths: &dyn Pather,
        _fs: &dyn Fs,
    ) -> Result<Vec<HandlerIntent>> {
        Ok(matches
            .iter()
            .filter(|m| !m.is_dir)
            .map(|m| HandlerIntent::Stage {
                pack: m.pack.clone(),
                handler: HANDLER_SHELL.into(),
                source: m.absolute_path.clone(),
            })
            .collect())
    }

    fn check_status(
        &self,
        file: &Path,
        pack: &str,
        datastore: &dyn DataStore,
    ) -> Result<HandlerStatus> {
        let has_state = datastore.has_handler_state(pack, HANDLER_SHELL)?;
        Ok(HandlerStatus {
            file: file.to_string_lossy().into_owned(),
            handler: HANDLER_SHELL.into(),
            deployed: has_state,
            message: if has_state {
                "sourced in shell".into()
            } else {
                "not sourced in shell".into()
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datastore::{FilesystemDataStore, NoopCommandRunner};
    use crate::handlers::HandlerConfig;
    use crate::operations::HandlerIntent;
    use crate::rules::RuleMatch;
    use crate::testing::TempEnvironment;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn file_match(pack: &str, rel: &str, abs: PathBuf) -> RuleMatch {
        RuleMatch {
            relative_path: rel.into(),
            absolute_path: abs,
            pack: pack.into(),
            handler: HANDLER_SHELL.into(),
            is_dir: false,
            options: HashMap::new(),
            preprocessor_source: None,
            rendered_bytes: None,
        }
    }

    fn dir_match(pack: &str, rel: &str, abs: PathBuf) -> RuleMatch {
        RuleMatch {
            relative_path: rel.into(),
            absolute_path: abs,
            pack: pack.into(),
            handler: HANDLER_SHELL.into(),
            is_dir: true,
            options: HashMap::new(),
            preprocessor_source: None,
            rendered_bytes: None,
        }
    }

    #[test]
    fn name_and_phase_identity() {
        assert_eq!(ShellHandler.name(), HANDLER_SHELL);
        assert_eq!(ShellHandler.phase(), ExecutionPhase::ShellInit);
    }

    #[test]
    fn to_intents_stages_each_matched_file() {
        let env = TempEnvironment::builder()
            .pack("dev")
            .file("aliases.sh", "alias g=git")
            .done()
            .build();
        let aliases = env.dotfiles_root.join("dev/aliases.sh");

        let intents = ShellHandler
            .to_intents(
                &[file_match("dev", "aliases.sh", aliases.clone())],
                &HandlerConfig::default(),
                env.paths.as_ref(),
                env.fs.as_ref(),
            )
            .unwrap();

        assert_eq!(intents.len(), 1);
        match &intents[0] {
            HandlerIntent::Stage {
                pack,
                handler,
                source,
            } => {
                assert_eq!(pack, "dev");
                assert_eq!(handler, HANDLER_SHELL);
                assert_eq!(source, &aliases);
            }
            other => panic!("expected Stage intent, got {other:?}"),
        }
    }

    #[test]
    fn to_intents_drops_directory_matches() {
        // The shell handler sources files; a directory match should
        // never produce an intent — the dir filter is the symmetric
        // partner of the path handler's "directories only" rule.
        let env = TempEnvironment::builder()
            .pack("dev")
            .file("aliases.sh", "alias g=git")
            .done()
            .build();

        let matches = vec![
            dir_match("dev", "shell", env.dotfiles_root.join("dev/shell")),
            file_match(
                "dev",
                "aliases.sh",
                env.dotfiles_root.join("dev/aliases.sh"),
            ),
        ];

        let intents = ShellHandler
            .to_intents(
                &matches,
                &HandlerConfig::default(),
                env.paths.as_ref(),
                env.fs.as_ref(),
            )
            .unwrap();

        assert_eq!(intents.len(), 1, "only the file match should stage");
        assert_eq!(intents[0].handler(), HANDLER_SHELL);
    }

    #[test]
    fn to_intents_empty_in_empty_out() {
        let env = TempEnvironment::builder().build();
        let intents = ShellHandler
            .to_intents(
                &[],
                &HandlerConfig::default(),
                env.paths.as_ref(),
                env.fs.as_ref(),
            )
            .unwrap();
        assert!(intents.is_empty());
    }

    #[test]
    fn check_status_pending_when_no_state() {
        let env = TempEnvironment::builder().build();
        let ds = FilesystemDataStore::new(
            env.fs.clone(),
            env.paths.clone(),
            Arc::new(NoopCommandRunner),
        );

        let status = ShellHandler
            .check_status(Path::new("aliases.sh"), "dev", &ds)
            .unwrap();

        assert_eq!(status.handler, HANDLER_SHELL);
        assert_eq!(status.file, "aliases.sh");
        assert!(!status.deployed);
        assert_eq!(status.message, "not sourced in shell");
    }

    #[test]
    fn check_status_deployed_when_state_present() {
        let env = TempEnvironment::builder()
            .pack("dev")
            .file("aliases.sh", "alias g=git")
            .done()
            .build();
        let ds = FilesystemDataStore::new(
            env.fs.clone(),
            env.paths.clone(),
            Arc::new(NoopCommandRunner),
        );

        let source = env.dotfiles_root.join("dev/aliases.sh");
        ds.create_data_link("dev", HANDLER_SHELL, &source).unwrap();

        let status = ShellHandler
            .check_status(Path::new("aliases.sh"), "dev", &ds)
            .unwrap();

        assert!(status.deployed);
        assert_eq!(status.message, "sourced in shell");
    }
}

//! Path handler — stages directories for addition to $PATH via dodot-init.sh.

use std::path::Path;

use crate::datastore::DataStore;
use crate::fs::Fs;
use crate::handlers::{ExecutionPhase, Handler, HandlerConfig, HandlerStatus, HANDLER_PATH};
use crate::operations::HandlerIntent;
use crate::paths::Pather;
use crate::rules::RuleMatch;
use crate::Result;

pub struct PathHandler;

impl Handler for PathHandler {
    fn name(&self) -> &str {
        HANDLER_PATH
    }

    fn phase(&self) -> ExecutionPhase {
        ExecutionPhase::PathExport
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
            .filter(|m| m.is_dir)
            .map(|m| HandlerIntent::Stage {
                pack: m.pack.clone(),
                handler: HANDLER_PATH.into(),
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
        let has_state = datastore.has_handler_state(pack, HANDLER_PATH)?;
        Ok(HandlerStatus {
            file: file.to_string_lossy().into_owned(),
            handler: HANDLER_PATH.into(),
            deployed: has_state,
            message: if has_state {
                "added to PATH".into()
            } else {
                "not in PATH".into()
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

    fn dir_match(pack: &str, rel: &str, abs: PathBuf) -> RuleMatch {
        RuleMatch {
            relative_path: rel.into(),
            absolute_path: abs,
            pack: pack.into(),
            handler: HANDLER_PATH.into(),
            is_dir: true,
            options: HashMap::new(),
            preprocessor_source: None,
            rendered_bytes: None,
        }
    }

    fn file_match(pack: &str, rel: &str, abs: PathBuf) -> RuleMatch {
        RuleMatch {
            relative_path: rel.into(),
            absolute_path: abs,
            pack: pack.into(),
            handler: HANDLER_PATH.into(),
            is_dir: false,
            options: HashMap::new(),
            preprocessor_source: None,
            rendered_bytes: None,
        }
    }

    #[test]
    fn name_and_phase_identity() {
        assert_eq!(PathHandler.name(), HANDLER_PATH);
        assert_eq!(PathHandler.phase(), ExecutionPhase::PathExport);
    }

    #[test]
    fn to_intents_stages_each_matched_directory() {
        let env = TempEnvironment::builder()
            .pack("dev")
            .file("bin/script.sh", "echo hi")
            .done()
            .build();
        let bin_dir = env.dotfiles_root.join("dev/bin");

        let intents = PathHandler
            .to_intents(
                &[dir_match("dev", "bin", bin_dir.clone())],
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
                assert_eq!(handler, HANDLER_PATH);
                assert_eq!(source, &bin_dir);
            }
            other => panic!("expected Stage intent, got {other:?}"),
        }
    }

    #[test]
    fn to_intents_drops_non_directory_matches() {
        // The path handler should not stage a regular file even if a
        // rule (mis-)matched one — only directories get added to $PATH.
        let env = TempEnvironment::builder()
            .pack("dev")
            .file("bin/script.sh", "echo hi")
            .done()
            .build();

        let matches = vec![
            file_match(
                "dev",
                "bin/script.sh",
                env.dotfiles_root.join("dev/bin/script.sh"),
            ),
            dir_match("dev", "bin", env.dotfiles_root.join("dev/bin")),
        ];

        let intents = PathHandler
            .to_intents(
                &matches,
                &HandlerConfig::default(),
                env.paths.as_ref(),
                env.fs.as_ref(),
            )
            .unwrap();

        assert_eq!(intents.len(), 1, "only the directory match should stage");
        assert_eq!(intents[0].handler(), HANDLER_PATH);
    }

    #[test]
    fn to_intents_empty_in_empty_out() {
        let env = TempEnvironment::builder().build();
        let intents = PathHandler
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

        let status = PathHandler
            .check_status(Path::new("bin"), "dev", &ds)
            .unwrap();

        assert_eq!(status.handler, HANDLER_PATH);
        assert_eq!(status.file, "bin");
        assert!(!status.deployed);
        assert_eq!(status.message, "not in PATH");
    }

    #[test]
    fn check_status_deployed_when_state_present() {
        let env = TempEnvironment::builder()
            .pack("dev")
            .file("bin/script.sh", "echo hi")
            .done()
            .build();
        let ds = FilesystemDataStore::new(
            env.fs.clone(),
            env.paths.clone(),
            Arc::new(NoopCommandRunner),
        );

        // Seed handler state by linking the bin dir as the path handler
        // would once executed.
        let bin_dir = env.dotfiles_root.join("dev/bin");
        ds.create_data_link("dev", HANDLER_PATH, &bin_dir).unwrap();

        let status = PathHandler
            .check_status(Path::new("bin"), "dev", &ds)
            .unwrap();

        assert!(status.deployed);
        assert_eq!(status.message, "added to PATH");
    }
}

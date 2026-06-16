//! Filter handlers — claim files that should not be processed.
//!
//! Two handlers share the [`ExecutionPhase::Filter`] slot:
//!
//! - [`IgnoreHandler`] — match wins, file is dropped from the pipeline,
//!   nothing surfaces in `dodot status`. Mirrors `.gitignore`'s mental
//!   model: "I don't want to see this."
//! - [`SkipHandler`] — match wins, no executable intent is produced,
//!   but the file is listed in `dodot status` with a `skipped` label.
//!   Mirrors a test marked skipped: "I saw it and chose not to act."
//!
//! Both produce zero [`HandlerIntent`]s. The difference is purely a
//! display contract enforced by the status renderer, which inspects the
//! handler name on each [`RuleMatch`].

use std::path::Path;

use crate::datastore::DataStore;
use crate::fs::Fs;
use crate::handlers::{
    ExecutionPhase, Handler, HandlerConfig, HandlerStatus, HANDLER_IGNORE, HANDLER_SKIP,
};
use crate::operations::HandlerIntent;
use crate::paths::Pather;
use crate::rules::RuleMatch;
use crate::Result;

pub struct IgnoreHandler;

impl Handler for IgnoreHandler {
    fn name(&self) -> &str {
        HANDLER_IGNORE
    }

    fn phase(&self) -> ExecutionPhase {
        ExecutionPhase::Filter
    }

    fn to_intents(
        &self,
        _matches: &[RuleMatch],
        _config: &HandlerConfig,
        _paths: &dyn Pather,
        _fs: &dyn Fs,
    ) -> Result<Vec<HandlerIntent>> {
        Ok(Vec::new())
    }

    fn check_status(
        &self,
        file: &Path,
        _pack: &str,
        _datastore: &dyn DataStore,
    ) -> Result<HandlerStatus> {
        // Status is computed directly from rule matches by the renderer
        // for filter handlers; this is here to satisfy the trait.
        Ok(HandlerStatus {
            file: file.to_string_lossy().into_owned(),
            handler: HANDLER_IGNORE.into(),
            deployed: false,
            message: String::new(),
        })
    }
}

pub struct SkipHandler;

impl Handler for SkipHandler {
    fn name(&self) -> &str {
        HANDLER_SKIP
    }

    fn phase(&self) -> ExecutionPhase {
        ExecutionPhase::Filter
    }

    fn to_intents(
        &self,
        _matches: &[RuleMatch],
        _config: &HandlerConfig,
        _paths: &dyn Pather,
        _fs: &dyn Fs,
    ) -> Result<Vec<HandlerIntent>> {
        Ok(Vec::new())
    }

    fn check_status(
        &self,
        file: &Path,
        _pack: &str,
        _datastore: &dyn DataStore,
    ) -> Result<HandlerStatus> {
        Ok(HandlerStatus {
            file: file.to_string_lossy().into_owned(),
            handler: HANDLER_SKIP.into(),
            deployed: false,
            message: String::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datastore::{FilesystemDataStore, NoopCommandRunner};
    use crate::handlers::HandlerConfig;
    use crate::rules::RuleMatch;
    use crate::testing::TempEnvironment;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn match_with(handler: &str, is_dir: bool) -> RuleMatch {
        RuleMatch {
            relative_path: "anything".into(),
            absolute_path: "/dev/null/anything".into(),
            pack: "p".into(),
            handler: handler.into(),
            is_dir,
            options: HashMap::new(),
            preprocessor_source: None,
            rendered_bytes: None,
        }
    }

    #[test]
    fn ignore_handler_identity() {
        assert_eq!(IgnoreHandler.name(), HANDLER_IGNORE);
        assert_eq!(IgnoreHandler.phase(), ExecutionPhase::Filter);
    }

    #[test]
    fn skip_handler_identity() {
        assert_eq!(SkipHandler.name(), HANDLER_SKIP);
        assert_eq!(SkipHandler.phase(), ExecutionPhase::Filter);
    }

    #[test]
    fn ignore_handler_never_emits_intents() {
        // Even when given a wall of matches of every shape, ignore must
        // produce nothing — that's its whole contract.
        let env = TempEnvironment::builder().build();
        let matches: Vec<RuleMatch> = vec![
            match_with(HANDLER_IGNORE, false),
            match_with(HANDLER_IGNORE, true),
            match_with(HANDLER_IGNORE, false),
        ];

        let intents = IgnoreHandler
            .to_intents(
                &matches,
                &HandlerConfig::default(),
                env.paths.as_ref(),
                env.fs.as_ref(),
            )
            .unwrap();

        assert!(intents.is_empty());
    }

    #[test]
    fn skip_handler_never_emits_intents() {
        let env = TempEnvironment::builder().build();
        let matches: Vec<RuleMatch> = vec![
            match_with(HANDLER_SKIP, false),
            match_with(HANDLER_SKIP, true),
        ];

        let intents = SkipHandler
            .to_intents(
                &matches,
                &HandlerConfig::default(),
                env.paths.as_ref(),
                env.fs.as_ref(),
            )
            .unwrap();

        assert!(intents.is_empty());
    }

    #[test]
    fn ignore_check_status_is_invariant_and_silent() {
        // Filter handlers do not touch the datastore — the renderer
        // synthesizes the visible state — so check_status must return
        // a fixed shape regardless of any per-pack state.
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .build();
        let ds = FilesystemDataStore::new(
            env.fs.clone(),
            env.paths.clone(),
            Arc::new(NoopCommandRunner),
        );
        // Seed unrelated state to prove check_status ignores it.
        let source = env.dotfiles_root.join("vim/vimrc");
        ds.create_data_link("vim", "symlink", &source).unwrap();

        let status = IgnoreHandler
            .check_status(Path::new(".DS_Store"), "vim", &ds)
            .unwrap();

        assert_eq!(status.handler, HANDLER_IGNORE);
        assert_eq!(status.file, ".DS_Store");
        assert!(!status.deployed);
        assert!(status.message.is_empty());
    }

    #[test]
    fn skip_check_status_is_invariant_and_silent() {
        let env = TempEnvironment::builder().build();
        let ds = FilesystemDataStore::new(
            env.fs.clone(),
            env.paths.clone(),
            Arc::new(NoopCommandRunner),
        );

        let status = SkipHandler
            .check_status(Path::new("README.md"), "any", &ds)
            .unwrap();

        assert_eq!(status.handler, HANDLER_SKIP);
        assert_eq!(status.file, "README.md");
        assert!(!status.deployed);
        assert!(status.message.is_empty());
    }
}

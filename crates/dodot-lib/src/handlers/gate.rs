//! `gate` filter handler — claims files dropped by a failed gate.
//!
//! Gates are evaluated at scan time (see [`crate::gates`]). When a
//! file's gate predicate evaluates false, the scanner emits a
//! [`RuleMatch`] with `handler = "gate"` to keep the file visible in
//! `dodot status` ("gated out") while ensuring no deploying handler
//! claims it. The handler itself produces no executable intent — same
//! shape as [`IgnoreHandler`](super::filter::IgnoreHandler) and
//! [`SkipHandler`](super::filter::SkipHandler).

use std::path::Path;

use crate::datastore::DataStore;
use crate::fs::Fs;
use crate::handlers::{ExecutionPhase, Handler, HandlerConfig, HandlerStatus, HANDLER_GATE};
use crate::operations::HandlerIntent;
use crate::paths::Pather;
use crate::rules::RuleMatch;
use crate::Result;

pub struct GateHandler;

impl Handler for GateHandler {
    fn name(&self) -> &str {
        HANDLER_GATE
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
        // Gate status is rendered directly from the rule match by the
        // status renderer (which inspects the match's options for the
        // label/dimensions). This impl exists to satisfy the trait.
        Ok(HandlerStatus {
            file: file.to_string_lossy().into_owned(),
            handler: HANDLER_GATE.into(),
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

    fn gated_match(pack: &str, rel: &str) -> RuleMatch {
        RuleMatch {
            relative_path: rel.into(),
            absolute_path: format!("/dotfiles/{pack}/{rel}").into(),
            pack: pack.into(),
            handler: HANDLER_GATE.into(),
            is_dir: true,
            options: HashMap::new(),
            preprocessor_source: None,
            rendered_bytes: None,
        }
    }

    #[test]
    fn name_and_phase_identity() {
        assert_eq!(GateHandler.name(), HANDLER_GATE);
        assert_eq!(GateHandler.phase(), ExecutionPhase::Filter);
    }

    #[test]
    fn to_intents_never_emits() {
        // Gate-failed entries must never produce executable intent —
        // they exist only so the status renderer can show "gated out".
        let env = TempEnvironment::builder().build();
        let matches = vec![gated_match("vim", "_linux"), gated_match("vim", "_macos")];

        let intents = GateHandler
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
    fn to_intents_empty_in_empty_out() {
        let env = TempEnvironment::builder().build();
        let intents = GateHandler
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
    fn check_status_is_invariant_and_silent() {
        // Gate status is rendered from the rule match by the status
        // renderer, so this trait impl must return a fixed shape and
        // never consult the datastore — verify by seeding unrelated
        // state and confirming `deployed` stays false.
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
        // Seed state under a *different* handler — the property is that
        // GateHandler::check_status never consults the datastore, so any
        // existing state (related or not) must not flip `deployed`.
        let source = env.dotfiles_root.join("vim/vimrc");
        ds.create_data_link("vim", "symlink", &source).unwrap();

        let status = GateHandler
            .check_status(Path::new("_linux"), "vim", &ds)
            .unwrap();

        assert_eq!(status.handler, HANDLER_GATE);
        assert_eq!(status.file, "_linux");
        assert!(!status.deployed);
        assert!(status.message.is_empty());
    }
}

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

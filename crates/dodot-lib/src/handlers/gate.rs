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

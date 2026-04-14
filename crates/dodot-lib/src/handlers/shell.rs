//! Shell handler — stages shell scripts for sourcing via dodot-init.sh.

use std::path::Path;

use crate::datastore::DataStore;
use crate::handlers::{Handler, HandlerCategory, HandlerConfig, HandlerStatus, HANDLER_SHELL};
use crate::operations::HandlerIntent;
use crate::paths::Pather;
use crate::rules::RuleMatch;
use crate::Result;

pub struct ShellHandler;

impl Handler for ShellHandler {
    fn name(&self) -> &str {
        HANDLER_SHELL
    }

    fn category(&self) -> HandlerCategory {
        HandlerCategory::Configuration
    }

    fn to_intents(
        &self,
        matches: &[RuleMatch],
        _config: &HandlerConfig,
        _paths: &dyn Pather,
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

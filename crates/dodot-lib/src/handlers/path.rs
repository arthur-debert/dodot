//! Path handler — stages directories for addition to $PATH via dodot-init.sh.

use std::path::Path;

use crate::datastore::DataStore;
use crate::handlers::{Handler, HandlerCategory, HandlerConfig, HandlerStatus, HANDLER_PATH};
use crate::operations::HandlerIntent;
use crate::paths::Pather;
use crate::rules::RuleMatch;
use crate::Result;

pub struct PathHandler;

impl Handler for PathHandler {
    fn name(&self) -> &str {
        HANDLER_PATH
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

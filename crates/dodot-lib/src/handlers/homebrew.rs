//! Homebrew handler — runs `brew bundle` with sentinel tracking.

use std::path::Path;

use sha2::{Digest, Sha256};

use crate::datastore::DataStore;
use crate::fs::Fs;
use crate::handlers::{Handler, HandlerCategory, HandlerConfig, HandlerStatus, HANDLER_HOMEBREW};
use crate::operations::HandlerIntent;
use crate::paths::Pather;
use crate::rules::RuleMatch;
use crate::Result;

pub struct HomebrewHandler<'a> {
    fs: &'a dyn Fs,
}

impl<'a> HomebrewHandler<'a> {
    pub fn new(fs: &'a dyn Fs) -> Self {
        Self { fs }
    }
}

impl Handler for HomebrewHandler<'_> {
    fn name(&self) -> &str {
        HANDLER_HOMEBREW
    }

    fn category(&self) -> HandlerCategory {
        HandlerCategory::CodeExecution
    }

    fn to_intents(
        &self,
        matches: &[RuleMatch],
        _config: &HandlerConfig,
        _paths: &dyn Pather,
    ) -> Result<Vec<HandlerIntent>> {
        let mut intents = Vec::new();

        for m in matches {
            if m.is_dir {
                continue;
            }

            let checksum = brewfile_checksum(self.fs, &m.absolute_path)?;
            let filename = m
                .relative_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            let sentinel = format!("{filename}-{checksum}");

            intents.push(HandlerIntent::Run {
                pack: m.pack.clone(),
                handler: HANDLER_HOMEBREW.into(),
                command: format!("brew bundle --file='{}'", m.absolute_path.display()),
                sentinel,
            });
        }

        Ok(intents)
    }

    fn check_status(
        &self,
        file: &Path,
        pack: &str,
        datastore: &dyn DataStore,
    ) -> Result<HandlerStatus> {
        let checksum = brewfile_checksum(self.fs, file)?;
        let filename = file.file_name().unwrap_or_default().to_string_lossy();
        let sentinel = format!("{filename}-{checksum}");
        let has_sentinel = datastore.has_sentinel(pack, HANDLER_HOMEBREW, &sentinel)?;

        Ok(HandlerStatus {
            file: file.to_string_lossy().into_owned(),
            handler: HANDLER_HOMEBREW.into(),
            deployed: has_sentinel,
            message: if has_sentinel {
                "brew packages installed".into()
            } else {
                "brew packages not installed".into()
            },
        })
    }
}

fn brewfile_checksum(fs: &dyn Fs, path: &Path) -> Result<String> {
    let contents = fs.read_file(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&contents);
    let hash = hasher.finalize();
    Ok(hash[..8].iter().map(|b| format!("{b:02x}")).collect())
}

//! Install handler — runs setup scripts with checksum-based sentinel tracking.

use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::datastore::DataStore;
use crate::fs::Fs;
use crate::handlers::{Handler, HandlerCategory, HandlerConfig, HandlerStatus, HANDLER_INSTALL};
use crate::operations::HandlerIntent;
use crate::paths::Pather;
use crate::rules::RuleMatch;
use crate::Result;

pub struct InstallHandler<'a> {
    fs: &'a dyn Fs,
}

impl<'a> InstallHandler<'a> {
    pub fn new(fs: &'a dyn Fs) -> Self {
        Self { fs }
    }
}

impl Handler for InstallHandler<'_> {
    fn name(&self) -> &str {
        HANDLER_INSTALL
    }

    fn category(&self) -> HandlerCategory {
        HandlerCategory::CodeExecution
    }

    fn to_intents(
        &self,
        matches: &[RuleMatch],
        _config: &HandlerConfig,
        _paths: &dyn Pather,
        _fs: &dyn Fs,
    ) -> Result<Vec<HandlerIntent>> {
        let mut intents = Vec::new();

        for m in matches {
            if m.is_dir {
                continue;
            }

            let checksum = file_checksum(self.fs, &m.absolute_path)?;
            let filename = m
                .relative_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            let sentinel = format!("{filename}-{checksum}");

            intents.push(HandlerIntent::Run {
                pack: m.pack.clone(),
                handler: HANDLER_INSTALL.into(),
                executable: "bash".into(),
                arguments: vec!["--".into(), m.absolute_path.to_string_lossy().into_owned()],
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
        let checksum = file_checksum(self.fs, file)?;
        let filename = file.file_name().unwrap_or_default().to_string_lossy();
        let sentinel = format!("{filename}-{checksum}");
        let has_sentinel = datastore.has_sentinel(pack, HANDLER_INSTALL, &sentinel)?;

        Ok(HandlerStatus {
            file: file.to_string_lossy().into_owned(),
            handler: HANDLER_INSTALL.into(),
            deployed: has_sentinel,
            message: if has_sentinel {
                "installed".into()
            } else {
                "never run".into()
            },
        })
    }
}

/// Compute a short SHA-256 hex digest of a file's contents.
fn file_checksum(fs: &dyn Fs, path: &Path) -> Result<String> {
    let mut reader = fs.open_read(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf).map_err(|e| crate::DodotError::Fs {
            path: path.to_path_buf(),
            source: e,
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let hash = hasher.finalize();
    // Use first 8 bytes (16 hex chars) for a short but unique sentinel
    Ok(hex::encode(&hash[..8]))
}

/// Minimal hex encoding (avoids pulling in the `hex` crate).
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempEnvironment;

    #[test]
    fn checksum_is_deterministic() {
        let env = TempEnvironment::builder()
            .pack("test")
            .file("install.sh", "#!/bin/sh\necho hello")
            .done()
            .build();

        let path = env.dotfiles_root.join("test/install.sh");
        let c1 = file_checksum(env.fs.as_ref(), &path).unwrap();
        let c2 = file_checksum(env.fs.as_ref(), &path).unwrap();
        assert_eq!(c1, c2);
        assert_eq!(c1.len(), 16); // 8 bytes = 16 hex chars
    }

    #[test]
    fn checksum_changes_with_content() {
        let env = TempEnvironment::builder()
            .pack("test")
            .file("a.sh", "version 1")
            .file("b.sh", "version 2")
            .done()
            .build();

        let ca = file_checksum(env.fs.as_ref(), &env.dotfiles_root.join("test/a.sh")).unwrap();
        let cb = file_checksum(env.fs.as_ref(), &env.dotfiles_root.join("test/b.sh")).unwrap();
        assert_ne!(ca, cb);
    }

    #[test]
    fn to_intents_produces_run_with_sentinel() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("install.sh", "#!/bin/sh\nsetup")
            .done()
            .build();

        let handler = InstallHandler::new(env.fs.as_ref());
        let matches = vec![crate::rules::RuleMatch {
            relative_path: "install.sh".into(),
            absolute_path: env.dotfiles_root.join("vim/install.sh"),
            pack: "vim".into(),
            handler: "install".into(),
            is_dir: false,
            options: std::collections::HashMap::new(),
            preprocessor_source: None,
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
                env.fs.as_ref(),
            )
            .unwrap();

        assert_eq!(intents.len(), 1);
        match &intents[0] {
            HandlerIntent::Run {
                executable,
                arguments,
                sentinel,
                ..
            } => {
                assert_eq!(executable, "bash");
                assert_eq!(arguments[0], "--");
                assert!(arguments[1].contains("install.sh"));
                assert!(sentinel.starts_with("install.sh-"));
                assert_eq!(sentinel.len(), "install.sh-".len() + 16);
            }
            other => panic!("expected Run, got {other:?}"),
        }
    }
}

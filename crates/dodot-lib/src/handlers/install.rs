//! Install handler — runs setup scripts with checksum-based sentinel tracking.
//!
//! # Interpreter selection
//!
//! The interpreter is chosen from the script's file extension rather than
//! from the user's login shell. This keeps script execution predictable:
//! a script runs in its own subprocess with a fresh environment, so the
//! user's interactive shell (aliases, functions, options) is irrelevant
//! to how the script behaves — only the interpreter is.
//!
//! - `.sh`, `.bash`, or unknown extension → `bash`
//! - `.zsh` → `zsh`
//!
//! The extension is the contract the pack author declares. A script named
//! `install.zsh` announces that it uses zsh-specific syntax; invoking it
//! with bash would be incorrect. A script named `install.sh` announces
//! portability and should work anywhere `bash` is available.

use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::datastore::DataStore;
use crate::fs::Fs;
use crate::handlers::{ExecutionPhase, Handler, HandlerConfig, HandlerStatus, HANDLER_INSTALL};
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

    fn phase(&self) -> ExecutionPhase {
        ExecutionPhase::Setup
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
                executable: interpreter_for(&m.absolute_path).into(),
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

/// Pick the interpreter for an install script based on its extension.
///
/// Module-level docs explain why extension — not the user's login shell —
/// is the right signal.
fn interpreter_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("zsh") => "zsh",
        _ => "bash",
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

    #[test]
    fn interpreter_for_selects_by_extension() {
        assert_eq!(interpreter_for(Path::new("install.sh")), "bash");
        assert_eq!(interpreter_for(Path::new("install.bash")), "bash");
        assert_eq!(interpreter_for(Path::new("install.zsh")), "zsh");
        // Unknown / missing extension falls back to bash.
        assert_eq!(interpreter_for(Path::new("install")), "bash");
        assert_eq!(interpreter_for(Path::new("install.ksh")), "bash");
        // Path components don't interfere with extension lookup.
        assert_eq!(interpreter_for(Path::new("/a/b/install.zsh")), "zsh");
    }

    #[test]
    fn to_intents_picks_interpreter_per_script() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("install.sh", "echo sh")
            .file("install.bash", "echo bash")
            .file("install.zsh", "echo zsh")
            .done()
            .build();

        let handler = InstallHandler::new(env.fs.as_ref());
        let make_match = |name: &str| crate::rules::RuleMatch {
            relative_path: name.into(),
            absolute_path: env.dotfiles_root.join(format!("vim/{name}")),
            pack: "vim".into(),
            handler: "install".into(),
            is_dir: false,
            options: std::collections::HashMap::new(),
            preprocessor_source: None,
        };
        let matches = vec![
            make_match("install.sh"),
            make_match("install.bash"),
            make_match("install.zsh"),
        ];

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

        let chosen: Vec<(String, String)> = intents
            .iter()
            .map(|i| match i {
                HandlerIntent::Run {
                    executable,
                    arguments,
                    ..
                } => (
                    executable.clone(),
                    arguments
                        .last()
                        .cloned()
                        .and_then(|p| {
                            std::path::Path::new(&p)
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                        })
                        .unwrap_or_default(),
                ),
                other => panic!("expected Run, got {other:?}"),
            })
            .collect();

        assert!(chosen.contains(&("bash".into(), "install.sh".into())));
        assert!(chosen.contains(&("bash".into(), "install.bash".into())));
        assert!(chosen.contains(&("zsh".into(), "install.zsh".into())));
    }
}

//! Install handler — runs setup scripts with checksum-based sentinel
//! tracking, via the shared [`crate::handlers::run_once`] machinery.
//!
//! The bulk of the behavior (checksum, sentinel, intent emission,
//! status lookup) lives in [`crate::handlers::run_once::RunOnceHandler`].
//! This module supplies the [`InstallCommand`] specialization, which
//! tells the shared handler how to invoke an `install.sh` (or `.bash`
//! / `.zsh`) script.
//!
//! # Interpreter selection
//!
//! The interpreter is chosen from the script's file extension rather
//! than from the user's login shell. This keeps script execution
//! predictable: a script runs in its own subprocess with a fresh
//! environment, so the user's interactive shell (aliases, functions,
//! options) is irrelevant to how the script behaves — only the
//! interpreter is.
//!
//! - `.sh`, `.bash`, or unknown extension → `bash`
//! - `.zsh` → `zsh`
//!
//! The extension is the contract the pack author declares. A script
//! named `install.zsh` announces that it uses zsh-specific syntax;
//! invoking it with bash would be incorrect. A script named
//! `install.sh` announces portability and should work anywhere `bash`
//! is available.

use std::path::Path;

use crate::handlers::run_once::RunOnceCommand;
use crate::handlers::{ExecutionPhase, HANDLER_INSTALL};

/// [`RunOnceCommand`] for the `install` handler.
///
/// Picks the interpreter from the script's extension and invokes it
/// as `<interpreter> -- <abs path>` (the `--` end-of-flags separator
/// guards against scripts whose names start with a dash).
pub struct InstallCommand;

impl RunOnceCommand for InstallCommand {
    fn handler_name(&self) -> &str {
        HANDLER_INSTALL
    }

    fn phase(&self) -> ExecutionPhase {
        ExecutionPhase::Setup
    }

    fn command_for(&self, path: &Path) -> (String, Vec<String>) {
        (
            interpreter_for(path).to_string(),
            vec!["--".into(), path.to_string_lossy().into_owned()],
        )
    }

    fn status_deployed(&self) -> &str {
        "installed"
    }

    fn status_pending(&self) -> &str {
        "never run"
    }
}

/// Pick the interpreter for an install script based on its extension.
///
/// Module-level docs explain why extension — not the user's login
/// shell — is the right signal.
fn interpreter_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("zsh") => "zsh",
        _ => "bash",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::Fs;
    use crate::handlers::run_once::RunOnceHandler;
    use crate::handlers::{Handler, HandlerConfig};
    use crate::operations::HandlerIntent;
    use crate::rules::RuleMatch;
    use crate::testing::TempEnvironment;
    use std::collections::HashMap;

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
    fn install_command_identity() {
        assert_eq!(InstallCommand.handler_name(), HANDLER_INSTALL);
        assert_eq!(InstallCommand.phase(), ExecutionPhase::Setup);
        assert_eq!(InstallCommand.status_deployed(), "installed");
        assert_eq!(InstallCommand.status_pending(), "never run");
    }

    #[test]
    fn install_command_picks_interpreter_per_script_via_handler() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("install.sh", "echo sh")
            .file("install.bash", "echo bash")
            .file("install.zsh", "echo zsh")
            .done()
            .build();

        let handler = RunOnceHandler::new(env.fs.as_ref(), InstallCommand);
        let make_match = |name: &str| RuleMatch {
            relative_path: name.into(),
            absolute_path: env.dotfiles_root.join(format!("vim/{name}")),
            pack: "vim".into(),
            handler: "install".into(),
            is_dir: false,
            options: HashMap::new(),
            preprocessor_source: None,
            rendered_bytes: None,
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
                env.fs.as_ref() as &dyn Fs,
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
                            Path::new(&p)
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

    #[test]
    fn install_command_emits_run_intent_with_expected_shape() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("install.sh", "#!/bin/sh\nsetup")
            .done()
            .build();

        let handler = RunOnceHandler::new(env.fs.as_ref(), InstallCommand);
        let matches = vec![RuleMatch {
            relative_path: "install.sh".into(),
            absolute_path: env.dotfiles_root.join("vim/install.sh"),
            pack: "vim".into(),
            handler: "install".into(),
            is_dir: false,
            options: HashMap::new(),
            preprocessor_source: None,
            rendered_bytes: None,
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
                env.fs.as_ref() as &dyn Fs,
            )
            .unwrap();

        assert_eq!(intents.len(), 1);
        match &intents[0] {
            HandlerIntent::Run {
                pack,
                handler: h,
                executable,
                arguments,
                sentinel,
            } => {
                assert_eq!(pack, "vim");
                assert_eq!(h, HANDLER_INSTALL);
                assert_eq!(executable, "bash");
                assert_eq!(arguments[0], "--");
                assert!(arguments[1].ends_with("install.sh"));
                assert!(sentinel.starts_with("install.sh-"));
                assert_eq!(sentinel.len(), "install.sh-".len() + 16);
            }
            other => panic!("expected Run, got {other:?}"),
        }
    }
}

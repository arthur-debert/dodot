//! Externals handler — declarative remote-resource deployment.
//!
//! The trigger file is `externals.toml` at the pack root. Each section
//! declares one external resource (currently `type = "file"`; the
//! git-repo and archive variants land in later PRs). The handler parses
//! the file and emits one [`HandlerIntent::Fetch`] per entry.
//!
//! The handler itself is read-only — fetching, hashing, and symlink
//! creation all happen in `crate::execution::fetch`. This mirrors the
//! existing handler/executor split: planning is idempotent and safe to
//! re-run; I/O lives in the executor.

use std::path::{Path, PathBuf};

use crate::datastore::DataStore;
use crate::fs::Fs;
use crate::handlers::{
    ExecutionPhase, Handler, HandlerConfig, HandlerStatus, MatchMode, HANDLER_EXTERNAL,
};
use crate::operations::HandlerIntent;
use crate::paths::Pather;
use crate::rules::RuleMatch;
use crate::Result;

/// Filename the handler matches against. Exposed so config / rules can
/// be derived from one source of truth.
pub const EXTERNALS_TOML: &str = "externals.toml";

pub struct ExternalsHandler;

impl Handler for ExternalsHandler {
    fn name(&self) -> &str {
        HANDLER_EXTERNAL
    }

    fn phase(&self) -> ExecutionPhase {
        ExecutionPhase::External
    }

    fn match_mode(&self) -> MatchMode {
        MatchMode::Precise
    }

    fn to_intents(
        &self,
        matches: &[RuleMatch],
        _config: &HandlerConfig,
        paths: &dyn Pather,
        fs: &dyn Fs,
    ) -> Result<Vec<HandlerIntent>> {
        let mut intents = Vec::new();
        for m in matches {
            if m.is_dir {
                continue;
            }
            let bytes = match m.rendered_bytes.as_deref() {
                Some(b) => b.to_vec(),
                None => match fs.read_file(&m.absolute_path) {
                    Ok(b) => b,
                    Err(_) => {
                        // Same posture as install handler's first-time
                        // passive placeholder: skip silently and let
                        // status surface "pending" through the symlink
                        // chain.
                        tracing::debug!(
                            pack = %m.pack,
                            file = %m.absolute_path.display(),
                            "externals.toml unreadable; skipping intent planning"
                        );
                        continue;
                    }
                },
            };

            let parsed = crate::external::parse_externals_toml(&bytes)?;
            for (name, entry) in parsed.entries {
                let user_path = resolve_target(&entry.target, paths.home_dir());
                intents.push(HandlerIntent::Fetch {
                    pack: m.pack.clone(),
                    handler: HANDLER_EXTERNAL.into(),
                    name,
                    spec: entry.spec,
                    user_path,
                });
            }
        }
        Ok(intents)
    }

    fn check_status(
        &self,
        file: &Path,
        pack: &str,
        datastore: &dyn DataStore,
    ) -> Result<HandlerStatus> {
        // Coarse-grained for v1: report deployed if *any* sentinel
        // exists for the pack/external pair. Per-entry status arrives
        // when we add the `dodot status` external-aware rendering in a
        // later PR.
        let deployed = datastore.has_handler_state(pack, HANDLER_EXTERNAL)?;
        Ok(HandlerStatus {
            file: file.to_string_lossy().into_owned(),
            handler: HANDLER_EXTERNAL.into(),
            deployed,
            message: if deployed {
                "externals deployed".into()
            } else {
                "externals not yet fetched".into()
            },
        })
    }
}

/// Expand a leading `~` or `~/` to the pather's home dir.
///
/// We don't go all the way to a full shell-style expansion (no
/// `~user/`, no env-var substitution) — those are explicit non-goals
/// for the declarative config surface. The target field is a path the
/// user writes once, not a shell snippet.
fn resolve_target(target: &str, home: &Path) -> PathBuf {
    if target == "~" {
        return home.to_path_buf();
    }
    if let Some(rest) = target.strip_prefix("~/") {
        return home.join(rest);
    }
    PathBuf::from(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::external::FetchSpec;
    use crate::testing::TempEnvironment;
    use std::collections::HashMap;

    fn make_match(env: &TempEnvironment, pack: &str) -> RuleMatch {
        let abs = env.dotfiles_root.join(pack).join(EXTERNALS_TOML);
        RuleMatch {
            relative_path: EXTERNALS_TOML.into(),
            absolute_path: abs,
            pack: pack.into(),
            handler: HANDLER_EXTERNAL.into(),
            is_dir: false,
            options: HashMap::new(),
            preprocessor_source: None,
            rendered_bytes: None,
        }
    }

    #[test]
    fn resolve_target_expands_tilde() {
        let home = Path::new("/home/alice");
        assert_eq!(
            resolve_target("~/.config/shared/aliases.sh", home),
            PathBuf::from("/home/alice/.config/shared/aliases.sh")
        );
        assert_eq!(resolve_target("~", home), PathBuf::from("/home/alice"));
        assert_eq!(
            resolve_target("/etc/shared", home),
            PathBuf::from("/etc/shared")
        );
    }

    #[test]
    fn to_intents_emits_one_fetch_per_entry() {
        let toml = r#"
            [aliases]
            type   = "file"
            url    = "https://example.com/aliases.sh"
            target = "~/.config/shared/aliases.sh"
            sha256 = "abc"

            [motd]
            type   = "file"
            url    = "https://example.com/motd"
            target = "~/.motd"
            sha256 = "def"
        "#;
        let env = TempEnvironment::builder()
            .pack("shared")
            .file(EXTERNALS_TOML, toml)
            .done()
            .build();
        let handler = ExternalsHandler;
        let pather = crate::paths::XdgPather::builder()
            .home(&env.home)
            .dotfiles_root(&env.dotfiles_root)
            .build()
            .unwrap();

        let intents = handler
            .to_intents(
                &[make_match(&env, "shared")],
                &HandlerConfig::default(),
                &pather,
                env.fs.as_ref(),
            )
            .unwrap();

        assert_eq!(intents.len(), 2);
        let names: Vec<&str> = intents
            .iter()
            .map(|i| match i {
                HandlerIntent::Fetch { name, .. } => name.as_str(),
                _ => unreachable!(),
            })
            .collect();
        // Alphabetical because parse_externals_toml uses BTreeMap.
        assert_eq!(names, vec!["aliases", "motd"]);

        match &intents[0] {
            HandlerIntent::Fetch {
                pack,
                user_path,
                spec,
                ..
            } => {
                assert_eq!(pack, "shared");
                assert_eq!(user_path, &env.home.join(".config/shared/aliases.sh"));
                assert!(matches!(spec, FetchSpec::File { .. }));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn to_intents_propagates_parse_errors() {
        let env = TempEnvironment::builder()
            .pack("shared")
            .file(EXTERNALS_TOML, "this = is :: broken")
            .done()
            .build();
        let handler = ExternalsHandler;
        let pather = crate::paths::XdgPather::builder()
            .home(&env.home)
            .dotfiles_root(&env.dotfiles_root)
            .build()
            .unwrap();

        let err = handler
            .to_intents(
                &[make_match(&env, "shared")],
                &HandlerConfig::default(),
                &pather,
                env.fs.as_ref(),
            )
            .unwrap_err();
        assert!(format!("{err}").contains("externals.toml parse error"));
    }

    #[test]
    fn missing_file_is_skipped_quietly() {
        let env = TempEnvironment::builder().build();
        let handler = ExternalsHandler;
        let pather = crate::paths::XdgPather::builder()
            .home(&env.home)
            .dotfiles_root(&env.dotfiles_root)
            .build()
            .unwrap();
        let intents = handler
            .to_intents(
                &[make_match(&env, "shared")],
                &HandlerConfig::default(),
                &pather,
                env.fs.as_ref(),
            )
            .unwrap();
        assert!(intents.is_empty());
    }

    #[test]
    fn directory_entries_are_skipped() {
        let env = TempEnvironment::builder().build();
        let handler = ExternalsHandler;
        let pather = crate::paths::XdgPather::builder()
            .home(&env.home)
            .dotfiles_root(&env.dotfiles_root)
            .build()
            .unwrap();
        let mut m = make_match(&env, "shared");
        m.is_dir = true;
        let intents = handler
            .to_intents(&[m], &HandlerConfig::default(), &pather, env.fs.as_ref())
            .unwrap();
        assert!(intents.is_empty());
    }
}

//! `dodot template install-filter` — register the dodot-template
//! clean filter in the dotfiles repo's `.git/config`.
//!
//! Mirrors the structure of [`crate::commands::git_filters`] (the
//! plist filter installer). The differences vs plist:
//!
//! 1. **Filter name**: `dodot-template` (vs `dodot-plist`).
//! 2. **Clean command**: `dodot template clean --path %f` (with the
//!    `%f` placeholder so git passes the path of the file being
//!    processed; we need it to look up the matching baseline).
//! 3. **No transforming smudge**: templates must NEVER be re-rendered
//!    at smudge time (would re-trigger secret-provider auth on every
//!    `git checkout`, `git stash pop`, etc — exactly the auth-fatigue
//!    scenario magic.lex rules out). We register `smudge = cat` so
//!    smudge is an explicit identity rather than implicit; makes the
//!    intent legible in `.git/config`.
//!
//! Per-clone, per-machine: the filter command lives in `.git/config`
//! which is not carried by the repo. The `.gitattributes` line that
//! BINDS files to the filter (`*.tmpl filter=dodot-template`) does
//! travel with the repo but is inert without the matching
//! `.git/config` entries.

use serde::Serialize;

use crate::commands::MessageResult;
use crate::packs::orchestration::ExecutionContext;
use crate::{DodotError, Result};

/// The `.git/config` snippet this command writes. Public so
/// `dodot template show-filter` (future) can print it for users
/// who want to install by hand.
pub fn config_block_text() -> String {
    [
        "[filter \"dodot-template\"]",
        "    clean    = dodot template clean --path %f",
        "    smudge   = cat",
        "    required = true",
    ]
    .join("\n")
}

/// The `.gitattributes` line that binds `*.tmpl` files to the filter.
/// Lives in the repo (committed alongside the templates) so every
/// clone gets the binding for free; the `.git/config` block makes
/// it active.
pub fn gitattributes_line() -> &'static str {
    "*.tmpl filter=dodot-template"
}

/// Result returned by `install_filter`. CLI exits 0 in all three
/// outcomes (every state is a success). Renders through the
/// `template-install-filter.jinja` template.
#[derive(Debug, Clone, Serialize)]
pub struct InstallFilterResult {
    pub message: String,
    pub details: Vec<String>,
    /// True iff `[filter "dodot-template"]` was already in
    /// `.git/config` — in which case we made no change. Surfaced
    /// separately from `message` so a JSON consumer can distinguish
    /// "installed now" from "already there".
    pub already_installed: bool,
}

/// Install the dodot-template clean filter into the dotfiles repo's
/// `.git/config`. Idempotent: re-running when already installed is
/// a no-op success.
pub fn install_filter(ctx: &ExecutionContext) -> Result<InstallFilterResult> {
    let root = ctx.paths.dotfiles_root().to_path_buf();
    let runner = ctx.command_runner.as_ref();

    if filter_is_installed(runner, &root)? {
        let mut details = Vec::new();
        append_gitattributes_hint(ctx, &mut details);
        return Ok(InstallFilterResult {
            message: "Template filter already installed in .git/config.".into(),
            details,
            already_installed: true,
        });
    }

    git_config_set(
        runner,
        &root,
        "filter.dodot-template.clean",
        "dodot template clean --path %f",
    )?;
    git_config_set(runner, &root, "filter.dodot-template.smudge", "cat")?;
    git_config_set(runner, &root, "filter.dodot-template.required", "true")?;

    let mut details = vec![format!(
        "Wrote [filter \"dodot-template\"] to {}/.git/config",
        root.display()
    )];
    append_gitattributes_hint(ctx, &mut details);
    Ok(InstallFilterResult {
        message: "Installed template clean filter.".into(),
        details,
        already_installed: false,
    })
}

/// Detect whether the filter is currently registered. Cheap (one
/// `git config --get`). Used by the post-`up` first-deploy prompt
/// to decide whether to offer installation.
pub fn is_installed(ctx: &ExecutionContext) -> Result<bool> {
    filter_is_installed(ctx.command_runner.as_ref(), ctx.paths.dotfiles_root())
}

/// Render the install snippet without writing anything. Companion
/// to `git-show-filters` for plist; callers (the CLI's `show`
/// branch in the post-`up` prompt) print the block so users can
/// inspect or install by hand.
pub fn show_filter(ctx: &ExecutionContext) -> Result<MessageResult> {
    let installed = is_installed(ctx)?;
    let block = config_block_text();
    let mut details: Vec<String> = block.lines().map(|s| format!("    {s}")).collect();
    details.push(String::new());
    details.push("Then add this line to .gitattributes (committed in the repo):".to_string());
    details.push(format!("    {}", gitattributes_line()));
    Ok(MessageResult {
        message: if installed {
            "Template filter is currently installed.".into()
        } else {
            "Template filter is NOT installed. To install, add to .git/config:".into()
        },
        details,
    })
}

fn append_gitattributes_hint(ctx: &ExecutionContext, details: &mut Vec<String>) {
    let attrs_path = ctx.paths.dotfiles_root().join(".gitattributes");
    let already_bound = ctx
        .fs
        .read_to_string(&attrs_path)
        .ok()
        .map(|s| s.lines().any(gitattributes_line_present))
        .unwrap_or(false);
    if !already_bound {
        details.push(String::new());
        details.push("Next: ensure your .gitattributes binds *.tmpl to this filter:".into());
        details.push(format!(
            "    echo '{}' >> .gitattributes",
            gitattributes_line()
        ));
        details.push("    git add .gitattributes && git commit -m 'enable template filter'".into());
    }
}

/// Match a `.gitattributes` line that binds `*.tmpl` to
/// `dodot-template`. Tolerant of whitespace and trailing comments.
fn gitattributes_line_present(line: &str) -> bool {
    let trimmed = line.split('#').next().unwrap_or("").trim();
    let mut parts = trimmed.split_ascii_whitespace();
    let pattern = parts.next();
    if pattern != Some("*.tmpl") {
        return false;
    }
    parts.any(|tok| tok == "filter=dodot-template")
}

fn filter_is_installed(
    runner: &dyn crate::datastore::CommandRunner,
    root: &std::path::Path,
) -> Result<bool> {
    match runner.run(
        "git",
        &[
            "-C".into(),
            root.display().to_string(),
            "config".into(),
            "--get".into(),
            "filter.dodot-template.clean".into(),
        ],
    ) {
        Ok(out) => Ok(out.exit_code == 0 && !out.stdout.trim().is_empty()),
        Err(DodotError::CommandFailed { exit_code: 1, .. }) => Ok(false),
        Err(DodotError::CommandFailed { stderr, .. })
            if stderr.contains("not a git repository") =>
        {
            Ok(false)
        }
        Err(e) => Err(e),
    }
}

fn git_config_set(
    runner: &dyn crate::datastore::CommandRunner,
    root: &std::path::Path,
    key: &str,
    value: &str,
) -> Result<()> {
    let out = runner.run(
        "git",
        &[
            "-C".into(),
            root.display().to_string(),
            "config".into(),
            key.into(),
            value.into(),
        ],
    )?;
    if out.exit_code != 0 {
        return Err(DodotError::CommandFailed {
            command: format!("git -C {} config {} {}", root.display(), key, value),
            exit_code: out.exit_code,
            stderr: out.stderr,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_block_carries_required_keys() {
        let block = config_block_text();
        assert!(block.contains("[filter \"dodot-template\"]"));
        assert!(block.contains("clean    = dodot template clean --path %f"));
        // Identity smudge is explicit, not implicit.
        assert!(block.contains("smudge   = cat"));
        // required=true so a missing dodot fails loudly rather than
        // silently storing/checking out the wrong representation.
        assert!(block.contains("required = true"));
    }

    #[test]
    fn gitattributes_line_recogniser_handles_whitespace_and_comments() {
        assert!(gitattributes_line_present("*.tmpl filter=dodot-template"));
        assert!(gitattributes_line_present(
            "  *.tmpl   filter=dodot-template  "
        ));
        assert!(gitattributes_line_present(
            "*.tmpl filter=dodot-template diff=template"
        ));
        assert!(gitattributes_line_present(
            "*.tmpl filter=dodot-template  # template filter"
        ));

        assert!(!gitattributes_line_present(""));
        assert!(!gitattributes_line_present("# commented out"));
        assert!(!gitattributes_line_present("*.tmpl filter=other"));
        assert!(!gitattributes_line_present("*.txt filter=dodot-template"));
        // Plist filter line must NOT match the template recogniser.
        assert!(!gitattributes_line_present("*.plist filter=dodot-plist"));
    }

    /// Test-only ExecutionContext that uses a `MockRunner` so we can
    /// observe the `git config` calls install_filter would issue.
    fn make_test_ctx(env: &crate::testing::TempEnvironment) -> ExecutionContext {
        use crate::config::ConfigManager;
        use crate::datastore::{CommandOutput, CommandRunner, FilesystemDataStore};
        use crate::fs::Fs;
        use crate::paths::Pather;
        use std::sync::{Arc, Mutex};

        // A mock runner that:
        //   - records every `run` call so tests can assert which
        //     `git config` keys were touched
        //   - emulates `git config --get`'s exit-1 for unset keys
        //     until set, then exit-0 with stdout == value
        struct MockRunner {
            store: Mutex<std::collections::HashMap<String, String>>,
            calls: Mutex<Vec<Vec<String>>>,
        }
        impl CommandRunner for MockRunner {
            fn run(&self, exe: &str, args: &[String]) -> Result<CommandOutput> {
                self.calls.lock().unwrap().push(
                    std::iter::once(exe.to_string())
                        .chain(args.iter().cloned())
                        .collect(),
                );
                if exe != "git" {
                    return Ok(CommandOutput {
                        exit_code: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    });
                }
                // args = ["-C", root, "config", ...]
                let rest: Vec<&str> = args.iter().skip(3).map(String::as_str).collect();
                if rest.first() == Some(&"--get") {
                    let key = rest.get(1).copied().unwrap_or("");
                    let store = self.store.lock().unwrap();
                    match store.get(key) {
                        Some(v) => Ok(CommandOutput {
                            exit_code: 0,
                            stdout: format!("{v}\n"),
                            stderr: String::new(),
                        }),
                        None => Err(DodotError::CommandFailed {
                            command: format!("git config --get {key}"),
                            exit_code: 1,
                            stderr: String::new(),
                        }),
                    }
                } else {
                    // Setting: ["config", "<key>", "<value>"]
                    let key = rest.first().copied().unwrap_or("");
                    let value = rest.get(1).copied().unwrap_or("");
                    self.store
                        .lock()
                        .unwrap()
                        .insert(key.to_string(), value.to_string());
                    Ok(CommandOutput {
                        exit_code: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    })
                }
            }
        }
        let runner = Arc::new(MockRunner {
            store: Mutex::new(std::collections::HashMap::new()),
            calls: Mutex::new(Vec::new()),
        });
        let runner_dyn: Arc<dyn CommandRunner> = runner.clone();
        let datastore = Arc::new(FilesystemDataStore::new(
            env.fs.clone(),
            env.paths.clone(),
            runner_dyn.clone(),
        ));
        let config_manager = Arc::new(ConfigManager::new(&env.dotfiles_root).unwrap());
        ExecutionContext {
            fs: env.fs.clone() as Arc<dyn Fs>,
            datastore,
            paths: env.paths.clone() as Arc<dyn Pather>,
            config_manager,
            syntax_checker: Arc::new(crate::shell::NoopSyntaxChecker),
            command_runner: runner_dyn,
            dry_run: false,
            no_provision: true,
            provision_rerun: false,
            force: false,
            view_mode: crate::commands::ViewMode::Full,
            group_mode: crate::commands::GroupMode::Name,
            verbose: false,
        }
    }

    #[test]
    fn install_filter_writes_three_config_keys_first_time() {
        // First install: `--get` reports unset → set clean / smudge /
        // required → return success with already_installed=false.
        let env = crate::testing::TempEnvironment::builder().build();
        let ctx = make_test_ctx(&env);
        let r = install_filter(&ctx).unwrap();
        assert!(!r.already_installed);
        assert!(r.message.starts_with("Installed"));
    }

    #[test]
    fn install_filter_is_idempotent() {
        let env = crate::testing::TempEnvironment::builder().build();
        let ctx = make_test_ctx(&env);
        install_filter(&ctx).unwrap();
        let second = install_filter(&ctx).unwrap();
        assert!(second.already_installed);
        assert!(second.message.contains("already installed"));
    }

    #[test]
    fn is_installed_reflects_state_correctly() {
        let env = crate::testing::TempEnvironment::builder().build();
        let ctx = make_test_ctx(&env);
        assert!(!is_installed(&ctx).unwrap());
        install_filter(&ctx).unwrap();
        assert!(is_installed(&ctx).unwrap());
    }

    #[test]
    fn show_filter_renders_block_and_attributes_line() {
        let env = crate::testing::TempEnvironment::builder().build();
        let ctx = make_test_ctx(&env);
        let r = show_filter(&ctx).unwrap();
        // Block lines + the gitattributes hint must both be present
        // in `details` so the user can copy them.
        let joined = r.details.join("\n");
        assert!(joined.contains("[filter \"dodot-template\"]"));
        assert!(joined.contains("clean    = dodot template clean --path %f"));
        assert!(joined.contains(gitattributes_line()));
    }
}

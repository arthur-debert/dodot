//! Git clean/smudge filter installation for plist support.
//!
//! Two operations:
//!
//! - [`install_filters`] writes the `[filter "dodot-plist"]` block to the
//!   dotfiles repo's `.git/config`. Per-clone, per-machine. Idempotent.
//! - [`show_filters`] prints the same config block without writing it,
//!   so the user can inspect or install by hand.
//!
//! See `docs/proposals/plists.lex` §5 for the architectural context.

use serde::Serialize;

use crate::commands::MessageResult;
use crate::packs::orchestration::ExecutionContext;
use crate::{DodotError, Result};

/// Reusable [`MessageResult`] forms for git-filter commands.
pub mod result {
    pub use crate::commands::MessageResult;
}

/// Render the `.git/config` snippet for the dodot-plist filter.
///
/// Public for `dodot git-show-filters` and for `dodot git-install-filters`
/// to include in the success message when the user picks the `show` path.
pub fn config_block_text() -> String {
    [
        "[filter \"dodot-plist\"]",
        "    clean  = dodot plist clean",
        "    smudge = dodot plist smudge",
        "    required = true",
    ]
    .join("\n")
}

/// Render the `.gitattributes` line that binds `*.plist` to the filter.
pub fn gitattributes_line() -> &'static str {
    "*.plist filter=dodot-plist"
}

/// Install the dodot-plist clean/smudge filter into the dotfiles repo's
/// `.git/config`. Idempotent: re-running when the filter is already
/// installed is a no-op success.
pub fn install_filters(ctx: &ExecutionContext) -> Result<MessageResult> {
    let root = ctx.paths.dotfiles_root().to_path_buf();
    let runner = ctx.command_runner.as_ref();

    if filter_is_installed(runner, &root)? {
        let mut details = Vec::new();
        append_gitattributes_hint(ctx, &mut details);
        return Ok(MessageResult {
            message: "Plist filters already installed in .git/config.".into(),
            details,
        });
    }

    git_config_set(
        runner,
        &root,
        "filter.dodot-plist.clean",
        "dodot plist clean",
    )?;
    git_config_set(
        runner,
        &root,
        "filter.dodot-plist.smudge",
        "dodot plist smudge",
    )?;
    git_config_set(runner, &root, "filter.dodot-plist.required", "true")?;

    let mut details = vec![format!(
        "Wrote [filter \"dodot-plist\"] to {}/.git/config",
        root.display()
    )];
    append_gitattributes_hint(ctx, &mut details);
    append_cfprefsd_hint(&mut details);
    Ok(MessageResult {
        message: "Installed plist clean/smudge filters.".into(),
        details,
    })
}

/// Print the `.git/config` and `.gitattributes` snippets without
/// writing anything. For users who want to install by hand or inspect
/// before agreeing.
pub fn show_filters(ctx: &ExecutionContext) -> Result<ShowFiltersResult> {
    let root = ctx.paths.dotfiles_root().to_path_buf();
    let runner = ctx.command_runner.as_ref();
    let installed = filter_is_installed(runner, &root)?;

    let attributes_present = ctx
        .fs
        .read_to_string(&root.join(".gitattributes"))
        .ok()
        .map(|s| s.lines().any(gitattributes_line_present))
        .unwrap_or(false);

    let block = config_block_text();
    let block_lines = block.lines().map(str::to_string).collect();
    Ok(ShowFiltersResult {
        config_block: block,
        config_block_lines: block_lines,
        gitattributes_line: gitattributes_line().to_string(),
        installed_in_git_config: installed,
        bound_in_gitattributes: attributes_present,
        repo_root: root.display().to_string(),
    })
}

/// Result for `dodot git-show-filters`. The CLI handler renders this
/// through the `git-filters` template; the `config_block_lines` field
/// is the line-broken form of `config_block` so the template can
/// indent each line uniformly without needing a `split` filter.
#[derive(Debug, Clone, Serialize)]
pub struct ShowFiltersResult {
    pub config_block: String,
    pub config_block_lines: Vec<String>,
    pub gitattributes_line: String,
    pub installed_in_git_config: bool,
    pub bound_in_gitattributes: bool,
    pub repo_root: String,
}

/// Quick check: is the dodot-plist clean filter currently registered in
/// the dotfiles repo's `.git/config`? Used by `dodot up` to decide
/// whether to prompt for installation, and by `dodot git-show-filters`
/// to annotate its output.
pub fn is_installed(ctx: &ExecutionContext) -> Result<bool> {
    let runner = ctx.command_runner.as_ref();
    let root = ctx.paths.dotfiles_root();
    filter_is_installed(runner, root)
}

/// Scan every active pack under the dotfiles root and return the
/// absolute paths of any `*.plist` files within. Used by `dodot up`
/// to decide whether the user should be offered the filter-install
/// prompt.
///
/// Pack selection goes through [`packs::discover_packs`] so it honours
/// the same conventions every other command does: `pack.ignore`
/// patterns from config, `.dodotignore` markers, valid pack-name
/// rules, and the `.config` exception. Pack-internal walking ignores
/// nested dot-directories (`.git`, etc.) so we don't recurse into
/// vendored repos that happen to live inside a pack.
///
/// Detection is "any `*.plist` in any active pack", not "tracked by
/// git". The looser check is intentional: an untracked plist in a pack
/// is almost certainly headed for a commit, and a false-positive
/// prompt is harmless. A stricter check would require shelling out to
/// `git ls-files` on every `up`.
pub fn detect_plist_files(ctx: &ExecutionContext) -> Result<Vec<std::path::PathBuf>> {
    let mut found = Vec::new();
    let root = ctx.paths.dotfiles_root();
    if !ctx.fs.is_dir(root) {
        return Ok(found);
    }
    let root_config = ctx.config_manager.root_config()?;
    let packs = crate::packs::discover_packs(ctx.fs.as_ref(), root, &root_config.pack.ignore)?;
    for pack in packs {
        scan_for_plists(ctx.fs.as_ref(), &pack.path, &mut found)?;
    }
    Ok(found)
}

fn scan_for_plists(
    fs: &dyn crate::fs::Fs,
    dir: &std::path::Path,
    found: &mut Vec<std::path::PathBuf>,
) -> Result<()> {
    let entries = match fs.read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()), // tolerate unreadable subdirs
    };
    for entry in entries {
        if entry.is_dir {
            // Skip nested .git or other dot-dirs to avoid scanning
            // the world.
            let name = entry
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if name.starts_with('.') {
                continue;
            }
            scan_for_plists(fs, &entry.path, found)?;
        } else if entry
            .path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.eq_ignore_ascii_case("plist"))
            .unwrap_or(false)
        {
            found.push(entry.path);
        }
    }
    Ok(())
}

// ── internals ──────────────────────────────────────────────────────────

/// macOS-only nudge appended to the `git-install-filters` success
/// message. Apps cache plist values via `cfprefsd`, so even a correct
/// `git pull` + `dodot up` won't be visible to a running app until
/// either the app restarts or `cfprefsd` is killed (it auto-respawns).
/// Surfacing the one-liner here avoids a "I deployed it but nothing
/// changed" support thread.
fn append_cfprefsd_hint(details: &mut Vec<String>) {
    if !cfg!(target_os = "macos") {
        return;
    }
    details.push(String::new());
    details.push("Note: macOS caches plist values in `cfprefsd`. After pulling".into());
    details.push("plist changes from another machine, run:".into());
    details.push("    killall cfprefsd".into());
    details.push("to make running apps re-read their preferences. (No data loss;".into());
    details.push("cfprefsd respawns immediately.)".into());
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
        details.push("Next: ensure your .gitattributes binds *.plist to this filter:".into());
        details.push(format!(
            "    echo '{}' >> .gitattributes",
            gitattributes_line()
        ));
        details.push("    git add .gitattributes && git commit -m 'enable plist filters'".into());
    }
}

/// Match a `.gitattributes` line that binds `*.plist` to the
/// `dodot-plist` filter. Tolerant of whitespace and comments.
fn gitattributes_line_present(line: &str) -> bool {
    let trimmed = line.split('#').next().unwrap_or("").trim();
    let mut parts = trimmed.split_ascii_whitespace();
    let pattern = parts.next();
    if pattern != Some("*.plist") {
        return false;
    }
    parts.any(|tok| tok == "filter=dodot-plist")
}

fn filter_is_installed(
    runner: &dyn crate::datastore::CommandRunner,
    root: &std::path::Path,
) -> Result<bool> {
    // `git config --get <key>` exits 1 when the key is not set; the
    // runner translates non-zero exits into `CommandFailed`. We treat
    // exit_code == 1 (and "not a git repo") as "not installed", and
    // surface other failures (git missing, perm errors) as errors.
    match runner.run(
        "git",
        &[
            "-C".into(),
            root.display().to_string(),
            "config".into(),
            "--get".into(),
            "filter.dodot-plist.clean".into(),
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
    fn config_block_has_required_key() {
        let block = config_block_text();
        assert!(block.contains("[filter \"dodot-plist\"]"));
        assert!(block.contains("clean  = dodot plist clean"));
        assert!(block.contains("smudge = dodot plist smudge"));
        assert!(block.contains("required = true"));
    }

    fn make_test_ctx(env: &crate::testing::TempEnvironment) -> ExecutionContext {
        use crate::config::ConfigManager;
        use crate::datastore::{CommandOutput, CommandRunner, FilesystemDataStore};
        use crate::fs::Fs;
        use crate::paths::Pather;
        use std::sync::Arc;

        struct NoopRunner;
        impl CommandRunner for NoopRunner {
            fn run(&self, _e: &str, _a: &[String]) -> Result<CommandOutput> {
                Ok(CommandOutput {
                    exit_code: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                })
            }
        }
        let runner: Arc<dyn CommandRunner> = Arc::new(NoopRunner);
        let datastore = Arc::new(FilesystemDataStore::new(
            env.fs.clone(),
            env.paths.clone(),
            runner.clone(),
        ));
        let config_manager = Arc::new(ConfigManager::new(&env.dotfiles_root).unwrap());
        ExecutionContext {
            fs: env.fs.clone() as Arc<dyn Fs>,
            datastore,
            paths: env.paths.clone() as Arc<dyn Pather>,
            config_manager,
            syntax_checker: Arc::new(crate::shell::NoopSyntaxChecker),
            command_runner: runner,
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
    fn detect_plist_files_finds_plists_in_packs() {
        use crate::testing::TempEnvironment;
        let env = TempEnvironment::builder()
            .pack("mac-defaults")
            .file("com.app.plist", "binary-or-xml")
            .file("README.md", "no plist")
            .done()
            .pack("nvim")
            .file("init.lua", "no plist")
            .done()
            .pack("system-prefs")
            .file("nested/com.other.plist", "deeper")
            .done()
            .build();

        let ctx = make_test_ctx(&env);
        let found = detect_plist_files(&ctx).expect("detect");
        assert_eq!(found.len(), 2, "expected 2 plists, got: {found:?}");
        let names: Vec<String> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"com.app.plist".to_string()));
        assert!(names.contains(&"com.other.plist".to_string()));
    }

    #[test]
    fn detect_plist_files_skips_dodotignored_packs() {
        use crate::testing::TempEnvironment;
        let env = TempEnvironment::builder()
            .pack("active")
            .file("a.plist", "in active")
            .done()
            .pack("muted")
            .file("b.plist", "in muted")
            .ignored()
            .done()
            .build();

        let ctx = make_test_ctx(&env);
        let found = detect_plist_files(&ctx).expect("detect");
        let names: Vec<String> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(
            names.contains(&"a.plist".to_string()),
            "active pack's plist should be found"
        );
        assert!(
            !names.contains(&"b.plist".to_string()),
            ".dodotignore'd pack's plist should be excluded, got: {names:?}"
        );
    }

    #[test]
    fn gitattributes_recogniser_handles_whitespace_and_comments() {
        assert!(gitattributes_line_present("*.plist filter=dodot-plist"));
        assert!(gitattributes_line_present(
            "  *.plist   filter=dodot-plist  "
        ));
        assert!(gitattributes_line_present(
            "*.plist filter=dodot-plist diff=plist"
        ));
        assert!(gitattributes_line_present(
            "*.plist filter=dodot-plist  # plist filter"
        ));

        assert!(!gitattributes_line_present(""));
        assert!(!gitattributes_line_present("# commented out"));
        assert!(!gitattributes_line_present("*.plist filter=other"));
        assert!(!gitattributes_line_present("*.txt filter=dodot-plist"));
    }
}

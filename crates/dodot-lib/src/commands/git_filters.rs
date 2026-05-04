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

/// Render the `.gitattributes` lines that bind each configured plist
/// extension to the `dodot-plist` filter. Default config produces one
/// line for `*.plist`; users who add e.g. `"binplist"` to the
/// `[symlink] plist_extensions` config get an additional line.
///
/// Callers must pass extensions that have already been normalized by
/// [`normalize_plist_extensions`]; the rendered patterns are dropped
/// straight into shell-adjacent contexts (the user's
/// `.gitattributes`, the install hint), and raw config values can
/// contain whitespace, quotes, or shell metacharacters that would
/// produce malformed output.
pub fn gitattributes_lines(extensions: &[String]) -> Vec<String> {
    extensions
        .iter()
        .map(|ext| format!("*.{ext} filter=dodot-plist"))
        .collect()
}

/// Normalize a raw `plist_extensions` config slice into a stable form
/// suitable for both detection and `.gitattributes` rendering.
///
/// For each entry: trim whitespace, strip a single leading `.`,
/// lowercase. Drop entries that are empty after that or that contain
/// any character outside `[A-Za-z0-9_+-]` (path separators, glob
/// metacharacters, quotes, whitespace, anything else that would
/// either silently fail to match files or turn a `.gitattributes`
/// line into something unsafe). Dedupe while preserving first-seen
/// order so user-visible output is stable.
///
/// The resulting Vec is what every code path should compare against
/// or render from. Detection compares case-insensitively; the
/// rendered glob (`*.<ext>`) is matched by git itself, which on
/// case-sensitive filesystems would treat `*.Plist` and `*.plist`
/// as different patterns — lowercasing here makes the contract
/// uniform regardless of how the user typed the config.
pub fn normalize_plist_extensions(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(raw.len());
    for entry in raw {
        let trimmed = entry.trim();
        let stripped = trimmed.strip_prefix('.').unwrap_or(trimmed);
        if stripped.is_empty() {
            continue;
        }
        if !stripped
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+'))
        {
            continue;
        }
        let lower = stripped.to_ascii_lowercase();
        if !out.contains(&lower) {
            out.push(lower);
        }
    }
    out
}

/// Resolve the active `plist_extensions` from the root config,
/// normalized via [`normalize_plist_extensions`]. Used by every code
/// path that needs to render or scan against the configured
/// extensions; honors the standard root → pack inheritance the
/// ConfigManager already manages (callers that want pack-scoped
/// resolution can call `config_for_pack` directly and run the result
/// through [`normalize_plist_extensions`]).
pub(crate) fn root_plist_extensions(ctx: &ExecutionContext) -> Result<Vec<String>> {
    Ok(normalize_plist_extensions(
        &ctx.config_manager.root_config()?.symlink.plist_extensions,
    ))
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

    let extensions = root_plist_extensions(ctx)?;
    let expected_lines = gitattributes_lines(&extensions);
    let attrs_content = ctx
        .fs
        .read_to_string(&root.join(".gitattributes"))
        .unwrap_or_default();
    // "Bound" means *every* configured extension has its line. A
    // partial bind (e.g. legacy file has `*.plist` but config now
    // also requires `*.binplist`) reports false so the install hint
    // surfaces the gap.
    let attributes_present = !expected_lines.is_empty()
        && expected_lines.iter().all(|expected| {
            attrs_content
                .lines()
                .any(|existing| gitattributes_line_matches(existing, expected))
        });

    let block = config_block_text();
    let block_lines = block.lines().map(str::to_string).collect();
    Ok(ShowFiltersResult {
        config_block: block,
        config_block_lines: block_lines,
        gitattributes_lines: expected_lines,
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
    /// One line per configured plist extension (default `["plist"]`).
    /// Templates iterate to render the full block.
    pub gitattributes_lines: Vec<String>,
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
/// absolute paths of files whose suffix matches any extension in the
/// pack-resolved `[symlink] plist_extensions` config (default
/// `["plist"]`). Used by `dodot up` to decide whether to offer the
/// filter-install prompt.
///
/// The configured list is normalized per-pack via
/// [`normalize_plist_extensions`] (trim, strip leading `.`,
/// lowercase, drop empty/invalid, dedupe), so `["plist"]`,
/// `[".Plist"]`, and `["  plist  "]` all behave identically.
///
/// Pack selection goes through [`packs::discover_packs`] so it honours
/// the same conventions every other command does: `pack.ignore`
/// patterns from config, `.dodotignore` markers, valid pack-name
/// rules, and the `.config` exception. Pack-internal walking ignores
/// nested dot-directories (`.git`, etc.) so we don't recurse into
/// vendored repos that happen to live inside a pack.
///
/// Detection is "any matching file in any active pack", not "tracked
/// by git". The looser check is intentional: an untracked plist in a
/// pack is almost certainly headed for a commit, and a false-positive
/// prompt is harmless. A stricter check would require shelling out to
/// `git ls-files` on every `up`.
pub fn detect_plist_files(ctx: &ExecutionContext) -> Result<Vec<std::path::PathBuf>> {
    let root = ctx.paths.dotfiles_root();
    if !ctx.fs.is_dir(root) {
        return Ok(Vec::new());
    }
    let root_config = ctx.config_manager.root_config()?;
    let packs = crate::packs::discover_packs(ctx.fs.as_ref(), root, &root_config.pack.ignore)?;
    detect_plist_files_in(ctx, &packs)
}

/// Like [`detect_plist_files`], but scoped to a caller-supplied pack
/// list. Used by callers that already discovered packs (e.g.
/// `commands::up`, which scopes drift detection to only the packs
/// the current run actually deployed) to avoid re-walking the
/// dotfiles tree and to honor `--pack` filters.
pub fn detect_plist_files_in(
    ctx: &ExecutionContext,
    packs: &[crate::packs::Pack],
) -> Result<Vec<std::path::PathBuf>> {
    let mut found = Vec::new();
    for pack in packs {
        // Honor pack-level overrides of `[symlink] plist_extensions`.
        let pack_config = ctx.config_manager.config_for_pack(&pack.path)?;
        let extensions = normalize_plist_extensions(&pack_config.symlink.plist_extensions);
        scan_for_plists(ctx.fs.as_ref(), &pack.path, &extensions, &mut found)?;
    }
    Ok(found)
}

fn scan_for_plists(
    fs: &dyn crate::fs::Fs,
    dir: &std::path::Path,
    extensions: &[String],
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
            scan_for_plists(fs, &entry.path, extensions, found)?;
        } else if entry
            .path
            .extension()
            .and_then(|e| e.to_str())
            .map(|ext| {
                extensions
                    .iter()
                    .any(|configured| configured.eq_ignore_ascii_case(ext))
            })
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
    let extensions = match root_plist_extensions(ctx) {
        Ok(e) => e,
        Err(_) => return, // surface elsewhere; don't block the install hint
    };
    let lines = gitattributes_lines(&extensions);
    let attrs_content = ctx
        .fs
        .read_to_string(&ctx.paths.dotfiles_root().join(".gitattributes"))
        .unwrap_or_default();
    let missing: Vec<&str> = lines
        .iter()
        .filter(|line| {
            !attrs_content
                .lines()
                .any(|existing| gitattributes_line_matches(existing, line))
        })
        .map(String::as_str)
        .collect();
    if missing.is_empty() {
        return;
    }
    // Print lines directly rather than wrapping them in an `echo
    // '...' >> .gitattributes` snippet. Even though
    // `normalize_plist_extensions` already filters out shell
    // metacharacters, emitting copy-pasteable shell that interpolates
    // config-derived data is the wrong shape on principle: the user
    // can paste these lines into their editor or run their own append.
    let label = if missing.len() == 1 {
        "Next — add this line to .gitattributes:"
    } else {
        "Next — add these lines to .gitattributes:"
    };
    details.push(String::new());
    details.push(label.into());
    for line in &missing {
        details.push(format!("    {line}"));
    }
    details.push(String::new());
    details
        .push("Then commit: git add .gitattributes && git commit -m 'enable plist filters'".into());
}

/// True if `existing` (a line from `.gitattributes`) binds the same
/// `*.<ext> filter=dodot-plist` pattern as `expected`. Tolerant of
/// whitespace, trailing attributes (e.g. `diff=plist`), and comments.
fn gitattributes_line_matches(existing: &str, expected: &str) -> bool {
    let strip = |s: &str| -> Option<String> {
        let trimmed = s.split('#').next().unwrap_or("").trim();
        let mut parts = trimmed.split_ascii_whitespace();
        let pattern = parts.next()?.to_string();
        let binds_filter = parts.any(|tok| tok == "filter=dodot-plist");
        if binds_filter {
            Some(pattern)
        } else {
            None
        }
    };
    match (strip(existing), strip(expected)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
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
            host_facts: Arc::new(crate::gates::HostFacts::detect()),
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
        let expected = "*.plist filter=dodot-plist";
        assert!(gitattributes_line_matches(
            "*.plist filter=dodot-plist",
            expected
        ));
        assert!(gitattributes_line_matches(
            "  *.plist   filter=dodot-plist  ",
            expected
        ));
        assert!(gitattributes_line_matches(
            "*.plist filter=dodot-plist diff=plist",
            expected
        ));
        assert!(gitattributes_line_matches(
            "*.plist filter=dodot-plist  # plist filter",
            expected
        ));

        assert!(!gitattributes_line_matches("", expected));
        assert!(!gitattributes_line_matches("# commented out", expected));
        assert!(!gitattributes_line_matches(
            "*.plist filter=other",
            expected
        ));
        assert!(!gitattributes_line_matches(
            "*.txt filter=dodot-plist",
            expected
        ));
    }

    #[test]
    fn gitattributes_lines_emits_one_per_extension() {
        let lines = gitattributes_lines(&["plist".to_string()]);
        assert_eq!(lines, vec!["*.plist filter=dodot-plist"]);

        let lines = gitattributes_lines(&[
            "plist".to_string(),
            "binplist".to_string(),
            "savedState".to_string(),
        ]);
        assert_eq!(
            lines,
            vec![
                "*.plist filter=dodot-plist",
                "*.binplist filter=dodot-plist",
                "*.savedState filter=dodot-plist",
            ]
        );
    }

    #[test]
    fn detect_plist_files_honors_custom_extension() {
        // With the default config, only `.plist` is detected. With
        // `binplist` added to the pack's `[symlink] plist_extensions`,
        // detection picks it up too. Pack-level inheritance is the
        // shipped path; root-level overrides the same way.
        use crate::testing::TempEnvironment;
        let env = TempEnvironment::builder()
            .pack("apps")
            .file("com.app.plist", "binary-or-xml")
            .file("com.other.binplist", "different ext")
            .file("README.md", "should be ignored")
            .config("[symlink]\nplist_extensions = [\"plist\", \"binplist\"]\n")
            .done()
            .build();
        let ctx = make_test_ctx(&env);
        let found = detect_plist_files(&ctx).expect("detect");
        let names: Vec<String> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(
            names.contains(&"com.app.plist".to_string()),
            "default-extension plist should be found: {names:?}"
        );
        assert!(
            names.contains(&"com.other.binplist".to_string()),
            "custom-extension plist should be found: {names:?}"
        );
        assert!(
            !names.iter().any(|n| n.ends_with(".md")),
            "non-plist files must not surface: {names:?}"
        );
    }

    #[test]
    fn normalize_plist_extensions_strips_lowercases_dedupes_and_filters() {
        // Trims, strips a leading `.`, lowercases.
        assert_eq!(
            normalize_plist_extensions(&[
                "plist".into(),
                ".plist".into(),
                "  .Plist  ".into(),
                "BinPlist".into(),
            ]),
            vec!["plist".to_string(), "binplist".to_string()],
            "leading dot, mixed case, and whitespace must collapse \
             to a single canonical entry"
        );

        // Filters empty / whitespace-only.
        assert!(normalize_plist_extensions(&["".into(), "   ".into(), ".".into()]).is_empty());

        // Rejects shell metacharacters, path separators, glob chars,
        // quotes — anything that could turn the rendered .gitattributes
        // line into something unsafe or that would silently fail to
        // match any file at scan time.
        let dangerous = [
            "evil; rm -rf".to_string(),
            "*.txt".to_string(),
            "weird path".to_string(),
            "foo/bar".to_string(),
            "quote'd".to_string(),
            "back\\slash".to_string(),
            "with\nnewline".to_string(),
        ];
        assert!(
            normalize_plist_extensions(&dangerous).is_empty(),
            "metacharacter-bearing entries must be dropped"
        );

        // Real-world mix passes through cleanly.
        assert_eq!(
            normalize_plist_extensions(&[
                "plist".into(),
                "binplist".into(),
                "savedState".into(),
                "mobileconfig".into(),
            ]),
            vec![
                "plist".to_string(),
                "binplist".to_string(),
                "savedstate".to_string(),
                "mobileconfig".to_string(),
            ]
        );
    }
}

//! `dodot refresh` — touch source mtimes when deployed bytes diverged.
//!
//! Walks the per-file baseline cache, hashes each deployed (datastore-
//! side) file, and copies the deployed file's mtime onto the template
//! source whenever the hashes differ. Why: git uses stat-cache mtimes
//! to decide whether to re-read a working-tree file, so without this
//! step a deployed-side edit never surfaces in `git status` (the
//! source mtime hasn't changed → git uses the cached hash → no clean-
//! filter invocation → no diff). Touching the source forces a re-read.
//!
//! See `docs/proposals/magic.lex` §"Update Trigger Bit". This command
//! is the engine the Tier 2 shell alias (`alias git='dodot refresh
//! --quiet && command git'`) and external file-watcher integrations
//! call before delegating to git.
//!
//! # Modes
//!
//! - **default**: writes a short report to stdout (touched / clean
//!   counts, per-file lines for touched entries).
//! - **`--quiet`**: silent, exit 0. Intended for the shell alias so a
//!   no-op refresh doesn't print on every git invocation.
//! - **`--list-paths`**: prints absolute source paths that need a
//!   touch (mtime not yet copied), one per line. Intended for editor
//!   / file-watcher integrations that want to drive the touch
//!   themselves; we don't write mtimes in this mode.
//!
//! Exit code: 0 in all healthy cases. Errors (real I/O failures only)
//! propagate as `DodotError::Fs`.

use serde::Serialize;

use crate::packs::orchestration::ExecutionContext;
use crate::preprocessing::baseline::hex_sha256;
use crate::preprocessing::divergence::collect_baselines;
use crate::Result;

/// What `refresh` did to a single processed file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshAction {
    /// Deployed file's hash matches the baseline; nothing to do.
    Clean,
    /// Source mtime was copied from the deployed file (default mode)
    /// or would be (`--list-paths` mode).
    Touched,
    /// Deployed file is missing from the datastore (e.g. user removed
    /// it). Reported but not actioned.
    MissingDeployed,
    /// Cached source path no longer exists on disk. Reported.
    MissingSource,
}

/// One row in the refresh report.
#[derive(Debug, Clone, Serialize)]
pub struct RefreshEntry {
    pub pack: String,
    pub handler: String,
    pub filename: String,
    /// Absolute source path. The CLI renderer (and the JSON output)
    /// both surface this verbatim — refresh entries are typically a
    /// short list, and the absolute path is unambiguous when the
    /// user wants to plug `--list-paths` output into a watcher.
    pub source_path: String,
    pub action: RefreshAction,
}

/// Aggregate result of a refresh invocation.
#[derive(Debug, Clone, Serialize)]
pub struct RefreshResult {
    pub entries: Vec<RefreshEntry>,
    /// True iff at least one entry was Touched. Drives the
    /// `--list-paths` and report-mode rendering.
    pub touched_any: bool,
    /// Operating mode chosen by the caller, surfaced so the renderer
    /// can pick the right template branch.
    pub mode: RefreshMode,
}

/// Refresh invocation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshMode {
    /// Default: write mtimes, render a short report.
    Report,
    /// `--quiet`: write mtimes, render nothing.
    Quiet,
    /// `--list-paths`: do NOT write mtimes; render only the source
    /// paths of divergent entries (one per line).
    ListPaths,
}

/// Run `dodot refresh` in the given mode.
///
/// Walks every cached baseline. For each:
///   - read the deployed bytes from `<data_dir>/packs/<pack>/<handler>/<filename>`
///   - hash them; compare to `baseline.rendered_hash`
///   - if equal → action `Clean`
///   - if differ AND mode != ListPaths → copy deployed mtime onto source, action `Touched`
///   - if differ AND mode == ListPaths → action `Touched` (no write; the source path will be printed)
///   - if deployed is missing → action `MissingDeployed`
///   - if source path is empty or missing → action `MissingSource`
pub fn refresh(ctx: &ExecutionContext, mode: RefreshMode) -> Result<RefreshResult> {
    let baselines = collect_baselines(ctx.fs.as_ref(), ctx.paths.as_ref())?;
    let mut entries = Vec::with_capacity(baselines.len());
    let mut touched_any = false;

    for (pack, handler, filename, baseline) in baselines {
        let source_path = baseline.source_path.clone();
        let deployed_path = ctx
            .paths
            .data_dir()
            .join("packs")
            .join(&pack)
            .join(&handler)
            .join(&filename);

        let action = if source_path.as_os_str().is_empty() || !ctx.fs.exists(&source_path) {
            RefreshAction::MissingSource
        } else if !ctx.fs.exists(&deployed_path) {
            RefreshAction::MissingDeployed
        } else {
            // Hash the deployed bytes. A read error here surfaces as a
            // hard error rather than silently logging — refresh is a
            // small command and we'd rather fail loudly than drop a
            // sync that the user thinks succeeded.
            let bytes = ctx.fs.read_file(&deployed_path)?;
            if hex_sha256(&bytes) == baseline.rendered_hash {
                RefreshAction::Clean
            } else {
                if mode != RefreshMode::ListPaths {
                    let deployed_mtime = ctx.fs.modified(&deployed_path)?;
                    let source_mtime = ctx.fs.modified(&source_path)?;
                    // The whole point of refresh is to invalidate
                    // git's stat-cache by changing the source mtime.
                    // If the deployed mtime happens to equal the
                    // current source mtime — possible on coarse-
                    // resolution filesystems (FAT, HFS+ at 1s
                    // granularity) or when a user edits and refreshes
                    // within the same second — copying it would be a
                    // no-op and git would not re-read the file. Bump
                    // by 1s in that case so the mtime strictly
                    // changes. We don't care that the source mtime
                    // ends up "ahead of" the deployed mtime; what
                    // matters is that it differs from the cached
                    // value git has.
                    let target = if deployed_mtime == source_mtime {
                        deployed_mtime + std::time::Duration::from_secs(1)
                    } else {
                        deployed_mtime
                    };
                    ctx.fs.set_modified(&source_path, target)?;
                }
                touched_any = true;
                RefreshAction::Touched
            }
        };

        entries.push(RefreshEntry {
            pack,
            handler,
            filename,
            source_path: source_path.display().to_string(),
            action,
        });
    }

    Ok(RefreshResult {
        entries,
        touched_any,
        mode,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::Fs;
    use crate::paths::Pather;
    use crate::preprocessing::baseline::Baseline;
    use crate::testing::TempEnvironment;

    fn make_ctx(env: &TempEnvironment) -> ExecutionContext {
        use crate::config::ConfigManager;
        use crate::datastore::{CommandOutput, CommandRunner, FilesystemDataStore};
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
            check_drift: false,
            view_mode: crate::commands::ViewMode::Full,
            group_mode: crate::commands::GroupMode::Name,
            verbose: false,
            host_facts: Arc::new(crate::gates::HostFacts::detect()),
        }
    }

    fn write_file(env: &TempEnvironment, path: &std::path::Path, body: &[u8]) {
        env.fs.mkdir_all(path.parent().unwrap()).unwrap();
        env.fs.write_file(path, body).unwrap();
    }

    /// Stage a baseline + matching pack source + matching deployed
    /// file. Returns the absolute source and deployed paths so the
    /// test can edit either side.
    fn stage_one(
        env: &TempEnvironment,
        pack: &str,
        template_name: &str,
        rendered: &[u8],
        source: &[u8],
    ) -> (std::path::PathBuf, std::path::PathBuf) {
        let src = env.dotfiles_root.join(pack).join(template_name);
        write_file(env, &src, source);
        let stripped = template_name.strip_suffix(".tmpl").unwrap_or(template_name);
        let deployed = env
            .paths
            .data_dir()
            .join("packs")
            .join(pack)
            .join("preprocessed")
            .join(stripped);
        write_file(env, &deployed, rendered);
        let baseline = Baseline::build(&src, rendered, source, Some(""), None);
        baseline
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                pack,
                "preprocessed",
                stripped,
            )
            .unwrap();
        (src, deployed)
    }

    #[test]
    fn empty_cache_yields_empty_report() {
        let env = TempEnvironment::builder().build();
        let ctx = make_ctx(&env);
        let r = refresh(&ctx, RefreshMode::Report).unwrap();
        assert!(r.entries.is_empty());
        assert!(!r.touched_any);
    }

    #[test]
    fn clean_state_is_a_noop() {
        // baseline + source + deployed all line up. No mtime touched.
        let env = TempEnvironment::builder().build();
        let (src, _) = stage_one(&env, "app", "cfg.toml.tmpl", b"rendered", b"src");
        // Capture the source mtime before refresh; a no-op must not
        // change it.
        let before = env.fs.modified(&src).unwrap();

        let ctx = make_ctx(&env);
        let r = refresh(&ctx, RefreshMode::Report).unwrap();
        assert_eq!(r.entries.len(), 1);
        assert!(matches!(r.entries[0].action, RefreshAction::Clean));
        assert!(!r.touched_any);
        assert_eq!(env.fs.modified(&src).unwrap(), before);
    }

    #[test]
    fn divergent_deployed_touches_source_mtime() {
        // The core scenario: user edits the deployed file → source
        // mtime gets bumped to match.
        let env = TempEnvironment::builder().build();
        let (src, deployed) = stage_one(&env, "app", "cfg.toml.tmpl", b"rendered", b"src");

        // Edit the deployed file to a divergent value AFTER the
        // baseline. Sleep briefly so the deployed mtime is strictly
        // later than the source's.
        std::thread::sleep(std::time::Duration::from_millis(20));
        env.fs.write_file(&deployed, b"rendered EDITED").unwrap();
        let deployed_mtime = env.fs.modified(&deployed).unwrap();

        let ctx = make_ctx(&env);
        let r = refresh(&ctx, RefreshMode::Report).unwrap();
        assert_eq!(r.entries.len(), 1);
        assert!(matches!(r.entries[0].action, RefreshAction::Touched));
        assert!(r.touched_any);

        // Source mtime now equals the deployed mtime.
        let new_src_mtime = env.fs.modified(&src).unwrap();
        assert_eq!(new_src_mtime, deployed_mtime);
    }

    #[test]
    fn list_paths_mode_does_not_write_mtimes() {
        // `--list-paths` reports divergent sources but never touches.
        // Editor / watcher integrations want to drive the touch
        // themselves so they can sequence it correctly with their own
        // build steps.
        let env = TempEnvironment::builder().build();
        let (src, deployed) = stage_one(&env, "app", "cfg.toml.tmpl", b"rendered", b"src");

        let before_src = env.fs.modified(&src).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        env.fs.write_file(&deployed, b"rendered EDITED").unwrap();

        let ctx = make_ctx(&env);
        let r = refresh(&ctx, RefreshMode::ListPaths).unwrap();
        assert_eq!(r.entries.len(), 1);
        assert!(matches!(r.entries[0].action, RefreshAction::Touched));
        assert!(r.touched_any);

        // mtime unchanged.
        assert_eq!(env.fs.modified(&src).unwrap(), before_src);
    }

    #[test]
    fn quiet_mode_still_writes_mtimes() {
        // `--quiet` is just an output-suppression flag; the work
        // itself happens. The shell alias depends on this.
        let env = TempEnvironment::builder().build();
        let (src, deployed) = stage_one(&env, "app", "cfg.toml.tmpl", b"rendered", b"src");

        std::thread::sleep(std::time::Duration::from_millis(20));
        env.fs.write_file(&deployed, b"rendered EDITED").unwrap();
        let deployed_mtime = env.fs.modified(&deployed).unwrap();

        let ctx = make_ctx(&env);
        let r = refresh(&ctx, RefreshMode::Quiet).unwrap();
        assert!(matches!(r.entries[0].action, RefreshAction::Touched));
        assert_eq!(env.fs.modified(&src).unwrap(), deployed_mtime);
    }

    #[test]
    fn missing_source_is_reported_not_an_error() {
        // The cached source path no longer exists (user renamed /
        // removed the .tmpl). Refresh keeps going; the entry is
        // surfaced so the user knows the cache is stale.
        let env = TempEnvironment::builder().build();
        // Stage a baseline whose source path doesn't exist on disk.
        let baseline = Baseline::build(
            &env.dotfiles_root.join("app/missing.toml.tmpl"),
            b"rendered",
            b"src",
            Some(""),
            None,
        );
        baseline
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "missing.toml",
            )
            .unwrap();
        // Deployed file exists.
        let deployed = env
            .paths
            .data_dir()
            .join("packs/app/preprocessed/missing.toml");
        write_file(&env, &deployed, b"rendered");

        let ctx = make_ctx(&env);
        let r = refresh(&ctx, RefreshMode::Report).unwrap();
        assert_eq!(r.entries.len(), 1);
        assert!(matches!(r.entries[0].action, RefreshAction::MissingSource));
        assert!(!r.touched_any);
    }

    #[test]
    fn missing_deployed_is_reported_not_an_error() {
        // The deployed file is gone; refresh has nothing to compare
        // against. Surface as MissingDeployed.
        let env = TempEnvironment::builder().build();
        let src = env.dotfiles_root.join("app/cfg.toml.tmpl");
        write_file(&env, &src, b"src");
        let baseline = Baseline::build(&src, b"rendered", b"src", Some(""), None);
        baseline
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "cfg.toml",
            )
            .unwrap();
        // Don't lay down the deployed file.

        let ctx = make_ctx(&env);
        let r = refresh(&ctx, RefreshMode::Report).unwrap();
        assert!(matches!(
            r.entries[0].action,
            RefreshAction::MissingDeployed
        ));
        assert!(!r.touched_any);
    }

    #[test]
    fn pure_data_edit_is_still_treated_as_divergent() {
        // Edge case: the user edited only a variable's *value* in the
        // deployed file. The deployed bytes diverge from the
        // baseline, so refresh touches the source. The clean filter
        // (R6, when installed) will then re-evaluate and decide
        // whether the change is worth a template-space diff. Refresh
        // itself is intentionally a coarse hash compare — it errs on
        // the side of triggering the filter rather than missing a
        // real edit.
        let env = TempEnvironment::builder().build();
        let (_src, deployed) = stage_one(
            &env,
            "app",
            "greet.tmpl",
            b"hello Alice",
            b"hello {{ name }}",
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
        env.fs.write_file(&deployed, b"hello Bob").unwrap();

        let ctx = make_ctx(&env);
        let r = refresh(&ctx, RefreshMode::Report).unwrap();
        assert!(matches!(r.entries[0].action, RefreshAction::Touched));
        assert!(r.touched_any);
    }

    #[test]
    fn divergent_with_equal_mtimes_still_bumps_source() {
        // Edge case from PR review: if the deployed mtime happens to
        // equal the source mtime (coarse FS, rapid edits within the
        // same second), `set_modified(source, deployed_mtime)` would
        // be a no-op — git's stat-cache wouldn't invalidate, and
        // refresh would silently fail at its core purpose. We bump
        // by 1s in that case so the source mtime *strictly* changes.
        let env = TempEnvironment::builder().build();
        let (src, deployed) = stage_one(&env, "app", "cfg.toml.tmpl", b"rendered", b"src");

        // Force the deployed mtime to exactly match the current
        // source mtime, then mutate the deployed bytes so refresh
        // sees a divergence to act on.
        let pinned = env.fs.modified(&src).unwrap();
        env.fs.write_file(&deployed, b"rendered EDITED").unwrap();
        env.fs.set_modified(&deployed, pinned).unwrap();
        assert_eq!(env.fs.modified(&deployed).unwrap(), pinned);

        let ctx = make_ctx(&env);
        let r = refresh(&ctx, RefreshMode::Report).unwrap();
        assert!(matches!(r.entries[0].action, RefreshAction::Touched));

        // Source mtime must STRICTLY exceed the original (no-op
        // behaviour would leave it unchanged).
        let after = env.fs.modified(&src).unwrap();
        assert!(
            after > pinned,
            "source mtime should strictly increase even when deployed mtime equals source mtime"
        );
    }

    #[test]
    fn entries_are_sorted_by_pack_handler_filename() {
        // Stable display order — the underlying walker is sorted, and
        // refresh inherits that. Pin it so callers can rely on
        // deterministic output.
        let env = TempEnvironment::builder().build();
        for (pack, name) in [
            ("zebra", "z.tmpl"),
            ("alpha", "b.tmpl"),
            ("alpha", "a.tmpl"),
        ] {
            stage_one(&env, pack, name, b"rendered", b"src");
        }
        let ctx = make_ctx(&env);
        let r = refresh(&ctx, RefreshMode::Report).unwrap();
        let order: Vec<_> = r
            .entries
            .iter()
            .map(|e| (e.pack.clone(), e.filename.clone()))
            .collect();
        assert_eq!(
            order,
            vec![
                ("alpha".into(), "a".into()),
                ("alpha".into(), "b".into()),
                ("zebra".into(), "z".into()),
            ]
        );
    }
}

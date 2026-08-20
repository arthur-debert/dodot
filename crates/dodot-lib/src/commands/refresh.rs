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
//!
//! # One cache, one root
//!
//! The baseline cache is shared across every dotfiles root that has ever
//! run `dodot up` on this machine, and each baseline stores the absolute
//! path of the source it was rendered from. Touching those paths blind
//! would let a refresh run from one root write mtimes into another
//! root's user-authored sources, so the mutation set is scoped to the
//! root the invocation is authorized for: only sources that canonically
//! lie inside it are touched, and every other baseline is reported
//! (`docs/adr/0002-guard-root-derived-mutations.md`).

use serde::Serialize;

use crate::packs::orchestration::ExecutionContext;
use crate::preprocessing::baseline::hex_sha256;
use crate::preprocessing::divergence::collect_baselines;
use crate::safety_lock::{scope_to_root, OsPathProbe, OutOfRootReason, RootIdentity, ScopeOutcome};
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
    /// The cached source belongs to a different dotfiles root. Reported;
    /// this invocation is not authorized to touch it.
    OutOfRoot,
    /// Cached source path no longer exists on disk. Reported.
    MissingSource,
    /// The baseline records no absolute source path at all — the stored
    /// path is empty (an entry written before the cache tracked source
    /// paths) or relative (meaningless without the process working
    /// directory, which Safety Lock does not read). Reported; the next
    /// `dodot up` rewrites it.
    StaleSource,
    /// The cached source exists but could not be resolved, so it cannot
    /// be placed inside the root. Reported, never touched.
    UnresolvableSource,
}

impl RefreshAction {
    /// Name the reason a baseline stayed out of the mutation set.
    ///
    /// One action per reason rather than a single "not mine" bucket: a
    /// source under another root, a deleted source, a source path the
    /// cache never recorded, and one Dodot cannot resolve are four
    /// different states of the user's machine with four different fixes.
    fn out_of_root(reason: OutOfRootReason) -> Self {
        match reason {
            OutOfRootReason::OutsideRoot => RefreshAction::OutOfRoot,
            OutOfRootReason::Missing => RefreshAction::MissingSource,
            OutOfRootReason::Stale => RefreshAction::StaleSource,
            OutOfRootReason::Uncanonicalizable => RefreshAction::UnresolvableSource,
        }
    }
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

/// Run `dodot refresh` in the given mode, touching only sources under
/// `authorized_root`.
///
/// Walks every cached baseline and scopes the walk to the authorized root
/// first (see the module docs). For each baseline:
///   - if its source is not inside the root → report why, touch nothing
///   - read the deployed bytes from `<data_dir>/packs/<pack>/<handler>/<filename>`
///   - hash them; compare to `baseline.rendered_hash`
///   - if equal → action `Clean`
///   - if differ AND mode != ListPaths → copy deployed mtime onto source, action `Touched`
///   - if differ AND mode == ListPaths → action `Touched` (no write; the source path will be printed)
///   - if deployed is missing → action `MissingDeployed`
///
/// `authorized_root` is the canonical root this invocation may write
/// under: the root the CLI gate resolved once at the process boundary and
/// either found already approved or had the user approve. It is not
/// re-derived here: the root that was authorized and
/// the root that is mutated must be the same one (ADR-0001).
///
/// The mtime is written to the *canonical* source path rather than the
/// spelling the cache stored, because that is the path containment was
/// decided on; a source reached through an in-root symlink is touched at
/// its resolved location. The report still names the stored path, which
/// is what the user recognizes and what `--list-paths` consumers feed
/// back to their watchers.
pub fn refresh(
    ctx: &ExecutionContext,
    mode: RefreshMode,
    authorized_root: &RootIdentity,
) -> Result<RefreshResult> {
    let baselines = collect_baselines(ctx.fs.as_ref(), ctx.paths.as_ref())?;
    let sources: Vec<std::path::PathBuf> = baselines
        .iter()
        .map(|(_, _, _, baseline)| baseline.source_path.clone())
        .collect();
    // The command layer's filesystem is the real one (`OsFs`), so
    // canonicalization asks the matching OS probe.
    let scope = scope_to_root(authorized_root, &sources, &OsPathProbe);

    let mut entries = Vec::with_capacity(baselines.len());
    let mut touched_any = false;

    for ((pack, handler, filename, baseline), outcome) in
        baselines.into_iter().zip(scope.outcomes())
    {
        let source_path = baseline.source_path.clone();
        let deployed_path = ctx
            .paths
            .data_dir()
            .join("packs")
            .join(&pack)
            .join(&handler)
            .join(&filename);

        // Only the authorized canonical path is ever written; everything
        // else is a row in the report.
        let authorized = match outcome {
            ScopeOutcome::InRoot(canonical) => canonical,
            ScopeOutcome::OutOfRoot(target) => {
                entries.push(RefreshEntry {
                    pack,
                    handler,
                    filename,
                    source_path: source_path.display().to_string(),
                    action: RefreshAction::out_of_root(target.reason),
                });
                continue;
            }
        };

        let action = if !ctx.fs.exists(&deployed_path) {
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
                    let source_mtime = ctx.fs.modified(authorized)?;
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
                    ctx.fs.set_modified(authorized, target)?;
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
        use crate::datastore::{CommandOutput, CommandRunner, CommandSpec, FilesystemDataStore};
        use std::sync::Arc;

        struct NoopRunner;
        impl CommandRunner for NoopRunner {
            fn run(&self, _command: CommandSpec<'_>) -> Result<CommandOutput> {
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
            show_diff: false,
            view_mode: crate::commands::ViewMode::Full,
            group_mode: crate::commands::GroupMode::Name,
            verbose: false,
            host_facts: Arc::new(crate::gates::HostFacts::detect()),
            env_stamp: Default::default(),
            tty: false,
            shell_probe: crate::shell::ProbePolicy::Never,
            provision_host: Arc::new(
                crate::provisioners::availability::ProvisionHost::assume_present(),
            ),
            shell_env: crate::shell::ShellEnv::default(),
        }
    }

    fn write_file(env: &TempEnvironment, path: &std::path::Path, body: &[u8]) {
        env.fs.mkdir_all(path.parent().unwrap()).unwrap();
        env.fs.write_file(path, body).unwrap();
    }

    /// The root this invocation is authorized to touch: the
    /// environment's dotfiles root, canonicalized.
    ///
    /// Canonicalized because the command canonicalizes every source
    /// before placing it, and a root that is not itself canonical
    /// contains none of them — `TempDir` hands back `/var/…` on macOS
    /// while every resolved source under it comes back as `/private/var/…`.
    fn root(env: &TempEnvironment) -> RootIdentity {
        canonical_root(&env.dotfiles_root)
    }

    fn canonical_root(path: &std::path::Path) -> RootIdentity {
        RootIdentity::new(std::fs::canonicalize(path).unwrap()).unwrap()
    }

    /// A second dotfiles root beside the first, sharing this
    /// environment's one data and cache directory — the arrangement the
    /// scoping exists for.
    fn second_root_dir(env: &TempEnvironment) -> std::path::PathBuf {
        let dir = env.home.join("other-dotfiles");
        env.fs.mkdir_all(&dir).unwrap();
        dir
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
        let root = env.dotfiles_root.clone();
        stage_under(env, &root, pack, template_name, rendered, source)
    }

    /// [`stage_one`] for a source under an arbitrary root, so one cache
    /// can hold baselines belonging to two different roots.
    fn stage_under(
        env: &TempEnvironment,
        root: &std::path::Path,
        pack: &str,
        template_name: &str,
        rendered: &[u8],
        source: &[u8],
    ) -> (std::path::PathBuf, std::path::PathBuf) {
        let src = root.join(pack).join(template_name);
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
        let r = refresh(&ctx, RefreshMode::Report, &root(&env)).unwrap();
        assert!(r.entries.is_empty());
        assert!(!r.touched_any);
    }

    #[test]
    fn clean_state_is_a_noop() {
        let env = TempEnvironment::builder().build();
        let (src, _) = stage_one(&env, "app", "cfg.toml.tmpl", b"rendered", b"src");
        // Capture the source mtime before refresh; a no-op must not
        // change it.
        let before = env.fs.modified(&src).unwrap();

        let ctx = make_ctx(&env);
        let r = refresh(&ctx, RefreshMode::Report, &root(&env)).unwrap();
        assert_eq!(r.entries.len(), 1);
        assert!(matches!(r.entries[0].action, RefreshAction::Clean));
        assert!(!r.touched_any);
        assert_eq!(env.fs.modified(&src).unwrap(), before);
    }

    #[test]
    fn divergent_deployed_touches_source_mtime() {
        let env = TempEnvironment::builder().build();
        let (src, deployed) = stage_one(&env, "app", "cfg.toml.tmpl", b"rendered", b"src");

        // Ensure the deployed mtime is strictly later than the source.
        std::thread::sleep(std::time::Duration::from_millis(20));
        env.fs.write_file(&deployed, b"rendered EDITED").unwrap();
        let deployed_mtime = env.fs.modified(&deployed).unwrap();

        let ctx = make_ctx(&env);
        let r = refresh(&ctx, RefreshMode::Report, &root(&env)).unwrap();
        assert_eq!(r.entries.len(), 1);
        assert!(matches!(r.entries[0].action, RefreshAction::Touched));
        assert!(r.touched_any);

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
        let r = refresh(&ctx, RefreshMode::ListPaths, &root(&env)).unwrap();
        assert_eq!(r.entries.len(), 1);
        assert!(matches!(r.entries[0].action, RefreshAction::Touched));
        assert!(r.touched_any);

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
        let r = refresh(&ctx, RefreshMode::Quiet, &root(&env)).unwrap();
        assert!(matches!(r.entries[0].action, RefreshAction::Touched));
        assert_eq!(env.fs.modified(&src).unwrap(), deployed_mtime);
    }

    #[test]
    fn missing_source_is_reported_not_an_error() {
        // The cached source path no longer exists (user renamed /
        // removed the .tmpl). Refresh keeps going; the entry is
        // surfaced so the user knows the cache is stale.
        let env = TempEnvironment::builder().build();
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
        let deployed = env
            .paths
            .data_dir()
            .join("packs/app/preprocessed/missing.toml");
        write_file(&env, &deployed, b"rendered");

        let ctx = make_ctx(&env);
        let r = refresh(&ctx, RefreshMode::Report, &root(&env)).unwrap();
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

        let ctx = make_ctx(&env);
        let r = refresh(&ctx, RefreshMode::Report, &root(&env)).unwrap();
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
        let r = refresh(&ctx, RefreshMode::Report, &root(&env)).unwrap();
        assert!(matches!(r.entries[0].action, RefreshAction::Touched));
        assert!(r.touched_any);
    }

    #[test]
    fn divergent_with_equal_mtimes_still_bumps_source() {
        // Coarse filesystems or rapid edits can leave deployed and
        // source mtimes equal. `set_modified(source, deployed_mtime)` would
        // be a no-op — git's stat-cache wouldn't invalidate, and
        // refresh would silently fail at its core purpose. We bump
        // by 1s in that case so the source mtime *strictly* changes.
        let env = TempEnvironment::builder().build();
        let (src, deployed) = stage_one(&env, "app", "cfg.toml.tmpl", b"rendered", b"src");

        let pinned = env.fs.modified(&src).unwrap();
        env.fs.write_file(&deployed, b"rendered EDITED").unwrap();
        env.fs.set_modified(&deployed, pinned).unwrap();
        assert_eq!(env.fs.modified(&deployed).unwrap(), pinned);

        let ctx = make_ctx(&env);
        let r = refresh(&ctx, RefreshMode::Report, &root(&env)).unwrap();
        assert!(matches!(r.entries[0].action, RefreshAction::Touched));

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
        let r = refresh(&ctx, RefreshMode::Report, &root(&env)).unwrap();
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

    // ── mutation scoping (ACC01-WS06) ────────────────────────────

    /// Two roots, one shared cache: whichever root the invocation is
    /// authorized for, the other root's sources keep their mtimes. This
    /// is the property the scoping exists for — approval for one root
    /// cannot reach into a second repository's working tree
    /// (ADR-0002).
    #[test]
    fn authorizing_one_root_never_touches_the_other_roots_sources() {
        let env = TempEnvironment::builder().build();
        let other_root = second_root_dir(&env);

        let (here, here_deployed) = stage_one(&env, "app", "cfg.toml.tmpl", b"rendered", b"src");
        let (there, there_deployed) = stage_under(
            &env,
            &other_root,
            "other",
            "cfg.toml.tmpl",
            b"rendered",
            b"src",
        );

        // Both deployed files diverge, so both baselines *want* a touch.
        std::thread::sleep(std::time::Duration::from_millis(20));
        env.fs
            .write_file(&here_deployed, b"rendered EDITED")
            .unwrap();
        env.fs
            .write_file(&there_deployed, b"rendered EDITED")
            .unwrap();
        let here_before = env.fs.modified(&here).unwrap();
        let there_before = env.fs.modified(&there).unwrap();

        let ctx = make_ctx(&env);
        let r = refresh(&ctx, RefreshMode::Report, &root(&env)).unwrap();

        let actions: Vec<(&str, &RefreshAction)> = r
            .entries
            .iter()
            .map(|e| (e.pack.as_str(), &e.action))
            .collect();
        assert!(
            matches!(
                actions.as_slice(),
                [
                    ("app", RefreshAction::Touched),
                    ("other", RefreshAction::OutOfRoot)
                ]
            ),
            "unexpected actions: {actions:?}"
        );
        assert_ne!(
            env.fs.modified(&here).unwrap(),
            here_before,
            "the authorized root's own source was not touched"
        );
        assert_eq!(
            env.fs.modified(&there).unwrap(),
            there_before,
            "another root's source was touched"
        );

        // Authorize the other root instead: the mirror image, from the
        // same cache.
        let r = refresh(&ctx, RefreshMode::Report, &canonical_root(&other_root)).unwrap();
        let actions: Vec<(&str, &RefreshAction)> = r
            .entries
            .iter()
            .map(|e| (e.pack.as_str(), &e.action))
            .collect();
        assert!(
            matches!(
                actions.as_slice(),
                [
                    ("app", RefreshAction::OutOfRoot),
                    ("other", RefreshAction::Touched)
                ]
            ),
            "unexpected actions: {actions:?}"
        );
        assert_ne!(env.fs.modified(&there).unwrap(), there_before);
    }

    /// A source that *lives* under the authorized root but *resolves*
    /// outside it: the cache stored an in-root spelling, and touching it
    /// would write through the link into another tree.
    #[test]
    fn a_source_symlinked_out_of_the_root_is_reported_not_touched() {
        let env = TempEnvironment::builder().build();
        let other_root = second_root_dir(&env);
        let outside = other_root.join("real.toml.tmpl");
        write_file(&env, &outside, b"src");

        // The pack entry is a symlink pointing out of the root; the
        // deployed file diverges, so it would be touched if the escape
        // went unnoticed.
        let link = env.dotfiles_root.join("app/cfg.toml.tmpl");
        env.fs.mkdir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let deployed = env.paths.data_dir().join("packs/app/preprocessed/cfg.toml");
        write_file(&env, &deployed, b"rendered EDITED");
        Baseline::build(&link, b"rendered", b"src", Some(""), None)
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "cfg.toml",
            )
            .unwrap();
        let before = env.fs.modified(&outside).unwrap();

        let ctx = make_ctx(&env);
        std::thread::sleep(std::time::Duration::from_millis(20));
        let r = refresh(&ctx, RefreshMode::Report, &root(&env)).unwrap();

        assert!(
            matches!(r.entries[0].action, RefreshAction::OutOfRoot),
            "unexpected action: {:?}",
            r.entries[0].action
        );
        assert!(!r.touched_any);
        assert_eq!(env.fs.modified(&outside).unwrap(), before);
    }

    /// A baseline written before the cache recorded source paths stores
    /// an empty one. Nothing was looked for and nothing is missing — it
    /// is a stale cache entry, reported apart from a source that was
    /// deleted.
    #[test]
    fn a_baseline_without_a_source_path_is_reported_as_stale() {
        let env = TempEnvironment::builder().build();
        Baseline::build(
            std::path::Path::new(""),
            b"rendered",
            b"src",
            Some(""),
            None,
        )
        .write(
            env.fs.as_ref(),
            env.paths.as_ref(),
            "app",
            "preprocessed",
            "cfg.toml",
        )
        .unwrap();
        let deployed = env.paths.data_dir().join("packs/app/preprocessed/cfg.toml");
        write_file(&env, &deployed, b"rendered");

        let ctx = make_ctx(&env);
        let r = refresh(&ctx, RefreshMode::Report, &root(&env)).unwrap();

        assert!(
            matches!(r.entries[0].action, RefreshAction::StaleSource),
            "unexpected action: {:?}",
            r.entries[0].action
        );
        assert!(!r.touched_any);
    }

    /// `--list-paths` feeds watchers, so it must not hand out another
    /// root's paths either: the scope is the mutation set, not a
    /// rendering choice.
    #[test]
    fn list_paths_mode_reports_only_authorized_sources() {
        let env = TempEnvironment::builder().build();
        let other_root = second_root_dir(&env);
        let (_here, here_deployed) = stage_one(&env, "app", "cfg.toml.tmpl", b"rendered", b"src");
        let (_there, there_deployed) = stage_under(
            &env,
            &other_root,
            "other",
            "cfg.toml.tmpl",
            b"rendered",
            b"src",
        );
        env.fs
            .write_file(&here_deployed, b"rendered EDITED")
            .unwrap();
        env.fs
            .write_file(&there_deployed, b"rendered EDITED")
            .unwrap();

        let ctx = make_ctx(&env);
        let r = refresh(&ctx, RefreshMode::ListPaths, &root(&env)).unwrap();

        let touched: Vec<&str> = r
            .entries
            .iter()
            .filter(|e| matches!(e.action, RefreshAction::Touched))
            .map(|e| e.source_path.as_str())
            .collect();
        assert_eq!(touched.len(), 1, "unexpected touched set: {touched:?}");
        assert!(touched[0].contains("/dotfiles/app/"), "{touched:?}");
    }
}

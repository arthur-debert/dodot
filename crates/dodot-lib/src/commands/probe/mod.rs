//! `probe` command family — introspection subcommands.
//!
//! Today there are several: `summary` (bare `probe`), `deployment-map`,
//! `show-data-dir`, `app`, and the `shell-init` family
//! (`shell_init`/`_aggregate`/`_history`/`_filter`/`_errors`). All
//! variants serialize through a single [`ProbeResult`] enum that is
//! `#[serde(tag = "kind")]`-tagged; the matching Jinja template
//! branches on `kind` to pick the right section.

mod render;
mod shell_init;
mod types;

pub use render::humanize_bytes;
pub use shell_init::{
    format_unix_ts, humanize_us, shell_init, shell_init_aggregate, shell_init_errors,
    shell_init_filter, shell_init_history,
};
pub use types::{
    AppProbeEntry, AppProbeView, DeploymentDisplayEntry, ProbeResult, ProbeSubcommandInfo,
    ShellInitAggregateRow, ShellInitAggregateView, ShellInitErrorsView, ShellInitFilterRun,
    ShellInitFilterTarget, ShellInitFilterView, ShellInitGroup, ShellInitHistoryRow,
    ShellInitHistoryView, ShellInitRow, ShellInitView, TreeLine, DEFAULT_FILTER_RUNS,
    DEFAULT_HISTORY_LIMIT, DEFAULT_SHOW_DATA_DIR_DEPTH, PROBE_SUBCOMMANDS,
};

use crate::packs::orchestration::ExecutionContext;
use crate::probe::{collect_data_dir_tree, collect_deployment_map};
use crate::Result;

/// Render the bare `dodot probe` summary.
pub fn summary(ctx: &ExecutionContext) -> Result<ProbeResult> {
    Ok(ProbeResult::Summary {
        data_dir: ctx.paths.data_dir().display().to_string(),
        available: PROBE_SUBCOMMANDS.to_vec(),
    })
}

/// Render the deployment map for display.
///
/// Reads the current datastore state (not the on-disk TSV) so the
/// output is always fresh even if the user never ran `dodot up`.
pub fn deployment_map(ctx: &ExecutionContext) -> Result<ProbeResult> {
    let raw = collect_deployment_map(ctx.fs.as_ref(), ctx.paths.as_ref())?;
    let home = ctx.paths.home_dir();
    let entries = raw
        .into_iter()
        .map(|e| render::into_display_entry(e, home))
        .collect();

    Ok(ProbeResult::DeploymentMap {
        data_dir: ctx.paths.data_dir().display().to_string(),
        map_path: ctx.paths.deployment_map_path().display().to_string(),
        entries,
    })
}

/// `dodot probe app <pack>` — advisory introspection of macOS
/// app-support paths for a pack.
///
/// Walks the pack's `_app/<X>/...` matches, configured `force_app`
/// hits, and `[symlink.app_aliases]` entries; checks each candidate
/// folder against the on-disk app-support root, and (on macOS)
/// enriches with brew cask metadata and Spotlight bundle IDs.
///
/// `refresh = true` invalidates the brew cache for every cask token
/// matched against this pack, forcing a fresh `brew info` fetch.
///
/// Resolver state is not consulted — this is purely advisory display.
pub fn app(pack_name: &str, refresh: bool, ctx: &ExecutionContext) -> Result<ProbeResult> {
    use std::collections::BTreeSet;

    // Resolve pack: try display name, fall back to a *validated* raw
    // on-disk dir name. Untrusted CLI input (e.g. `dodot probe app
    // ..` or `dodot probe app foo/bar`) must never reach
    // `paths.pack_path`, which would let `read_dir` below traverse
    // outside the dotfiles root.
    let pack_dir = crate::packs::orchestration::resolve_pack_dir_name(pack_name, ctx)
        .unwrap_or_else(|_| {
            if is_single_normal_path_component(pack_name) {
                pack_name.to_string()
            } else {
                // Invalid path-like input — produce an empty-but-named
                // view rather than an error. `pack_dir == ""` is the
                // sentinel the rest of the function checks to skip
                // any filesystem traversal.
                String::new()
            }
        });
    let display_name = if pack_dir.is_empty() {
        pack_name.to_string()
    } else {
        crate::packs::display_name_for(&pack_dir).to_string()
    };
    let pack_config = if pack_dir.is_empty() {
        ctx.config_manager.root_config()?
    } else {
        match ctx
            .config_manager
            .config_for_pack(&ctx.paths.pack_path(&pack_dir))
        {
            Ok(c) => c,
            // Pack-level config is optional; fall back to root config so
            // alias/force_app entries declared at root still surface for
            // a pack that hasn't been created yet.
            Err(_) => ctx.config_manager.root_config()?,
        }
    };

    // Collect distinct folder names this pack would route to.
    //
    // Three sources:
    //   - `app_aliases[<pack>]` value
    //   - `force_app` entries that appear at the top of the pack's tree
    //   - `_app/<X>/` subdirectory names found by walking the pack
    let mut folders: Vec<(String, &'static str)> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    if let Some(alias) = pack_config.symlink.app_aliases.get(&display_name) {
        if seen.insert(alias.clone()) {
            folders.push((alias.clone(), "alias"));
        }
    }

    let pack_path = ctx.paths.pack_path(&pack_dir);
    if !pack_dir.is_empty() && ctx.fs.exists(&pack_path) {
        if let Ok(entries) = ctx.fs.read_dir(&pack_path) {
            for e in entries {
                if e.is_dir
                    && pack_config.symlink.force_app.iter().any(|f| f == &e.name)
                    && seen.insert(e.name.clone())
                {
                    folders.push((e.name.clone(), "force_app"));
                }
            }
            // _app/<X>/ subtree
            let app_dir = pack_path.join("_app");
            if ctx.fs.exists(&app_dir) {
                if let Ok(children) = ctx.fs.read_dir(&app_dir) {
                    for e in children {
                        if e.is_dir && seen.insert(e.name.clone()) {
                            folders.push((e.name.clone(), "_app/"));
                        }
                    }
                }
            }
        }
    }

    // On non-macOS we still produce a useful (if minimal) view: just
    // the list of folders and their existence under the *collapsed*
    // app-support root (= xdg). Skip the brew/mdls work entirely.
    let macos = cfg!(target_os = "macos");
    let app_support = ctx.paths.app_support_dir();
    let cache_dir = ctx.paths.probes_brew_cache_dir();

    // `--refresh` clears the entire brew probe cache before any
    // matching runs, so the next `brew info` call rehydrates fresh
    // data. Per-folder invalidation can't work here because the cache
    // is keyed by cask token (not folder name) — we don't know the
    // tokens until matching has already populated the cache.
    if refresh && macos {
        crate::probe::brew::invalidate_all_cache(&cache_dir, ctx.fs.as_ref());
    }

    let now = crate::probe::brew::now_secs_unix();
    let folder_names: Vec<String> = folders.iter().map(|(f, _)| f.clone()).collect();
    // The on-demand `dodot probe app` subcommand is allowed to
    // populate the cache, so cache_only=false. The matcher returns
    // the installed-token set so we don't re-run `brew list` below.
    let matches = if macos {
        crate::probe::brew::match_folders_to_installed_casks(
            &folder_names,
            ctx.command_runner.as_ref(),
            &cache_dir,
            now,
            ctx.fs.as_ref(),
            /*cache_only=*/ false,
        )
    } else {
        crate::probe::brew::InstalledCaskMatches::default()
    };

    let mut entries: Vec<AppProbeEntry> = Vec::new();
    let mut suggested: BTreeSet<String> = BTreeSet::new();

    for (folder, source_rule) in &folders {
        let target = app_support.join(folder);
        let target_exists = ctx.fs.exists(&target);
        let cask = matches.folder_to_token.get(folder).cloned();

        let mut app_bundle = None;
        let mut bundle_id = None;
        if macos {
            if let Some(token) = &cask {
                if let Ok(Some(info)) = crate::probe::brew::info_cask(
                    token,
                    &cache_dir,
                    now,
                    ctx.fs.as_ref(),
                    ctx.command_runner.as_ref(),
                ) {
                    app_bundle = info.app_bundle_name();
                    if let Some(bundle_name) = &app_bundle {
                        let app_path = std::path::PathBuf::from("/Applications").join(bundle_name);
                        bundle_id = crate::probe::macos_native::bundle_id(
                            &app_path,
                            ctx.command_runner.as_ref(),
                        );
                    }
                    for plist in info.preferences_plists() {
                        suggested.insert(plist);
                    }
                }
            }
        }

        entries.push(AppProbeEntry {
            folder: folder.clone(),
            target_path: render::display_path(&target, ctx.paths.home_dir()),
            target_exists,
            source_rule: (*source_rule).into(),
            cask,
            app_bundle,
            bundle_id,
        });
    }

    Ok(ProbeResult::App(AppProbeView {
        pack: display_name,
        macos,
        entries,
        suggested_adoptions: suggested.into_iter().collect(),
    }))
}

/// True iff `value` is a single, normal path component — no path
/// separators, no `.`/`..` components, not empty. Used as a
/// security guard before passing untrusted CLI input to
/// `Pather::pack_path` (which would otherwise let traversal escape
/// the dotfiles root via the resulting `read_dir` calls).
fn is_single_normal_path_component(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    let mut comps = std::path::Path::new(value).components();
    matches!(
        (comps.next(), comps.next()),
        (Some(std::path::Component::Normal(_)), None)
    )
}

/// Render the data-dir tree.
pub fn show_data_dir(ctx: &ExecutionContext, max_depth: usize) -> Result<ProbeResult> {
    let tree = collect_data_dir_tree(ctx.fs.as_ref(), ctx.paths.as_ref(), max_depth)?;
    let total_nodes = tree.count_nodes();
    let total_size = tree.total_size();
    let mut lines = Vec::new();
    render::flatten_tree(&tree, "", true, &mut lines, true);
    Ok(ProbeResult::ShowDataDir {
        data_dir: ctx.paths.data_dir().display().to_string(),
        lines,
        total_nodes,
        total_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_result_deployment_map_serialises_with_kind_tag() {
        let result = ProbeResult::DeploymentMap {
            data_dir: "/d".into(),
            map_path: "/d/deployment-map.tsv".into(),
            entries: Vec::new(),
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["kind"], "deployment-map");
        assert!(json["entries"].is_array());
    }

    #[test]
    fn probe_result_show_data_dir_serialises_with_kind_tag() {
        let result = ProbeResult::ShowDataDir {
            data_dir: "/d".into(),
            lines: Vec::new(),
            total_nodes: 1,
            total_size: 0,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["kind"], "show-data-dir");
        assert_eq!(json["total_nodes"], 1);
    }

    #[test]
    fn probe_subcommands_list_matches_variants() {
        // Failsafe: if we add a probe subcommand to the enum we should
        // add it to the summary list too. This assertion catches the
        // former getting ahead of the latter.
        let names: Vec<&str> = PROBE_SUBCOMMANDS.iter().map(|s| s.name).collect();
        assert!(names.contains(&"deployment-map"));
        assert!(names.contains(&"show-data-dir"));
    }
}

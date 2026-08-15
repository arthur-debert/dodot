//! `down` command — remove all deployed state for packs.
//!
//! Output rendering: same principle as `up` — for real removals, render
//! through `status::status()` so the per-file labels match what `dodot
//! status` would show. After `down`, files appear in their `Pending`
//! handler-specific form (`not in PATH`, `not sourced`, `pending`,
//! `never run`). The action itself is communicated via the message
//! line.
//!
//! Dry-run keeps the per-handler "would remove" rendering.
//!
//! Teardown removes the *live* destination symlinks a deployment
//! created (issue #225), not just the datastore state — otherwise every
//! `down` leaves dangling links in the user's config tree. See
//! [`orchestration::remove_live_user_links`] for the scoping rules.
//!
//! Besides packs discovered in the repo, `down` also removes *orphaned*
//! datastore state — state for packs deleted from the dotfiles root
//! since they were deployed (issue #255). A no-args `down` sweeps every
//! orphan; `down <pack>` accepts an orphaned pack name (dir or display
//! form) even though the repo scan no longer knows it. Orphans can't
//! render through `status` (there is no pack to show), so a real run
//! reports what it swept via the warnings channel; dry-run lists them
//! with the same "would remove" rows as normal packs. An orphan's live
//! links are unrecoverable (nothing left to replan) and stay behind.

use tracing::{debug, info};

use crate::commands::{handler_symbol, status, DisplayFile, DisplayPack, PackStatusResult};
use crate::handlers::HANDLER_SYMLINK;
use crate::packs;
use crate::packs::orchestration::{self, ExecutionContext};
use crate::probe;
use crate::shell;
use crate::Result;

/// Run the `down` command: remove all state for specified (or all) packs.
pub fn down(pack_filter: Option<&[String]>, ctx: &ExecutionContext) -> Result<PackStatusResult> {
    info!(dry_run = ctx.dry_run, "starting down command");

    // Orphaned state: datastore subtrees whose pack no longer exists in
    // the dotfiles root. Scanned before validation so `dodot down
    // <orphan>` is accepted instead of rejected as PackNotFound — the
    // repo scan can't know a deleted pack, but the datastore does.
    // (#255)
    let orphan_dirs = orchestration::scan_orphaned(ctx)?;
    let matches_orphan = |name: &str| {
        orphan_dirs
            .iter()
            .any(|d| d == name || packs::display_name_for(d) == name)
    };

    let mut warnings = Vec::new();
    if let Some(names) = pack_filter {
        let repo_names: Vec<String> = names
            .iter()
            .filter(|n| !matches_orphan(n))
            .cloned()
            .collect();
        warnings = orchestration::validate_pack_names(&repo_names, ctx)?;
    }

    let root_config = ctx.config_manager.root_config()?;
    let mut all_packs = packs::discover_packs(
        ctx.fs.as_ref(),
        ctx.paths.dotfiles_root(),
        &root_config.pack.ignore,
    )?;
    info!(count = all_packs.len(), "discovered packs");

    if let Some(names) = pack_filter {
        all_packs.retain(|p| names.iter().any(|n| n == &p.display_name || n == &p.name));
    }

    // Discover `.dodotignore`-marked packs so `down` reports them in the
    // same ignored rows `status` shows and sweeps any stale
    // datastore state they left behind. (issue #222)
    let ignored = orchestration::scan_ignored(pack_filter, ctx)?;

    let mut affected_packs = Vec::new();
    let mut dry_run_display: Vec<DisplayPack> = Vec::new();
    let mut any_removed = false;

    for pack in &all_packs {
        // Datastore is keyed by the on-disk directory name, not the
        // display name — the directory `010-nvim` keeps its `010-nvim/`
        // subtree in the datastore.
        let handlers = ctx.datastore.list_pack_handlers(&pack.name)?;

        if handlers.is_empty() {
            debug!(pack = %pack.display_name, "already down, skipping");
            continue;
        }

        info!(pack = %pack.display_name, handlers = ?handlers, "removing pack state");
        any_removed = true;
        affected_packs.push(pack.display_name.clone());

        if ctx.dry_run {
            dry_run_display.push(build_dry_run_display(
                &pack.name,
                &pack.display_name,
                &handlers,
                ctx,
            )?);
        } else {
            // Live destination symlinks first, while the datastore
            // paths they point at still identify them as ours (#225).
            orchestration::remove_live_user_links(pack, ctx)?;
            for handler in &handlers {
                ctx.datastore.remove_state(&pack.name, handler)?;
            }
        }
    }

    // Remove orphaned state (#255). Unfiltered for a no-args run —
    // `down` removes *all* deployed state, including state for packs
    // the repo has since deleted. A filtered run only touches orphans
    // named in the filter (scoped intent); any orphan left behind is
    // warned about below so the user knows a plain `dodot down` clears
    // it. Orphans can't render through the post-removal status pass
    // (there is no pack in the repo to show), so a real run reports
    // them via the warnings channel instead.
    let selected_orphans: Vec<String> = match pack_filter {
        None => orphan_dirs.clone(),
        Some(names) => orphan_dirs
            .iter()
            .filter(|d| {
                names
                    .iter()
                    .any(|n| n == *d || n == packs::display_name_for(d))
            })
            .cloned()
            .collect(),
    };
    for dir in &selected_orphans {
        info!(pack = %dir, "removing orphaned pack state");
        any_removed = true;
        if ctx.dry_run {
            let handlers = ctx.datastore.list_pack_handlers(dir)?;
            dry_run_display.push(build_dry_run_display(
                dir,
                packs::display_name_for(dir),
                &handlers,
                ctx,
            )?);
        }
    }
    if !ctx.dry_run && !selected_orphans.is_empty() {
        orchestration::sweep_pack_state(&selected_orphans, ctx)?;
        warnings.push(format!(
            "removed state for {} no longer in {}: {}",
            if selected_orphans.len() == 1 {
                "pack"
            } else {
                "packs"
            },
            ctx.paths.dotfiles_root().display(),
            display_names(&selected_orphans),
        ));
    }
    let remaining_orphans: Vec<String> = orphan_dirs
        .iter()
        .filter(|d| !selected_orphans.contains(d))
        .cloned()
        .collect();
    if !remaining_orphans.is_empty() {
        warnings.push(orchestration::orphan_warning(&remaining_orphans, ctx));
    }

    // Sweep stale state for now-ignored packs so the regenerated
    // (global) init script stops sourcing them. A pack deployed before
    // it was ignored would otherwise linger in the datastore. Unfiltered
    // — the init script covers every pack regardless of this run's
    // filter. (#222) Counts as removal so the message reflects that
    // something was deactivated. The count is computed in both dry-run
    // and real runs so `down --dry-run` reports the same outcome a real
    // run would produce; only the mutation is gated on `!dry_run`.
    if orchestration::packs_with_state(&ignored.sweep_dir_names, ctx)? > 0 {
        any_removed = true;
    }
    if !ctx.dry_run {
        orchestration::sweep_pack_state(&ignored.sweep_dir_names, ctx)?;
    }

    if !ctx.dry_run {
        info!("regenerating shell init script");
        shell::write_init_script(
            ctx.fs.as_ref(),
            ctx.paths.as_ref(),
            root_config.profiling.enabled,
        )?;
        info!("writing deployment map");
        probe::write_deployment_map(ctx.fs.as_ref(), ctx.paths.as_ref())?;
    }

    let display_packs = if ctx.dry_run {
        dry_run_display
    } else {
        // Render through status — files for removed packs will now show as
        // pending in their handler-specific vocabulary.
        status::status(Some(&affected_packs), ctx)?.packs
    };

    let message = if any_removed {
        "Packs deactivated."
    } else {
        "Nothing to deactivate."
    };

    Ok(PackStatusResult {
        message: Some(message.into()),
        dry_run: ctx.dry_run,
        packs: display_packs,
        warnings,
        notes: Vec::new(),
        conflicts: Vec::new(),
        ignored_packs: ignored.display_packs,
        inactive_packs: Vec::new(),
        view_mode: ctx.view_mode.as_str().into(),
        group_mode: ctx.group_mode.as_str().into(),
        diffs: Vec::new(),
        // `down` deactivates; nagging about shell hookup on the way
        // out is advice for a deployment the user just removed.
        shell_hookup: None,
    })
}

/// Render the given orphaned pack dir names as user-facing display
/// names, comma-joined, for the warnings channel.
fn display_names(dirs: &[String]) -> String {
    dirs.iter()
        .map(|d| packs::display_name_for(d))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Build the per-pack dry-run display: lists what would be removed,
/// per-handler. For symlink handlers we list individual data-link entries
/// since the user usually wants to know which files would be affected.
///
/// Takes the datastore key (`dir_name`) and user-facing name separately
/// so orphaned packs — which have no repo-side [`packs::Pack`] — render
/// the same way as discovered ones.
fn build_dry_run_display(
    dir_name: &str,
    display_name: &str,
    handlers: &[String],
    ctx: &ExecutionContext,
) -> Result<DisplayPack> {
    let mut files = Vec::new();
    for handler in handlers {
        if handler == HANDLER_SYMLINK {
            let handler_dir = ctx.paths.handler_data_dir(dir_name, handler);
            let entries = ctx.fs.read_dir(&handler_dir)?;
            for entry in entries {
                files.push(DisplayFile {
                    name: entry.name.clone(),
                    symbol: handler_symbol(handler).into(),
                    description: "state would be removed".into(),
                    status: "pending".into(),
                    status_label: "[dry-run] would remove".into(),
                    handler: handler.clone(),
                    note_ref: None,
                });
            }
        } else {
            files.push(DisplayFile {
                name: handler.clone(),
                symbol: handler_symbol(handler).into(),
                description: "state would be removed".into(),
                status: "pending".into(),
                status_label: "[dry-run] would remove".into(),
                handler: handler.clone(),
                note_ref: None,
            });
        }
    }
    Ok(DisplayPack::new(display_name.to_string(), files))
}

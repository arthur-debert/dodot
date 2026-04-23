//! `status` command — shows current deployment state with chain verification.
//!
//! For each file, status verifies the actual filesystem state rather than
//! just checking whether datastore directories exist. This catches broken
//! symlinks, missing source files, and config drift.
//!
//! Additionally, status performs cross-pack conflict detection and surfaces
//! potential conflicts as warnings — even for packs that aren't deployed
//! yet. This lets users see problems before they run `up`.

use tracing::{debug, info};

use crate::commands::{
    handler_description, handler_symbol, DisplayConflict, DisplayFile, DisplayPack,
    PackStatusResult,
};
use crate::config::mappings_to_rules;
use crate::conflicts;
use crate::handlers::symlink::resolve_target;
use crate::handlers::{self, HANDLER_SYMLINK};
use crate::packs::orchestration::{self, ExecutionContext};
use crate::packs::{self};
use crate::rules::Scanner;
use crate::Result;

/// Deployment health for a single file, determined by chain verification.
enum Health {
    /// Not deployed (no data link in datastore).
    Pending,
    /// Not deployed AND a non-dodot file/symlink already occupies the
    /// target path — `dodot up` would fail without `--force`. The reason
    /// is rendered as a footnote so the right column stays compact.
    PendingConflict { reason: String },
    /// Deployed and all links verified correct.
    Deployed,
    /// Deployed but the chain is broken.
    Broken(String),
    /// Data link exists and is healthy, but the user link is not at the
    /// path that current config would produce. A re-deploy would move it.
    Stale(String),
}

impl Health {
    /// Style name for standout template tag matching.
    fn style(&self) -> &'static str {
        match self {
            Health::Pending => "pending",
            Health::PendingConflict { .. } => "warning",
            Health::Deployed => "deployed",
            Health::Broken(_) => "broken",
            Health::Stale(_) => "stale",
        }
    }

    /// Human-readable label for display.
    fn label(&self, handler: &str) -> String {
        match self {
            Health::Pending | Health::PendingConflict { .. } => match handler {
                "symlink" => "pending".into(),
                "shell" => "not sourced".into(),
                "path" => "not in PATH".into(),
                "install" => "never run".into(),
                "homebrew" => "not installed".into(),
                _ => "pending".into(),
            },
            Health::Deployed => match handler {
                "symlink" => "deployed".into(),
                "shell" => "sourced".into(),
                "path" => "in PATH".into(),
                "install" => "installed".into(),
                "homebrew" => "installed".into(),
                _ => "deployed".into(),
            },
            Health::Broken(reason) => reason.clone(),
            Health::Stale(reason) => reason.clone(),
        }
    }

    /// If this health represents a pending conflict, return the reason
    /// (suitable for use as a footnote). `None` otherwise.
    fn footnote_reason(&self) -> Option<&str> {
        match self {
            Health::PendingConflict { reason } => Some(reason.as_str()),
            _ => None,
        }
    }
}

/// Build the footnote text for a non-symlink file or directory that
/// already occupies the user-target path and would block `dodot up`.
///
/// The conflict definition here matches the executor's pre-check
/// (`execution/mod.rs`): `dodot up` only refuses when the user-target
/// is a non-symlink that exists. Existing symlinks (correct, dangling,
/// or pointing elsewhere) are gracefully replaced by `create_user_link`
/// and are *not* conflicts. Caller must verify those conditions before
/// calling this helper.
fn describe_blocking_target(
    user_target: &std::path::Path,
    fs: &dyn crate::fs::Fs,
    home: &std::path::Path,
) -> String {
    let display = if let Ok(rel) = user_target.strip_prefix(home) {
        format!("~/{}", rel.display())
    } else {
        user_target.display().to_string()
    };
    let kind = if fs.is_dir(user_target) {
        "directory"
    } else {
        "file"
    };
    format!("{display} (existing {kind}) — `dodot up` will refuse without `--force`")
}

/// Verify symlink handler chain for a single file.
///
/// Checks: data link exists → points to source → source exists →
/// user link exists at resolve_target → points to data link.
fn verify_symlink(
    source: &std::path::Path,
    pack: &str,
    rel_path: &str,
    is_dir: bool,
    config: &crate::handlers::HandlerConfig,
    ctx: &ExecutionContext,
) -> Health {
    let filename = match source.file_name() {
        Some(f) => f,
        None => return Health::Pending,
    };

    let data_link = ctx
        .paths
        .handler_data_dir(pack, HANDLER_SYMLINK)
        .join(filename);

    // Step 1: Does the data link exist and is it a symlink?
    if !ctx.fs.is_symlink(&data_link) {
        if ctx.fs.exists(&data_link) {
            return Health::Broken("broken: data link exists but is not a symlink".into());
        }
        // No data link yet. Before declaring plain "pending", peek at the
        // user-target path: if a non-symlink file or directory already
        // lives there, `dodot up` will refuse without `--force` — surface
        // that as a conflict-aware pending so the user sees it before
        // running up.
        //
        // Symlinks at the target are NOT conflicts: the executor's
        // create_user_link gracefully replaces them (correct ones are
        // left alone, wrong/dangling ones are removed and recreated). A
        // dangling symlink left over from `dodot down` is the canonical
        // case — flagging it as a conflict would be a false positive.
        //
        // #44: a non-symlink file whose content is byte-identical to the
        // source is also NOT a conflict — the executor will auto-replace
        // it without `--force`. Stay plain `pending` for that case.
        let user_target = resolve_target(pack, rel_path, is_dir, config, ctx.paths.as_ref());
        if !ctx.fs.is_symlink(&user_target) && ctx.fs.exists(&user_target) {
            if crate::equivalence::is_equivalent(&user_target, source, ctx.fs.as_ref()) {
                return Health::Pending;
            }
            let reason =
                describe_blocking_target(&user_target, ctx.fs.as_ref(), ctx.paths.home_dir());
            return Health::PendingConflict { reason };
        }
        return Health::Pending;
    }

    // Step 2: Does data link point to the correct source?
    match ctx.fs.readlink(&data_link) {
        Ok(target) if target == source => {}
        Ok(target) => {
            return Health::Broken(format!("broken: data link points to {}", target.display()));
        }
        Err(_) => return Health::Broken("broken: cannot read data link".into()),
    }

    // Step 3: Does the source file still exist?
    if !ctx.fs.exists(source) {
        return Health::Broken("broken: source file missing".into());
    }

    // Step 4: Check user link at the currently-resolved target
    let user_target = resolve_target(pack, rel_path, is_dir, config, ctx.paths.as_ref());

    if ctx.fs.is_symlink(&user_target) {
        match ctx.fs.readlink(&user_target) {
            Ok(link_target) if link_target == data_link => {
                // Full chain verified
                Health::Deployed
            }
            Ok(_) => {
                // User link exists but points elsewhere (another pack, manual link, etc.)
                Health::Stale("stale: user link points elsewhere, re-deploy to fix".into())
            }
            Err(_) => Health::Broken("broken: cannot read user link".into()),
        }
    } else if ctx.fs.exists(&user_target) {
        // Non-symlink file at target. If its content is byte-identical to
        // the source, `up` will auto-replace it (#44) — surface as Stale
        // (re-deploy fixes), not Broken. Otherwise it's a real conflict.
        if crate::equivalence::is_equivalent(&user_target, source, ctx.fs.as_ref()) {
            Health::Stale("stale: user link missing, re-deploy to fix".into())
        } else {
            Health::Broken("conflict: non-symlink file at target path".into())
        }
    } else {
        // No user link — data link exists but user link missing.
        // This happens when config changed (drift) or deployment was interrupted.
        Health::Stale("stale: user link missing, re-deploy to fix".into())
    }
}

/// Verify shell/path handler chain for a single file.
///
/// Checks: data link exists → points to source → source exists.
fn verify_staged(
    source: &std::path::Path,
    pack: &str,
    handler: &str,
    ctx: &ExecutionContext,
) -> Health {
    let filename = match source.file_name() {
        Some(f) => f,
        None => return Health::Pending,
    };

    let data_link = ctx.paths.handler_data_dir(pack, handler).join(filename);

    if !ctx.fs.is_symlink(&data_link) {
        if ctx.fs.exists(&data_link) {
            return Health::Broken("broken: data link exists but is not a symlink".into());
        }
        return Health::Pending;
    }

    match ctx.fs.readlink(&data_link) {
        Ok(target) if target == source => {}
        Ok(target) => {
            return Health::Broken(format!("broken: data link points to {}", target.display()));
        }
        Err(_) => return Health::Broken("broken: cannot read data link".into()),
    }

    if !ctx.fs.exists(source) {
        return Health::Broken("broken: source file missing".into());
    }

    Health::Deployed
}

/// Run the `status` command: scan packs and verify deployment chain per file.
///
/// Also performs cross-pack conflict detection and surfaces potential
/// conflicts as warnings.
pub fn status(pack_filter: Option<&[String]>, ctx: &ExecutionContext) -> Result<PackStatusResult> {
    info!("starting status command");

    // Validate pack names before doing anything
    let mut warnings = Vec::new();
    if let Some(names) = pack_filter {
        warnings = orchestration::validate_pack_names(names, ctx)?;
    }

    let root_config = ctx.config_manager.root_config()?;
    let packs::DiscoveredPacks {
        packs: mut all_packs,
        ignored: mut ignored_packs,
    } = packs::scan_packs(
        ctx.fs.as_ref(),
        ctx.paths.dotfiles_root(),
        &root_config.pack.ignore,
    )?;
    info!(count = all_packs.len(), "discovered packs");

    if let Some(names) = pack_filter {
        all_packs.retain(|p| names.iter().any(|n| n == &p.name));
        ignored_packs.retain(|name| names.iter().any(|n| n == name));
    }

    let registry = handlers::create_registry(ctx.fs.as_ref());
    let mut display_packs = Vec::new();

    // Collect intents across all packs for conflict detection
    let mut pack_intents = Vec::new();

    for mut pack in all_packs {
        info!(pack = %pack.name, "checking pack status");
        let pack_config = ctx.config_manager.config_for_pack(&pack.path)?;
        pack.config = pack_config.to_handler_config();
        let rules = mappings_to_rules(&pack_config.mappings);

        let scanner = Scanner::new(ctx.fs.as_ref());

        // Walk and preprocess so the status display sees *post-preprocessing*
        // filenames (e.g. `config.toml` rather than `config.toml.tmpl`).
        // Without this step, status reports templates under their source
        // name and wrongly marks them "pending" because the verification
        // path (`~/.config.toml.tmpl`) doesn't exist.
        let entries = scanner.walk_pack(&pack.path, &pack_config.pack.ignore)?;
        let preprocess_result = if pack_config.preprocessor.enabled {
            let registry = crate::preprocessing::default_registry(
                &pack_config.preprocessor.template,
                ctx.paths.as_ref(),
            )?;
            if !registry.is_empty() {
                match crate::preprocessing::pipeline::preprocess_pack(
                    entries,
                    &registry,
                    &pack,
                    ctx.fs.as_ref(),
                    ctx.datastore.as_ref(),
                ) {
                    Ok(r) => r,
                    Err(err) => {
                        // Preprocessing failure surfaces as a warning; we
                        // still want to show whatever we can from the
                        // intent-collection attempt below.
                        warnings.push(format!(
                            "preprocessing failed for pack '{}': {}",
                            pack.name, err
                        ));
                        crate::preprocessing::pipeline::PreprocessResult::passthrough(Vec::new())
                    }
                }
            } else {
                crate::preprocessing::pipeline::PreprocessResult::passthrough(entries)
            }
        } else {
            crate::preprocessing::pipeline::PreprocessResult::passthrough(entries)
        };
        let all_entries = preprocess_result.merged_entries();
        let matches = scanner.match_entries(&all_entries, &rules, &pack.name);

        // Collect intents for conflict detection
        match orchestration::collect_pack_intents(&pack, ctx) {
            Ok(intents) => {
                pack_intents.push((pack.name.clone(), intents));
            }
            Err(err) => {
                warnings.push(format!(
                    "could not collect intents for pack '{}'; conflict detection may be incomplete: {}",
                    pack.name, err
                ));
            }
        }

        let mut files = Vec::new();
        let mut footnotes: Vec<String> = Vec::new();
        for m in &matches {
            let rel_str = m.relative_path.to_string_lossy().into_owned();

            // Per-file chain verification based on handler type
            let health = match m.handler.as_str() {
                "symlink" => verify_symlink(
                    &m.absolute_path,
                    &pack.name,
                    &rel_str,
                    m.is_dir,
                    &pack.config,
                    ctx,
                ),
                "shell" | "path" => verify_staged(&m.absolute_path, &pack.name, &m.handler, ctx),
                _ => {
                    // install, homebrew — use existing handler check_status
                    let handler = registry.get(m.handler.as_str());
                    let deployed = handler
                        .and_then(|h| {
                            h.check_status(&m.absolute_path, &pack.name, ctx.datastore.as_ref())
                                .ok()
                        })
                        .map(|s| s.deployed)
                        .unwrap_or(false);
                    if deployed {
                        Health::Deployed
                    } else {
                        Health::Pending
                    }
                }
            };

            // Compute actual target path for symlink handler display
            let user_target = if m.handler == HANDLER_SYMLINK {
                let target = resolve_target(
                    &pack.name,
                    &rel_str,
                    m.is_dir,
                    &pack.config,
                    ctx.paths.as_ref(),
                );
                let home = ctx.paths.home_dir();
                let display = if let Ok(rel) = target.strip_prefix(home) {
                    format!("~/{}", rel.display())
                } else {
                    target.display().to_string()
                };
                Some(display)
            } else {
                None
            };

            let mut status_label = health.label(&m.handler);
            // For PendingConflict, append the next per-pack footnote ID to
            // the right-column label and stash the reason in the pack's
            // footnotes vec.
            if let Some(reason) = health.footnote_reason() {
                let footnote_id = footnotes.len() + 1;
                status_label = format!("{status_label} ({footnote_id})");
                footnotes.push(reason.to_string());
            }
            files.push(DisplayFile {
                name: rel_str.clone(),
                symbol: handler_symbol(&m.handler).into(),
                description: handler_description(&m.handler, &rel_str, user_target.as_deref()),
                status: health.style().into(),
                status_label,
                handler: m.handler.clone(),
            });
        }

        display_packs.push(DisplayPack {
            name: pack.name.clone(),
            files,
            footnotes,
        });
    }

    // Detect and surface cross-pack conflicts as structured display data
    let detected_conflicts = conflicts::detect_cross_pack_conflicts(&pack_intents, ctx.fs.as_ref());
    let home = ctx.paths.home_dir();
    let display_conflicts: Vec<DisplayConflict> = detected_conflicts
        .iter()
        .map(|c| DisplayConflict::from_conflict(c, home))
        .collect();
    if !display_conflicts.is_empty() {
        info!(
            count = display_conflicts.len(),
            "cross-pack conflicts detected"
        );
    } else {
        debug!("no cross-pack conflicts");
    }

    Ok(PackStatusResult {
        message: None,
        dry_run: false,
        packs: display_packs,
        warnings,
        conflicts: display_conflicts,
        ignored_packs,
    })
}

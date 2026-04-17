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
    handler_description, handler_symbol, DisplayFile, DisplayPack, PackStatusResult,
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
            Health::Deployed => "deployed",
            Health::Broken(_) => "broken",
            Health::Stale(_) => "stale",
        }
    }

    /// Human-readable label for display.
    fn label(&self, handler: &str) -> String {
        match self {
            Health::Pending => match handler {
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
}

/// Verify symlink handler chain for a single file.
///
/// Checks: data link exists → points to source → source exists →
/// user link exists at resolve_target → points to data link.
fn verify_symlink(
    source: &std::path::Path,
    pack: &str,
    rel_path: &str,
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
    let user_target = resolve_target(rel_path, config, ctx.paths.as_ref());

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
        // Regular file at target — conflict
        Health::Broken("conflict: non-symlink file at target path".into())
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

/// Format cross-pack conflict warnings for status output.
fn conflict_warnings(conflicts: &[conflicts::Conflict], home: &std::path::Path) -> Vec<String> {
    let mut warnings = Vec::new();
    if conflicts.is_empty() {
        return warnings;
    }

    warnings.push("cross-pack conflicts detected:".into());
    for c in conflicts {
        let target_display = if let Ok(rel) = c.target.strip_prefix(home) {
            format!("~/{}", rel.display())
        } else {
            c.target.display().to_string()
        };
        warnings.push(format!("  target: {target_display}"));
        for claimant in &c.claimants {
            warnings.push(format!(
                "    - pack '{}' ({} handler): {}",
                claimant.pack,
                claimant.handler,
                claimant.source.display()
            ));
        }
    }
    warnings.push(
        "fix your configuration — `dodot up` will refuse to deploy until conflicts are resolved."
            .into(),
    );

    warnings
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
    let mut all_packs = packs::discover_packs(
        ctx.fs.as_ref(),
        ctx.paths.dotfiles_root(),
        &root_config.pack.ignore,
    )?;
    info!(count = all_packs.len(), "discovered packs");

    if let Some(names) = pack_filter {
        all_packs.retain(|p| names.iter().any(|n| n == &p.name));
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
        for m in &matches {
            // Skip directory entries for symlink handler — only show leaf files (#11)
            // Keep directory entries for other handlers (e.g. path handler uses bin/ dirs)
            if m.is_dir && m.handler == HANDLER_SYMLINK {
                continue;
            }

            let rel_str = m.relative_path.to_string_lossy().into_owned();

            // Per-file chain verification based on handler type
            let health = match m.handler.as_str() {
                "symlink" => {
                    verify_symlink(&m.absolute_path, &pack.name, &rel_str, &pack.config, ctx)
                }
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
                let target = resolve_target(&rel_str, &pack.config, ctx.paths.as_ref());
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

            files.push(DisplayFile {
                name: rel_str.clone(),
                symbol: handler_symbol(&m.handler).into(),
                description: handler_description(&m.handler, &rel_str, user_target.as_deref()),
                status: health.style().into(),
                status_label: health.label(&m.handler),
                handler: m.handler.clone(),
            });
        }

        display_packs.push(DisplayPack {
            name: pack.name.clone(),
            files,
        });
    }

    // Detect and surface cross-pack conflicts as warnings
    let detected_conflicts = conflicts::detect_cross_pack_conflicts(&pack_intents, ctx.fs.as_ref());
    if !detected_conflicts.is_empty() {
        info!(
            count = detected_conflicts.len(),
            "cross-pack conflicts detected"
        );
        let home = ctx.paths.home_dir();
        warnings.extend(conflict_warnings(&detected_conflicts, home));
    } else {
        debug!("no cross-pack conflicts");
    }

    Ok(PackStatusResult {
        message: None,
        dry_run: false,
        packs: display_packs,
        warnings,
    })
}

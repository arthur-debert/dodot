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
    handler_description, handler_symbol, DisplayConflict, DisplayFile, DisplayNote, DisplayPack,
    PackStatusResult,
};
use crate::config::mappings_to_rules;
use crate::conflicts;
use crate::handlers::symlink::resolve_target;
use crate::handlers::{self, HANDLER_GATE, HANDLER_IGNORE, HANDLER_SKIP, HANDLER_SYMLINK};
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
    /// Deployed and the chain is healthy, but the file has a known
    /// quality issue: pre-flight syntax error or recurring runtime
    /// failures observed by the profiling instrumentation. `label` is
    /// the short status-column text, `reason` is the footnote body.
    DeployedWithError { label: String, reason: String },
    /// Deployed but the chain is broken.
    Broken(String),
    /// Data link exists and is healthy, but the user link is not at the
    /// path that current config would produce. A re-deploy would move it.
    Stale(String),
    /// File matched the `mappings.skip` list (README, LICENSE, …).
    /// No handler runs on it, but it surfaces in status so users can see
    /// the rule applied rather than wondering why the file is "missing."
    Skipped,
    /// File carries a gate label whose predicate evaluates false on this
    /// host (e.g. `install._linux.sh` on macOS). The file is not deployed
    /// but is surfaced so users see the rule applied. The footnote shows
    /// the label, what the predicate expected, and what the host has.
    Gated {
        label: String,
        expected: String,
        actual: String,
    },
}

impl Health {
    /// Style name for standout template tag matching.
    fn style(&self) -> &'static str {
        match self {
            Health::Pending => "pending",
            Health::PendingConflict { .. } => "warning",
            Health::Deployed => "deployed",
            Health::DeployedWithError { .. } => "broken",
            Health::Broken(_) => "broken",
            Health::Stale(_) => "stale",
            Health::Skipped => "skipped",
            Health::Gated { .. } => "skipped",
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
            Health::DeployedWithError { label, .. } => label.clone(),
            Health::Broken(reason) => reason.clone(),
            Health::Stale(reason) => reason.clone(),
            Health::Skipped => "skipped".into(),
            Health::Gated { label, .. } => format!("gated out ({label})"),
        }
    }

    /// If this health carries a footnote-worthy reason (pending conflict,
    /// deployed-with-error), return it. `None` otherwise.
    fn footnote_reason(&self) -> Option<String> {
        match self {
            Health::PendingConflict { reason } => Some(reason.clone()),
            Health::DeployedWithError { reason, .. } => Some(reason.clone()),
            Health::Gated {
                expected, actual, ..
            } => Some(format!("predicate {expected}; host {actual}")),
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
        let user_target = resolve_target(pack, rel_path, config, ctx.paths.as_ref());
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
    let user_target = resolve_target(pack, rel_path, config, ctx.paths.as_ref());

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
/// For shell handler, also checks for a syntax-error sidecar from the
/// pre-flight check that runs at `dodot up` time — its presence flips
/// a healthy chain to `DeployedWithError`.
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

    // Shell handler only: layered post-deploy quality checks. Both
    // signals fire only for shell sources. Syntax error wins over
    // runtime failures — fix-the-parse is the more fundamental issue,
    // and a file that doesn't parse can't have meaningful runtime
    // exit-code data anyway.
    if handler == "shell" {
        let filename_str = filename.to_string_lossy();

        // (1) Pre-flight syntax-error sidecar from `dodot up`.
        let sidecar = crate::shell::error_sidecar_path(ctx.paths.as_ref(), pack, &filename_str);
        if ctx.fs.exists(&sidecar) {
            if let Ok(body) = ctx.fs.read_to_string(&sidecar) {
                let reason = body.trim().to_string();
                if !reason.is_empty() {
                    return Health::DeployedWithError {
                        label: "syntax error".into(),
                        reason,
                    };
                }
            }
        }

        // (2) Runtime exit-code data from the profiling instrumentation,
        //     if any has been collected. Returns None when profiling is
        //     off, when no profiles exist yet, or when this source has
        //     run cleanly in every recent shell.
        if let Some((label, reason)) = recent_runtime_failures(source, pack, &filename_str, ctx) {
            return Health::DeployedWithError { label, reason };
        }
    }

    Health::Deployed
}

/// How many recent shell-init profiles to scan for runtime failures.
/// Five is enough to catch the "fails sometimes" case without making
/// status I/O-heavy; users who want deeper history reach for
/// `dodot probe shell-init --history`.
const RUNTIME_FAILURE_WINDOW: usize = 5;

/// Maximum number of stderr characters to inline into the status
/// footnote. Long stderr (stack traces, dumps) is truncated with an
/// ellipsis; the user can run `dodot probe shell-init <pack>/<file>`
/// for the full text.
const STATUS_STDERR_BUDGET: usize = 240;

/// Look at the last few shell-init profiles for any non-zero exit
/// status from `source`. Returns `Some((short_label, footnote_body))`
/// if at least one failure was seen; `None` otherwise (including when
/// profiling is off, so no profiles exist).
///
/// When the most recent failing run also has stderr captured (in its
/// sibling `errors.log`), the footnote inlines a trimmed excerpt so
/// the user sees the actual error message without having to chase
/// down a separate command. The pointer at the bottom of the footnote
/// directs them to the per-file probe view for the full picture.
fn recent_runtime_failures(
    source: &std::path::Path,
    pack: &str,
    filename: &str,
    ctx: &ExecutionContext,
) -> Option<(String, String)> {
    let profiles = crate::probe::shell_init::read_recent_profiles(
        ctx.fs.as_ref(),
        ctx.paths.as_ref(),
        RUNTIME_FAILURE_WINDOW,
    )
    .ok()?;
    if profiles.is_empty() {
        return None;
    }
    let target_str = source.to_string_lossy();

    // `profiles` is newest-first. We want the *most recent* non-zero
    // exit for the label/footnote — set on the first failure we see
    // and never overwritten. (An older failure is irrelevant to the
    // user's current understanding of the file's state.)
    let mut runs_seen = 0;
    let mut runs_failed = 0;
    let mut last_failure_exit: Option<i32> = None;
    let mut last_failure_stderr: Option<String> = None;
    for profile in &profiles {
        if let Some(entry) = profile
            .entries
            .iter()
            .find(|e| e.phase == "source" && e.target == target_str)
        {
            runs_seen += 1;
            if entry.exit_status != 0 {
                runs_failed += 1;
                if last_failure_exit.is_none() {
                    last_failure_exit = Some(entry.exit_status);
                    // Pull the matching stderr record, if the run has
                    // an errors.log sibling. Pre-stderr-capture profiles
                    // and clean-stderr failures both yield None here.
                    last_failure_stderr = profile
                        .errors
                        .iter()
                        .find(|er| er.target == target_str)
                        .map(|er| er.message.trim_end().to_string())
                        .filter(|s| !s.is_empty());
                }
            }
        }
    }
    let last_exit = last_failure_exit?;
    if runs_failed == 0 {
        return None;
    }

    let label = format!("exited {last_exit} ({runs_failed}/{runs_seen})");
    let mut reason = format!(
        "non-zero exit in {runs_failed} of {runs_seen} recent shell startups (last failure: exit {last_exit})."
    );
    if let Some(stderr) = last_failure_stderr {
        reason.push_str(" stderr: ");
        reason.push_str(&truncate_for_footnote(&stderr, STATUS_STDERR_BUDGET));
    }
    reason.push_str(&format!(
        " Run `dodot probe shell-init {pack}/{filename}` for per-run history and full stderr."
    ));
    Some((label, reason))
}

/// Trim multi-line stderr to a single-line excerpt that fits the
/// footnote budget. Newlines become `↵` so the user sees a hint that
/// more lines exist; over-long messages get an ellipsis.
fn truncate_for_footnote(stderr: &str, budget: usize) -> String {
    let one_line = stderr.replace('\n', " ↵ ");
    if one_line.chars().count() <= budget {
        return one_line;
    }
    let truncated: String = one_line.chars().take(budget).collect();
    format!("{truncated}…")
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
        all_packs.retain(|p| names.iter().any(|n| n == &p.display_name || n == &p.name));
        ignored_packs.retain(|name| {
            names
                .iter()
                .any(|n| n == name || n == crate::packs::display_name_for(name))
        });
    }

    let registry = handlers::create_registry(ctx.fs.as_ref());
    let host = ctx.host_facts.as_ref();
    let mut display_packs = Vec::new();
    let mut notes: Vec<DisplayNote> = Vec::new();
    let mut inactive_packs: Vec<String> = Vec::new();

    // Collect intents across all packs for conflict detection
    let mut pack_intents = Vec::new();

    for mut pack in all_packs {
        info!(pack = %pack.display_name, "checking pack status");
        let pack_config = ctx.config_manager.config_for_pack(&pack.path)?;
        pack.config = pack_config.to_handler_config();

        // C3: pack-level OS gate. Inactive packs surface in their own
        // section ("inactive on this OS") and skip the per-file
        // walk/preprocess/match cycle entirely.
        if !crate::gates::pack_os_active(&pack_config.pack.os, host) {
            inactive_packs.push(format!(
                "{} (os={}, current={})",
                pack.display_name,
                pack_config.pack.os.join(","),
                host.os
            ));
            continue;
        }
        let rules = mappings_to_rules(&pack_config.mappings);

        let scanner = Scanner::new(ctx.fs.as_ref());

        // Build gate state once per pack — used by both the
        // walk (directory-segment gates) and match_entries (basename
        // gates). Reusing the value keeps the two passes consistent.
        let gates = {
            let mut t = crate::gates::GateTable::with_builtins();
            if !pack_config.gates.is_empty() {
                t.merge_user(&pack_config.gates)?;
            }
            t
        };

        // Walk and preprocess so the status display sees *post-preprocessing*
        // filenames (e.g. `config.toml` rather than `config.toml.tmpl`).
        // Without this step, status reports templates under their source
        // name and wrongly marks them "pending" because the verification
        // path (`~/.config.toml.tmpl`) doesn't exist.
        let entries = scanner.walk_pack(&pack.path, &pack_config.pack.ignore, &gates, host)?;
        let preprocess_result = if pack_config.preprocessor.enabled {
            // [secret] is intentionally root-only — see SecretSection docs.
            let root_config = ctx.config_manager.root_config()?;
            let (registry, _secret_registry) = crate::preprocessing::default_registry(
                &pack_config.preprocessor,
                &root_config.secret,
                ctx.paths.as_ref(),
                ctx.command_runner.clone(),
            )?;
            if !registry.is_empty() {
                // status is a Passive command — never evaluate
                // templates, never write rendered files or baselines.
                // See `secrets.lex` §7.4 / issue #121.
                match crate::preprocessing::pipeline::preprocess_pack(
                    entries,
                    &registry,
                    &pack,
                    ctx.fs.as_ref(),
                    ctx.datastore.as_ref(),
                    ctx.paths.as_ref(),
                    crate::preprocessing::PreprocessMode::Passive,
                    /* force */ false,
                ) {
                    Ok(r) => r,
                    Err(err) => {
                        // Preprocessing failure surfaces as a warning; we
                        // still want to show whatever we can from the
                        // intent-collection attempt below.
                        warnings.push(format!(
                            "preprocessing failed for pack '{}': {}",
                            pack.display_name, err
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
        let matches = scanner.match_entries(
            &all_entries,
            &rules,
            &pack.name,
            &gates,
            host,
            &pack_config.mappings.gates,
        )?;

        // Collect intents for conflict detection. The first tuple
        // element is the user-facing label that surfaces in any
        // resulting `DisplayConflict.claimants` entry, so it tracks
        // the pack's display name rather than its raw on-disk name.
        // status is a Passive command — same §7.4 contract as the
        // direct preprocess_pack call above.
        match orchestration::plan_pack(&pack, ctx, crate::preprocessing::PreprocessMode::Passive) {
            Ok(plan) => {
                warnings.extend(plan.warnings);
                pack_intents.push((pack.display_name.clone(), plan.intents));
            }
            Err(err) => {
                warnings.push(format!(
                    "could not collect intents for pack '{}'; conflict detection may be incomplete: {}",
                    pack.display_name, err
                ));
            }
        }

        let mut files = Vec::new();
        for m in &matches {
            // The `ignore` filter handler claims files only to keep them
            // off the catchall and out of status. Drop them here so the
            // user sees nothing — same contract as `.gitignore`.
            if m.handler == HANDLER_IGNORE {
                continue;
            }

            let rel_str = m.relative_path.to_string_lossy().into_owned();

            // Skip rows for `_lib/` entries on non-macOS. Two cases
            // need to be handled:
            //
            // - `_lib/<rest>` files — the resolver returns
            //   `Resolution::Skip` and the planner drops the intent.
            // - the top-level `_lib` directory itself — its match
            //   reaches status but `dir_intents` forces per-file mode
            //   and every nested file resolves to Skip, so nothing
            //   under the directory is ever deployed.
            //
            // Either way, rendering a "pending symlink" row alongside
            // the planner's "skipping on this platform" warning would
            // contradict the warning and mislead users.
            if m.handler == HANDLER_SYMLINK
                && !cfg!(target_os = "macos")
                && (rel_str == "_lib" || rel_str.starts_with("_lib/"))
            {
                continue;
            }

            // Per-file chain verification based on handler type
            let health = match m.handler.as_str() {
                h if h == HANDLER_SKIP => Health::Skipped,
                h if h == HANDLER_GATE => {
                    // Scanner stamped these in `options` when the gate
                    // evaluated false: `gate_label`, `gate_predicate`,
                    // `gate_host`. Missing values fall back to `<unknown>`
                    // so a malformed match never crashes the renderer.
                    let label = m
                        .options
                        .get("gate_label")
                        .cloned()
                        .unwrap_or_else(|| "<unknown>".into());
                    let expected = m
                        .options
                        .get("gate_predicate")
                        .cloned()
                        .unwrap_or_else(|| "<unknown>".into());
                    let actual = m
                        .options
                        .get("gate_host")
                        .cloned()
                        .unwrap_or_else(|| "<unknown>".into());
                    Health::Gated {
                        label,
                        expected,
                        actual,
                    }
                }
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
                let target = resolve_target(&pack.name, &rel_str, &pack.config, ctx.paths.as_ref());
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

            let status_label = health.label(&m.handler);
            // For PendingConflict, allocate a command-wide note index and
            // stash the reason in the global notes list. The row keeps
            // its plain handler label; the template renders `[N]` next
            // to it and the body appears in the notes section.
            let note_ref = health.footnote_reason().map(|reason| {
                notes.push(DisplayNote {
                    body: reason,
                    hint: None,
                });
                notes.len() as u32
            });
            files.push(DisplayFile {
                name: rel_str.clone(),
                symbol: handler_symbol(&m.handler).into(),
                description: handler_description(&m.handler, &rel_str, user_target.as_deref()),
                status: health.style().into(),
                status_label,
                handler: m.handler.clone(),
                note_ref,
            });
        }

        display_packs.push(DisplayPack::new(pack.display_name.clone(), files));
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

    // Surface ignored packs by their display name, not the raw
    // on-disk directory — the prefix grammar must stay invisible to
    // the rendered "Ignored Packs" section just like every other
    // user-facing surface.
    let ignored_display: Vec<String> = ignored_packs
        .iter()
        .map(|d| crate::packs::display_name_for(d).to_string())
        .collect();

    Ok(PackStatusResult {
        message: None,
        dry_run: false,
        packs: display_packs,
        warnings,
        notes,
        conflicts: display_conflicts,
        ignored_packs: ignored_display,
        inactive_packs,
        view_mode: ctx.view_mode.as_str().into(),
        group_mode: ctx.group_mode.as_str().into(),
    })
}

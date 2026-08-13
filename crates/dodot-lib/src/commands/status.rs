//! `status` command — shows current deployment state with chain verification.
//!
//! For each file, status verifies the actual filesystem state rather than
//! just checking whether datastore directories exist. This catches broken
//! symlinks, missing source files, and config drift.
//!
//! Additionally, status performs cross-pack conflict detection and surfaces
//! potential conflicts as warnings — even for packs that aren't deployed
//! yet. This lets users see problems before they run `up`.
//!
//! Deployed shell sources are verified observationally: when
//! `[profiling]` is enabled, the row reports the newest shell-init run
//! recorded for the file (`sourced` / `exited N`), historical failures
//! in the recent window surface as a warning footnote with a ✓/✗
//! timeline, and a file no run has observed yet renders the pending
//! presentation — dodot claims `sourced` only after seeing it happen.

use tracing::{debug, info};

use crate::commands::{
    handler_description, handler_symbol, DisplayConflict, DisplayDiff, DisplayFile, DisplayNote,
    DisplayPack, PackStatusResult,
};
use crate::config::mappings_to_rules;
use crate::conflicts;
use crate::datastore::DidRunStatus;
use crate::handlers::run_once::{file_checksum, run_once_status_messages};
use crate::handlers::{
    self, HANDLER_GATE, HANDLER_HOMEBREW, HANDLER_IGNORE, HANDLER_INSTALL, HANDLER_NIX,
    HANDLER_SKIP, HANDLER_SYMLINK,
};
use crate::operations::HandlerIntent;
use crate::packs::orchestration::{self, ExecutionContext};
use crate::packs::{self};
use crate::rules::Scanner;
use crate::Result;

/// Deployment health for a single file, determined by chain verification.
enum Health {
    /// Not deployed (no data link in datastore). Also the
    /// presentation for a deployed shell source that no shell-init
    /// run has observed yet (see [`verify_staged`]) — dodot claims
    /// `sourced` only after seeing it happen.
    Pending,
    /// Not deployed AND a non-dodot file/symlink already occupies the
    /// target path — `dodot up` would fail without `--force`. The reason
    /// is rendered as a footnote so the right column stays compact.
    PendingConflict { reason: String },
    /// Deployed and all links verified correct.
    Deployed,
    /// Deployed and the chain is healthy, but the file has a known
    /// quality issue: a pre-flight syntax error recorded by `dodot up`.
    /// `label` is the short status-column text, `reason` is the
    /// footnote body. A syntax error always wins over runtime history —
    /// a file that doesn't parse has no meaningful exit-code data.
    DeployedWithError { label: String, reason: String },
    /// Deployed with a healthy chain, and the shell-init profiling
    /// instrumentation observed the file in at least one recent
    /// startup, at least one of which exited non-zero. The row verdict
    /// comes from the NEWEST run only — `sourced` (deployed style)
    /// when it exited zero, `exited N` (broken style) when it didn't —
    /// while the whole window renders as a warning footnote with a
    /// ✓/✗ timeline, so historical instability never masquerades as
    /// current state (and never rolls a currently-clean file or its
    /// pack up as an error).
    RecentFailures {
        history: RunHistory,
        /// `dodot probe shell-init <pack>/<file>` — rendered in
        /// normal emphasis inside the warning footnote.
        probe_command: String,
    },
    /// Deployed but the chain is broken.
    Broken(String),
    /// Data link exists and is healthy, but the user link is not at the
    /// path that current config would produce. A re-deploy would move it.
    Stale(String),
    /// Run-once handler (install / homebrew) recorded a successful run
    /// for a *different* content hash than the file currently has on
    /// disk. The script has not been re-run automatically — the
    /// notify-don't-rerun policy leaves the prior state in place
    /// until the user passes `--provision-rerun`. `label` is the
    /// short status-column text (carries the per-handler "older
    /// version" copy plus a `(N+ M-)` line summary when a snapshot is
    /// on disk).
    RanOlderVersion { label: String },
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
            // Newest observed run decides: a currently-clean file
            // stays green no matter how bumpy the recent history —
            // the history lives in the warning footnote instead.
            Health::RecentFailures { history, .. } => {
                if history.latest_exit() == 0 {
                    "deployed"
                } else {
                    "broken"
                }
            }
            Health::Broken(_) => "broken",
            Health::Stale(_) => "stale",
            // "ran older version" rolls up to the same bucket as
            // `Stale`: the script HAS run, but the source has moved on
            // and a `dodot up --provision-rerun` is needed to bring
            // the system back in sync. Sharing the style with stale
            // keeps the pack-level summary ("pending") sensible: any
            // pack with an older-version entry is one user action away
            // from being current.
            Health::RanOlderVersion { .. } => "stale",
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
                "install" | "homebrew" | "nix" => run_once_status_messages(handler).pending,
                _ => "pending".into(),
            },
            Health::Deployed => match handler {
                "symlink" => "linked".into(),
                "shell" => "sourced".into(),
                "path" => "in PATH".into(),
                "install" | "homebrew" | "nix" => run_once_status_messages(handler).deployed,
                _ => "deployed".into(),
            },
            Health::DeployedWithError { label, .. } => label.clone(),
            // Only shell sources carry run history, so the success
            // label is the shell handler's deployed vocabulary.
            Health::RecentFailures { history, .. } => {
                if history.latest_exit() == 0 {
                    "sourced".into()
                } else {
                    format!("exited {}", history.latest_exit())
                }
            }
            Health::Broken(reason) => reason.clone(),
            Health::Stale(reason) => reason.clone(),
            Health::RanOlderVersion { label } => label.clone(),
            Health::Skipped => "skipped".into(),
            Health::Gated { label, .. } => format!("gated out ({label})"),
        }
    }

    /// If this health carries a footnote, build the command-wide note
    /// for it. Hard failures (pending conflict, syntax error, gate
    /// mismatch) are error-kind notes; run history is a warning-kind
    /// note with a ✓/✗ timeline — recent instability annotates the
    /// row, it must never present as a current error. `None` for
    /// footnote-less healths.
    fn footnote(&self) -> Option<DisplayNote> {
        match self {
            Health::PendingConflict { reason } => Some(DisplayNote::error(reason.clone())),
            Health::DeployedWithError { reason, .. } => Some(DisplayNote::error(reason.clone())),
            Health::Gated {
                expected, actual, ..
            } => Some(DisplayNote::error(format!(
                "expected {expected}; got {actual}"
            ))),
            Health::RecentFailures {
                history,
                probe_command,
            } => Some(DisplayNote {
                body: format!(
                    "{} of the last {} runs failed",
                    history.runs_failed(),
                    history.runs_seen()
                ),
                hint: None,
                kind: "warning".into(),
                timeline: Some(history.exits.iter().map(|&e| e == 0).collect()),
                command: Some(probe_command.clone()),
            }),
            _ => None,
        }
    }
}

/// Exit statuses of the recent shell-init runs that sourced one
/// deployed shell file, oldest→newest — so the last entry is the
/// newest run, whose exit code decides the row verdict. Built from
/// the last [`RUNTIME_FAILURE_WINDOW`] profiles; profiles that never
/// touched the file don't count. Never empty: an unobserved file
/// yields no history at all (see [`recent_run_history`]).
struct RunHistory {
    exits: Vec<i32>,
}

impl RunHistory {
    /// Exit status of the newest observed run.
    fn latest_exit(&self) -> i32 {
        *self.exits.last().expect("RunHistory is never empty")
    }

    /// Number of applicable runs observed in the window.
    fn runs_seen(&self) -> usize {
        self.exits.len()
    }

    /// How many of those runs exited non-zero.
    fn runs_failed(&self) -> usize {
        self.exits.iter().filter(|&&e| e != 0).count()
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

/// Render an absolute deploy path with `$HOME` collapsed to `~/…` for
/// display. Mirrors the formatting used by the dry-run renderer in
/// `commands::up::extract_op_info` so identical paths surface as
/// identical strings whether they came from a planned intent or a
/// completed operation.
fn format_path_relative_to_home(path: &std::path::Path, home: &std::path::Path) -> String {
    if let Ok(rel) = path.strip_prefix(home) {
        format!("~/{}", rel.display())
    } else {
        path.display().to_string()
    }
}

/// Display name for a symlink intent — the file's pack-relative path
/// when the source lives under the pack tree.
///
/// Preprocessor outputs live in `<data>/<pack>/preprocessed/<virtual>`
/// (e.g. `subdir/config.toml` is the virtual path of
/// `subdir/config.toml.tmpl`), so stripping that prefix recovers the
/// user-meaningful `subdir/config.toml` — not just `config.toml`,
/// which would collapse nested templates onto the same row and lose
/// the subdirectory the user sees in their pack.
///
/// Final fallback is the source basename, only reached for sources
/// that live neither under the pack nor under the preprocessed dir
/// (no production path produces such intents today; the fallback is
/// defensive).
fn intent_display_name(
    source: &std::path::Path,
    pack_path: &std::path::Path,
    preprocessed_dir: &std::path::Path,
) -> String {
    if let Ok(rel) = source.strip_prefix(pack_path) {
        return rel.to_string_lossy().into_owned();
    }
    if let Ok(rel) = source.strip_prefix(preprocessed_dir) {
        return rel.to_string_lossy().into_owned();
    }
    source
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// Verify symlink handler chain for a single file.
///
/// `user_target` is the resolved deploy path — provided by the caller
/// (typically a [`HandlerIntent::Link`] from the planner). Status must
/// not re-derive it: a second derivation drifts from the planner for
/// escape-prefix dirs (`_home/`/`_xdg/`/`_app/`/`_lib/`), producing
/// permanent "pending" rows for files the planner successfully
/// deployed under a *different* path. Source of truth lives in
/// `resolve_target`, called once per intent in the planner.
///
/// Checks: data link exists → points to source → source exists →
/// user link exists at `user_target` → points to data link.
fn verify_symlink(
    source: &std::path::Path,
    user_target: &std::path::Path,
    pack: &str,
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
        if !ctx.fs.is_symlink(user_target) && ctx.fs.exists(user_target) {
            if crate::equivalence::is_equivalent(user_target, source, ctx.fs.as_ref()) {
                return Health::Pending;
            }
            let reason =
                describe_blocking_target(user_target, ctx.fs.as_ref(), ctx.paths.home_dir());
            return Health::PendingConflict { reason };
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

    if ctx.fs.is_symlink(user_target) {
        match ctx.fs.readlink(user_target) {
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
    } else if ctx.fs.exists(user_target) {
        // Non-symlink file at target. If its content is byte-identical to
        // the source, `up` will auto-replace it (#44) — surface as Stale
        // (re-deploy fixes), not Broken. Otherwise it's a real conflict.
        if crate::equivalence::is_equivalent(user_target, source, ctx.fs.as_ref()) {
            Health::Stale("stale: user link missing, re-deploy to fix".into())
        } else {
            Health::Broken("conflict: non-symlink file at target path".into())
        }
    } else {
        // Data link exists but the user link is missing — config drift
        // or an interrupted deployment.
        Health::Stale("stale: user link missing, re-deploy to fix".into())
    }
}

/// Verify shell/path handler chain for a single file.
///
/// Checks: data link exists → points to source → source exists.
/// For the shell handler, two post-deploy quality signals layer on a
/// healthy chain: a syntax-error sidecar from the `dodot up`
/// pre-flight (a hard error that always wins), and — when
/// `observe_runtime` is set, i.e. `[profiling]` is enabled — the
/// observation-driven verdict from recent shell-init profiles. The
/// row then reports the NEWEST applicable run (`sourced` on exit
/// zero, `exited N` otherwise), recent failures render as a warning
/// footnote, and a file no run has observed yet gets the pending
/// presentation — dodot claims `sourced` only after seeing it. With
/// profiling off no observation can ever arrive, so the chain
/// verdict stands.
fn verify_staged(
    source: &std::path::Path,
    pack: &str,
    handler: &str,
    observe_runtime: bool,
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
    // runtime history — fix-the-parse is the more fundamental issue,
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

        // (2) Observation-driven verdict from the shell-init
        //     profiles. Skipped entirely when profiling is off —
        //     no observation can ever arrive, so the chain verdict
        //     below stands rather than pinning the row on a pending
        //     state it could never leave.
        if observe_runtime {
            return match recent_run_history(source, ctx) {
                // Deployed but never seen in a shell startup: the
                // pending presentation ("not sourced"), not a claim
                // of success we haven't observed.
                None => Health::Pending,
                Some(history) if history.runs_failed() == 0 => Health::Deployed,
                Some(history) => Health::RecentFailures {
                    history,
                    probe_command: format!("dodot probe shell-init {pack}/{filename_str}"),
                },
            };
        }
    }

    Health::Deployed
}

/// Classify a run-once handler row (install / homebrew) by consulting
/// the datastore's three-way [`DidRunStatus`] for the file.
///
/// On `RanDifferent`, `out_diffs` accumulates a unified-diff entry for
/// the row when `show_diff` is set AND the previous-run snapshot is on
/// disk. `out_diffs` is left untouched for the other two states and
/// for sentinels predating the snapshot convention.
///
/// `display_name` is the pack name used in the diff payload (display
/// name, not on-disk name) so the JSON / text output mirrors the rest
/// of the status row.
fn run_once_health(
    file: &std::path::Path,
    pack: &str,
    display_name: &str,
    handler: &str,
    ctx: &ExecutionContext,
    show_diff: bool,
    out_diffs: &mut Vec<DisplayDiff>,
) -> Health {
    // Source missing entirely: surface the same "broken" state used by
    // the symlink/staged paths so the row doesn't claim a state we
    // can't verify. The handler would skip the file at intent time
    // anyway (see `RunOnceHandler::to_intents`), but status looks at
    // matches, not intents.
    if !ctx.fs.exists(file) {
        return Health::Broken("broken: source file missing".into());
    }
    let filename = match file.file_name() {
        Some(f) => f.to_string_lossy().into_owned(),
        None => return Health::Pending,
    };
    let current_hash = match file_checksum(ctx.fs.as_ref(), file) {
        Ok(h) => h,
        Err(e) => return Health::Broken(format!("broken: cannot hash source file: {e}")),
    };

    let messages = run_once_status_messages(handler);
    let status = match ctx
        .datastore
        .did_run(pack, handler, &filename, &current_hash)
    {
        Ok(s) => s,
        Err(e) => return Health::Broken(format!("broken: datastore error: {e}")),
    };

    match status {
        DidRunStatus::NeverRan => Health::Pending,
        DidRunStatus::RanCurrent => Health::Deployed,
        DidRunStatus::RanDifferent {
            previous_snapshot, ..
        } => {
            // Pull the current source bytes for both the line summary
            // and the (optional) full diff. A read error here drops us
            // back to the no-snapshot label rather than failing the
            // whole status command — diff is an enhancement, not a
            // hard requirement.
            let current_bytes = ctx.fs.read_file(file).ok();
            let label = match (previous_snapshot.as_deref(), current_bytes.as_deref()) {
                (Some(prev), Some(cur)) => {
                    let summary = line_summary(prev, cur);
                    if show_diff {
                        out_diffs.push(DisplayDiff {
                            pack: display_name.to_string(),
                            file: filename.clone(),
                            handler: handler.to_string(),
                            body: unified_diff(&filename, prev, cur),
                        });
                    }
                    format!("{} ({})", messages.ran_different, summary)
                }
                _ => {
                    // No snapshot on disk — sentinel predates the
                    // snapshot convention. Render the state without a
                    // line summary so the user knows a re-run is
                    // pending but not what changed.
                    format!("{} (no diff data)", messages.ran_different)
                }
            };
            Health::RanOlderVersion { label }
        }
    }
}

/// `(N lines added, M lines removed)` summary for a `RanDifferent`
/// row. Counts unified-diff `+` / `-` bodies (excluding the two
/// header lines) so the result matches what a user would see in `diff
/// -u` output. UTF-8 is assumed; non-UTF-8 inputs render via lossy
/// decode for the diff itself but don't affect the line counts (the
/// raw byte slices are decoded with `from_utf8_lossy`).
fn line_summary(prev: &[u8], cur: &[u8]) -> String {
    let prev_s = String::from_utf8_lossy(prev);
    let cur_s = String::from_utf8_lossy(cur);
    let patch = diffy::create_patch(&prev_s, &cur_s);
    let mut added = 0usize;
    let mut removed = 0usize;
    for hunk in patch.hunks() {
        for line in hunk.lines() {
            match line {
                diffy::Line::Insert(_) => added += 1,
                diffy::Line::Delete(_) => removed += 1,
                diffy::Line::Context(_) => {}
            }
        }
    }
    format!(
        "{added} {} added, {removed} removed",
        if added == 1 { "line" } else { "lines" },
    )
}

/// Build the unified-diff body for a `RanOlderVersion` row whose
/// snapshot is on disk. The returned string is a complete patch with
/// `--- <file> (previous run)` / `+++ <file> (current)` headers,
/// ready to drop into the templated output.
fn unified_diff(filename: &str, prev: &[u8], cur: &[u8]) -> String {
    let prev_s = String::from_utf8_lossy(prev);
    let cur_s = String::from_utf8_lossy(cur);
    let mut opts = diffy::DiffOptions::default();
    opts.set_original_filename(format!("{filename} (previous run)"))
        .set_modified_filename(format!("{filename} (current)"));
    opts.create_patch(&prev_s, &cur_s).to_string()
}

/// How many recent shell-init profiles to scan for run history.
/// Five is enough to catch the "fails sometimes" case without making
/// status I/O-heavy; users who want deeper history reach for
/// `dodot probe shell-init --history`.
const RUNTIME_FAILURE_WINDOW: usize = 5;

/// Collect the exit status of every recent shell-init run that
/// sourced `source`, oldest→newest. `None` when no applicable run has
/// been observed — no profiles yet, or no recent profile contains the
/// file. Unrelated profile entries never count. Classification (clean
/// / flaky / currently failing) happens at the caller; this only
/// reports what was observed. Per-run stderr stays behind
/// `dodot probe shell-init <pack>/<file>` — the status footnote
/// carries counts and the ✓/✗ timeline, not error text.
fn recent_run_history(source: &std::path::Path, ctx: &ExecutionContext) -> Option<RunHistory> {
    let profiles = crate::probe::shell_init::read_recent_profiles(
        ctx.fs.as_ref(),
        ctx.paths.as_ref(),
        RUNTIME_FAILURE_WINDOW,
    )
    .ok()?;
    let target_str = source.to_string_lossy();

    // `profiles` is newest-first; reverse so the timeline reads
    // oldest→newest and the last entry is the newest verdict.
    let mut exits: Vec<i32> = profiles
        .iter()
        .filter_map(|profile| {
            profile
                .entries
                .iter()
                .find(|e| e.phase == "source" && e.target == target_str)
                .map(|e| e.exit_status)
        })
        .collect();
    if exits.is_empty() {
        return None;
    }
    exits.reverse();
    Some(RunHistory { exits })
}

/// Run the `status` command: scan packs and verify deployment chain per file.
///
/// Also performs cross-pack conflict detection and surfaces potential
/// conflicts as warnings.
pub fn status(pack_filter: Option<&[String]>, ctx: &ExecutionContext) -> Result<PackStatusResult> {
    info!("starting status command");

    let mut warnings = Vec::new();
    if let Some(names) = pack_filter {
        warnings = orchestration::validate_pack_names(names, ctx)?;
    }

    let root_config = ctx.config_manager.root_config()?;
    // Shell rows are verified observationally (newest shell-init run)
    // only when profiling is on; with it off the chain verdict stands.
    let observe_runtime = root_config.profiling.enabled;
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

    let registry = handlers::create_registry(ctx.fs.as_ref(), ctx.command_runner.as_ref());
    let host = ctx.host_facts.as_ref();
    let mut display_packs = Vec::new();
    let mut notes: Vec<DisplayNote> = Vec::new();
    let mut inactive_packs: Vec<String> = Vec::new();
    // Accumulator for unified diffs of `RanOlderVersion` rows. Always
    // constructed (even when `--diff` is off) so the run-once branch
    // can take a `&mut` without conditional plumbing; only mutated
    // when `ctx.show_diff` is true and the row's snapshot is on disk.
    let mut diffs: Vec<DisplayDiff> = Vec::new();

    let mut pack_intents = Vec::new();
    // Capture the active packs (post-filter, post-OS-gate) so the
    // optional `--check-drift` pass at the end of status() respects
    // the same scope and doesn't re-emit warnings for filtered or
    // gated packs.
    let mut active_packs: Vec<(String, String, std::path::PathBuf)> = Vec::new();

    for mut pack in all_packs {
        info!(pack = %pack.display_name, "checking pack status");
        let pack_config = ctx.config_manager.config_for_pack(&pack.path)?;
        pack.config = pack_config.to_handler_config();

        // Pack-level OS gate. Inactive packs surface in their own
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
        active_packs.push((
            pack.name.clone(),
            pack.display_name.clone(),
            pack.path.clone(),
        ));
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
        // Apply all gate sources BEFORE preprocessing — same posture as
        // the `up` planning path. Without this, a gated-out template
        // (basename suffix or `[mappings.gates]` glob) would still be
        // partitioned as a preprocessor file, replaced by a virtual
        // entry with the on-disk filename / absolute_path lost. status
        // would then show that virtual entry instead of the original
        // gate-out row, confusing the user.
        let entries = orchestration::filter_pre_preprocess_gates(
            entries,
            &gates,
            host,
            &pack.name,
            &pack_config.mappings.gates,
        )?;
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

        // Collect intents for conflict detection AND drive symlink
        // rendering off the same intents the executor sees. Re-walking
        // matches and re-resolving targets in parallel drifts for
        // escape-prefix dirs (`_app/` etc.), since matches are
        // top-level entries but the planner expands those per-leaf.
        // Driving display off intents collapses the two paths into one
        // and makes "render through status" actually truthful for
        // every file the executor touched.
        //
        // The first tuple element is the user-facing label that
        // surfaces in any resulting `DisplayConflict.claimants` entry,
        // so it tracks the pack's display name rather than its raw
        // on-disk name. status is a Passive command — same §7.4
        // contract as the direct preprocess_pack call above.
        let intents_for_pack: Vec<HandlerIntent> = match orchestration::plan_pack(
            &pack,
            ctx,
            crate::preprocessing::PreprocessMode::Passive,
        ) {
            Ok(plan) => {
                warnings.extend(plan.warnings);
                let intents = plan.intents.clone();
                pack_intents.push((pack.display_name.clone(), plan.intents));
                intents
            }
            Err(err) => {
                warnings.push(format!(
                    "could not collect intents for pack '{}'; conflict detection may be incomplete: {}",
                    pack.display_name, err
                ));
                Vec::new()
            }
        };

        let mut files = Vec::new();

        // Pass 1: filter / non-deployable handlers (skip, gate) and the
        // remaining deployable handlers that we still verify match-side
        // (shell, path, install, homebrew). Symlink rows are emitted
        // below, off the planner's intents, so they correctly expand
        // escape-prefix dirs and never re-derive a target.
        for m in &matches {
            // The `ignore` filter handler claims files only to keep them
            // off the catchall and out of status. Drop them here so the
            // user sees nothing — same contract as `.gitignore`.
            if m.handler == HANDLER_IGNORE {
                continue;
            }
            if m.handler == HANDLER_SYMLINK {
                continue;
            }

            let rel_str = m.relative_path.to_string_lossy().into_owned();

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
                "shell" | "path" => verify_staged(
                    &m.absolute_path,
                    &pack.name,
                    &m.handler,
                    observe_runtime,
                    ctx,
                ),
                h if h == HANDLER_INSTALL || h == HANDLER_HOMEBREW || h == HANDLER_NIX => {
                    run_once_health(
                        &m.absolute_path,
                        &pack.name,
                        &pack.display_name,
                        &m.handler,
                        ctx,
                        ctx.show_diff,
                        &mut diffs,
                    )
                }
                _ => {
                    // Future run-once handlers without dedicated routing
                    // (or any other Provision/Setup handler) fall back
                    // to the binary deployed/pending classification via
                    // the registry.
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

            let status_label = health.label(&m.handler);
            // For footnote-carrying healths, allocate a command-wide
            // note index and stash the note in the global list. The
            // row keeps its plain handler label; the template renders
            // `[N]` next to it (dim for errors, warning-styled for
            // run-history warnings) and the body appears in the
            // matching notes section.
            let note_ref = health.footnote().map(|note| {
                notes.push(note);
                notes.len() as u32
            });
            files.push(DisplayFile {
                name: rel_str.clone(),
                symbol: handler_symbol(&m.handler).into(),
                description: handler_description(&m.handler, &rel_str, None),
                status: health.style().into(),
                status_label,
                handler: m.handler.clone(),
                note_ref,
            });
        }

        // Pass 2: symlink rows from planner intents — one row per Link
        // intent. For escape-prefix dirs (`_app/` etc.) the planner
        // recurses per-leaf, so this produces N rows where the matches
        // loop would have produced 1 wrongly-targeted row. For wholesale
        // dirs (`nvim`) and plain top-level files the planner produces
        // exactly one intent. `_lib/` on non-macOS yields zero intents
        // (`Resolution::Skip`), so those rows drop out without an
        // explicit suppress branch here.
        let home = ctx.paths.home_dir();
        let preprocessed_dir = ctx.paths.handler_data_dir(&pack.name, "preprocessed");
        for intent in &intents_for_pack {
            let HandlerIntent::Link {
                source, user_path, ..
            } = intent
            else {
                continue;
            };

            let name = intent_display_name(source, &pack.path, &preprocessed_dir);
            let user_target_display = format_path_relative_to_home(user_path, home);
            let health = verify_symlink(source, user_path, &pack.name, ctx);
            let status_label = health.label(HANDLER_SYMLINK);
            let note_ref = health.footnote().map(|note| {
                notes.push(note);
                notes.len() as u32
            });
            files.push(DisplayFile {
                name: name.clone(),
                symbol: handler_symbol(HANDLER_SYMLINK).into(),
                description: handler_description(
                    HANDLER_SYMLINK,
                    &name,
                    Some(&user_target_display),
                ),
                status: health.style().into(),
                status_label,
                handler: HANDLER_SYMLINK.into(),
                note_ref,
            });
        }

        display_packs.push(DisplayPack::new(pack.display_name.clone(), files));
    }

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

    // Opt-in drift detection for externals. Surfaced as warnings on
    // the result so they appear in the same channel as conflict /
    // cross-pack notes — no separate rendering path. Scoped to the
    // packs that survived `pack_filter` and the per-pack OS gate so
    // `dodot status <pack> --check-drift` doesn't leak warnings
    // for unrelated packs.
    if ctx.check_drift {
        warnings.extend(collect_drift_warnings(ctx, &active_packs)?);
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
        diffs,
    })
}

/// Run `--check-drift` over the supplied packs and emit a one-line
/// warning per anomaly. `Clean` reports are dropped silently;
/// everything else (drifted, missing, check-failed, not-implemented)
/// surfaces so users know what the check could and couldn't see.
///
/// `active_packs` is `(on-disk name, display name, pack path)` so
/// path resolution uses the on-disk name (datastore subdirs are
/// keyed by it) while the warning text uses the display name (which
/// is what the user typed / sees elsewhere). Mixing those up would
/// look in the wrong datastore subtree for prefix-grammar packs.
fn collect_drift_warnings(
    ctx: &ExecutionContext,
    active_packs: &[(String, String, std::path::PathBuf)],
) -> Result<Vec<String>> {
    use crate::external::{detect_drift_for_pack, DriftKind};
    use crate::handlers::externals::EXTERNALS_TOML;

    let mut out = Vec::new();
    let git_runner = crate::external::ShellGitRunner::new();

    for (on_disk_name, display_name, pack_path) in active_packs {
        let toml_path = pack_path.join(EXTERNALS_TOML);
        if !ctx.fs.exists(&toml_path) {
            continue;
        }
        let bytes = match ctx.fs.read_file(&toml_path) {
            Ok(b) => b,
            Err(e) => {
                out.push(format!(
                    "{display_name}: drift check skipped — cannot read externals.toml: {e}"
                ));
                continue;
            }
        };
        let reports = detect_drift_for_pack(
            on_disk_name,
            &bytes,
            ctx.paths.as_ref(),
            ctx.fs.as_ref(),
            Some(&git_runner),
        )?;
        for r in reports {
            match r.kind {
                DriftKind::Clean => {}
                DriftKind::Drifted => out.push(format!(
                    "{display_name} / {}: drift — {}",
                    r.entry_name, r.detail
                )),
                DriftKind::Missing => out.push(format!(
                    "{display_name} / {}: deployed copy missing — {}",
                    r.entry_name, r.detail
                )),
                DriftKind::CheckFailed => out.push(format!(
                    "{display_name} / {}: drift check errored — {}",
                    r.entry_name, r.detail
                )),
                DriftKind::NotImplemented => out.push(format!(
                    "{display_name} / {}: drift check not implemented ({})",
                    r.entry_name, r.detail
                )),
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{intent_display_name, line_summary, unified_diff};
    use std::path::Path;

    #[test]
    fn intent_display_name_pack_relative_for_pack_file() {
        let name = intent_display_name(
            Path::new("/dot/iina/_app/foo/bar.conf"),
            Path::new("/dot/iina"),
            Path::new("/data/iina/preprocessed"),
        );
        assert_eq!(name, "_app/foo/bar.conf");
    }

    #[test]
    fn intent_display_name_strips_preprocessed_prefix_for_rendered_files() {
        // Defensive: if the preprocessor pipeline ever produces a
        // rendered source under a subdir of the `preprocessed` dir
        // (e.g. `subdir/config.toml` rendered from
        // `subdir/config.toml.tmpl`), status must surface the
        // user-meaningful virtual-relative path — not just the
        // basename, which would collide with a pack-root file of the
        // same name. Today the top-level scanner is depth-1 so no
        // production path produces such sources, but the helper has to
        // be correct in case that changes.
        let name = intent_display_name(
            Path::new("/data/iina/preprocessed/subdir/config.toml"),
            Path::new("/dot/iina"),
            Path::new("/data/iina/preprocessed"),
        );
        assert_eq!(name, "subdir/config.toml");
    }

    #[test]
    fn intent_display_name_falls_back_to_basename_for_unrelated_paths() {
        // Last-resort fallback for sources that live neither under
        // the pack nor under the preprocessed dir. No production path
        // produces such intents today; the fallback is purely defensive.
        let name = intent_display_name(
            Path::new("/elsewhere/foo.conf"),
            Path::new("/dot/iina"),
            Path::new("/data/iina/preprocessed"),
        );
        assert_eq!(name, "foo.conf");
    }

    #[test]
    fn line_summary_counts_added_and_removed_lines() {
        // diffy collapses unchanged lines into Context entries, which
        // line_summary ignores; only Insert/Delete are counted.
        let prev = "alpha\nbeta\ngamma\n";
        let cur = "alpha\nbeta-prime\ngamma\ndelta\n";
        let summary = line_summary(prev.as_bytes(), cur.as_bytes());
        assert_eq!(summary, "2 lines added, 1 removed");
    }

    #[test]
    fn line_summary_singular_for_one_added() {
        // Singular `line` when exactly one was added — small UX
        // touch so the row reads cleanly in the common single-edit case.
        let prev = "alpha\nbeta\n";
        let cur = "alpha\nbeta\ngamma\n";
        let summary = line_summary(prev.as_bytes(), cur.as_bytes());
        assert_eq!(summary, "1 line added, 0 removed");
    }

    #[test]
    fn line_summary_zero_when_inputs_match() {
        // Identical content yields a no-op patch with no Insert/Delete.
        let s = "echo hi\n";
        let summary = line_summary(s.as_bytes(), s.as_bytes());
        assert_eq!(summary, "0 lines added, 0 removed");
    }

    #[test]
    fn unified_diff_carries_per_run_filename_decorations() {
        let prev = "echo old\n";
        let cur = "echo new\n";
        let body = unified_diff("install.sh", prev.as_bytes(), cur.as_bytes());
        assert!(
            body.contains("--- install.sh (previous run)"),
            "expected previous-run header in diff, got: {body}"
        );
        assert!(
            body.contains("+++ install.sh (current)"),
            "expected current header in diff, got: {body}"
        );
        assert!(
            body.contains("-echo old"),
            "expected removed line in diff, got: {body}"
        );
        assert!(
            body.contains("+echo new"),
            "expected added line in diff, got: {body}"
        );
    }

    // ── run_once_health (three-state for install / homebrew) ──

    use super::{run_once_health, Health};
    use crate::commands::DisplayDiff;
    use crate::fs::Fs;
    use crate::handlers::HANDLER_INSTALL;
    use crate::packs::orchestration::ExecutionContext;
    use crate::paths::Pather;
    use crate::testing::TempEnvironment;

    fn ctx_for(env: &TempEnvironment) -> ExecutionContext {
        use crate::config::ConfigManager;
        use crate::datastore::{CommandOutput, CommandRunner, FilesystemDataStore};
        use crate::Result;
        use std::sync::Arc;
        struct NoopRunner;
        impl CommandRunner for NoopRunner {
            fn run(&self, _: &str, _: &[String]) -> Result<CommandOutput> {
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
        let config_manager =
            Arc::new(ConfigManager::new(&env.dotfiles_root).expect("test config manager"));
        ExecutionContext {
            fs: env.fs.clone(),
            datastore,
            paths: env.paths.clone(),
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
        }
    }

    #[test]
    fn run_once_health_pending_when_no_sentinel() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("install.sh", "echo hi")
            .done()
            .build();
        let ctx = ctx_for(&env);
        let abs = env.dotfiles_root.join("vim/install.sh");
        let mut diffs = Vec::new();
        let h = run_once_health(&abs, "vim", "vim", HANDLER_INSTALL, &ctx, false, &mut diffs);
        assert!(matches!(h, Health::Pending));
        assert!(diffs.is_empty());
    }

    #[test]
    fn run_once_health_deployed_when_current_hash_matches() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("install.sh", "echo hi")
            .done()
            .build();
        let ctx = ctx_for(&env);
        let abs = env.dotfiles_root.join("vim/install.sh");
        let checksum = crate::handlers::run_once::file_checksum(env.fs.as_ref(), &abs).unwrap();
        let dir = env.paths.handler_data_dir("vim", HANDLER_INSTALL);
        env.fs.mkdir_all(&dir).unwrap();
        env.fs
            .write_file(
                &dir.join(format!("install.sh-{checksum}")),
                b"completed|100",
            )
            .unwrap();
        let mut diffs = Vec::new();
        let h = run_once_health(&abs, "vim", "vim", HANDLER_INSTALL, &ctx, false, &mut diffs);
        assert!(matches!(h, Health::Deployed));
        assert!(diffs.is_empty());
    }

    #[test]
    fn run_once_health_older_version_carries_line_summary_when_snapshot_present() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("install.sh", "echo new\necho line2\n")
            .done()
            .build();
        let ctx = ctx_for(&env);
        let abs = env.dotfiles_root.join("vim/install.sh");

        let dir = env.paths.handler_data_dir("vim", HANDLER_INSTALL);
        env.fs.mkdir_all(&dir).unwrap();
        env.fs
            .write_file(&dir.join("install.sh-aaaaaaaaaaaaaaaa"), b"completed|100")
            .unwrap();
        env.fs
            .write_file(
                &dir.join("install.sh-aaaaaaaaaaaaaaaa.snapshot"),
                b"echo old\n",
            )
            .unwrap();

        let mut diffs = Vec::new();
        let h = run_once_health(&abs, "vim", "vim", HANDLER_INSTALL, &ctx, false, &mut diffs);
        match h {
            Health::RanOlderVersion { label } => {
                assert!(
                    label.contains("older version"),
                    "label should mention older version, got: {label}"
                );
                assert!(
                    label.contains("lines added") && label.contains("removed"),
                    "label should contain a (N+ M-) summary, got: {label}"
                );
            }
            _ => panic!("expected RanOlderVersion"),
        }
        assert!(diffs.is_empty());
    }

    #[test]
    fn run_once_health_older_version_no_snapshot_falls_back_to_label_only() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("install.sh", "echo new\n")
            .done()
            .build();
        let ctx = ctx_for(&env);
        let abs = env.dotfiles_root.join("vim/install.sh");

        let dir = env.paths.handler_data_dir("vim", HANDLER_INSTALL);
        env.fs.mkdir_all(&dir).unwrap();
        env.fs
            .write_file(&dir.join("install.sh-aaaaaaaaaaaaaaaa"), b"completed|100")
            .unwrap();

        let mut diffs = Vec::new();
        let h = run_once_health(&abs, "vim", "vim", HANDLER_INSTALL, &ctx, true, &mut diffs);
        match h {
            Health::RanOlderVersion { label } => {
                assert!(
                    label.contains("older version") && label.contains("no diff data"),
                    "label should mention `no diff data`, got: {label}"
                );
            }
            _ => panic!("expected RanOlderVersion"),
        }
        assert!(
            diffs.is_empty(),
            "no snapshot should yield no diff entry, got {} entries",
            diffs.len()
        );
    }

    #[test]
    fn run_once_health_show_diff_emits_diff_payload_when_snapshot_present() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("install.sh", "echo new\n")
            .done()
            .build();
        let ctx = ctx_for(&env);
        let abs = env.dotfiles_root.join("vim/install.sh");

        let dir = env.paths.handler_data_dir("vim", HANDLER_INSTALL);
        env.fs.mkdir_all(&dir).unwrap();
        env.fs
            .write_file(&dir.join("install.sh-aaaaaaaaaaaaaaaa"), b"completed|100")
            .unwrap();
        env.fs
            .write_file(
                &dir.join("install.sh-aaaaaaaaaaaaaaaa.snapshot"),
                b"echo old\n",
            )
            .unwrap();

        let mut diffs: Vec<DisplayDiff> = Vec::new();
        let _h = run_once_health(
            &abs,
            "vim",
            "vim-display",
            HANDLER_INSTALL,
            &ctx,
            true,
            &mut diffs,
        );
        assert_eq!(diffs.len(), 1, "expected exactly one diff entry");
        let d = &diffs[0];
        assert_eq!(d.pack, "vim-display");
        assert_eq!(d.file, "install.sh");
        assert_eq!(d.handler, HANDLER_INSTALL);
        assert!(d.body.contains("install.sh (previous run)"));
        assert!(d.body.contains("install.sh (current)"));
        assert!(d.body.contains("-echo old"));
        assert!(d.body.contains("+echo new"));
    }
}

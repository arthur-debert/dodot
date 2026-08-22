//! `adopt` command — move existing files into a pack, creating symlinks back.
//!
//! ## Calling shape
//!
//! ```text
//! dodot adopt <path>...                # pack name inferred per source
//! dodot adopt <path>... --into <pack>  # all sources land in <pack>
//! ```
//!
//! Inference (see [`infer::infer_target`]) reads each source's deployed
//! location and determines:
//!
//! - **Pack name**, when the source root carries pack structure
//!   (`$XDG_CONFIG_HOME/<X>/...` → `<X>`). HOME-rooted sources have no
//!   inherent pack structure and require `--into <pack>`.
//! - **In-pack path**, chosen so re-deploying with `dodot up` lands the
//!   symlink back at the original source — round-trip preservation
//!   relative to `handlers::symlink::resolve_target`.
//! - **Whether the source is a pack-root directory** (e.g. `~/.config/nvim/`),
//!   in which case we expand it into per-child plans rather than making
//!   the whole directory one big symlink-to-pack-root.
//!
//! ## Order of operations
//!
//! `docs/proposals/adopt-safety.lex` §5 fixes the order, and its first
//! rule is that nothing reaches a final pack path until every check
//! that can refuse the run has passed:
//!
//! 1. **Plan** ([`plan`]) — resolve the destination pack, infer each
//!    source's in-pack path, classify entries, check destination
//!    conflicts, check that the sources are readable and the dotfiles
//!    root writable, and reject duplicate or overlapping entries.
//!    Writes nothing, including no inferred pack directory.
//!
//! 2. **Prepare** ([`Preparation`]) — copy the prospective content into
//!    a hidden preparation directory inside the dotfiles root, laid out
//!    at the in-pack paths the plan assigned. A failure here removes the
//!    preparation directory and leaves every final path untouched.
//!
//! 3. **Validate** ([`check_deploy_conflicts`]) — run the cross-pack
//!    deployment conflict analysis against the *prospective* pack tree,
//!    read out of the preparation directory. `--force` does not bypass
//!    it. `--dry-run` reports the plan here and stops.
//!
//! 4. **Publish** — for an inferred pack that did not exist at plan
//!    time, one no-replace rename of the prepared pack directory onto
//!    the pack path: the pack appears complete or does not appear. A
//!    pack path that came into existence after planning makes
//!    publication refuse rather than replace it, and the kernel decides
//!    that inside the same operation that moves the tree.
//!
//! 5. **Replace sources** ([`swap_all`]) — per source, replace the
//!    original with a symlink to its published pack path. Files use a
//!    symlink-at-temp + rename-over-original (POSIX atomic).
//!    Directories use rename-to-backup + symlink + rm-backup
//!    (recoverable, not atomic). A per-source failure removes that
//!    source's pack entry and is reported; sources already replaced
//!    stay replaced.
//!
//! 6. **Finish** — remove the preparation directory.
//!
//! Two pieces of `adopt-safety.lex` are deliberately not here yet, and
//! each has its own work stream:
//!
//! - Publication **into a pack that already exists** still copies into
//!   final pack paths and then validates, which is the pre-proposal
//!   behavior. `#377` (WS03) is where that path moves behind the
//!   preparation directory and gains its recovery record.
//! - The **rest of a failed source replacement's recovery** — removing
//!   the intermediate directories publication created for that entry,
//!   and removing a newly published pack when no source was replaced at
//!   all — is `#378` (WS04). Step 5 currently removes the failed
//!   source's own pack entry and no more.
//!
//! ## Auto-creating packs
//!
//! When all sources point at a single inferred pack name and that pack
//! doesn't exist on disk, adopt creates it — at publication, by
//! renaming the prepared tree into place, so a refused run leaves no
//! pack behind. No `.dodot.toml` is written; the user can run `dodot
//! config gen` later if they want one. When `--into <pack>` is supplied
//! and `<pack>` does not exist, adopt refuses — explicit pack names are
//! typo-checked against the existing pack inventory.

mod infer;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::commands::status;
use crate::commands::{DisplayFile, DisplayNote, DisplayPack, PackStatusResult};
use crate::conflicts;
use crate::fs::Fs;
use crate::packs;
use crate::packs::orchestration::{self, ExecutionContext};
use crate::rules;
use crate::{DodotError, Result};

use self::infer::{infer_target, InferredTarget};

/// Re-export so the round-trip property test in `commands::tests` can
/// drive the same `home.X` / `_home/X/` conventions inference uses.
/// Keeping this internal-but-cross-module ensures inference and the
/// resolver don't drift apart. Test-only because production code
/// always goes through the richer `infer_target` entry point.
#[cfg(test)]
pub(crate) use self::infer::derive_home_in_pack as derive_pack_filename;

// ── Plans ────────────────────────────────────────────────────────────

/// Plan for a single source: the resolved source path, what to call it in the
/// pack, and the destination path.
struct AdoptPlan {
    /// The resolved source (post --no-follow handling).
    source: PathBuf,
    /// Path relative to the pack root — `home.vimrc`, `lua/plugins/init.lua`,
    /// `_darwin/config`. The preparation directory lays content out at
    /// this path, and overlap detection compares entries on it.
    in_pack: PathBuf,
    pack_dest: PathBuf,
    /// `true` if the source is a directory (after --no-follow resolution).
    is_dir: bool,
    /// `true` when `pack_dest` already had content before adoption (only
    /// possible with `--force`). Rollback paths must NOT remove this plan's
    /// `pack_dest`: on copy failure we've preserved the old content in
    /// place; on later failure the new content is committed-destructively
    /// per the user's --force opt-in, and we can't restore the old content
    /// anyway.
    destructive_overwrite: bool,
}

// ── Public entry point ───────────────────────────────────────────────

/// Move sources into a pack, creating symlinks from their original locations.
///
/// `pack_override` is `Some(name)` when the user passed `--into <name>`;
/// `None` lets per-source inference decide. See the module-level docs
/// for the inference rules and two-phase failure semantics.
///
/// `only_os` is `Some(label)` when the user passed `--only-os <label>`.
/// Each source's in-pack path is prepended with a `_<label>/` gate-dir
/// segment so re-deploying via `dodot up` will only land the symlink on
/// hosts matching the gate predicate. The label is validated against the
/// gate table (built-ins + user `[gates]`) at the root level.
pub fn adopt(
    pack_override: Option<&str>,
    sources: &[PathBuf],
    force: bool,
    no_follow: bool,
    dry_run: bool,
    only_os: Option<&str>,
    ctx: &ExecutionContext,
) -> Result<PackStatusResult> {
    // Validate `--only-os` label up front against the resolved root
    // gate table. Failing here gives the user a clear error before any
    // filesystem work happens.
    if let Some(label) = only_os {
        let root_config = ctx.config_manager.root_config()?;
        let mut gates = crate::gates::GateTable::with_builtins();
        if !root_config.gates.is_empty() {
            gates.merge_user(&root_config.gates)?;
        }
        if !gates.contains(label) {
            return Err(DodotError::Config(format!(
                "unknown gate label `{label}` for --only-os: \
                 not in the built-in seed and not defined in [gates]. \
                 Built-ins: darwin, linux, macos, arm64, aarch64, x86_64."
            )));
        }
    }
    if sources.is_empty() {
        return Err(DodotError::Other("no files specified".into()));
    }

    // ── Resolve pack: per-source inference, then aggregate ───────────
    //
    // Exactly one pack per adopt invocation (see
    // `resolve_pack_for_sources` for how candidates aggregate). The
    // single-pack constraint keeps the result shape (one
    // PackStatusResult) and the conflict-check semantics simple. Future
    // work can lift this to multi-pack invocations once the result
    // structure supports it.
    let resolved = resolve_pack_for_sources(pack_override, sources, ctx)?;

    let pack_dir = resolved.pack_dir.clone();
    let pack_display = resolved.display_name.clone();
    let pack_path = ctx.paths.pack_path(&pack_dir);

    // Whether the destination pack is already on disk decides which
    // publication path runs. Only an *inferred* name can be missing:
    // an explicit `--into` naming a missing pack already errored in
    // `resolve_pack_for_sources`.
    let pack_existed = ctx.fs.exists(&pack_path);

    if ctx.fs.exists(&pack_path.join(".dodotignore")) {
        return Err(DodotError::PackInvalid {
            name: pack_display.clone(),
            reason: "pack is marked ignored via .dodotignore".into(),
        });
    }

    // ── Step 1: Plan ─────────────────────────────────────────────────
    //
    // Nothing on disk changes here, so every refusal below leaves the
    // dotfiles root as it was — an inferred pack included.
    let (plans, skipped_already_adopted) = plan(
        &pack_dir,
        &pack_path,
        pack_existed,
        sources,
        pack_override,
        force,
        no_follow,
        only_os,
        ctx,
    )?;

    // If every input was already adopted, there's nothing to do.
    if plans.is_empty() {
        let mut result = adopt_result(&pack_display, &pack_path, &plans, ctx)?;
        result.dry_run = dry_run;
        for msg in skipped_already_adopted {
            result.warnings.push(msg);
        }
        return Ok(result);
    }

    // ── Steps 2–4: Prepare, Validate, Publish ────────────────────────
    //
    // A new pack goes through the preparation directory and publishes
    // with one rename. An existing pack still copies into final paths
    // and validates afterwards — the pre-`adopt-safety` behavior that
    // #377 (WS03) replaces.
    let mut preparation: Option<Preparation> = None;
    if pack_existed {
        if let Err(e) = copy_all(&plans, ctx.fs.as_ref()) {
            cleanup_pack_copies(&plans, ctx.fs.as_ref());
            return Err(e);
        }
        if let Err(e) = check_deploy_conflicts(ctx, None) {
            cleanup_pack_copies(&plans, ctx.fs.as_ref());
            return Err(e);
        }
        if dry_run {
            cleanup_pack_copies(&plans, ctx.fs.as_ref());
            let mut result = adopt_result(&pack_display, &pack_path, &plans, ctx)?;
            result.dry_run = true;
            for msg in skipped_already_adopted {
                result.warnings.push(msg);
            }
            return Ok(result);
        }
    } else {
        let prep = Preparation::create(ctx.fs.as_ref(), ctx.paths.dotfiles_root(), &pack_dir)?;

        if let Err(e) = prep.fill(&plans, ctx.fs.as_ref()) {
            prep.discard(ctx.fs.as_ref());
            return Err(e);
        }

        if let Err(e) = check_deploy_conflicts(ctx, Some((&pack_dir, prep.pack_root()))) {
            prep.discard(ctx.fs.as_ref());
            return Err(e);
        }

        if dry_run {
            prep.discard(ctx.fs.as_ref());
            let mut result = adopt_result(&pack_display, &pack_path, &plans, ctx)?;
            result.dry_run = true;
            for msg in skipped_already_adopted {
                result.warnings.push(msg);
            }
            return Ok(result);
        }

        if let Err(e) = prep.publish_new_pack(&pack_path, ctx.fs.as_ref()) {
            prep.discard(ctx.fs.as_ref());
            return Err(e);
        }
        preparation = Some(prep);
    }

    // ── Step 5: Replace sources ──────────────────────────────────────
    //
    // Per-source, and failures are recorded rather than fatal.
    let failures = swap_all(&plans, ctx.fs.as_ref());

    // ── Step 6: Finish ───────────────────────────────────────────────
    if let Some(prep) = preparation {
        prep.discard(ctx.fs.as_ref());
    }

    let mut result = status::status(Some(std::slice::from_ref(&pack_display)), ctx)?;
    result.dry_run = false;
    for msg in skipped_already_adopted {
        result.warnings.push(msg);
    }

    // Capitalization-heuristic advisory (M5) + brew enrichment (M6).
    //
    // Both gate on the same precondition (at least one AppSupport
    // source) and consume the same brew probe data (the matching
    // installed-cask token for the pack name). See
    // `docs/proposals/macos-paths.lex` §8.1–§8.2.
    //
    // Resolver/pack-tree state is unaffected throughout — these are
    // purely user-facing strings on `PackStatusResult.warnings`.
    let force_home = ctx.config_manager.root_config()?.symlink.force_home.clone();
    let any_app_support = sources.iter().any(|s| {
        absolutize(s)
            .ok()
            .and_then(|abs| {
                let is_dir = ctx.fs.stat(&abs).map(|m| m.is_dir).unwrap_or(false);
                infer::infer_target(&abs, is_dir, ctx.paths.as_ref(), &force_home).ok()
            })
            .map(|t| t.source_root == infer::SourceRoot::AppSupport)
            .unwrap_or(false)
    });

    // Compute the cask match once — both the M5 rename tip and the
    // M6 confirmation/sibling-plist block read from `matches`. adopt
    // is an interactive, on-demand command so populating the cache
    // here is fine (cache_only=false).
    let cache_dir = ctx.paths.probes_brew_cache_dir();
    let now = crate::probe::brew::now_secs_unix();
    let cask_matches = if any_app_support {
        crate::probe::brew::match_folders_to_installed_casks(
            std::slice::from_ref(&pack_display),
            ctx.command_runner.as_ref(),
            &cache_dir,
            now,
            ctx.fs.as_ref(),
            /*cache_only=*/ false,
        )
    } else {
        crate::probe::brew::InstalledCaskMatches::default()
    };
    let cask_token: Option<&str> = cask_matches
        .folder_to_token
        .get(&pack_display)
        .map(String::as_str);

    if pack_override.is_none() && infer::is_gui_app_folder(&pack_display) && any_app_support {
        // Prefer the cask token as the rename suggestion when we have
        // one — for reverse-DNS bundle IDs that's a *much* better
        // suggestion than whitespace-strip-lowercase
        // (`com.colliderli.iina` → `iina` instead of
        // `comcolliderliiina`). Falls back to the lowercase fallback
        // for the spaces/uppercase cases the heuristic also catches.
        let lowercase_fallback: String = pack_display
            .chars()
            .filter(|c| !c.is_whitespace())
            .flat_map(char::to_lowercase)
            .collect();
        let suggested_alias = cask_token.unwrap_or(lowercase_fallback.as_str());
        if !suggested_alias.is_empty() && suggested_alias != pack_display {
            let cask_credit = match cask_token {
                Some(token) => format!(" (matches homebrew cask `{token}`)"),
                None => String::new(),
            };
            result.warnings.push(format!(
                "tip: pack `{pack_display}` looks like a macOS GUI-app folder{cask_credit}. \
                 Consider renaming the pack to `{suggested_alias}` and adding\n  \
                 [symlink.app_aliases]\n  {suggested_alias} = \"{pack_display}\"\n\
                 to your .dodot.toml so future files can use bare paths instead \
                 of `_app/{pack_display}/...`."
            ));
        }
    }

    // Brew-cask enrichment (M6): when the pack name matched an
    // installed cask, append confirmation + sibling-adoption
    // suggestions. macOS-only via `match_folders_to_installed_casks`;
    // on Linux `cask_token` stays `None` and this block is skipped.
    if let Some(token) = cask_token {
        result.warnings.push(format!(
            "homebrew cask `{token}` confirms this is the app-support directory \
             for pack `{pack_display}`."
        ));
        // Pull cask info from cache (now warm) for sibling-plist
        // suggestions. Failures are silent — the confirmation above
        // is already enough signal.
        if let Ok(Some(info)) = crate::probe::brew::info_cask(
            token,
            &cache_dir,
            now,
            ctx.fs.as_ref(),
            ctx.command_runner.as_ref(),
        ) {
            let plists = info.preferences_plists();
            let candidates: Vec<&str> = plists
                .iter()
                .filter_map(|p| {
                    let leaf = p.split('/').next_back()?;
                    if leaf.is_empty() {
                        None
                    } else {
                        Some(leaf)
                    }
                })
                .collect();
            if !candidates.is_empty() {
                let list = candidates.join(", ");
                result.warnings.push(format!(
                    "homebrew also reports preferences for cask `{token}`: {list}. \
                     Adopt them too with `dodot adopt ~/Library/Preferences/<file> --into {pack_display}`."
                ));
            }
        }
    }

    // Plist-aware tip: if any of the adopted plans is a `.plist` file
    // and the user has not yet registered the dodot-plist clean/smudge
    // filters, point them at `dodot git-install-filters`. The up-time
    // prompt also covers this, but adopt is the most likely first
    // moment a user has a plist in a pack — surfacing the install
    // command immediately saves them one round-trip.
    let adopted_any_plist = plans.iter().any(|p| {
        p.source
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.eq_ignore_ascii_case("plist"))
            .unwrap_or(false)
    });
    if adopted_any_plist && !crate::commands::git_filters::is_installed(ctx).unwrap_or(true) {
        result.warnings.push(
            "tip: pack now contains a .plist file. Run `dodot git-install-filters` to enable \
             canonical XML diffs (binary plists become diffable in `git status`/`git diff`)."
                .into(),
        );
    }

    // Adopt failures are real errors — surface them in the same
    // command-wide notes list that drives `[N]` markers for status/up.
    // To keep the model consistent ("every note is referenced by a row"),
    // synthesize an error row in the target pack for the file we tried
    // (and failed) to adopt. Post-rollback the pack doesn't actually
    // contain that file, so this row is purely informational about the
    // attempt — but it anchors the `[N]` back to a visible listing entry
    // instead of leaving an orphaned footnote at the bottom.
    for f in &failures {
        let src_name = f
            .source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| f.source.display().to_string());
        result.notes.push(DisplayNote::error(format!(
            "adopt failed: {}: {}",
            f.source.display(),
            f.reason
        )));
        let note_ref = Some(result.notes.len() as u32);
        if let Some(pack) = result.packs.iter_mut().find(|p| p.name == pack_display) {
            pack.files.push(DisplayFile {
                name: src_name,
                symbol: "×".into(),
                description: "adopt failed".into(),
                status: "error".into(),
                status_label: "error".into(),
                handler: String::new(),
                note_ref,
            });
            pack.recompute_summary();
        }
    }
    Ok(result)
}

// ── Preparation directory ────────────────────────────────────────────

/// Name prefix of the preparation directory a run stages content in.
///
/// It begins with `.`, and that is what keeps the directory out of pack
/// discovery: [`packs::scan_packs`] skips every dotfiles-root directory
/// whose name starts with `.` except `.config`, so a `dodot status`
/// running mid-adopt reports the user's packs and not a half-copied one.
///
/// Adopt writes no marker file inside the directory to achieve that. A
/// `.dodotignore` there would travel with the rename that publishes the
/// pack and hide the pack it had just published, so the exclusion has to
/// be a property of the directory's *name* — the published tree holds
/// the user's content and nothing else.
const PREPARATION_PREFIX: &str = ".dodot-adopt-";

/// How many names [`Preparation::create`] tries before giving up.
///
/// [`nonce`] cannot repeat among live processes, so a taken name means
/// a leftover from a dead process whose pid this one now carries, and
/// the next name differs by the counter. More than a couple of those in
/// a row is not a collision to ride out — it is a dotfiles root in a
/// state worth reporting.
const PREPARATION_NAME_ATTEMPTS: u32 = 8;

/// A run's staging area: `<dotfiles_root>/.dodot-adopt-<nonce>/`, holding
/// the prospective pack tree at `<prefix><nonce>/<pack_dir>/<in_pack>`.
///
/// It lives inside the dotfiles root because publication is a `rename`
/// of the prepared pack directory onto the final pack path, and `rename`
/// does not cross filesystems. Copying *into* it may cross one — the
/// user's `$HOME` and their dotfiles repo can sit on different
/// filesystems — which is the same work adopt already did before.
///
/// The `<nonce>` suffix names one run's directory, and creating it
/// exclusively ([`Preparation::create`]) is what makes that ownership
/// real: a run either makes the directory or picks another name, never
/// joins one. The fixed prefix makes a leftover from a killed process
/// identifiable. Adopt never publishes from a leftover and never
/// deletes one: only the run that created a preparation directory has a
/// handle on it.
struct Preparation {
    /// The `.dodot-adopt-<nonce>` directory itself.
    root: PathBuf,
    /// The prospective pack tree, `root/<pack_dir>`. Publishing a new
    /// pack renames this onto the final pack path.
    pack_root: PathBuf,
}

impl Preparation {
    /// Create an empty preparation directory in `dotfiles_root` holding
    /// a prospective pack directory named `pack_dir`.
    ///
    /// The root is created exclusively, which is what makes the
    /// returned `Preparation` this run's alone: a name that is already
    /// taken — a concurrent run's directory, or a leftover from a
    /// killed one — sends the loop to the next name instead of being
    /// adopted as if this run had made it. Without that, two runs could
    /// fill and discard one directory, and either could publish the
    /// other's content or delete the tree the other was still
    /// validating.
    ///
    /// Failing to create the pack directory inside removes the root
    /// just claimed, so a failure here leaves the dotfiles root exactly
    /// as it was.
    fn create(fs: &dyn Fs, dotfiles_root: &Path, pack_dir: &str) -> Result<Self> {
        let mut attempt = 0;
        loop {
            let root = dotfiles_root.join(format!("{PREPARATION_PREFIX}{}", nonce()));
            match fs.mkdir_exclusive(&root) {
                Ok(()) => {
                    let pack_root = root.join(pack_dir);
                    if let Err(e) = fs.mkdir_all(&pack_root) {
                        remove_best_effort(fs, &root);
                        return Err(e);
                    }
                    return Ok(Preparation { root, pack_root });
                }
                Err(e) => {
                    attempt += 1;
                    if !crate::fs::is_already_exists(&e) || attempt >= PREPARATION_NAME_ATTEMPTS {
                        return Err(e);
                    }
                }
            }
        }
    }

    fn pack_root(&self) -> &Path {
        &self.pack_root
    }

    /// Copy every plan's source into the prospective pack tree at the
    /// in-pack path the plan assigned.
    ///
    /// Nothing outside the preparation directory is written, so the
    /// caller answers a failure by discarding the whole directory.
    fn fill(&self, plans: &[AdoptPlan], fs: &dyn Fs) -> Result<()> {
        for plan in plans {
            let dest = self.pack_root.join(&plan.in_pack);
            if let Some(parent) = dest.parent() {
                if !parent.as_os_str().is_empty() && !fs.exists(parent) {
                    fs.mkdir_all(parent)?;
                }
            }
            copy_tree(&plan.source, &dest, fs)?;
        }
        Ok(())
    }

    /// Publish the prospective tree as a new pack: one no-replace
    /// rename onto `pack_path`.
    ///
    /// The pack appears complete or does not appear. A `pack_path` that
    /// came into existence between planning and here makes publication
    /// refuse rather than replace it — nothing has been published yet,
    /// so refusing costs the run nothing.
    ///
    /// The kernel decides that in the same operation that moves the
    /// tree ([`Fs::rename_noreplace`]). Testing `pack_path` here and
    /// then calling a plain `rename` would leave an interval in which
    /// the newcomer arrives after the test and gets replaced by the
    /// rename — and a plain `rename` replaces a symlink or an empty
    /// directory silently, which is exactly the shape a half-finished
    /// concurrent run leaves behind.
    fn publish_new_pack(&self, pack_path: &Path, fs: &dyn Fs) -> Result<()> {
        fs.rename_noreplace(&self.pack_root, pack_path)
            .map_err(|e| {
                if crate::fs::is_already_exists(&e) {
                    DodotError::Other(format!(
                        "pack path {} appeared while adopt was preparing; refusing to \
                         merge into it. Re-run adopt to plan against the pack that now \
                         exists.",
                        pack_path.display()
                    ))
                } else {
                    e
                }
            })
    }

    /// Remove the preparation directory and everything still in it.
    ///
    /// Best effort: this runs on the way out of both the refusal and the
    /// success paths, and a failure to clean up is not a reason to fail
    /// a run that has otherwise done what it said.
    fn discard(&self, fs: &dyn Fs) {
        remove_best_effort(fs, &self.root);
    }
}

// ── Result assembly ──────────────────────────────────────────────────

/// The command's result: the destination pack's status, or — when the
/// pack is not on disk — a listing of what the plan would have adopted.
///
/// A `--dry-run` against an inferred pack that does not exist yet has no
/// pack to report the status of, and creating one to have something to
/// render is exactly what `docs/proposals/adopt-safety.lex` §5.3 forbids.
/// The synthesized rows report the same plan the real run then executes.
fn adopt_result(
    pack_display: &str,
    pack_path: &Path,
    plans: &[AdoptPlan],
    ctx: &ExecutionContext,
) -> Result<PackStatusResult> {
    if ctx.fs.exists(pack_path) {
        return status::status(Some(&[pack_display.to_string()]), ctx);
    }

    let files: Vec<DisplayFile> = plans
        .iter()
        .map(|p| DisplayFile {
            name: p.in_pack.display().to_string(),
            symbol: "+".into(),
            description: format!("would adopt {}", p.source.display()),
            status: "pending".into(),
            status_label: "planned".into(),
            handler: String::new(),
            note_ref: None,
        })
        .collect();

    Ok(PackStatusResult {
        message: None,
        dry_run: false,
        packs: vec![DisplayPack::new(pack_display.to_string(), files)],
        warnings: Vec::new(),
        notes: Vec::new(),
        conflicts: Vec::new(),
        ignored_packs: Vec::new(),
        inactive_packs: Vec::new(),
        view_mode: ctx.view_mode.as_str().into(),
        group_mode: ctx.group_mode.as_str().into(),
        diffs: Vec::new(),
        shell_hookup: status::shell_hookup_notice(ctx),
        failed: false,
    })
}

// ── Pack resolution (override / inference / aggregation) ─────────────

/// Outcome of resolving the (single) pack the entire adopt invocation
/// targets.
struct ResolvedPack {
    /// On-disk directory name (may carry a `NNN-` ordering prefix).
    pack_dir: String,
    /// User-facing display name (ordering prefix stripped).
    display_name: String,
}

/// Determine which pack the entire adopt invocation lands in.
///
/// Two paths:
///
/// - `pack_override` is `Some(name)`: use exactly that name. The pack
///   must already exist — this is the typo-guard the user opts into by
///   spelling out `--into`. Resolved through `resolve_pack_dir_name`
///   so display-name and raw-on-disk-name (`010-nvim` ↔ `nvim`) both
///   work.
/// - `pack_override` is `None`: run inference per source, require all
///   to agree on a single inferred name (or all decline; in the latter
///   case we error pointing at `--into`). The inferred name resolves
///   through `resolve_pack_dir_name` for typo-equivalent matches; if
///   no match exists, the inferred name is taken as the on-disk
///   directory name and the pack is auto-created upstream.
fn resolve_pack_for_sources(
    pack_override: Option<&str>,
    sources: &[PathBuf],
    ctx: &ExecutionContext,
) -> Result<ResolvedPack> {
    if let Some(name) = pack_override {
        // Explicit --into: resolve against existing packs, error on miss.
        let pack_dir = orchestration::resolve_pack_dir_name(name, ctx)?;
        let display_name = packs::display_name_for(&pack_dir).to_string();
        return Ok(ResolvedPack {
            pack_dir,
            display_name,
        });
    }

    // No override: collect per-source inferences, demand consensus.
    let force_home = ctx.config_manager.root_config()?.symlink.force_home.clone();

    let fs = ctx.fs.as_ref();
    let mut candidates: BTreeSet<String> = BTreeSet::new();
    let mut declined: Vec<PathBuf> = Vec::new();
    for raw in sources {
        let abs = absolutize(raw)?;
        // Existence check before inference: if a missing XDG pack-root
        // dir (typo or not-yet-created path) reaches inference, it
        // looks like a non-dir and gets refused as `LooseXdgFile`,
        // which is misleading. Propagate NotFound here so the user
        // sees the same "source does not exist" message they'd get for
        // any missing source — preflight covers the same case but
        // running it from this earlier inference pass keeps the error
        // path uniform.
        if !fs.exists(&abs) && !fs.is_symlink(&abs) {
            return Err(DodotError::Fs {
                path: abs,
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "source does not exist"),
            });
        }
        let is_dir = fs.stat(&abs).map(|m| m.is_dir).unwrap_or(false);
        match infer_target(&abs, is_dir, ctx.paths.as_ref(), &force_home) {
            Ok(t) => match t.natural_pack {
                Some(name) => {
                    candidates.insert(name);
                }
                None => declined.push(abs),
            },
            Err(e) => {
                return Err(DodotError::Other(format!(
                    "refusing to adopt {}: {e}",
                    abs.display()
                )))
            }
        }
    }

    match candidates.len() {
        0 => Err(DodotError::Other(format!(
            "could not infer a pack name for {} source(s); pass --into <pack>",
            declined.len()
        ))),
        1 => {
            // Sole candidate: prefer an existing pack with this display
            // name (handles `010-nvim` on-disk vs `nvim` inferred), else
            // fall through to use the inferred name as the on-disk dir.
            let inferred = candidates.into_iter().next().unwrap();
            let pack_dir = orchestration::resolve_pack_dir_name(&inferred, ctx)
                .unwrap_or_else(|_| inferred.clone());
            let display_name = packs::display_name_for(&pack_dir).to_string();
            // If a HOME source declined inference but we still resolved
            // a pack via the XDG sources, that's fine — they'll all land
            // in the same pack. Their in-pack paths use the HOME prefixes
            // so they round-trip regardless of pack name.
            let _ = declined;
            Ok(ResolvedPack {
                pack_dir,
                display_name,
            })
        }
        _ => {
            let names: Vec<String> = candidates.into_iter().collect();
            Err(DodotError::Other(format!(
                "sources infer different packs ({}); split into separate adopt \
                 invocations or pass --into <pack> to force a single destination",
                names.join(", ")
            )))
        }
    }
}

// ── Step 1: Plan ─────────────────────────────────────────────────────

/// Resolve every source into an [`AdoptPlan`] and run every check that
/// can refuse the run, writing nothing.
///
/// `pack_exists` says whether `pack_path` is on disk. When it is not,
/// the pack is one this run would publish, so the writability probe goes
/// to the dotfiles root — the directory the preparation directory is
/// created in and the rename lands in — instead of a pack path that does
/// not exist yet and must not be created to be tested.
///
/// Returns the plans and the human-readable messages for sources that
/// were skipped because they are already adopted.
#[allow(clippy::too_many_arguments)]
fn plan(
    pack_name: &str,
    pack_path: &Path,
    pack_exists: bool,
    sources: &[PathBuf],
    pack_override: Option<&str>,
    force: bool,
    no_follow: bool,
    only_os: Option<&str>,
    ctx: &ExecutionContext,
) -> Result<(Vec<AdoptPlan>, Vec<String>)> {
    let fs = ctx.fs.as_ref();
    let dotfiles_root = ctx.paths.dotfiles_root().to_path_buf();
    let data_dir = ctx.paths.data_dir().to_path_buf();

    let root_config = ctx.config_manager.root_config()?;
    let pack_config = ctx.config_manager.config_for_pack(pack_path)?;
    let ignore_patterns = {
        let mut combined = root_config.pack.ignore.clone();
        combined.extend(pack_config.pack.ignore.iter().cloned());
        combined
    };
    // The merged force_home list: pack-level overrides root, but for
    // adopt we feed both layers to inference so a user's pack-scoped
    // force_home addition is honored. The resolver does the same merge
    // when deploying.
    let force_home = {
        let mut combined = root_config.symlink.force_home.clone();
        combined.extend(pack_config.symlink.force_home.iter().cloned());
        combined
    };

    let mut plans: Vec<AdoptPlan> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();

    for raw_source in sources {
        let abs = absolutize(raw_source)?;

        if !fs.exists(&abs) && !fs.is_symlink(&abs) {
            return Err(DodotError::Fs {
                path: abs,
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "source does not exist"),
            });
        }

        // Already-adopted detection: source is a symlink whose target lives
        // inside the dotfiles root or the data dir.
        //
        // Two sub-cases, distinguished so the user knows what to do next:
        //
        // - `target.starts_with(&data_dir)` — fully managed via dodot's
        //   chain (`user_path → data_link → source`). Nothing to do.
        //
        // - `target.starts_with(&dotfiles_root)` (and not data_dir) — the
        //   source is in a pack but the user's symlink points *directly*
        //   at it, missing dodot's data-link layer. `dodot up <pack>` will
        //   upgrade this to the full chain transparently — point users at
        //   that command instead of leaving them confused about why
        //   status still shows "pending".
        if fs.is_symlink(&abs) {
            if let Ok(raw_target) = fs.readlink(&abs) {
                // readlink() returns the symlink's raw target which may be
                // a relative path; resolve against the link's parent so
                // `starts_with` checks work for both forms.
                let resolved = crate::equivalence::resolve_symlink_target(&abs, &raw_target);
                if resolved.starts_with(&data_dir) {
                    skipped.push(format!(
                        "skipped: {} is already managed by dodot (-> {})",
                        abs.display(),
                        raw_target.display()
                    ));
                    continue;
                }
                if resolved.starts_with(&dotfiles_root) {
                    skipped.push(format!(
                        "skipped: {} is a direct symlink to pack source (-> {}); \
                         run `dodot up {}` to upgrade it to dodot's full chain",
                        abs.display(),
                        raw_target.display(),
                        pack_name,
                    ));
                    continue;
                }
            }
        }

        // Decide whether to follow a symlink source or treat it as the link itself.
        let lmeta = fs.lstat(&abs)?;
        let is_source_symlink = lmeta.is_symlink;
        let treat_as_link = is_source_symlink && no_follow;

        // Effective metadata for is_dir and for the copy operation.
        let is_dir = if treat_as_link {
            false
        } else {
            let smeta = fs.stat(&abs)?;
            smeta.is_dir
        };

        let inferred =
            infer_target(&abs, is_dir, ctx.paths.as_ref(), &force_home).map_err(|reason| {
                DodotError::Other(format!("refusing to adopt {}: {reason}", abs.display()))
            })?;

        // Pick the override-aware encoding when --into changed the pack
        // name. This keeps `_xdg/<X>/...` and `_app/<X>/...`
        // round-trip-correct even when the user reroutes the file into
        // a different pack than its source-root segment suggests.
        let in_pack = match (&inferred.natural_pack, pack_override) {
            (Some(natural), Some(over)) if natural != over => inferred.in_pack_override.clone(),
            _ => inferred.in_pack_natural.clone(),
        };
        // `--only-os <label>` wraps the entry in a `_<label>/`
        // gate dir so the deployed symlink only lands on matching
        // hosts. The wrap composes with routing prefixes (`_home/`,
        // `_xdg/`, ...) — those still work after the gate dir strips
        // on a matching host.
        let in_pack = if let Some(label) = only_os {
            std::path::PathBuf::from(format!("_{label}")).join(&in_pack)
        } else {
            in_pack
        };

        if inferred.expand_children {
            // Source IS a pack-root directory under XDG (or AppSupport)
            // — enumerate children and adopt each as a top-level pack
            // entry. This is the "I want this whole `~/.config/nvim/`
            // to become the `nvim` pack" ergonomic.
            //
            // Override-aware: if `--into` rerouted the destination pack
            // (so `pack_override` differs from the natural pack name),
            // each child must use the explicit-prefix encoding
            // (`_xdg/<X>/<child>`, `_app/<X>/<child>`) so the round-trip
            // still lands the deployed file at the original location.
            // The same rule that applies to file sources applies here.
            let override_differs = matches!(
                (&inferred.natural_pack, pack_override),
                (Some(natural), Some(over)) if natural != over
            );
            let entries = fs.read_dir(&abs)?;
            for entry in entries {
                let child_in_pack = expand_child_in_pack(&inferred, &entry.name, override_differs);
                // Same gate-dir wrap as the single-source path.
                let child_in_pack = if let Some(label) = only_os {
                    std::path::PathBuf::from(format!("_{label}")).join(&child_in_pack)
                } else {
                    child_in_pack
                };
                push_plan(
                    &mut plans,
                    fs,
                    &abs.join(&entry.name),
                    pack_path,
                    &child_in_pack,
                    no_follow,
                    force,
                    &ignore_patterns,
                )?;
            }
        } else {
            push_plan(
                &mut plans,
                fs,
                &abs,
                pack_path,
                &in_pack,
                no_follow,
                force,
                &ignore_patterns,
            )?;
        }
    }

    // Entries are also checked against each other: two entries landing
    // at the same in-pack path, or one entry containing another.
    check_overlaps(&plans)?;

    // Permission pre-flight. We do this after planning so every error up to
    // this point gives precise guidance; perms check catches late issues.
    let _ = pack_name;
    check_writable(
        fs,
        if pack_exists {
            pack_path
        } else {
            &dotfiles_root
        },
    )?;
    for plan in &plans {
        // Pass the plan's `is_dir` (already resolved with `--no-follow`
        // semantics) so a symlink-to-dir under `--no-follow` isn't probed
        // via `read_dir` on the target.
        check_readable(fs, &plan.source, plan.is_dir)?;
        if let Some(src_parent) = plan.source.parent() {
            check_writable(fs, src_parent)?;
        }
    }

    Ok((plans, skipped))
}

/// Refuse a plan set in which one entry contains another, on either
/// side of the adoption.
///
/// No publication order makes such a pair come out right. Given
/// `dodot adopt ~/.config/nvim/lua ~/.config/nvim/lua/plugins/init.lua`:
/// replace the directory source first and the file source now resolves
/// through the new symlink back into the pack, so replacing it
/// overwrites the pack's own entry with a symlink to itself; replace the
/// file first and publishing the directory buries it.
///
/// The check runs on source paths *and* in-pack paths, and treats
/// entries that arrived through directory expansion no differently from
/// ones the user typed. Same-path duplicates are caught earlier, in
/// [`push_plan`], where the second entry's in-pack path is compared
/// against the plans already built.
///
/// Which of the two overlapped decides what the refusal says. Source
/// containment is a directory the user already asked for wholesale, so
/// the remedy is to name it alone. Two sources that don't contain each
/// other can still nest once inference has placed them: with `--into
/// nvim`, `~/.config/other/lua` lands at `_xdg/other/lua` because the
/// override reroutes it, while `~/.config/nvim/_xdg/other` is already
/// written in that encoding and lands at `_xdg/other`. Calling one of
/// those sources the container would describe a containment the user's
/// arguments do not have, so that refusal names the two pack paths
/// instead and asks for destinations that don't nest.
fn check_overlaps(plans: &[AdoptPlan]) -> Result<()> {
    for (i, a) in plans.iter().enumerate() {
        for b in &plans[i + 1..] {
            if let Some((outer, inner)) = source_containment(a, b) {
                return Err(DodotError::Other(format!(
                    "{} contains {}; adopt the outer one alone — adopting a \
                     directory already carries its contents",
                    outer.source.display(),
                    inner.source.display()
                )));
            }
            if let Some((outer, inner)) = in_pack_containment(a, b) {
                return Err(DodotError::Other(format!(
                    "{} and {} would land at {} and {} in the pack, one inside \
                     the other; adopt them in one run only if their pack paths \
                     don't nest, or adopt them separately",
                    outer.source.display(),
                    inner.source.display(),
                    outer.in_pack.display(),
                    inner.in_pack.display()
                )));
            }
        }
    }
    Ok(())
}

/// `(outer, inner)` when one plan's source path contains the other's.
fn source_containment<'a>(
    a: &'a AdoptPlan,
    b: &'a AdoptPlan,
) -> Option<(&'a AdoptPlan, &'a AdoptPlan)> {
    if b.source.starts_with(&a.source) {
        Some((a, b))
    } else if a.source.starts_with(&b.source) {
        Some((b, a))
    } else {
        None
    }
}

/// `(outer, inner)` when one plan's in-pack path contains the other's.
fn in_pack_containment<'a>(
    a: &'a AdoptPlan,
    b: &'a AdoptPlan,
) -> Option<(&'a AdoptPlan, &'a AdoptPlan)> {
    if b.in_pack.starts_with(&a.in_pack) {
        Some((a, b))
    } else if a.in_pack.starts_with(&b.in_pack) {
        Some((b, a))
    } else {
        None
    }
}

/// Compute the in-pack path for one child of an expanded pack-root
/// directory.
///
/// `override_differs` is true when `--into <Y>` rerouted to a pack
/// other than the source's natural pack name. In that case children
/// need the explicit-prefix encoding (`_xdg/<X>/<child>`,
/// `_app/<X>/<child>`) so Priority 2's directory prefixes bypass
/// pack-namespacing — round-trip unchanged from the file-source case.
///
/// When `override_differs` is false (no override, or override matches
/// inferred name), the natural-pack encoding wins: bare `<child>` for
/// XDG (default rule routes back via the matching pack name), and
/// `_app/<X>/<child>` for AppSupport (the `_app/` prefix is mandatory
/// even at natural pack name — see `docs/proposals/macos-paths.lex`
/// §7.2).
fn expand_child_in_pack(
    parent: &InferredTarget,
    child_name: &str,
    override_differs: bool,
) -> PathBuf {
    use self::infer::SourceRoot;
    match parent.source_root {
        SourceRoot::XdgConfig => {
            if override_differs {
                // `parent.in_pack_override` is `_xdg/<X>` for the
                // pack-root dir itself; append child basename per entry.
                parent.in_pack_override.join(child_name)
            } else {
                PathBuf::from(child_name)
            }
        }
        SourceRoot::AppSupport => {
            // `parent.in_pack_override` is `_app/<X>` for the dir itself.
            // AppSupport always needs the prefix, so the override flag
            // doesn't change behavior here.
            parent.in_pack_override.join(child_name)
        }
        SourceRoot::Home => {
            // Not currently produced by inference (HOME never expands);
            // fall back to bare child name to keep the helper total.
            PathBuf::from(child_name)
        }
        SourceRoot::Library => {
            // `parent.in_pack_override` is `_lib/<sub>` (e.g.
            // `_lib/Preferences`) when expansion is enabled; append the
            // child filename to land each entry under
            // `_lib/<sub>/<child>` so deploy routes back through the
            // Priority 2d prefix.
            parent.in_pack_override.join(child_name)
        }
    }
}

/// Build and validate a single AdoptPlan, appending it to `plans`.
///
/// Centralises the destination-conflict, ignore-pattern, and per-invocation
/// collision checks so they're applied uniformly between the regular
/// path and the directory-expansion path.
#[allow(clippy::too_many_arguments)]
fn push_plan(
    plans: &mut Vec<AdoptPlan>,
    fs: &dyn Fs,
    source: &Path,
    pack_path: &Path,
    in_pack: &Path,
    no_follow: bool,
    force: bool,
    ignore_patterns: &[String],
) -> Result<()> {
    let lmeta = fs.lstat(source)?;
    let is_source_symlink = lmeta.is_symlink;
    let treat_as_link = is_source_symlink && no_follow;
    let is_dir = if treat_as_link {
        false
    } else {
        fs.stat(source)?.is_dir
    };

    // Filename-ignore check against pack + root ignore patterns.
    //
    // Ignore patterns apply to *top-level pack entries* (matching
    // `rules::Scanner::walk_pack`'s semantics on `dodot up`). For a
    // nested adopt like `lua/plugins/foo.lua`, the top-level entry is
    // `lua/`, so we test the *first* path component — not the leaf
    // basename. Using the leaf would let through adoptions that
    // `dodot up` would later silently ignore (or vice versa).
    use std::path::Component;
    let top_level_name = in_pack
        .components()
        .find_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .unwrap_or_else(|| in_pack.display().to_string());
    if rules::should_skip_entry(&top_level_name, ignore_patterns) {
        return Err(DodotError::Other(format!(
            "refusing to adopt {}: top-level entry '{}' matches an ignore pattern or is reserved",
            source.display(),
            top_level_name
        )));
    }

    let pack_dest = pack_path.join(in_pack);

    // Destination conflict check. With --force, we'll remove the existing
    // destination before copy; without, this is a hard refusal.
    let dest_exists = fs.exists(&pack_dest) || fs.is_symlink(&pack_dest);
    if dest_exists && !force {
        return Err(DodotError::SymlinkConflict { path: pack_dest });
    }

    // Cross-plan filename collision: can't adopt two things with the same
    // pack-relative path in a single invocation.
    if plans.iter().any(|p| p.pack_dest == pack_dest) {
        return Err(DodotError::Other(format!(
            "two sources produce the same pack path '{}'; adopt them separately",
            in_pack.display()
        )));
    }

    plans.push(AdoptPlan {
        source: source.to_path_buf(),
        in_pack: in_pack.to_path_buf(),
        pack_dest,
        is_dir,
        destructive_overwrite: dest_exists,
    });
    Ok(())
}

/// Resolve a possibly-relative path to an absolute, lexically-normalized one.
/// Relative inputs resolve against CWD, then `..` and `.` are collapsed
/// without touching the filesystem.
fn absolutize(raw: &Path) -> Result<PathBuf> {
    let abs = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| DodotError::Fs {
                path: raw.to_path_buf(),
                source: e,
            })?
            .join(raw)
    };
    Ok(crate::equivalence::normalize_path(&abs))
}

fn check_writable(fs: &dyn Fs, dir: &Path) -> Result<()> {
    // Probe write by creating and removing a unique marker file.
    //
    // Deliberately not under `PREPARATION_PREFIX`: that prefix names
    // preparation directories, and a leftover probe file wearing it
    // would read as one.
    let probe = dir.join(format!(".dodot-write-probe-{}", nonce()));
    fs.write_file(&probe, b"").map_err(|e| {
        DodotError::Other(format!("not writable: {}: {}", dir.display(), err_msg(&e)))
    })?;
    let _ = fs.remove_file(&probe);
    Ok(())
}

fn check_readable(fs: &dyn Fs, path: &Path, is_dir: bool) -> Result<()> {
    // For directories, read_dir; for files or symlinks, lstat (which does
    // not follow) is enough — we don't need to reach through a symlink
    // target, especially under `--no-follow`.
    if is_dir {
        fs.read_dir(path).map(|_| ())
    } else {
        fs.lstat(path).map(|_| ())
    }
}

// ── Publication into an existing pack (pre-adopt-safety; #377) ────

fn copy_all(plans: &[AdoptPlan], fs: &dyn Fs) -> Result<()> {
    for plan in plans {
        let had_existing_dest = fs.exists(&plan.pack_dest) || fs.is_symlink(&plan.pack_dest);
        // Ensure parent directory exists. Expansion under XDG can place
        // children at the pack root (no missing parent), but a deeply
        // nested in-pack path (e.g. `lua/plugins/foo.lua`) needs the
        // intermediate directories created before copy.
        if let Some(parent) = plan.pack_dest.parent() {
            if !parent.as_os_str().is_empty() && !fs.exists(parent) {
                fs.mkdir_all(parent)?;
            }
        }
        if had_existing_dest {
            // --force path: stage the new content into a sibling temp path
            // first so a failed copy leaves the old destination intact.
            // Only after the copy succeeds do we remove the old content and
            // move the stage into place.
            let stage = temp_sibling(&plan.pack_dest, "stage");
            if let Err(e) = copy_tree(&plan.source, &stage, fs) {
                remove_best_effort(fs, &stage);
                return Err(e);
            }
            remove_path(&plan.pack_dest, fs)?;
            if let Err(e) = fs.rename(&stage, &plan.pack_dest) {
                remove_best_effort(fs, &stage);
                return Err(e);
            }
        } else {
            copy_tree(&plan.source, &plan.pack_dest, fs)?;
        }
    }
    Ok(())
}

fn remove_path(path: &Path, fs: &dyn Fs) -> Result<()> {
    if fs.is_symlink(path) {
        fs.remove_file(path)
    } else if fs.is_dir(path) {
        fs.remove_dir_all(path)
    } else {
        fs.remove_file(path)
    }
}

/// Recursively copy `src` into `dst`. Preserves inner symlinks as symlinks
/// (does not follow them) and Unix permissions on files and directories.
fn copy_tree(src: &Path, dst: &Path, fs: &dyn Fs) -> Result<()> {
    let meta = fs.lstat(src)?;
    if meta.is_symlink {
        let target = fs.readlink(src)?;
        fs.symlink(&target, dst)?;
        return Ok(());
    }
    if meta.is_dir {
        fs.mkdir_all(dst)?;
        // Best-effort mode preserve on the directory itself; ignore failures
        // (tempdirs on some platforms reject chmod on freshly-created dirs).
        let _ = fs.set_permissions(dst, meta.mode);
        for entry in fs.read_dir(src)? {
            copy_tree(&entry.path, &dst.join(&entry.name), fs)?;
        }
        return Ok(());
    }
    if meta.is_file {
        fs.copy_file(src, dst)?;
        let _ = fs.set_permissions(dst, meta.mode);
        return Ok(());
    }
    Err(DodotError::Other(format!(
        "unsupported file type: {}",
        src.display()
    )))
}

fn cleanup_pack_copies(plans: &[AdoptPlan], fs: &dyn Fs) {
    for plan in plans {
        // Destructive-overwrite plans: on copy failure, `pack_dest` still
        // holds the preserved old content; on later failure the new
        // content is committed-destructively per --force. Either way,
        // don't remove.
        if plan.destructive_overwrite {
            continue;
        }
        remove_best_effort(fs, &plan.pack_dest);
    }
}

fn remove_best_effort(fs: &dyn Fs, path: &Path) {
    if fs.is_symlink(path) {
        let _ = fs.remove_file(path);
    } else if fs.is_dir(path) {
        let _ = fs.remove_dir_all(path);
    } else if fs.exists(path) {
        let _ = fs.remove_file(path);
    }
}

// ── Step 3: Validate ──────────────────────────────────────────────

/// Refuse the run if deploying the pack tree would collide with another
/// pack. `--force` does not bypass this.
///
/// `prospective` is `Some((pack_dir, prepared_root))` when the entries
/// being adopted are still in the preparation directory — the new-pack
/// path. The prepared tree is planned as a pack named `pack_dir` but
/// read out of `prepared_root`, and its intents join whatever the pack
/// of that name already contributes, so the analysis sees the pack's
/// current entries composed with the prepared ones at their final
/// in-pack paths. `detect_cross_pack_conflicts` only flags claims from
/// *different* packs, so composing under one name is what keeps a pack
/// from conflicting with its own prospective content.
///
/// `None` means the entries are already at their final pack paths, which
/// is how an existing pack still publishes until #377 (WS03).
///
/// Intents are collected in [`PreprocessMode::Passive`](crate::preprocessing::PreprocessMode::Passive):
/// the question here is only which deployment targets each pack claims,
/// and answering it actively would render every staged `*.tmpl` and
/// resolve every staged secret — writing the rendered output and its
/// baseline into the datastore, and prompting the user's secret
/// provider — before adopt has decided whether the run goes ahead. A
/// conflict refusal and `--dry-run` both leave that behind, which is the
/// same reason `status` reads passively (`docs/proposals/secrets.lex`
/// §7.4). Passive planning reads a preprocessor entry's cached baseline
/// and falls back to a passthrough placeholder when it has none, so the
/// entry still claims its target and still takes part in the analysis.
fn check_deploy_conflicts(
    ctx: &ExecutionContext,
    prospective: Option<(&str, &Path)>,
) -> Result<()> {
    let root_config = ctx.config_manager.root_config()?;
    let packs::DiscoveredPacks { packs: all, .. } = packs::scan_packs(
        ctx.fs.as_ref(),
        ctx.paths.dotfiles_root(),
        &root_config.pack.ignore,
    )?;

    let mut pack_intents = Vec::new();
    for mut pack in all {
        let pack_config = ctx.config_manager.config_for_pack(&pack.path)?;
        pack.config = pack_config.to_handler_config();
        // Propagate per-pack errors: if any pack can't be scanned we can't
        // truthfully say "no conflict with that pack," so refuse outright
        // rather than risk a false negative that lets us mutate into a
        // state `dodot up` will later reject.
        let intents = collect_intents_passive(&pack, ctx)?;
        pack_intents.push((pack.display_name.clone(), intents));
    }

    if let Some((pack_dir, prepared_root)) = prospective {
        let mut prospective_pack = packs::Pack::new(
            pack_dir.to_string(),
            prepared_root.to_path_buf(),
            Default::default(),
        );
        let pack_config = ctx.config_manager.config_for_pack(prepared_root)?;
        prospective_pack.config = pack_config.to_handler_config();
        let intents = collect_intents_passive(&prospective_pack, ctx)?;
        let display = prospective_pack.display_name.clone();
        match pack_intents.iter_mut().find(|(name, _)| *name == display) {
            Some((_, already)) => already.extend(intents),
            None => pack_intents.push((display, intents)),
        }
    }

    let conflicts = conflicts::detect_cross_pack_conflicts(&pack_intents, ctx.fs.as_ref());
    if !conflicts.is_empty() {
        return Err(DodotError::CrossPackConflict { conflicts });
    }
    Ok(())
}

/// The intents a pack would deploy, planned without preprocessing side
/// effects — see [`check_deploy_conflicts`] for why adopt reads this
/// way. Handler warnings are dropped: conflict analysis reads targets,
/// and the run reports through `status` afterwards.
fn collect_intents_passive(
    pack: &packs::Pack,
    ctx: &ExecutionContext,
) -> Result<Vec<crate::operations::HandlerIntent>> {
    orchestration::plan_pack(pack, ctx, crate::preprocessing::PreprocessMode::Passive)
        .map(|plan| plan.intents)
}

// ── Step 5: Replace sources ───────────────────────────────────────

struct AdoptFailure {
    source: PathBuf,
    reason: String,
}

fn swap_all(plans: &[AdoptPlan], fs: &dyn Fs) -> Vec<AdoptFailure> {
    let mut failures = Vec::new();
    for plan in plans {
        let result = if plan.is_dir {
            swap_dir(&plan.source, &plan.pack_dest, fs)
        } else {
            swap_file_atomic(&plan.source, &plan.pack_dest, fs)
        };
        if let Err(e) = result {
            // Roll back just this source: its pack copy is now orphaned.
            remove_best_effort(fs, &plan.pack_dest);
            failures.push(AdoptFailure {
                source: plan.source.clone(),
                reason: format!("{}", e),
            });
        }
    }
    failures
}

/// Atomic file swap: create symlink at a temp sibling, then rename over the
/// original. `rename` is atomic on POSIX and replaces the existing file.
fn swap_file_atomic(source: &Path, pack_dest: &Path, fs: &dyn Fs) -> Result<()> {
    let tmp = temp_sibling(source, "tmp");
    fs.symlink(pack_dest, &tmp)?;
    if let Err(e) = fs.rename(&tmp, source) {
        let _ = fs.remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// Directory swap: rename original aside, create symlink, remove backup. On
/// symlink failure, restore the backup.
fn swap_dir(source: &Path, pack_dest: &Path, fs: &dyn Fs) -> Result<()> {
    let backup = temp_sibling(source, "old");
    fs.rename(source, &backup)?;
    match fs.symlink(pack_dest, source) {
        Ok(()) => {
            let _ = fs.remove_dir_all(&backup);
            Ok(())
        }
        Err(e) => {
            let _ = fs.rename(&backup, source);
            Err(e)
        }
    }
}

// ── helpers ──────────────────────────────────────────────────────

fn temp_sibling(path: &Path, tag: &str) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    parent.join(format!(".dodot-adopt-{}-{}-{}", tag, name, nonce()))
}

/// A name component no other live run can pick: this process's id, a
/// process-global counter, and the current time.
///
/// Pids are unique among live processes and the counter is unique
/// within this one, so two concurrent runs — or two names taken by one
/// run — never coincide; uniqueness is structural rather than a bet on
/// the clock. Time only separates this run from a leftover of a dead
/// process whose pid has since been reused, and
/// [`Preparation::create`] does not rest on even that: it creates the
/// directory exclusively and moves to the next name if it is taken.
fn nonce() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}-{:x}-{:x}", std::process::id(), seq, n)
}

fn err_msg(e: &DodotError) -> String {
    format!("{e}")
}

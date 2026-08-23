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
//!    source's in-pack path, classify entries ([`classify`]), check
//!    destination conflicts, check that the sources are readable and
//!    the dotfiles root writable, and reject duplicate or overlapping
//!    entries. Writes nothing, including no inferred pack directory.
//!
//! 2. **Prepare** ([`Preparation`]) — copy the prospective content into
//!    a hidden preparation directory inside the dotfiles root, laid out
//!    at the in-pack paths the plan assigned. A failure here removes the
//!    preparation directory and leaves every final path untouched.
//!
//! 3. **Validate** ([`check_deploy_conflicts`]) — run the cross-pack
//!    deployment conflict analysis against the *prospective* pack tree:
//!    the destination pack's entries with the ones this run replaces
//!    taken out, plus the prepared entries read out of the preparation
//!    directory. Replacement, not union — a claim the run is about to
//!    overwrite must not refuse it. `--force` does not bypass
//!    it. It refuses on a collision and equally on an analysis it
//!    cannot complete — an `externals.toml` that exists only as a
//!    template dodot has not rendered, or has not re-rendered since
//!    it last changed, declares targets dodot has not read, and
//!    rendering it here is the side effect this step must not have.
//!    `--dry-run` reports the plan here and stops.
//!
//! 4. **Publish** — the one step whose shape depends on the
//!    destination.
//!
//!    For an inferred pack that did not exist at plan time
//!    ([`Preparation::publish_new_pack`]), one no-replace rename of the
//!    prepared pack directory onto the pack path: the pack appears
//!    complete or does not appear. A pack path that came into existence
//!    after planning makes publication refuse rather than replace it,
//!    and the kernel decides that inside the same operation that moves
//!    the tree.
//!
//!    For a pack that already exists
//!    ([`Preparation::publish_into_existing`]), a sequence: per entry,
//!    create the intermediate directories the plan needs, displace an
//!    existing destination into the preparation directory when
//!    `--force` planned to replace it, and rename the prepared entry
//!    into its final path. Each entry appears atomically; the sequence
//!    does not, and is *recoverable* instead — a failure at entry N
//!    undoes every rename this publication made, removes the
//!    intermediate directories it created and left empty, and reports
//!    the in-pack paths it put back. A rollback step that fails in turn
//!    is reported too, naming where the content it could not move is;
//!    that run keeps its preparation directory rather than discarding
//!    what may be the last copy. So is a path another process has
//!    written since publication: a recovery moves a published entry
//!    out only while the path still holds that entry, and returns
//!    displaced content only onto a path that is still free. Only
//!    in-process, though: a killed process leaves the pack
//!    mid-sequence with its preparation directory still on disk.
//!
//! 5. **Replace sources** ([`swap_all`]) — per source, replace the
//!    original with a symlink to its published pack path. Files use a
//!    symlink-at-temp + rename-over-original (POSIX atomic).
//!    Directories use rename-to-backup + symlink + rm-backup
//!    (recoverable, not atomic).
//!
//!    Sources are independent, and a failure does not stop the run:
//!    every planned source is attempted, and each ends either replaced
//!    or untouched with its pack entry taken back out
//!    ([`restore_failed_entry`]). Taking it back out means the whole of
//!    what publication did for that entry — a `--force` displacement
//!    renamed back to its final path, the intermediate directories
//!    publication created and this leaves empty removed, and a pack
//!    this run published removed when no source was replaced at all.
//!    A step of that can fail in turn — or find a path another writer
//!    has taken over, or a published directory another writer has
//!    edited inside ([`still_published`]) — and then nothing is
//!    deleted to get past it: the entry is named along with the path
//!    its content is at, exactly as a publication rollback does it.
//!    Any failed planned source makes the command exit nonzero
//!    ([`PackStatusResult::failed`](crate::commands::PackStatusResult::failed)),
//!    while the result still renders every source, replaced and failed
//!    alike.
//!
//! 6. **Finish** — remove the preparation directory, discarding the
//!    content step 4 displaced. Nothing before this point discards it:
//!    step 5 may still need an entry's pre-adopt content back, so a
//!    `--force` displacement stays recoverable until every source has
//!    been replaced or reported. Two runs do not reach this step: one
//!    whose publication rollback could not finish, and one whose step-5
//!    recovery could not finish either. Both can leave content that
//!    belongs elsewhere inside the preparation directory, and both name
//!    what they could not move rather than deleting it.
//!
//! ## What adopt refuses, and what it leaves alone
//!
//! An entry is adoptable when a later pack scan would read it at the
//! in-pack position adopt gives it — the [`classify`] module owns that
//! predicate and the reasoning behind it. The two answers to a match
//! live here, in [`plan`]:
//!
//! - A **source the user typed** that no scan would read refuses the
//!   run. Answering it with a report and a success would be a lie about
//!   what the command did.
//! - A **child found expanding a directory** that no scan would read
//!   stays at its original path while its adoptable siblings complete,
//!   and is reported once ([`report_left_in_place`]). Reserved
//!   filenames are the exception and refuse in both cases: copying one
//!   into the pack would replace the pack's configuration or hide the
//!   pack.
//!
//! An expanded directory with no adoptable child at all refuses
//! ([`no_adoptable_children`]) rather than reporting every child and
//! exiting zero, which would claim an adoption that did not happen.
//!
//! The report is the only chance there is: `[pack] ignore` matches are
//! invisible in `dodot status` by design, so no later command tells the
//! user the file is still a real file among symlinks. Adopt persists no
//! record of it — the report exists for the one run.
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

mod classify;
mod infer;

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use crate::commands::status;
use crate::commands::{DisplayFile, DisplayNote, DisplayPack, PackStatusResult};
use crate::conflicts;
use crate::fs::{FileId, Fs};
use crate::packs;
use crate::packs::orchestration::{self, ExecutionContext};
use crate::{DodotError, Result};

use self::classify::{classify, pack_dir_refusal, EffectiveIgnore, SkipRule};
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
    /// `true` when `pack_dest` already had content at plan time, which
    /// only `--force` allows. Publication displaces that content into the
    /// preparation directory before renaming this entry into place, so
    /// the run can put it back until step 6 discards it. A destination
    /// occupied at publication time by a plan with this `false` is one
    /// that appeared after planning, and publication refuses it rather
    /// than displacing something the user never agreed to overwrite.
    destructive_overwrite: bool,
}

/// A child of an expanded directory that adopt leaves where it is.
///
/// `docs/proposals/adopt-safety.lex` §3.3: a discovered child no pack
/// scan would read stays a real file at its original path while its
/// adoptable siblings complete, and §4 reports it once. Adopt writes no
/// record of it anywhere — the report exists for the one run, because
/// `[pack] ignore` is silent in `dodot status` by design and adopt is
/// the single moment dodot both knows the fact and was asked about that
/// directory.
struct LeftInPlace {
    /// The child's original path, which this run does not touch.
    path: PathBuf,
    /// The discovery rule that keeps a pack scan from reading it.
    rule: SkipRule,
}

/// What [`plan`] decided: the entries to adopt, and the two kinds of
/// entry it decided against.
struct PlannedRun {
    plans: Vec<AdoptPlan>,
    /// Messages for sources skipped because they are already adopted.
    skipped_already_adopted: Vec<String>,
    /// Discovered children left at their original paths (§3.3).
    left_in_place: Vec<LeftInPlace>,
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

    // The pack directory is a scanned position too, and it is the one
    // adopt can pick for the user: an inferred name comes from the
    // source's own path, so `~/.config/node_modules/settings.json`
    // infers a `node_modules` pack that the default `[pack] ignore`
    // list makes the dotfiles-root scan skip. Publishing it would
    // replace the source with a symlink into a pack no later `dodot up`
    // and no `dodot status` ever reads. Checked before the
    // `.dodotignore` refusal below, in the order the scan applies the
    // two: it filters directory names first, and only reads the marker
    // inside the ones it kept.
    let root_ignore = EffectiveIgnore::root(
        ctx.fs.as_ref(),
        ctx.paths.dotfiles_root(),
        ctx.config_manager.root_config()?.pack.ignore.clone(),
    );
    if let Some(message) = pack_dir_refusal(&pack_dir, &root_ignore, ctx.paths.dotfiles_root()) {
        return Err(DodotError::Other(message));
    }

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
    let PlannedRun {
        plans,
        skipped_already_adopted,
        left_in_place,
    } = plan(
        &pack_display,
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
        report_left_in_place(&mut result, &left_in_place, &pack_display);
        return Ok(result);
    }

    // ── Steps 2–4: Prepare, Validate, Publish ────────────────────────
    //
    // Both destinations stage into the preparation directory and are
    // validated out of it, so no final pack path is written until every
    // check that can refuse the run has passed. They part only at
    // publication: a new pack is one rename of the whole prepared tree,
    // an existing pack a sequence of per-entry renames that undoes
    // itself on failure.
    let prep = Preparation::create(ctx.fs.as_ref(), ctx.paths.dotfiles_root(), &pack_dir)?;

    if let Err(e) = prep.fill(&plans, ctx.fs.as_ref()) {
        prep.discard(ctx.fs.as_ref());
        return Err(e);
    }

    // What publication will overwrite in the destination pack, so
    // validation plans the tree the run leaves rather than the union of
    // before and after. Only meaningful for a pack that already exists:
    // a new pack has nothing to supersede.
    let superseded: Vec<PathBuf> = if pack_existed {
        plans.iter().map(|p| p.in_pack.clone()).collect()
    } else {
        Vec::new()
    };

    if let Err(e) = check_deploy_conflicts(
        ctx,
        ProspectiveTree {
            pack_dir: &pack_dir,
            prepared_root: prep.pack_root(),
            config_at: &pack_path,
            superseded: &superseded,
        },
    ) {
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
        report_left_in_place(&mut result, &left_in_place, &pack_display);
        return Ok(result);
    }

    let published = if pack_existed {
        prep.publish_into_existing(&pack_path, &pack_display, &plans, ctx.fs.as_ref())
    } else {
        // A publication that never moved the tree touched nothing, so
        // its failure leaves nothing behind worth keeping.
        match prep.publish_new_pack(&pack_path, &plans, ctx.fs.as_ref()) {
            Ok(published) => published,
            Err(error) => Published::Failed {
                error,
                keep_preparation: false,
            },
        }
    };
    let publication = match published {
        Published::Ok(publication) => publication,
        Published::Failed {
            error,
            keep_preparation,
        } => {
            // A rollback that could not finish left the pack's pre-adopt
            // content in the preparation directory, and that is the only
            // copy of it. Discarding here is what the error the user is
            // about to read tells them has *not* happened.
            if !keep_preparation {
                prep.discard(ctx.fs.as_ref());
            }
            return Err(error);
        }
    };

    // ── Step 5: Replace sources ──────────────────────────────────────
    //
    // Per-source, and failures are recorded rather than fatal: every
    // planned source is attempted whatever happened to the ones before
    // it. Whatever `--force` displaced is still in the preparation
    // directory while this runs, which is what lets a failed source put
    // its destination's pre-adopt content back.
    let failures = swap_all(&plans, &publication, &pack_path, ctx.fs.as_ref());

    // ── Step 6: Finish ───────────────────────────────────────────────
    //
    // Every source outcome is known now, so the displacements that
    // survived go — that discard is where a successful `--force` takes
    // effect. The exception is a recovery step that failed in turn: the
    // preparation directory can then hold the only copy of a
    // destination's pre-adopt content, and the notes below say where
    // everything the recovery could not move is.
    if failures.iter().all(|f| f.stranded.is_none()) {
        prep.discard(ctx.fs.as_ref());
    }

    // A run whose every source replacement failed on a pack this run
    // published has no pack left to report the status of — §5.5 took it
    // back out with the last entry. The failure rows below are then the
    // whole report.
    let mut result = if ctx.fs.exists(&pack_path) {
        status::status(Some(std::slice::from_ref(&pack_display)), ctx)?
    } else {
        bare_result(&pack_display, Vec::new(), ctx)
    };
    result.dry_run = false;
    for msg in skipped_already_adopted {
        result.warnings.push(msg);
    }
    report_left_in_place(&mut result, &left_in_place, &pack_display);

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
    // (and failed) to adopt. The row describes the attempt rather than
    // the pack's contents: the recovery usually took that entry back
    // out, and where it could not, the notes below the row are what say
    // so. Either way the row anchors the `[N]` to a visible listing
    // entry instead of leaving an orphaned footnote at the bottom.
    for f in &failures {
        let src_name = f
            .source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| f.source.display().to_string());
        // What the note may claim depends on what the recovery
        // achieved. Saying the entry was taken back out when it is
        // still standing sends the user to a pack they think is clean,
        // and it contradicts the note right below that names the path
        // its content is at — so the stranded wording claims nothing
        // about the pack and leaves that note to say what remains.
        let outcome = match f.stranded {
            None => "its pack entry was taken back out",
            Some(_) => "putting the pack back the way it was failed too",
        };
        result.notes.push(DisplayNote::error(format!(
            "adopt failed: {}: {} — {outcome}",
            f.source.display(),
            f.reason
        )));
        let note_ref = Some(result.notes.len() as u32);
        // A recovery step that failed in turn is the one outcome that
        // leaves content somewhere other than where it belongs: the
        // destination `--force` displaced still in the staging
        // directory, or the copy publication put in the pack still
        // standing there. Nothing was deleted to get past it and the
        // staging directory is still on disk, so the note is a pair of
        // paths and an instruction, not an apology. It follows its own
        // failure note so the two read as one account of one source.
        if let Some(entry) = &f.stranded {
            result.notes.push(DisplayNote::error(format!(
                "adopt could not put back the pack's pre-adopt state for {}: \
                 the content it could not move is at {}. Nothing was deleted \
                 to get past that, and the staging directory {} is kept rather \
                 than discarded — move what you need back by hand, then remove \
                 that directory.",
                entry.in_pack,
                entry.at,
                prep.root.display()
            )));
        }
        // The pack has no row of its own when `status` put it somewhere
        // other than the listing — a pack gated off on this host — and
        // none at all when this run published it and the last failed
        // source took it back out. The failure still has to be visible,
        // so the row it anchors to is created rather than skipped.
        let pack = match result.packs.iter_mut().position(|p| p.name == pack_display) {
            Some(index) => &mut result.packs[index],
            None => {
                result
                    .packs
                    .push(DisplayPack::new(pack_display.clone(), Vec::new()));
                result.packs.last_mut().expect("just pushed")
            }
        };
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

    // Exit status (`adopt-safety.lex` §5.5): a partially adopted run is
    // not a successful one. The result still renders every planned
    // source, replaced and failed alike, so the user sees what did land
    // — but a script reading the exit status has to be able to tell the
    // two apart. The §4 left-in-place report does not reach here: those
    // entries were never planned for adoption.
    result.failed = !failures.is_empty();
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

/// Name of the subdirectory inside a preparation directory that holds
/// content `--force` displaced out of the pack — see
/// [`Preparation::displaced_root`].
const DISPLACED_DIR: &str = ".displaced";

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
    ///
    /// The ids of the entries the tree carries in are read before the
    /// rename and returned with it, for the same reason
    /// [`Preparation::publish_one`] records them: the recovery at §5.5
    /// removes an entry from a pack this run published, and removing
    /// is only this run's to do while the path still holds what this
    /// run put there.
    fn publish_new_pack(
        &self,
        pack_path: &Path,
        plans: &[AdoptPlan],
        fs: &dyn Fs,
    ) -> Result<Published> {
        let prepared_ids: Vec<Option<PreparedId>> = plans
            .iter()
            .map(|plan| prepared_identity(fs, &self.pack_root.join(&plan.in_pack)))
            .collect();

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
            .map(|()| {
                let published = plans
                    .iter()
                    .zip(prepared_ids)
                    .filter_map(|(plan, prepared)| {
                        let id = published_identity(fs, prepared, &plan.pack_dest)?;
                        Some((plan.in_pack.clone(), id))
                    })
                    .collect();
                Published::Ok(Publication::NewPack { published })
            })
    }

    /// Publish the prepared entries into a pack that already exists, or
    /// undo the whole sequence and say what it put back.
    ///
    /// Per entry: create the intermediate directories the plan needs,
    /// displace an existing destination into the preparation directory
    /// when `--force` planned to replace it, then rename the prepared
    /// entry into its final path. Each entry's appearance is atomic. The
    /// sequence is not, and `docs/proposals/adopt-safety.lex` §5.4 does
    /// not claim it is — it claims recovery instead. A failure at entry
    /// N renames every entry published so far back into the preparation
    /// directory, renames every displaced destination back to its final
    /// path, removes the intermediate directories this call created and
    /// left empty, and names the in-pack paths it restored in
    /// [`DodotError::PublicationRolledBack`]. Sources are untouched
    /// throughout: replacing them is step 5, and it has not started.
    ///
    /// A rollback step can itself fail — the rename that puts a
    /// displaced destination back can hit the same I/O or permission
    /// error that stopped publication. The entries that reaches are
    /// reported through [`DodotError::PublicationRollbackIncomplete`]
    /// instead, naming where each one's content is now, and the
    /// returned [`Published`] asks the caller to *keep* the preparation
    /// directory: it holds the only remaining copy of that content, and
    /// discarding it is what would turn a failed run into a lost file.
    ///
    /// The recovery is in-process only. A killed process leaves the pack
    /// mid-sequence with its preparation directory still on disk holding
    /// whatever was displaced — identifiable by the `.dodot-adopt-`
    /// prefix, and neither published from nor deleted by any later run.
    ///
    /// Displaced content is left in the preparation directory on the way
    /// out of a *successful* publication too, because step 5 may still
    /// need an entry's pre-adopt content back. [`Preparation::discard`]
    /// is where it goes.
    fn publish_into_existing(
        &self,
        pack_path: &Path,
        pack_display: &str,
        plans: &[AdoptPlan],
        fs: &dyn Fs,
    ) -> Published {
        let mut record = PublicationRecord::default();
        for plan in plans {
            if let Err(e) = self.publish_one(pack_path, plan, fs, &mut record) {
                let outcome = record.undo(fs);
                let reason = err_msg(&e);
                if outcome.stranded.is_empty() {
                    return Published::Failed {
                        error: DodotError::PublicationRolledBack {
                            pack: pack_display.to_string(),
                            reason,
                            restored: outcome.restored,
                        },
                        keep_preparation: false,
                    };
                }
                return Published::Failed {
                    error: DodotError::PublicationRollbackIncomplete {
                        pack: pack_display.to_string(),
                        reason,
                        restored: outcome.restored,
                        stranded: outcome.stranded,
                        preparation: self.root.display().to_string(),
                    },
                    keep_preparation: true,
                };
            }
        }
        Published::Ok(Publication::Existing(record))
    }

    /// Publish one prepared entry, appending to `record` everything a
    /// rollback would have to reverse.
    ///
    /// The entry is recorded whether or not its final rename succeeded,
    /// because a displacement that happened before a rename that did not
    /// still has to go back. What is *not* recorded is a step that
    /// failed before changing anything — an intermediate directory that
    /// could not be created, or a displacement rename that did not move.
    fn publish_one(
        &self,
        pack_path: &Path,
        plan: &AdoptPlan,
        fs: &dyn Fs,
        record: &mut PublicationRecord,
    ) -> Result<()> {
        create_intermediates(fs, pack_path, &plan.in_pack, &mut record.created_dirs)?;

        let mut entry = PublishedEntry {
            in_pack: plan.in_pack.clone(),
            final_path: plan.pack_dest.clone(),
            prepared: self.pack_root.join(&plan.in_pack),
            displaced: None,
            published: false,
            identity: None,
        };

        // Only a plan that *planned* to replace something displaces.
        // Plan refused an occupied destination without `--force`, so a
        // destination occupied here that no plan claimed is one that
        // appeared since — and moving that into the preparation
        // directory would discard at step 6 a file the user never
        // agreed to overwrite. The no-replace rename below refuses it
        // instead.
        if plan.destructive_overwrite
            && (fs.exists(&plan.pack_dest) || fs.is_symlink(&plan.pack_dest))
        {
            let displaced = self.displaced_root().join(&plan.in_pack);
            if let Some(parent) = displaced.parent() {
                fs.mkdir_all(parent)?;
            }
            fs.rename(&plan.pack_dest, &displaced)?;
            entry.displaced = Some(displaced);
        }

        let prepared_id = prepared_identity(fs, &entry.prepared);
        let result = fs.rename_noreplace(&entry.prepared, &plan.pack_dest);
        entry.published = result.is_ok();
        if entry.published {
            entry.identity = published_identity(fs, prepared_id, &plan.pack_dest);
        }
        record.entries.push(entry);
        result
    }

    /// Where content displaced out of the pack waits until
    /// [`Preparation::discard`].
    ///
    /// A sibling of the prospective pack tree rather than a child of it,
    /// so nothing displaced can ride a rename back into the pack. The
    /// leading `.` is what keeps it from colliding with the pack
    /// directory beside it: a pack scan skips dot-prefixed names, so no
    /// pack adopt publishes into is named this.
    fn displaced_root(&self) -> PathBuf {
        self.root.join(DISPLACED_DIR)
    }

    /// Remove the preparation directory and everything still in it,
    /// including whatever publication displaced out of the pack.
    ///
    /// Best effort: this runs on the way out of both the refusal and the
    /// success paths, and a failure to clean up is not a reason to fail
    /// a run that has otherwise done what it said.
    ///
    /// After it, a `--force` displacement has taken effect as the user
    /// asked (`adopt-safety.lex` §5.6) — the pre-adopt content is gone.
    fn discard(&self, fs: &dyn Fs) {
        remove_best_effort(fs, &self.root);
    }
}

/// What one existing-pack publication has done so far, in the order it
/// did it, and enough to reverse all of it.
#[derive(Default)]
struct PublicationRecord {
    /// Intermediate directories this publication brought into existence
    /// inside the pack, outermost first. Directories that were already
    /// there are not in the list and are not a rollback's to remove.
    created_dirs: Vec<PathBuf>,
    /// One per entry publication reached, in plan order.
    entries: Vec<PublishedEntry>,
}

/// One entry's share of a [`PublicationRecord`].
struct PublishedEntry {
    /// The entry's path relative to the pack root, which is how the
    /// rollback report names it.
    in_pack: PathBuf,
    /// Where the entry belongs inside the pack.
    final_path: PathBuf,
    /// Where the prepared content sat before publication, and where a
    /// rollback puts it back.
    prepared: PathBuf,
    /// Where `--force` moved the pre-adopt destination content to inside
    /// the preparation directory. `None` when the destination was free.
    displaced: Option<PathBuf>,
    /// Whether the rename into `final_path` succeeded. A recorded entry
    /// with this `false` displaced something and then failed to publish.
    published: bool,
    /// Which entry publication moved to `final_path`, and what it
    /// carried inside it, read from `prepared` just before the rename.
    ///
    /// A rollback compares it against what stands at `final_path` now,
    /// and moves that content only while the two agree: an in-pack
    /// path can hold something else by then — another process's file
    /// at the same path, or an edit inside a directory this run
    /// published — and sweeping that into the preparation directory
    /// hands it to [`Preparation::discard`]. `None` is the answer when
    /// the identity could not be read, and reads as "not provably
    /// ours" for the same reason.
    identity: Option<PublishedId>,
}

/// What a rollback has to recognise at an in-pack path before it may
/// move or remove what stands there: the entry publication put there,
/// and — when that entry is a directory — everything inside it.
///
/// The entry's own id does not answer the question for a directory.
/// Writing to a file nested inside one changes that file's ctime and
/// leaves the directory's own `dev`/`ino`/ctime exactly as publication
/// left them, so an identity that stopped at the top of the tree would
/// let a recovery delete a concurrent writer's edit — see
/// [`still_published`], which is where the two halves are checked.
struct PublishedId {
    /// The entry's own id, read at `final_path` after the rename that
    /// published it.
    entry: FileId,
    /// Every descendant, by path relative to the entry, with the id it
    /// had in the prepared tree. Empty for a file or a symlink.
    ///
    /// Recorded from the preparation directory rather than from the
    /// pack, and still true of the pack afterwards: a rename restamps
    /// only the entry it moves, so a descendant carries the same
    /// `dev`/`ino`/ctime across publication. Reading them there is the
    /// safer of the two, because the preparation directory is this
    /// run's alone — nothing another process wrote can be recorded in
    /// it as this run's own.
    descendants: Vec<(PathBuf, FileId)>,
}

/// The same identity read at the path publication is about to move the
/// entry *from*.
///
/// Separate from [`PublishedId`] because the two disagree about the
/// entry's own id and agree about everything below it: the rename
/// gives the entry a new ctime, so [`published_identity`] re-reads
/// that one at the destination and cross-checks it against this
/// `dev`/`ino`, while the descendants carry over untouched.
struct PreparedId {
    entry: FileId,
    descendants: Vec<(PathBuf, FileId)>,
}

/// What a rollback reversed, and what it could not.
///
/// The second list is why this is a struct rather than the list of
/// restored paths alone: a rollback step that fails leaves content
/// somewhere other than where it belongs, and reporting that entry as
/// restored would tell the user the pack holds its pre-adopt content
/// while the only copy of it sits in a directory the caller is about
/// to delete.
#[derive(Default)]
struct UndoOutcome {
    /// In-pack paths whose pre-adopt state is back, in plan order. For
    /// an entry `--force` displaced that is the content it held before
    /// the run; for every other entry it is not existing.
    restored: Vec<String>,
    /// Entries the rollback could not put back, in plan order.
    stranded: Vec<StrandedEntry>,
}

/// An entry a rollback left off its pre-adopt state, and where the
/// content it could not move is now.
#[derive(Debug)]
pub struct StrandedEntry {
    /// The entry's path relative to the pack root.
    pub in_pack: String,
    /// Where its content currently is: inside the preparation
    /// directory when the rename back into the pack failed, or at its
    /// final in-pack path when what publication put there is still
    /// standing — because the rename out of the pack failed, or
    /// because the path no longer holds the entry this run published
    /// and moving it would be adopt destroying a stranger's file.
    pub at: String,
}

impl PublicationRecord {
    /// Put the pack back the way publication found it, and name both
    /// what it put back and what it could not.
    ///
    /// Reverse order, so an entry comes out before the directory holding
    /// it. Published content goes back to the preparation directory it
    /// came from rather than being deleted — the run is over either way,
    /// but content that survives to [`Preparation::discard`] is content
    /// the user can still find if the discard is what fails. Displaced
    /// content then returns to the final path, which is the pre-adopt
    /// state for a `--force` entry and an absence for every other one.
    ///
    /// No step is assumed to have worked, and no step deletes anything:
    /// an entry reaches `restored` only once its final path actually
    /// holds its pre-adopt state, and anything else lands in `stranded`
    /// with the path its content is at while the caller keeps the
    /// preparation directory rather than discarding what may be the
    /// last copy. A rollback that cannot move published content out of
    /// the pack reports that entry rather than clearing the path, and a
    /// recovery that deletes is the failure mode this whole sequence
    /// exists to avoid. The only removals are the intermediate
    /// directories below, and only while they are still empty.
    ///
    /// Nothing here assumes a path is still what this run left at it,
    /// either. An in-pack path can hold another process's file by now:
    /// [`vacate`] moves out only the entry it can identify as
    /// publication's own, and [`restore_displaced`] refuses a
    /// destination that has since been taken rather than replacing it.
    /// Both cases report the entry instead. The failure that caused
    /// the rollback is still the error the user reads — a recovery
    /// failure is reported alongside it, not instead of it.
    fn undo(&self, fs: &dyn Fs) -> UndoOutcome {
        let mut outcome = UndoOutcome::default();
        for entry in self.entries.iter().rev() {
            let in_pack = entry.in_pack.display().to_string();

            // Take this publication's content back out of the pack, so
            // the final path is free for whatever was there before.
            // `vacate` moves only what publication put there and never
            // deletes, so a step that cannot finish leaves the entry
            // standing and reports it; the displaced content it blocks
            // stays in the preparation directory the caller then keeps.
            let final_path_free = !entry.published || vacate(fs, entry);

            match &entry.displaced {
                // `--force` moved something out; the entry is restored
                // only once that something is back. `rename_noreplace`,
                // so a path that has picked up someone else's content
                // since keeps it: see `restore_displaced`.
                Some(displaced) => {
                    if final_path_free && restore_displaced(fs, displaced, &entry.final_path) {
                        outcome.restored.push(in_pack);
                    } else {
                        outcome.stranded.push(StrandedEntry {
                            in_pack,
                            at: displaced.display().to_string(),
                        });
                    }
                }
                // Nothing was displaced, so the pre-adopt state is an
                // absence and clearing the final path is the whole
                // restoration.
                None if entry.published => {
                    if final_path_free {
                        outcome.restored.push(in_pack);
                    } else {
                        outcome.stranded.push(StrandedEntry {
                            in_pack,
                            at: entry.final_path.display().to_string(),
                        });
                    }
                }
                // The entry publication stopped on is recorded but
                // changed nothing unless it had displaced something
                // first, and an entry that changed nothing was neither
                // restored nor stranded.
                None => {}
            }
        }
        outcome.restored.reverse();
        outcome.stranded.reverse();

        // Empty ones only, and the kernel is what decides that: a
        // directory that picked up something else's content between its
        // creation and now is no longer describable as one this run left
        // behind, and `remove_dir_empty` refuses it inside the same
        // operation rather than across a gap another process can write
        // into.
        for dir in self.created_dirs.iter().rev() {
            let _ = fs.remove_dir_empty(dir);
        }
        outcome
    }
}

/// How a publication ended, and — when it failed — what the caller
/// should do with the preparation directory.
///
/// A `Result` would carry only the error, and the caller discards the
/// preparation directory on every way out of a run. `keep_preparation`
/// is the one case where it must not: a rollback that could not put a
/// displaced destination back left that content inside the preparation
/// directory, and discarding it there deletes the pre-adopt file the
/// run promised to protect.
enum Published {
    Ok(Publication),
    Failed {
        error: DodotError,
        keep_preparation: bool,
    },
}

/// What publication put in the pack, kept until every source
/// replacement has committed or rolled back.
///
/// A source replacement that fails takes its entry back out of the pack
/// (`adopt-safety.lex` §5.5), and what "back out" means is exactly what
/// publication did for that entry — which is why the record outlives
/// publication instead of being dropped at the end of it. The two
/// variants are the two shapes §5.4 publishes in, and they differ in
/// what a rollback may remove: everything inside a pack this run
/// created is this run's, while a pack that was already there holds
/// entries and directories no failure of this run's may touch.
enum Publication {
    /// One rename brought the whole prepared tree in, so nothing in the
    /// pack predates the run and the last failed source takes the pack
    /// directory with it.
    ///
    /// `published` is the [`PublishedId`] of each planned entry —
    /// what it was inside the prepared tree, which the one rename
    /// carried into the pack with it, contents and all. A recovery
    /// removing such an entry checks it first: "nothing here predates
    /// the run" is true of the tree publication moved, and says
    /// nothing about a path another process has written since.
    NewPack {
        published: HashMap<PathBuf, PublishedId>,
    },
    /// Entries published one at a time into a pack that already
    /// existed, with the record of what each one displaced and which
    /// intermediate directories the sequence created.
    Existing(PublicationRecord),
}

/// Create the directories `in_pack` needs inside `pack_path`, appending
/// the ones this call brought into existence to `created`.
///
/// One level at a time and exclusively, so `created` names exactly what
/// a rollback may remove. A level that was already there — or that
/// another process created between two of these calls — belongs to
/// whoever made it, and the `AlreadyExists` arm leaves it alone.
///
/// `AlreadyExists` says something occupies the name, not that it is a
/// directory. A regular file or a symlink to one there means the entry
/// cannot be published and no deeper level should be created trying:
/// this refuses at that level and names it, rather than descending and
/// failing later with the rename's error.
fn create_intermediates(
    fs: &dyn Fs,
    pack_path: &Path,
    in_pack: &Path,
    created: &mut Vec<PathBuf>,
) -> Result<()> {
    let Some(parent) = in_pack.parent() else {
        return Ok(());
    };
    let mut dir = pack_path.to_path_buf();
    for component in parent.components() {
        dir = dir.join(component);
        match fs.mkdir_exclusive(&dir) {
            Ok(()) => created.push(dir.clone()),
            Err(e) if crate::fs::is_already_exists(&e) => {
                if !fs.is_dir(&dir) {
                    return Err(DodotError::Other(format!(
                        "{} is not a directory, and adopting {} needs it to be one",
                        dir.display(),
                        in_pack.display()
                    )));
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
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

    Ok(bare_result(pack_display, files, ctx))
}

/// A result for the destination pack built from `files` alone, without
/// reading the pack off disk.
///
/// The two callers are the two moments there is no pack to read: a
/// `--dry-run` against an inferred pack that does not exist yet, and a
/// run whose every source replacement failed on a pack it had published
/// and has now taken back out. Both would otherwise have to create a
/// pack to have something to render, which is exactly what
/// `adopt-safety.lex` §5.3 and §5.5 forbid.
fn bare_result(
    pack_display: &str,
    files: Vec<DisplayFile>,
    ctx: &ExecutionContext,
) -> PackStatusResult {
    PackStatusResult {
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
    }
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
/// Classification (§3) happens here too, and it is the reason an
/// unadoptable entry never reaches a pack: a source the user typed that
/// no pack scan would read is a refusal, and a child discovered by
/// directory expansion that no pack scan would read stays where it is
/// and is reported.
///
/// Returns the plans, the human-readable messages for sources that were
/// skipped because they are already adopted, and the children left in
/// place.
#[allow(clippy::too_many_arguments)]
fn plan(
    pack_display: &str,
    pack_path: &Path,
    pack_exists: bool,
    sources: &[PathBuf],
    pack_override: Option<&str>,
    force: bool,
    no_follow: bool,
    only_os: Option<&str>,
    ctx: &ExecutionContext,
) -> Result<PlannedRun> {
    let fs = ctx.fs.as_ref();
    let dotfiles_root = ctx.paths.dotfiles_root().to_path_buf();
    let data_dir = ctx.paths.data_dir().to_path_buf();

    let root_config = ctx.config_manager.root_config()?;
    let pack_config = ctx.config_manager.config_for_pack(pack_path)?;
    // The effective `[pack] ignore` list, and nothing else: the config
    // resolver has already applied pack-replaces-root-replaces-default,
    // so this is the single list `dodot up` applies to this pack rather
    // than a concatenation of layers. `EffectiveIgnore` also records
    // which layer set it, because that is the file a refusal tells the
    // user to edit (`docs/proposals/adopt-safety.lex` §3.1, §3.2).
    let ignore = EffectiveIgnore::resolve(
        fs,
        &dotfiles_root,
        pack_path,
        pack_config.pack.ignore.clone(),
    );
    // Classification follows the top-level walk into a gate directory
    // whose predicate holds on this host, so it needs the same gate
    // table and host facts the walk uses.
    let gates = {
        let mut table = crate::gates::GateTable::with_builtins();
        if !pack_config.gates.is_empty() {
            table.merge_user(&pack_config.gates)?;
        }
        table
    };
    let host = ctx.host_facts.as_ref();
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
    let mut left_in_place: Vec<LeftInPlace> = Vec::new();

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
                        pack_display,
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
            // Every child and the rule that skipped it, kept for the
            // zero-adoptable refusal below — which needs the whole list
            // to show the user that the directory holds nothing dodot
            // would manage.
            let mut unadoptable: Vec<(String, SkipRule)> = Vec::new();
            let mut adopted_a_child = false;
            for entry in entries {
                let child_in_pack = expand_child_in_pack(&inferred, &entry.name, override_differs);
                // Same gate-dir wrap as the single-source path.
                let child_in_pack = if let Some(label) = only_os {
                    std::path::PathBuf::from(format!("_{label}")).join(&child_in_pack)
                } else {
                    child_in_pack
                };
                let child_source = abs.join(&entry.name);
                match classify(&child_in_pack, entry.is_dir, &ignore, &gates, host) {
                    // A discovered `.dodot.toml` or `.dodotignore` is
                    // the one discovered entry that refuses the run
                    // (§3.3). Copying either into the pack would
                    // replace the pack's own configuration or hide the
                    // pack entirely — an outcome different in kind from
                    // leaving noise behind, and not one to produce as a
                    // side effect of adopting a directory.
                    Some(rule @ SkipRule::Reserved { .. }) => {
                        return Err(DodotError::Other(rule.refusal(
                            &child_source,
                            &child_in_pack,
                            pack_display,
                        )));
                    }
                    Some(rule) => {
                        unadoptable.push((entry.name.clone(), rule.clone()));
                        left_in_place.push(LeftInPlace {
                            path: child_source,
                            rule,
                        });
                    }
                    None => {
                        adopted_a_child = true;
                        push_plan(
                            &mut plans,
                            fs,
                            &child_source,
                            pack_path,
                            &child_in_pack,
                            no_follow,
                            force,
                        )?;
                    }
                }
            }
            if !adopted_a_child {
                return Err(DodotError::Other(no_adoptable_children(
                    &abs,
                    &unadoptable,
                    pack_display,
                )));
            }
        } else {
            // A source the user typed that no pack scan would read is a
            // refusal, not a report: answering it with a success would
            // be a lie about what the command did (§3.2).
            if let Some(rule) = classify(&in_pack, is_dir, &ignore, &gates, host) {
                return Err(DodotError::Other(rule.refusal(
                    &abs,
                    &in_pack,
                    pack_display,
                )));
            }
            push_plan(&mut plans, fs, &abs, pack_path, &in_pack, no_follow, force)?;
        }
    }

    // Entries are also checked against each other: two entries landing
    // at the same in-pack path, or one entry containing another.
    check_overlaps(&plans)?;

    // Permission pre-flight. We do this after planning so every error up to
    // this point gives precise guidance; perms check catches late issues.
    // The dotfiles root always, because every run creates its
    // preparation directory there. The pack path additionally when it is
    // on disk, because publication renames into it; when it is not, the
    // rename lands in the dotfiles root and there is no pack path to
    // probe that creating it would not be the thing this step exists to
    // avoid.
    check_writable(fs, &dotfiles_root)?;
    if pack_exists {
        check_writable(fs, pack_path)?;
    }
    for plan in &plans {
        // Pass the plan's `is_dir` (already resolved with `--no-follow`
        // semantics) so a symlink-to-dir under `--no-follow` isn't probed
        // via `read_dir` on the target.
        check_readable(fs, &plan.source, plan.is_dir)?;
        if let Some(src_parent) = plan.source.parent() {
            check_writable(fs, src_parent)?;
        }
    }

    Ok(PlannedRun {
        plans,
        skipped_already_adopted: skipped,
        left_in_place,
    })
}

/// The §3.4 refusal: an expanded directory holding nothing a pack scan
/// would read.
///
/// Reporting every child as left in place and exiting zero would claim
/// an adoption that did not happen, so this is an error. It lists the
/// children and the rule each matched, so the user can see the directory
/// holds nothing dodot would manage instead of guessing why the command
/// refused.
fn no_adoptable_children(dir: &Path, children: &[(String, SkipRule)], pack: &str) -> String {
    if children.is_empty() {
        return format!(
            "refusing to adopt {}: expanding this directory found no adoptable \
             entries — it has no children.",
            dir.display()
        );
    }
    let headline = if children.len() == 1 {
        "its only child is skipped by a discovery rule:".to_string()
    } else {
        format!(
            "all {} children are skipped by a discovery rule:",
            children.len()
        )
    };
    let width = children
        .iter()
        .map(|(name, _)| name.chars().count())
        .max()
        .unwrap_or(0);
    let mut message = format!(
        "refusing to adopt {}: expanding this directory found no adoptable \
         entries, {headline}",
        dir.display()
    );
    for (name, rule) in children {
        message.push_str(&format!(
            "\n  {name:<width$}  {}",
            rule.short(pack),
            width = width
        ));
    }
    message
}

/// The §4 report: each left-in-place path once, with the rule that
/// matched it.
///
/// These are not failures, and nothing here touches the exit status.
/// §5.5 draws that from what happened to the *planned* sources, and a
/// left-in-place entry was never planned — so a run whose only report is
/// this one exits 0, the same as a run with nothing to report.
fn report_left_in_place(result: &mut PackStatusResult, left: &[LeftInPlace], pack: &str) {
    for entry in left {
        result.warnings.push(format!(
            "left in place: {} — {}",
            entry.path.display(),
            entry.rule.reported(pack)
        ));
    }
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
/// Centralises the destination-conflict and per-invocation collision
/// checks so they're applied uniformly between the regular path and the
/// directory-expansion path. Classification is the caller's, because the
/// two paths answer an unadoptable entry differently — a typed source
/// refuses the run, a discovered child is left where it is.
#[allow(clippy::too_many_arguments)]
fn push_plan(
    plans: &mut Vec<AdoptPlan>,
    fs: &dyn Fs,
    source: &Path,
    pack_path: &Path,
    in_pack: &Path,
    no_follow: bool,
    force: bool,
) -> Result<()> {
    let lmeta = fs.lstat(source)?;
    let is_source_symlink = lmeta.is_symlink;
    let treat_as_link = is_source_symlink && no_follow;
    let is_dir = if treat_as_link {
        false
    } else {
        fs.stat(source)?.is_dir
    };

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

// ── Copying ───────────────────────────────────────────────────────

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

/// The prepared entries as [`check_deploy_conflicts`] reads them.
struct ProspectiveTree<'a> {
    /// The destination pack's on-disk directory name. The prepared
    /// entries are planned under it so they compose with what a pack of
    /// that name already claims instead of conflicting with it.
    pack_dir: &'a str,
    /// Where the prepared entries are laid out, at the in-pack paths
    /// publication will give them.
    prepared_root: &'a Path,
    /// The pack path the entries are headed for. Its `.dodot.toml` is
    /// the configuration that governs them once published, and the
    /// preparation directory is not underneath it, so the analysis has
    /// to be told where to read that configuration from. For a pack that
    /// does not exist yet the path resolves to the root configuration,
    /// which is the same answer the published pack would give.
    config_at: &'a Path,
    /// The in-pack paths publication will replace, relative to the pack
    /// root and exactly as they will sit on disk — an `--only-os` run's
    /// `_<label>/` segment included. The prepared entry at each one is
    /// the tree's version of it, so the pack's own copy is left out of
    /// the scan rather than claiming its old deployment targets
    /// alongside the replacement. [`plan_pack_without`](orchestration::plan_pack_without)
    /// documents how a path here is matched against a walked entry and
    /// why a nested path leaves the entry holding it in the plan.
    superseded: &'a [PathBuf],
}

/// Refuse the run if deploying the pack tree would collide with another
/// pack. `--force` does not bypass this.
///
/// The entries being adopted are still in the preparation directory
/// whichever destination they are headed for, so the analysis always
/// reads them from there. The prepared tree is planned as a pack named
/// `pack_dir` but read out of `prepared_root`, and its intents join
/// whatever the pack of that name already contributes: the analysis sees
/// the pack's current entries composed with the prepared ones at their
/// final in-pack paths, without a final pack path having been written.
/// `detect_cross_pack_conflicts` only flags claims from *different*
/// packs, so composing under one name is what keeps a pack from
/// conflicting with its own prospective content.
///
/// The composition is a *replacement*, not a union. Publication into an
/// existing pack overwrites the destination pack's entry at each
/// superseded in-pack path, so that entry's claims will not exist in
/// the published tree and the destination pack is planned without them
/// ([`plan_pack_without`](orchestration::plan_pack_without)). Unioning
/// instead would let an obsolete claim refuse the run: an entry
/// `--force` replaces with conflict-free content could still collide
/// with another pack through the target it used to declare, which also
/// makes `adopt --force` unable to repair a conflict that already
/// exists. Only the destination pack is planned this way — every other
/// pack keeps every entry it has, since publication does not touch them.
///
/// Intents are collected in [`PreprocessMode::Passive`](crate::preprocessing::PreprocessMode::Passive):
/// the question here is only which deployment targets each pack claims,
/// and answering it actively would render every staged `*.tmpl` and
/// resolve every staged secret — writing the rendered output and its
/// baseline into the datastore, and prompting the user's secret
/// provider — before adopt has decided whether the run goes ahead. A
/// conflict refusal and `--dry-run` both leave that behind, which is the
/// same reason `status` reads passively (`docs/proposals/secrets.lex`
/// §7.4).
///
/// Passive planning reads a preprocessor entry's cached baseline. A
/// first-time template has none — it surfaces as a placeholder with no
/// rendered content — and a template edited since the last `dodot up`
/// has one describing the render before the edit. For most handlers
/// neither costs anything here: a symlink target follows from the
/// file's path, so an unrendered `config.toml.tmpl` still claims
/// `~/.config/…/config.toml` whatever its contents say. Both are
/// decisive for `externals`, which reads every target it claims out of
/// `externals.toml`; an unrendered `externals.toml.tmpl` produces no
/// `Fetch` intent at all, and one rendered before its last edit
/// produces the targets it used to claim. Reading either as this
/// pack's current claims would let adopt publish into exactly the
/// collision this analysis exists to refuse. `plan_pack` names those files in
/// [`PackPlan::unresolved_claims`](crate::packs::orchestration::PackPlan::unresolved_claims),
/// and this function refuses on any of them — the same posture it takes
/// toward a pack it cannot scan, for the same reason: an answer dodot
/// cannot compute must not be mutated on. The user renders the template
/// with one `dodot up` and re-runs adopt.
fn check_deploy_conflicts(ctx: &ExecutionContext, prospective: ProspectiveTree<'_>) -> Result<()> {
    let root_config = ctx.config_manager.root_config()?;
    let packs::DiscoveredPacks { packs: all, .. } = packs::scan_packs(
        ctx.fs.as_ref(),
        ctx.paths.dotfiles_root(),
        &root_config.pack.ignore,
    )?;

    let mut pack_intents = Vec::new();
    let mut unresolved = Vec::new();
    for mut pack in all {
        let pack_config = ctx.config_manager.config_for_pack(&pack.path)?;
        pack.config = pack_config.to_handler_config();
        // The destination pack is the one publication rewrites, so it is
        // planned without the entries the prepared tree replaces.
        let superseded: &[PathBuf] = if pack.path == prospective.config_at {
            prospective.superseded
        } else {
            &[]
        };
        // Propagate per-pack errors: if any pack can't be scanned we can't
        // truthfully say "no conflict with that pack," so refuse outright
        // rather than risk a false negative that lets us mutate into a
        // state `dodot up` will later reject.
        let plan = collect_intents_passive(&pack, &pack.path, ctx, superseded)?;
        unresolved.extend(plan.unresolved_claims);
        pack_intents.push((pack.display_name.clone(), plan.intents));
    }

    let mut prospective_pack = packs::Pack::new(
        prospective.pack_dir.to_string(),
        prospective.prepared_root.to_path_buf(),
        Default::default(),
    );
    let pack_config = ctx.config_manager.config_for_pack(prospective.config_at)?;
    prospective_pack.config = pack_config.to_handler_config();
    // Scanned at the staging path, governed by the destination pack's
    // configuration: the prepared tree is what the pack will hold, so
    // the pack's own rules, gates, ignore list and preprocessor
    // settings are the ones that decide what it claims. The staging
    // directory has no `.dodot.toml` of its own to answer with.
    let plan = collect_intents_passive(&prospective_pack, prospective.config_at, ctx, &[])?;
    unresolved.extend(plan.unresolved_claims);
    let display = prospective_pack.display_name.clone();
    match pack_intents.iter_mut().find(|(name, _)| *name == display) {
        Some((_, already)) => already.extend(plan.intents),
        None => pack_intents.push((display, plan.intents)),
    }

    // Incompleteness first: a conflict found among the claims dodot did
    // compute is still a true conflict, but reporting it would tell the
    // user to resolve that one and re-run into a second refusal. Naming
    // the files awaiting a render first gets them to one `dodot up` and
    // a run whose verdict is complete.
    if !unresolved.is_empty() {
        return Err(DodotError::ConflictCheckIncomplete { unresolved });
    }

    let conflicts = conflicts::detect_cross_pack_conflicts(&pack_intents, ctx.fs.as_ref());
    if !conflicts.is_empty() {
        return Err(DodotError::CrossPackConflict { conflicts });
    }
    Ok(())
}

/// What a pack would deploy, planned without preprocessing side
/// effects — see [`check_deploy_conflicts`] for why adopt reads this
/// way.
///
/// The whole [`PackPlan`](crate::packs::orchestration::PackPlan) comes
/// back because the caller needs two of its fields: `intents` are the
/// claims to compare, and `unresolved_claims` are the claims this plan
/// does not contain, which decide whether comparing the intents proves
/// anything. Handler warnings are still dropped — the run reports
/// through `status` afterwards.
///
/// `superseded` names in-pack paths this plan should leave out, which
/// is empty for every pack but the one publication is about to rewrite.
///
/// `config_at` is the pack path whose configuration governs the scan.
/// It is the pack's own path for a pack on disk, and the *destination*
/// pack's path for the prospective tree, which sits in the staging
/// directory and carries no configuration of its own.
fn collect_intents_passive(
    pack: &packs::Pack,
    config_at: &Path,
    ctx: &ExecutionContext,
    superseded: &[PathBuf],
) -> Result<orchestration::PackPlan> {
    orchestration::plan_pack_without(
        pack,
        config_at,
        ctx,
        crate::preprocessing::PreprocessMode::Passive,
        superseded,
    )
}

// ── Step 5: Replace sources ───────────────────────────────────────

/// One planned source that could not be replaced, why, and what
/// putting its pack entry back achieved.
///
/// The recovery outcome travels with the failure rather than in a list
/// beside it because the report has to say different things about the
/// two cases, and a note that guessed wrong would tell the user their
/// pack is clean while a duplicate of an unreplaced source stands in
/// it.
struct AdoptFailure {
    source: PathBuf,
    reason: String,
    /// What the recovery could not put back, if anything, and where
    /// that content is now: the preparation directory for a `--force`
    /// displacement that could not return, the in-pack path for a
    /// published entry that could not come out. `None` means the pack
    /// holds this entry's pre-adopt state again. `Some` keeps the
    /// preparation directory instead of discarding it: what is in there
    /// can be the only copy of a destination's pre-adopt content.
    stranded: Option<StrandedEntry>,
}

/// Replace every planned source with a symlink to its published pack
/// path, and take the pack entry of each one that fails back out.
///
/// Sources are independent (`adopt-safety.lex` §5.5) and a failure does
/// not stop the run. Stopping early would be the worse of the two
/// options: publication has already put every planned entry in the pack,
/// so abandoning the remaining sources would leave each of them a real
/// file at its original path *and* a copy of itself in the pack — the
/// duplicated state that makes the next `dodot up` report a conflict the
/// user never created. Continuing means each source ends in exactly one
/// of two states, replaced or untouched-with-its-pack-entry-rolled-back,
/// whatever happened to the others — or, when the rollback of that entry
/// fails in turn, untouched with what the rollback could not move named
/// in the report rather than deleted.
///
/// The directory sweep runs once at the end rather than per failure, and
/// only when something failed. `remove_dir_empty` is what keeps it from
/// reaching a directory a successful source still occupies: emptiness is
/// the kernel's verdict inside the same operation that removes, not a
/// test this code makes and then acts on.
///
/// Returns one [`AdoptFailure`] per source that could not be replaced,
/// in plan order, each carrying whatever its own recovery could not put
/// back — which is what lets the report say the entry came out only
/// where it did.
fn swap_all(
    plans: &[AdoptPlan],
    publication: &Publication,
    pack_path: &Path,
    fs: &dyn Fs,
) -> Vec<AdoptFailure> {
    let mut failures: Vec<AdoptFailure> = Vec::new();
    let mut failed: Vec<&AdoptPlan> = Vec::new();
    for plan in plans {
        let result = if plan.is_dir {
            swap_dir(&plan.source, &plan.pack_dest, fs)
        } else {
            swap_file_atomic(&plan.source, &plan.pack_dest, fs)
        };
        if let Err(e) = result {
            failed.push(plan);
            failures.push(AdoptFailure {
                source: plan.source.clone(),
                reason: err_msg(&e),
                stranded: restore_failed_entry(plan, publication, fs),
            });
        }
    }
    if !failed.is_empty() {
        prune_emptied_dirs(&failed, publication, pack_path, fs);
    }
    failures
}

/// Put the pack back where it was for one source that could not be
/// replaced, and name the content the attempt could not move.
///
/// "Where it was" is whatever publication did for this entry, undone
/// (§5.5). Under `--force` that is the displaced destination renamed
/// back; otherwise it is the published entry gone.
///
/// Gone by a rename into the preparation directory, not a deletion: the
/// path the entry came from is empty until step 6 discards it, so the
/// bytes stay reachable for the rest of the run at no cost. The one
/// place that deletes is a pack this run published, where there is no
/// preparation directory left to rename into — the whole prepared tree
/// became the pack — and where nothing that arrived with that tree
/// predates the run.
///
/// Both of them act on the path only while it still holds the entry
/// publication put there. What stands at an in-pack path now is not
/// necessarily what publication left: another process can have
/// written it since, and moving or removing *that* is the recovery
/// destroying a file adopt never adopted.
///
/// No step is assumed to have worked, and none of them deletes anything
/// to get past a failure: that is §5.4's rule and §5.5 restores "what it
/// means in §5.4, and for the same reason". Whatever the recovery could
/// not move is returned as a [`StrandedEntry`] naming the path its
/// content is at — the preparation directory for a `--force`
/// displacement that could not go back, the in-pack path for a published
/// entry that could not come out or was not this run's to touch. The caller keeps the preparation
/// directory whenever one comes back: discarding it can destroy the
/// pre-adopt destination that `--force` promised to hold until the run
/// committed.
fn restore_failed_entry(
    plan: &AdoptPlan,
    publication: &Publication,
    fs: &dyn Fs,
) -> Option<StrandedEntry> {
    let entry = match publication {
        // Nothing in a pack this run published predates the run, so
        // there is nothing to put back and removing the entry is the
        // whole restoration. The pack path did not exist before this
        // run and arrived as one rename of a tree this run built, so
        // what stands inside it came in with that tree.
        //
        // Which is a statement about the rename, not about the path
        // now: another process can have replaced the entry since, and
        // removing *that* is the recovery destroying a file adopt
        // never adopted. So the removal happens only while the path
        // still holds the entry publication carried in, and anything
        // else is left standing and reported — the same answer
        // [`vacate`] gives for a pack that already existed.
        Publication::NewPack { published } => {
            if still_published(fs, &plan.pack_dest, published.get(&plan.in_pack)) {
                remove_best_effort(fs, &plan.pack_dest);
            }
            // A removal that failed, or one this declined to make,
            // leaves something standing at the path; saying the entry
            // was taken back out would be a lie the user cannot check.
            return occupied(fs, &plan.pack_dest).then(|| StrandedEntry {
                in_pack: plan.in_pack.display().to_string(),
                at: plan.pack_dest.display().to_string(),
            });
        }
        Publication::Existing(record) => record.entries.iter().find(|e| e.in_pack == plan.in_pack),
    };
    // Publication records every plan it reaches, so a plan missing
    // from the record is one publication never touched: nothing of
    // this run's is at its final path, and there is nothing to undo.
    let entry = entry?;

    let in_pack = entry.in_pack.display().to_string();
    // An entry publication recorded but never got into the pack has
    // nothing of this run's at its final path, so there is nothing to
    // take out and nothing to rename into the preparation directory —
    // what stands there, if anything, is the user's own.
    let vacated = !entry.published || vacate(fs, entry);
    match &entry.displaced {
        // `--force` moved something out; the entry is restored only
        // once that something is back, and until it is, the only copy
        // of it is the one in the preparation directory.
        Some(displaced) => {
            if vacated && restore_displaced(fs, displaced, &entry.final_path) {
                None
            } else {
                Some(StrandedEntry {
                    in_pack,
                    at: displaced.display().to_string(),
                })
            }
        }
        // Nothing was displaced, so the pre-adopt state is an absence
        // and clearing the final path is the whole restoration.
        None => (!vacated).then(|| StrandedEntry {
            in_pack,
            at: entry.final_path.display().to_string(),
        }),
    }
}

/// Take this run's published copy out of `entry.final_path` and say
/// whether the path ended up free for whatever was there before it.
///
/// A rename back to `entry.prepared`: that path was vacated by
/// publication and step 6 discards it, so the bytes stay reachable
/// until the run ends. A rename that fails leaves the entry standing
/// rather than deleting it — removing it instead would make this
/// recovery the thing that destroys content (§5.4, which §5.5 restores
/// by).
///
/// What sits at an in-pack path is not necessarily what publication
/// put there, so the move happens only while the path still holds the
/// entry publication recorded. Sweeping a stranger's file into the
/// preparation directory is not a rescue: step 6 discards that
/// directory, so a run whose every other step succeeded would delete a
/// file adopt never adopted. A path holding anything else is left as
/// it is and reported, which is the same answer this function gives a
/// rename it could not do.
///
/// The rename into `prepared` is `rename_noreplace` for the same
/// reason as the one out of it: publication emptied that path, so
/// anything at it now arrived from outside this run.
///
/// The answer is what the path holds afterwards rather than what the
/// rename returned: another process can have taken it away, and a
/// caller about to rename a displaced destination back needs to know
/// the path is actually free.
fn vacate(fs: &dyn Fs, entry: &PublishedEntry) -> bool {
    if still_published(fs, &entry.final_path, entry.identity.as_ref()) {
        let _ = fs.rename_noreplace(&entry.final_path, &entry.prepared);
    }
    !occupied(fs, &entry.final_path)
}

/// The identity to record for an entry publication has just moved to
/// `final_path`, given `prepared` — the [`PreparedId`] read at the
/// path it came from, before the move.
///
/// The entry's own id is read at the destination, because a `rename`
/// stamps the entry it moves with a new ctime and the recorded id has
/// to describe the entry as it now sits in the pack. Read *back*
/// against `prepared`, because a destination read after the rename is
/// a path another process can have taken in between, and recording
/// that as this run's own is what would license a recovery to destroy
/// it. The two reads agreeing on `dev`/`ino` is what says the path
/// still holds the entry the rename moved.
///
/// The descendants come over from `prepared` unchanged: the rename
/// restamps only the entry it moves, so what it recorded of the tree
/// inside is as true at `final_path` as it was in the preparation
/// directory.
///
/// `None` whenever that cannot be established, which reads downstream
/// as "not provably ours" and leaves the path alone.
fn published_identity(
    fs: &dyn Fs,
    prepared: Option<PreparedId>,
    final_path: &Path,
) -> Option<PublishedId> {
    let prepared = prepared?;
    let published = fs.lstat(final_path).ok()?.id;
    published
        .same_entry(&prepared.entry)
        .then_some(PublishedId {
            entry: published,
            descendants: prepared.descendants,
        })
}

/// Read the identity of the entry sitting at `prepared`, before the
/// rename that publishes it.
///
/// `None` when any part of it is unreadable. That leaves the entry
/// with no recorded identity, which reads downstream as "not provably
/// ours" and puts it out of reach of every recovery step that moves or
/// removes.
fn prepared_identity(fs: &dyn Fs, prepared: &Path) -> Option<PreparedId> {
    Some(PreparedId {
        entry: fs.lstat(prepared).ok()?.id,
        descendants: subtree_ids(fs, prepared)?,
    })
}

/// Every entry below `root`, by path relative to it, paired with its
/// id, sorted by path.
///
/// Sorted so that two reads of an unchanged tree compare equal
/// whatever order the filesystem lists names in.
///
/// A `root` that is not a directory has no descendants and gives an
/// empty list. `None` whenever any part of the walk fails: a tree
/// whose state cannot be read is one a recovery cannot call its own,
/// the same answer an unreadable entry gives.
///
/// A symlink is recorded by its own id and not followed. Descending
/// through one would read a tree outside the entry, and adopt copied
/// the link rather than what it points at.
fn subtree_ids(fs: &dyn Fs, root: &Path) -> Option<Vec<(PathBuf, FileId)>> {
    let mut ids = Vec::new();
    collect_subtree_ids(fs, root, PathBuf::new(), &mut ids)?;
    ids.sort_by(|a, b| a.0.cmp(&b.0));
    Some(ids)
}

/// Append every entry below `path` to `into`, naming each by
/// `relative` — its path from the tree root the walk started at. The
/// root itself is not appended: its id is recorded separately, because
/// the rename that publishes it gives it a new ctime and the ids in
/// here survive that rename untouched.
fn collect_subtree_ids(
    fs: &dyn Fs,
    path: &Path,
    relative: PathBuf,
    into: &mut Vec<(PathBuf, FileId)>,
) -> Option<()> {
    let meta = fs.lstat(path).ok()?;
    if !relative.as_os_str().is_empty() {
        into.push((relative.clone(), meta.id));
    }
    if meta.is_dir && !meta.is_symlink {
        for entry in fs.read_dir(path).ok()? {
            collect_subtree_ids(fs, &entry.path, relative.join(&entry.name), into)?;
        }
    }
    Some(())
}

/// Whether `path` still holds the entry publication put there, and —
/// for a directory — the content it put inside it, as recorded in
/// `published`.
///
/// `false` whenever that cannot be established — the id was
/// unreadable at publication, the path is unreadable now, the two ids
/// differ, or the tree below the path is no longer the one publication
/// carried in. Each of those means a recovery about to move the path's
/// content cannot say the content is this run's, and the safe answer
/// is the one that moves nothing.
///
/// The tree comparison is what makes the answer true of a directory
/// rather than only of its top entry. Writing to a file nested inside
/// an adopted directory changes that file's ctime and leaves the
/// directory's own `dev`/`ino`/ctime as publication left them, so a
/// check that read only the top would answer "still ours" for a tree
/// another process had edited — and the caller would then remove it
/// recursively, or rename it into the preparation directory that step
/// 6 deletes. Either way the concurrent writer's content is gone,
/// which is the one thing `adopt-safety.lex` §5.4 says a recovery
/// never does.
///
/// A check, not a lock: another process can replace the path between
/// this `lstat` and the caller's rename. It converts the silent case
/// — recovery assumes the path is its own and destroys what is there
/// — into the reported one, which is as far as POSIX renames reach.
fn still_published(fs: &dyn Fs, path: &Path, published: Option<&PublishedId>) -> bool {
    let Some(published) = published else {
        return false;
    };
    let Ok(now) = fs.lstat(path) else {
        return false;
    };
    published.entry == now.id
        && subtree_ids(fs, path).is_some_and(|current| current == published.descendants)
}

/// Put displaced content back at `final_path`, and say whether it
/// landed.
///
/// `rename_noreplace`: the caller has just freed `final_path`, and a
/// plain `rename` would replace whatever appeared at it in the
/// meantime — a file another process wrote between the vacating and
/// this call, destroyed by the step whose whole purpose is to avoid
/// destroying content. Refusing instead leaves the displaced content
/// in the preparation directory, which the caller then reports and
/// keeps rather than discarding.
fn restore_displaced(fs: &dyn Fs, displaced: &Path, final_path: &Path) -> bool {
    fs.rename_noreplace(displaced, final_path).is_ok()
}

/// Whether anything stands at `path` — a broken symlink included, which
/// [`Fs::exists`] follows past.
fn occupied(fs: &dyn Fs, path: &Path) -> bool {
    fs.exists(path) || fs.is_symlink(path)
}

/// Remove the directories publication created that the failed entries
/// have just emptied.
///
/// Removing an entry alone would leave `nvim/lua/plugins/` behind, or an
/// empty `nvim/` for a one-source inferred pack — the residue
/// `adopt-safety.lex` §1.2 describes. Which directories are removable
/// differs by publication shape, and that is the whole of the split:
///
/// - A pack that already existed holds directories that predate the
///   run, and none of those is a rollback's to remove. The record names
///   exactly the ones publication created, so the sweep walks that list
///   and nothing else.
/// - A pack this run published arrived as one tree, so every directory
///   inside it — the pack directory included — is this run's. The sweep
///   walks each failed entry's ancestors up to the pack root, which is
///   also what removes the pack when no source was replaced at all.
///
/// Empty ones only, and always by `remove_dir_empty`: a directory
/// holding a successful entry, or content another process put there,
/// refuses inside the same operation that would have removed it.
fn prune_emptied_dirs(
    failed: &[&AdoptPlan],
    publication: &Publication,
    pack_path: &Path,
    fs: &dyn Fs,
) {
    match publication {
        Publication::Existing(record) => {
            for dir in record.created_dirs.iter().rev() {
                let _ = fs.remove_dir_empty(dir);
            }
        }
        Publication::NewPack { .. } => {
            for plan in failed {
                let mut dir = plan.pack_dest.parent().map(Path::to_path_buf);
                while let Some(current) = dir {
                    if !current.starts_with(pack_path) || fs.remove_dir_empty(&current).is_err() {
                        break;
                    }
                    if current == pack_path {
                        break;
                    }
                    dir = current.parent().map(Path::to_path_buf);
                }
            }
        }
    }
}

/// Atomic file swap: create symlink at a temp sibling, then rename over the
/// original. `rename` is atomic on POSIX and replaces the existing file.
///
/// The original is a readable file at its own path until the instant it
/// is the symlink, and the temp sibling sits in the source's own
/// directory, so the rename never crosses a filesystem however far the
/// pack is from the source.
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
///
/// Recoverable rather than atomic, and `adopt-safety.lex` §5.5 says so
/// rather than claiming otherwise: a process killed between the rename
/// and the symlink leaves the directory at the backup path beside its
/// original location. [`temp_sibling`] puts the original's own name in
/// that path so it is restorable by hand — one `mv` back.
///
/// The rename back can fail in turn, and then the directory is at the
/// backup path rather than where the user left it. The error says so
/// and names the path instead of reporting only the symlink failure,
/// which would send the user looking at an original path that is no
/// longer there.
fn swap_dir(source: &Path, pack_dest: &Path, fs: &dyn Fs) -> Result<()> {
    let backup = temp_sibling(source, "old");
    fs.rename(source, &backup)?;
    match fs.symlink(pack_dest, source) {
        Ok(()) => {
            let _ = fs.remove_dir_all(&backup);
            Ok(())
        }
        // `rename_noreplace`: the symlink that failed left `source`
        // free, and anything standing there now arrived from outside
        // this run — a plain `rename` back would destroy it. A refusal
        // takes the branch below, which names the backup path.
        Err(e) if fs.rename_noreplace(&backup, source).is_ok() => Err(e),
        Err(e) => Err(DodotError::Other(format!(
            "{e}; and the directory could not be moved back to {} — it is \
             at {}",
            source.display(),
            backup.display()
        ))),
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

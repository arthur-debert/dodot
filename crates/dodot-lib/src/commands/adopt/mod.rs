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
//! ## Two-phase model
//!
//! 1. **Copy phase** — recursively copy each source into the pack, preserving
//!    inner symlinks and Unix permissions. Originals are never touched in this
//!    phase. If anything fails, the partial copies are removed and the error
//!    surfaces; home is pristine throughout.
//!
//! 2. **Swap phase** — per source, atomically replace the original with a
//!    symlink to the pack copy. Files use a symlink-at-temp + rename-over-original
//!    trick (POSIX atomic). Directories use a rename-to-backup + symlink + rm-backup
//!    dance (one-step recoverable). A per-file failure cleans up that source's pack
//!    copy only; previously-adopted sources remain adopted.
//!
//! Cross-pack deployment conflicts are detected after the copy phase and before
//! the swap phase — adoption is refused if deploying the adopted files would
//! collide with another pack. This check is not bypassed by `--force`.
//!
//! ## Auto-creating packs
//!
//! When all sources point at a single inferred pack name and that pack
//! doesn't exist on disk, adopt creates it (an empty directory — no
//! `.dodot.toml` is written; the user can run `dodot config gen` later
//! if they want one). When `--into <pack>` is supplied and `<pack>` does
//! not exist, adopt refuses — explicit pack names are typo-checked
//! against the existing pack inventory.

mod infer;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::commands::status;
use crate::commands::{DisplayFile, DisplayNote, PackStatusResult};
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
    /// Destination inside the pack.
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
/// `only_os` is `Some(label)` when the user passed `--only-os <label>`
/// (Phase C5 of the conditional-running proposal). Each source's
/// in-pack path is prepended with a `_<label>/` gate-dir segment so
/// re-deploying via `dodot up` will only land the symlink on hosts
/// matching the gate predicate. The label is validated against the
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
    // Each source contributes a candidate pack name (its naturally-inferred
    // pack, or None if the source root has no pack structure). We require
    // exactly one pack per adopt invocation:
    //
    //   - All sources agree on a single inferred name → use it.
    //   - Sources disagree → refuse; ask the user to split or use --into.
    //   - All sources decline (HOME-only) → require --into.
    //   - --into supplied → it wins regardless of inference.
    //
    // The single-pack constraint keeps the result shape (one
    // PackStatusResult) and the conflict-check semantics simple. Future
    // work can lift this to multi-pack invocations once the result
    // structure supports it.
    let resolved = resolve_pack_for_sources(pack_override, sources, ctx)?;

    let pack_dir = resolved.pack_dir.clone();
    let pack_display = resolved.display_name.clone();
    let pack_path = ctx.paths.pack_path(&pack_dir);

    // ── Auto-create the pack if inferred and missing ─────────────────
    //
    // Inferred-but-absent packs are created as empty directories. The
    // explicit `--into` path goes through `resolve_pack_dir_name` and
    // errors on miss instead — that's the typo-guard the user opted
    // into by naming a specific pack.
    if !ctx.fs.exists(&pack_path) {
        ctx.fs.mkdir_all(&pack_path)?;
    }

    if ctx.fs.exists(&pack_path.join(".dodotignore")) {
        return Err(DodotError::PackInvalid {
            name: pack_display.clone(),
            reason: "pack is marked ignored via .dodotignore".into(),
        });
    }

    let (plans, skipped_already_adopted) = preflight(
        &pack_dir,
        &pack_path,
        sources,
        pack_override,
        force,
        no_follow,
        only_os,
        ctx,
    )?;

    // If every input was already adopted, there's nothing to do.
    if plans.is_empty() {
        let mut result = status::status(Some(std::slice::from_ref(&pack_display)), ctx)?;
        result.dry_run = dry_run;
        for msg in skipped_already_adopted {
            result.warnings.push(msg);
        }
        return Ok(result);
    }

    // Phase 1 — copy every source into the pack. On failure, cleanup and bail.
    if let Err(e) = copy_all(&plans, ctx.fs.as_ref()) {
        cleanup_pack_copies(&plans, ctx.fs.as_ref());
        return Err(e);
    }

    // Cross-pack deploy conflict simulation happens with the copies in place.
    if let Err(e) = check_deploy_conflicts(ctx) {
        cleanup_pack_copies(&plans, ctx.fs.as_ref());
        return Err(e);
    }

    // Dry-run stops here: we've verified the plan is viable, now unwind.
    if dry_run {
        cleanup_pack_copies(&plans, ctx.fs.as_ref());
        let mut result = status::status(Some(std::slice::from_ref(&pack_display)), ctx)?;
        result.dry_run = true;
        for msg in skipped_already_adopted {
            result.warnings.push(msg);
        }
        return Ok(result);
    }

    // Phase 2 — per-source atomic swap. Failures are recorded, not fatal.
    let failures = swap_all(&plans, ctx.fs.as_ref());

    let mut result = status::status(Some(std::slice::from_ref(&pack_display)), ctx)?;
    result.dry_run = false;
    for msg in skipped_already_adopted {
        result.warnings.push(msg);
    }

    // M5 capitalization-heuristic advisory + M6 brew enrichment.
    //
    // Both gate on the same precondition (at least one AppSupport
    // source) and consume the same brew probe data (the matching
    // installed-cask token for the pack name). They were separate
    // blocks pre-cask-aware-rename — the consolidation here lets the
    // M5 rename suggestion *prefer the cask token* over a
    // whitespace-stripped-lowercase folder name, which matters for
    // reverse-DNS bundle IDs (`com.colliderli.iina` → `iina`, not
    // `comcolliderliiina`). See `docs/proposals/macos-paths.lex`
    // §8.1–§8.2.
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
        result.notes.push(DisplayNote {
            body: format!("adopt failed: {}: {}", f.source.display(), f.reason),
            hint: None,
        });
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

// ── Pre-flight ───────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn preflight(
    pack_name: &str,
    pack_path: &Path,
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
        // #44: distinguish two sub-cases so the user knows what to do next:
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

        // ── Inference: source-root match + in-pack path computation ──
        let inferred =
            infer_target(&abs, is_dir, ctx.paths.as_ref(), &force_home).map_err(|reason| {
                DodotError::Other(format!("refusing to adopt {}: {reason}", abs.display()))
            })?;

        // Pick the override-aware encoding when --into changed the pack
        // name. This keeps `_xdg/<X>/...` (and the future `_app/<X>/...`)
        // round-trip-correct even when the user reroutes the file into
        // a different pack than its source-root segment suggests.
        let in_pack = match (&inferred.natural_pack, pack_override) {
            (Some(natural), Some(over)) if natural != over => inferred.in_pack_override.clone(),
            _ => inferred.in_pack_natural.clone(),
        };
        // C5: `--only-os <label>` wraps the entry in a `_<label>/`
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
            // Source IS a pack-root directory under XDG (or AppSupport
            // future) — enumerate children and adopt each as a top-level
            // pack entry. This is the "I want this whole `~/.config/nvim/`
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
                // C5: same gate-dir wrap as the single-source path.
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

    // Permission pre-flight. We do this after planning so every error up to
    // this point gives precise guidance; perms check catches late issues.
    let _ = pack_name; // reserved for future per-pack perm messages
    check_writable(fs, pack_path)?;
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
/// `_app/<X>/<child>` for AppSupport (default rule routes through
/// `$XDG`, not `app_support_dir`, so the prefix is mandatory even at
/// natural pack name — see `docs/proposals/macos-paths.lex` §7.2).
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
            // AppSupport always needs the prefix (see §7.2 note), so the
            // override flag doesn't change behavior here. Reserved for
            // when Pather exposes app_support_dir() per macos-paths M1.
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
        pack_dest,
        is_dir,
        destructive_overwrite: dest_exists,
    });
    Ok(())
}

/// Resolve a possibly-relative path to an absolute, lexically-normalized one.
/// Mirrors the original adopt behavior: relative inputs resolve against
/// CWD, then `..` and `.` are collapsed without touching the filesystem.
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
    let probe = dir.join(format!(".dodot-adopt-probe-{}", nonce()));
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

// ── Phase 1: copy ─────────────────────────────────────────────────

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
            // No existing destination: copy directly.
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

// ── Deploy conflict check ─────────────────────────────────────────

fn check_deploy_conflicts(ctx: &ExecutionContext) -> Result<()> {
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
        let intents = orchestration::collect_pack_intents(&pack, ctx)?;
        pack_intents.push((pack.display_name.clone(), intents));
    }

    let conflicts = conflicts::detect_cross_pack_conflicts(&pack_intents, ctx.fs.as_ref());
    if !conflicts.is_empty() {
        return Err(DodotError::CrossPackConflict { conflicts });
    }
    Ok(())
}

// ── Phase 2: atomic swap ──────────────────────────────────────────

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
        // Clean up temp link before returning.
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
            // Best-effort cleanup of the backup directory.
            let _ = fs.remove_dir_all(&backup);
            Ok(())
        }
        Err(e) => {
            // Restore original on failure.
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

fn nonce() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}", n)
}

fn err_msg(e: &DodotError) -> String {
    format!("{e}")
}

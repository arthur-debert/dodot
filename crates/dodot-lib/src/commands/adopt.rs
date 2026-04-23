//! `adopt` command — move existing files into a pack, creating symlinks back.
//!
//! Two-phase model:
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

use std::path::{Path, PathBuf};

use crate::commands::status;
use crate::commands::PackStatusResult;
use crate::conflicts;
use crate::fs::Fs;
use crate::packs;
use crate::packs::orchestration::{self, ExecutionContext};
use crate::rules;
use crate::{DodotError, Result};

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

/// Move sources into a pack, creating symlinks from their original locations.
///
/// See the module-level docs for the two-phase model and failure semantics.
pub fn adopt(
    pack_name: &str,
    sources: &[PathBuf],
    force: bool,
    no_follow: bool,
    dry_run: bool,
    ctx: &ExecutionContext,
) -> Result<PackStatusResult> {
    if sources.is_empty() {
        return Err(DodotError::Other("no files specified".into()));
    }

    let pack_path = ctx.paths.pack_path(pack_name);
    if !ctx.fs.exists(&pack_path) {
        return Err(DodotError::PackNotFound {
            name: pack_name.into(),
        });
    }
    if ctx.fs.exists(&pack_path.join(".dodotignore")) {
        return Err(DodotError::PackInvalid {
            name: pack_name.into(),
            reason: "pack is marked ignored via .dodotignore".into(),
        });
    }

    let (plans, skipped_already_adopted) =
        preflight(pack_name, &pack_path, sources, force, no_follow, ctx)?;

    // If every input was already adopted, there's nothing to do.
    if plans.is_empty() {
        let mut result = status::status(Some(&[pack_name.to_string()]), ctx)?;
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
        let mut result = status::status(Some(&[pack_name.to_string()]), ctx)?;
        result.dry_run = true;
        for msg in skipped_already_adopted {
            result.warnings.push(msg);
        }
        return Ok(result);
    }

    // Phase 2 — per-source atomic swap. Failures are recorded, not fatal.
    let failures = swap_all(&plans, ctx.fs.as_ref());

    let mut result = status::status(Some(&[pack_name.to_string()]), ctx)?;
    result.dry_run = false;
    for msg in skipped_already_adopted {
        result.warnings.push(msg);
    }
    for f in &failures {
        result.warnings.push(format!(
            "adopt failed: {}: {}",
            f.source.display(),
            f.reason
        ));
    }
    Ok(result)
}

// ── Pre-flight ───────────────────────────────────────────────────

fn preflight(
    pack_name: &str,
    pack_path: &Path,
    sources: &[PathBuf],
    force: bool,
    no_follow: bool,
    ctx: &ExecutionContext,
) -> Result<(Vec<AdoptPlan>, Vec<String>)> {
    let fs = ctx.fs.as_ref();
    let home = ctx.paths.home_dir().to_path_buf();
    let dotfiles_root = ctx.paths.dotfiles_root().to_path_buf();
    let data_dir = ctx.paths.data_dir().to_path_buf();

    let root_config = ctx.config_manager.root_config()?;
    let pack_config = ctx.config_manager.config_for_pack(pack_path)?;
    let ignore_patterns = {
        let mut combined = root_config.pack.ignore.clone();
        combined.extend(pack_config.pack.ignore.iter().cloned());
        combined
    };

    let mut plans: Vec<AdoptPlan> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();

    for raw_source in sources {
        // Resolve to absolute, then normalize. Relative paths are resolved
        // against CWD. We normalize logically (strip `.`, collapse `..`)
        // rather than calling `canonicalize()` because canonicalize follows
        // symlinks, which would break `--no-follow`.
        let abs = if raw_source.is_absolute() {
            raw_source.clone()
        } else {
            std::env::current_dir()
                .map_err(|e| DodotError::Fs {
                    path: raw_source.clone(),
                    source: e,
                })?
                .join(raw_source)
        };
        let abs = normalize_path(&abs);

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

        // Nested-source refusal: parent must be HOME (dodot's flat-at-top-level
        // rule applied to source paths too). Allow adopting from HOME directly.
        // Canonicalize the parent (not the source itself — that would follow
        // a symlink source and break `--no-follow`) so OS-level path
        // equivalences like `/var` ↔ `/private/var` on macOS compare equal.
        let parent = abs
            .parent()
            .ok_or_else(|| DodotError::Other(format!("no parent directory: {}", abs.display())))?;
        let canon_parent = std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
        let canon_home = std::fs::canonicalize(&home).unwrap_or_else(|_| home.clone());
        if canon_parent != canon_home {
            return Err(DodotError::Other(format!(
                "nested source not allowed: {}\n  hint: adopt the top-level directory instead (parent must be {})",
                abs.display(),
                home.display()
            )));
        }

        let file_name = abs
            .file_name()
            .ok_or_else(|| DodotError::Other(format!("no filename: {}", abs.display())))?
            .to_string_lossy()
            .into_owned();

        // Pack-filename derivation for a `$HOME/.<name>` source:
        //   - Strip the leading dot.
        //   - If the resulting name is in `force_home` (canonical $HOME
        //     tools — bashrc, zshrc, ssh, …), drop straight into the pack
        //     under that name; the symlink handler's `force_home` rule
        //     will route deploys back to `$HOME/.<name>`.
        //   - Otherwise prefix with `home.` so the file uses the per-file
        //     `home.X` convention (#48). Without this prefix, the post-
        //     #48 default would route the re-deployed file to
        //     `$XDG_CONFIG_HOME/<pack>/<name>`, breaking the adopt
        //     round-trip (`adopt vim ~/.vimrc` then `up` should leave
        //     `~/.vimrc` pointing back at the pack — not relocate it
        //     to `~/.config/vim/vimrc`).
        let stripped = file_name.strip_prefix('.').unwrap_or(&file_name);
        let in_force_home = pack_config
            .symlink
            .force_home
            .iter()
            .any(|entry| entry.strip_prefix('.').unwrap_or(entry) == stripped);
        // The `home.` rename only applies to FILES — there's no
        // `home.<dir>` directory convention. Directories keep the
        // legacy "strip leading dot" behavior; for whole-subtree $HOME
        // routing the user would use the `_home/` directory prefix.
        let pack_filename = if !is_dir && file_name.starts_with('.') && !in_force_home {
            format!("home.{stripped}")
        } else {
            stripped.to_string()
        };

        // Filename-ignore check against pack + root ignore patterns.
        if rules::should_skip_entry(&pack_filename, &ignore_patterns) {
            return Err(DodotError::Other(format!(
                "refusing to adopt {}: name '{}' matches an ignore pattern or is reserved",
                abs.display(),
                pack_filename
            )));
        }

        let pack_dest = pack_path.join(&pack_filename);

        // Destination conflict check. With --force, we'll remove the existing
        // destination before copy; without, this is a hard refusal.
        let dest_exists = fs.exists(&pack_dest) || fs.is_symlink(&pack_dest);
        if dest_exists && !force {
            return Err(DodotError::SymlinkConflict { path: pack_dest });
        }

        // Cross-plan filename collision: can't adopt two things with the same
        // stripped name in a single invocation.
        if plans.iter().any(|p| p.pack_dest == pack_dest) {
            return Err(DodotError::Other(format!(
                "two sources produce the same pack filename '{}'; adopt them separately",
                pack_filename
            )));
        }

        plans.push(AdoptPlan {
            source: abs,
            pack_dest,
            is_dir,
            destructive_overwrite: dest_exists, // only true under --force
        });
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
        pack_intents.push((pack.name.clone(), intents));
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

/// Normalize a path by collapsing `.` and `..` components without touching
/// the filesystem.
///
/// Unlike `std::fs::canonicalize`, this does not follow symlinks — important
/// for `--no-follow`, where we want to preserve the source as a link rather
/// than resolve through it. Parent refs (`..`) are collapsed purely
/// lexically, which is correct for the nested-parent check here since the
/// caller has already joined against `current_dir()` for relative inputs.
fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other),
        }
    }
    result
}

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

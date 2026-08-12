//! `reset` command — factory-reset all dodot-owned state.
//!
//! Removes every top-level entry under the data dir
//! (`~/.local/share/dodot`): `packs/`, the generated shell init,
//! `deployment-map.tsv`, probes, prompts and tutorial state —
//! everything dodot owns, including entries future versions may add.
//! The data dir itself is kept (empty); the next `dodot up` rebuilds
//! from scratch.
//!
//! What reset deliberately does NOT touch:
//!
//! - The dotfiles repo — sources are the user's, never dodot's.
//! - Deploy-target symlinks in `$HOME` / `$XDG_CONFIG_HOME`. Wiping
//!   the datastore leaves them dangling until the next `up` re-links
//!   them, exactly like after `down`.
//! - The cache dir (`~/.cache/dodot`). Its contents are rederivable
//!   and self-heal on the next `up`; a troubleshooting reset gains
//!   nothing from chasing them.
//!
//! This is the troubleshooting escape hatch for "how did it get into
//! THIS state" situations (issue #256): orphaned datastore entries
//! after a dotfiles-repo move, stale layouts from older dodot
//! versions. It should never be *needed*, but when it is, it beats
//! asking users to hand-`rm -rf` internals.
//!
//! Confirmation policy lives in the CLI handler (interactive prompt /
//! `--force`), not here — this function assumes the caller already
//! decided to proceed. `ctx.dry_run` previews the removals without
//! mutating anything.

use tracing::info;

use crate::commands::{shorten_path, MessageResult};
use crate::packs::orchestration::ExecutionContext;
use crate::Result;

/// Run the `reset` command: remove every top-level entry under the
/// data dir. Honors `ctx.dry_run` (list, don't remove). Returns a
/// `MessageResult` whose details name each removed entry (directories
/// with a trailing `/`), sorted by name.
pub fn reset(ctx: &ExecutionContext) -> Result<MessageResult> {
    let data_dir = ctx.paths.data_dir().to_path_buf();
    let display_dir = shorten_path(&data_dir, ctx.paths.home_dir());
    info!(data_dir = %data_dir.display(), dry_run = ctx.dry_run, "starting reset command");

    let entries = if ctx.fs.is_dir(&data_dir) {
        ctx.fs.read_dir(&data_dir)?
    } else {
        Vec::new()
    };

    if entries.is_empty() {
        return Ok(MessageResult {
            message: format!("Nothing to reset — no dodot state under {display_dir}."),
            details: Vec::new(),
        });
    }

    let mut details = Vec::new();
    for entry in &entries {
        let label = if entry.is_dir {
            format!("{}/", entry.name)
        } else {
            entry.name.clone()
        };
        if ctx.dry_run {
            details.push(format!("would remove {label}"));
        } else {
            info!(entry = %entry.path.display(), "removing");
            // `read_dir` reports symlinks as `is_symlink` (not
            // `is_dir`), so links land in the `remove_file` arm and
            // are removed without following — reset never deletes
            // through a link.
            if entry.is_dir {
                ctx.fs.remove_dir_all(&entry.path)?;
            } else {
                ctx.fs.remove_file(&entry.path)?;
            }
            details.push(format!("removed {label}"));
        }
    }

    let message = if ctx.dry_run {
        format!("Would reset all dodot state under {display_dir}.")
    } else {
        format!("All dodot state under {display_dir} removed. Run `dodot up` to redeploy.")
    };

    Ok(MessageResult { message, details })
}

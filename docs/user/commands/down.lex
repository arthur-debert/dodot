:: verified ::
dodot down

The "stop deploying these packs" command. Removes the symlinks dodot made, clears its `$PATH` and shell-source registrations, and removes the install / Brewfile sentinels — so the next `dodot up` treats the pack as un-deployed.

Your dotfiles repo is not touched. `down` only retracts the bookkeeping; the source files in the pack are left where they are.

1. When you reach for it

    - You're temporarily moving away from a pack you don't want active right now.
    - You're about to add a `.dodotignore` marker — run `dodot down <pack>` *first*, so the deployed state gets cleaned up. Once the marker is in place, the pack is no longer discovered, and `down` can't see it either.
    - You're cleaning up after experimenting — running `dodot down` with no arguments tears every pack down at once.
    - You want a fresh re-execute of an install script: `down` followed by `up` clears the sentinel and re-runs.

2. What it does

    For each pack in scope, dodot:

    - lists every handler that has stored state for the pack (`symlink`, `shell`, `path`, `install`, `homebrew`, …);
    - removes the entire on-disk state directory for each — clearing symlinks, shell-source registrations, PATH entries, and content-hashed sentinels;
    - regenerates the shell init script and the deployment map without the removed packs.

    What `down` does *not* do:

    - It does not modify or delete anything in your dotfiles repo. Source files survive.
    - It does not roll back code-execution side-effects. Packages installed by `brew bundle`, files created by `install.sh`, system defaults written via `defaults write` — those are system state, not dodot state. Cleanup is the script author's job.
    - It does not work on `.dodotignore`'d packs. Discovery skips them, so `down` doesn't see them either.

3. Flags

        | Flag        | Effect                                                       |
        | `--dry-run` | Preview removals without making any changes.                 |

    :: table align=ll ::

4. Examples

        # Daily drivers
        dodot down                     # tear down every active pack
        dodot down git                 # tear down a single pack
        dodot down git nvim            # tear down multiple

        # Preview before pulling the trigger
        dodot down --dry-run git nvim

        # Force an install script to re-run on next up
        dodot down git
        dodot up git                   # sentinel was cleared, install.sh runs again

    :: shell ::

5. Watch out for

    - *`down` clears provisioning sentinels.* `dodot down git` followed by `dodot up git` will *re-run* `install.sh` and `brew bundle` because their content-hash sentinels were removed. That's usually what you want when intentionally tearing down; it can surprise if you only meant to retract symlinks. Pass `--no-provision` on the subsequent `up` to skip the re-execution.
    - *Already-open shells lag.* `down` regenerates `dodot-init.sh`, but a shell session that's already open keeps its current `$PATH` and sourced functions until you re-source the rc or open a new shell.
    - *Side-effects don't undo themselves.* If `install.sh` did `mkdir ~/foo`, that directory is still there after `down`. If `brew bundle` installed a hundred packages, they're still installed. Plan provisioning scripts to be idempotent and (where it matters) to track their own undo state, so re-runs after `down` are safe.
    - *Add the `.dodotignore` marker AFTER `down`, not before.* See [./../handlers/controlling-activation.lex] §4.

:: verified ::
dodot up

The "make my live config match what's in this repo" command. Discovers your packs, dispatches each source file to the right handler, materialises symlinks and shell-init state, and runs install scripts and Brewfiles whose source content has changed since the last run. The command you'll actually run a lot.

1. When you reach for it

    - First setup on a new machine: `dodot up` from inside the dotfiles repo.
    - You added or renamed source files in a pack: `dodot up` to register the changes.
    - You edited a source `install.sh` or `Brewfile`: `dodot up` re-runs it, since its content hash has changed.
    - You're about to merge a branch in your dotfiles repo: `dodot up --dry-run` to preview the diff before pulling the trigger.

    For day-to-day edits to source files that are *already* deployed (config files you symlinked, shell scripts already sourced), you do not need `dodot up` — those edits go live at the deployed location through the symlink chain. See the "Live edits" sections in [./../handlers/symlink.lex], [./../handlers/shell.lex], and [./../handlers/path.lex] for the per-handler specifics.

2. What it does

    `up` runs in three phases and is all-or-nothing on conflicts.

    2.1. Plan

        Discovers active packs (every directory under your dotfiles root that doesn't carry a `.dodotignore` marker, with `[pack] os` and `[pack] ignore` filters applied). For each pack, scans its source files and dispatches them to handlers via the rules in `[mappings]`. The output is a list of *intents* — abstract "deploy this here" / "run that there" descriptions, no filesystem changes yet.

        On an active (non-dry-run) `up`, secret providers are pre-flighted up front. A failing provider stops `up` before any pack deploys — no point making half a deployment when half the rendered files would be missing secrets.

    2.2. Detect cross-pack conflicts

        With all intents collected, dodot looks for two packs trying to deploy to the same path — e.g. `git/home.gitconfig` and `dotfiles/home.gitconfig` both targeting `~/.gitconfig`. If any are found, *no pack deploys* — even with `--force`. Cross-pack conflicts are configuration bugs, not "are you sure?" warnings; the fix is to remove or rename the duplicate. The error lists every conflict so you can see all of them at once instead of fixing one and hitting the next.

    2.3. Execute

        For each pack, dodot wipes that pack's stored configuration-handler state (symlink/shell/path) and re-applies from current source. Provisioning handlers (install/homebrew) are gated on content-hash sentinels — they re-run when the source script's bytes have changed, skip otherwise. After all packs are processed, the shell init script is regenerated and the deployment map is written.

        The reconciliation in this phase is what makes `up` idempotent: deleting a source file from a pack and running `up` cleans up its previously-deployed symlink — there is no separate "reconcile" step.

3. Configuration vs provisioning

    Two categories of handler behave differently under `up`:

    - *Configuration handlers* (`symlink`, `shell`, `path`) produce idempotent filesystem work. They run in full on every `up`.
    - *Provisioning handlers* (`install`, `homebrew`) run user-authored code. They are tracked by content-hash sentinels and skip on re-run unless the source content has changed.

    Two flags interact with this split:

    - `--no-provision` skips provisioning handlers entirely on this run. Useful when you want a fast `up` that re-links configuration without paying for `brew bundle` or your install script.
    - `--provision-rerun` forces provisioning handlers to run even when their sentinel matches. Use when you want to re-execute without changing the source — e.g. confirming `brew bundle` is still happy, or re-running an install script after manually undoing what it did.

4. Flags

        | Flag                  | Effect                                                                                       |
        | `--dry-run`           | Plan and detect conflicts without making filesystem changes. Skips secret-provider preflight too — Passive mode. |
        | `--no-provision`      | Skip install + homebrew handlers this run.                                                   |
        | `--provision-rerun`   | Force install + homebrew to re-run even when sentinels match.                                |
        | `--force`             | Overwrite pre-existing target files when their location is already occupied. *Not* a fix for cross-pack conflicts. |

    :: table align=ll ::

5. After up: what's live, what isn't

    `dodot up` updates files; it does not reach into running processes. Specifically:

    - Symlinked configs are live the moment they're created. Whether the program *reading* them sees the change depends on the program: file-watching editors (nvim with `autoread`, vscode) reload immediately; shells re-read their rc on next launch; daemons / window managers / systemd units need an explicit reload command; SSH re-reads its config on each new connection.
    - Shell additions (sourced scripts, `$PATH` entries) are live in *new* shell sessions. Already-open shells keep their prior environment until you `source ~/.zshrc` (or whichever rc) or open a new shell.
    - On macOS, plist changes update the on-disk binary file, but `cfprefsd` may keep serving stale values to running apps from its in-memory cache. After `up` detects a plist change relative to the previous run, dodot offers a `killall cfprefsd` prompt; you can also run that by hand at any time.

6. First-time-on-this-repo prompt

    On the first `up` that detects features needing git-side wiring (templates, plists, or the pre-commit hook), dodot offers to install them in one Y/n — the *install ladder*. Three rungs, in dependency order: pre-commit hook, plist clean/smudge filter, template clean filter. Pick `Yes` to install whichever rungs apply, `Show` to preview the changes first, `No` to dismiss the ladder forever. (You can resurface it later with `dodot prompts reset magic.install_ladder`.)

    See [./git-augmentation.lex] for what each rung does and when you'd want it.

7. Examples

        # Daily drivers
        dodot up                       # deploy every active pack
        dodot up nvim                  # deploy a single pack
        dodot up nvim git              # deploy multiple

        # Before merging a dotfiles branch
        dodot up --dry-run             # show what would change

        # Provisioning controls
        dodot up --no-provision        # skip install/brew this run
        dodot up --provision-rerun     # force install/brew to re-execute

        # Conflict resolution at the deployed location
        dodot up --force git           # overwrite an existing ~/.gitconfig

    :: shell ::

8. Watch out for

    - *`--force` is local, not cross-pack.* It overwrites a file at the target location, but cross-pack conflicts (two packs pointing at the same path) ignore `--force` — the fix is in your packs, not in flag-twiddling.
    - *`.dodotignore`'d packs aren't reconciled.* Adding a `.dodotignore` marker to a previously-deployed pack stops it from being discovered, but `up` only reconciles discovered packs, so the previous deployment's symlinks are *not* cleaned up. Run `dodot down <pack>` *before* dropping the marker. See [./../handlers/controlling-activation.lex] §4.
    - *Open shells lag.* Shell and PATH edits don't reach already-open shell sessions. Source manually or open a new one — there's no in-place reload.
    - *Install scripts run as themselves.* Your `install.sh` runs in a fresh subprocess with its own environment; aliases, functions, and shell options from your interactive shell are not visible to it. The script's extension picks the interpreter (`.sh`/`.bash` → `bash`, `.zsh` → `zsh`), independent of your login shell. See [./../handlers/install.lex].

:: verified ::
The shell handler

Sources your version-controlled shell scripts into your interactive shell at login. Each matched source script is staged in the datastore; the generated `dodot-init.sh` (which you load with `eval "$(dodot init-sh)"`) emits a `source` line that runs the source script in your live shell session.

1. Default claims

    Source filenames matched by the `[mappings] shell` default:

    - `*.sh`
    - `*.bash`
    - `*.zsh`

    Any file at a pack's root with one of these extensions gets sourced. The convention in hand-curated dotfile repos is that loose shell files at the top of a pack — `aliases.sh`, `path.zsh`, `functions.bash`, `50_prompt.sh` — are there to be sourced into the interactive shell, so dodot routes the whole shape rather than a fixed allowlist of names.

    `install.sh` (handled by the install handler) is the carve-out: it runs once instead of being sourced.

    The rule is depth-1 only. A `.sh` file inside a subdirectory of the pack — for example `hypr/scripts/foo.sh` — is not pulled in by this rule; it flows through the symlink handler the same way every other nested file does. That's the right behaviour for window-manager and tmux helper scripts that live at `~/.config/<app>/scripts/*.sh` and are invoked by another tool, not the shell.

2. The extension is the contract

    Your source scripts run *in your live interactive shell*. dodot does not filter by shell — every matched source script is sourced, whichever shell is running. The extension is the convention you declare on the source side:

    - `.sh` — portable; works in any POSIX shell
    - `.bash` — bash-only syntax; will not parse cleanly in zsh
    - `.zsh` — zsh-only syntax; will not parse cleanly in bash

    Most users run one shell and never hit a mismatch. If you switch shells regularly, split your shell config by extension so each one only sources what it can run.

3. Configuration

    No dedicated `[shell]` section — the mapping list IS the configuration:

        [mappings]
        shell = ["aliases.sh", "myextras.zsh", "work.bash"]

    :: toml ::

    The list fully replaces the defaults. If you set `shell`, the wildcard default goes away — re-list `*.sh`/`*.bash`/`*.zsh` alongside your additions if you still want catchall coverage.

4. Live edits

    Once a source script is staged by `dodot up`, edits to the source go live for the next shell session — dodot doesn't need a second `dodot up` to pick up content changes. An already-open shell that has sourced its config doesn't auto-reload; re-source manually with `source ~/.zshrc` (or open a new session) to pick up the edit.

    Adding a new source script to the pack — or removing one — does need another `dodot up` so the staging registers the change. New shells then pick it up.

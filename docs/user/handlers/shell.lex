:: verified ::
The shell handler

Sources your version-controlled shell scripts into your interactive shell at login. Each matched source script is staged in the datastore; the generated `dodot-init.sh` (which you load with `eval "$(dodot init-sh)"`) emits a `source` line that runs the source script in your live shell session.

1. Default claims

    Source filenames matched by the `[mappings] shell` default:

    - `aliases.{sh,bash,zsh}`
    - `profile.{sh,bash,zsh}`
    - `login.{sh,bash,zsh}`
    - `env.{sh,bash,zsh}`

    Twelve patterns covering four conventional names × three extensions.

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

    The list fully replaces the defaults. If you set `shell`, the twelve-pattern default goes away — re-list any defaults you still want alongside your additions.

4. Live edits

    Once a source script is staged by `dodot up`, edits to the source go live for the next shell session — dodot doesn't need a second `dodot up` to pick up content changes. An already-open shell that has sourced its config doesn't auto-reload; re-source manually with `source ~/.zshrc` (or open a new session) to pick up the edit.

    Adding a new source script to the pack — or removing one — does need another `dodot up` so the staging registers the change. New shells then pick it up.

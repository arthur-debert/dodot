:: verified ::
Mappings — assigning source files to handlers

A *mapping* is the rule that says "files matching this pattern go to that handler." dodot ships with a default set of mappings, so most dotfile repos work with no configuration. When the conventions don't match your filenames, override them under `[mappings]` in `.dodot.toml`.

1. The dispatch model

    Each pack scan produces a list of source-file matches. Every mapping is a rule with three parts: a pattern, a handler name, and a priority. The matcher walks rules in descending priority order; the first rule whose pattern matches a source filename wins, and that file is claimed by that handler. Subsequent rules don't see the file.

    Rules with the same priority are checked in declaration order. Filter handlers (`ignore`, `skip`) sit at higher priorities than deploying handlers, so a file the user has marked to drop never reaches the dispatch path even if a precise mapping would otherwise claim it.

2. Defaults

    Default mappings as they ship — listed by priority, highest first:

        | Priority | Handler  | Default claims                                                                                                          |
        | 100      | ignore   | (empty by default)                                                                                                      |
        | 50       | skip     | `README`/`README.*`, `LICENSE`/`LICENSE.*`, `CHANGELOG`/`CHANGELOG.*`, `CONTRIBUTING`/`CONTRIBUTING.*`, `AUTHORS`/`AUTHORS.*`, `NOTICE`/`NOTICE.*`, `COPYING`/`COPYING.*` (case-insensitive) |
        | 20       | install  | `install.sh`, `install.bash`, `install.zsh`                                                                             |
        | 10       | homebrew | `Brewfile`                                                                                                              |
        | 10       | nix      | `packages.nix`                                                                                                          |
        | 10       | path     | `bin/`                                                                                                                  |
        | 10       | shell    | `*.sh`, `*.bash`, `*.zsh` (any shell-extension file at the pack's root)                                                 |
        | 0        | symlink  | `*` (catch-all)                                                                                                         |

    :: table align=rll ::

    The `gate` handler is not in the priority ladder. Gate matching runs at *scan time*, before the rule matcher — gate predicates strip the `._<label>` suffix from a source filename if the host matches, or surface a "gated out" entry if it doesn't. See [./controlling-activation.lex] for the full story.

    `install` sits at priority 20 — above the priority-10 shell wildcard — so as long as `install.sh` is in `mappings.install` (the default), it routes to the install handler rather than being claimed by the shell glob. Without the gap, the install hook would be silently sourced by every shell session. If you override `mappings.install` to drop `install.sh`, the shell wildcard *will* claim it — that's the user's choice.

    Default mappings as raw TOML (the form `dodot config gen` emits):

        [mappings]
        path     = "bin"
        install  = ["install.sh", "install.bash", "install.zsh"]
        shell    = ["*.sh", "*.bash", "*.zsh"]
        homebrew = "Brewfile"
        nix      = "packages.nix"
        ignore   = []
        skip     = [
            "README", "README.*",
            "LICENSE", "LICENSE.*",
            "CHANGELOG", "CHANGELOG.*",
            "CONTRIBUTING", "CONTRIBUTING.*",
            "AUTHORS", "AUTHORS.*",
            "NOTICE", "NOTICE.*",
            "COPYING", "COPYING.*",
        ]

    :: toml ::

3. Configuration shape

    Each `[mappings]` key has a fixed shape. Setting the wrong shape (a string for a list-typed key, or vice versa) is a config-load error.

    Key shapes:
        | Key      | Type    | Notes                                                                          |
        | path     | string  | One directory name per pack. Trailing `/` auto-added.                          |
        | install  | list    | Multiple matched files all run, each with its own sentinel.                    |
        | shell    | list    | Every matched file is sourced.                                                 |
        | homebrew | string  | One `Brewfile` per pack.                                                       |
        | nix      | string  | One `packages.nix` per pack.                                                   |
        | ignore   | list    | Matches drop silently — no entry in `dodot status`.                            |
        | skip     | list    | Matches surface as `skipped` in `dodot status`. Case-insensitive.              |

    :: table align=lll ::

4. Override rules

    Each key in `[mappings]` *replaces* its default wholesale; values are not merged. If you set `[mappings] shell`, the wildcard default goes away — re-list `*.sh`/`*.bash`/`*.zsh` alongside your additions if you still want catchall coverage.

    Pack-level mappings are merged with root-level only at the *key* level, not within values. A pack `.dodot.toml` setting `[mappings] shell = [...]` replaces a root `[mappings] shell` in full for that pack; keys the pack doesn't set inherit from root (or the built-in defaults if root doesn't set them either).

    Example pack override — claim a non-default install-script name without losing the default shell mappings:

        # in <pack>/.dodot.toml
        [mappings]
        install = ["bootstrap.sh"]

    :: toml ::

    Restricting the shell handler to a fixed allowlist of names (opting out of the wildcard) — useful when a pack has loose `.sh` scripts that you'd rather symlink than source:

        [mappings]
        shell = ["aliases.sh", "profile.sh", "myextras.zsh"]

    :: toml ::

    Keeping the wildcard but adding extra extensions:

        [mappings]
        shell = ["*.sh", "*.bash", "*.zsh", "*.fish"]

    :: toml ::

5. Generating a starter file

    Print a fully-commented `.dodot.toml` to stdout:

        $ dodot config gen

    :: shell ::

    Or write it directly:

        $ dodot config gen -o .dodot.toml
        $ dodot config gen > .dodot.toml      # equivalent

    :: shell ::

    The comments in the generated file should be enough to clarify each key's purpose; uncomment the keys you want to override and leave the rest commented to keep the defaults.

6. What mappings can't do

    Mappings re-route source filenames to existing handlers. They do *not* let you add a brand-new handler from configuration — the handler set is fixed in the dodot binary. They also don't change handler behaviour (sentinel keying, symlink resolution rules, shell-init generation); for those, see the per-handler config sections (`[symlink]`, `[path]`, …) and the per-handler snippets under `docs/user/handlers/`.

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
        | 10       | homebrew | `Brewfile`                                                                                                              |
        | 10       | install  | `install.sh`, `install.bash`, `install.zsh`                                                                             |
        | 10       | path     | `bin/`                                                                                                                  |
        | 10       | shell    | `aliases.{sh,bash,zsh}`, `profile.{sh,bash,zsh}`, `login.{sh,bash,zsh}`, `env.{sh,bash,zsh}`                             |
        | 0        | symlink  | `*` (catch-all)                                                                                                         |

    :: table align=rll ::

    The `gate` handler is not in the priority ladder. Gate matching runs at *scan time*, before the rule matcher — gate predicates strip the `._<label>` suffix from a source filename if the host matches, or surface a "gated out" entry if it doesn't. See [./controlling-activation.lex] for the full story.

    Default mappings as raw TOML (the form `dodot config gen` emits):

        [mappings]
        path     = "bin"
        install  = ["install.sh", "install.bash", "install.zsh"]
        shell    = [
            "aliases.sh", "aliases.bash", "aliases.zsh",
            "profile.sh", "profile.bash", "profile.zsh",
            "login.sh",   "login.bash",   "login.zsh",
            "env.sh",     "env.bash",     "env.zsh",
        ]
        homebrew = "Brewfile"
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

        | Key      | Type    | Notes                                                                          |
        | path     | string  | One directory name per pack. Trailing `/` auto-added.                          |
        | install  | list    | Multiple matched files all run, each with its own sentinel.                    |
        | shell    | list    | Every matched file is sourced.                                                 |
        | homebrew | string  | One `Brewfile` per pack.                                                       |
        | ignore   | list    | Matches drop silently — no entry in `dodot status`.                            |
        | skip     | list    | Matches surface as `skipped` in `dodot status`. Case-insensitive.              |

    :: table align=lll ::

4. Override rules

    Each key in `[mappings]` *replaces* its default wholesale; values are not merged. If you set `[mappings] shell`, the twelve-pattern default goes away — re-list any defaults you still want alongside your additions.

    Pack-level mappings are merged with root-level only at the *key* level, not within values. A pack `.dodot.toml` setting `[mappings] shell = [...]` replaces a root `[mappings] shell` in full for that pack; keys the pack doesn't set inherit from root (or the built-in defaults if root doesn't set them either).

    Example pack override — claim a non-default install-script name without losing the default shell mappings:

        # in <pack>/.dodot.toml
        [mappings]
        install = ["bootstrap.sh"]

    :: toml ::

    Adding new patterns without losing the default — for shell, re-list the defaults plus your additions:

        [mappings]
        shell = [
            "aliases.sh", "aliases.bash", "aliases.zsh",
            "profile.sh", "profile.bash", "profile.zsh",
            "login.sh",   "login.bash",   "login.zsh",
            "env.sh",     "env.bash",     "env.zsh",
            "myextras.zsh",
            "work.bash",
        ]

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

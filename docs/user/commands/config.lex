:: verified ::
dodot config

The "show me / generate / edit configuration" command. dodot ships sensible defaults for every option; `.dodot.toml` only carries what you want to override. This command is your entry point to all of it: see what's resolved, generate a starter file, get and set individual keys.

1. When you reach for it

    - Starting a new pack or a new dotfiles repo and you want a documented `.dodot.toml` to begin from: `dodot config gen`.
    - You're not sure what dodot is *currently* using for a given key (after the three layers merge): `dodot config get <key>` or `dodot config list`.
    - You want to set or unset a single key without hand-editing TOML: `dodot config set <key> <value>` / `dodot config unset <key>`.
    - You're driving dodot from another tool and need a JSON Schema for the config: `dodot config schema`.

2. The three layers

    Configuration is loaded in three layers, last wins:

    1. Compiled-in defaults — what dodot ships with.
    2. `$DOTFILES_ROOT/.dodot.toml` — root config, applies to every pack.
    3. `$DOTFILES_ROOT/<pack>/.dodot.toml` — pack config, that pack only.

    Merge rules:

    - Scalars and arrays: override (later layer replaces earlier in full).
    - Maps: deep-merge (nested keys combine; scalars within still override).

    So `[symlink] force_home = ["ssh", "myextra"]` in a root config replaces the default list wholesale, not extends it. `[symlink.targets]` (a map) merges per-key across layers — root entries are kept unless a pack overrides the same key.

3. Subcommands

    Subcommands:
        | Subcommand | Effect                                                                       |
        | `list`     | Show every resolved key/value pair (the default if no subcommand is given). |
        | `get`      | Show the resolved value and the inline documentation for one key.            |
        | `set`      | Persist a value to the config file.                                          |
        | `unset`    | Remove a key from the config file (resolution falls back to the prior layer). |
        | `gen`      | Print a fully-commented sample config to stdout (or `-o <file>` to write).   |
        | `schema`   | Emit a JSON Schema document describing the full config struct.               |

    :: table align=ll ::

    The `--scope` flag picks which file `set` / `unset` write to. Two scopes today: `local` (the dotfiles-root `.dodot.toml`) and `global` (cross-machine config under XDG-data). Default scope is `local`.

4. Key names

    Keys are dotted paths matching the TOML structure: `[symlink] force_home` is `symlink.force_home`; `[mappings] install` is `mappings.install`; `[symlink.app_aliases]` keys are `symlink.app_aliases.<alias>`. `dodot config list` is the source of truth — copy from its output.

5. Examples

        # Show resolved configuration
        dodot config                       # same as `dodot config list`
        dodot config get symlink.force_home
        dodot config get mappings.install

        # Generate starters
        dodot config gen                   # commented sample to stdout
        dodot config gen -o .dodot.toml    # write to file
        dodot config gen > .dodot.toml     # equivalent

        # Edit individual keys
        dodot config set preprocessor.enabled false
        dodot config unset preprocessor.template.vars.editor

        # Tooling integration
        dodot config schema | jq '.properties.symlink'

    :: shell ::

6. Watch out for

    - *`set` writes to disk immediately.* There's no transaction / preview. Use `dodot config gen` if you want to see the full file shape before committing changes.
    - *Some keys aren't valid at root scope.* The merge is per-key but some keys are *meaningless* at root. The clearest example: `[pack] os` is rejected at root level — it would gate every pack against one OS, which is never useful. The error message tells you to move the key into a pack-level `.dodot.toml`. See [./../glossary/dodot-toml.lex] for the glossary entry.
    - *Arrays don't merge.* If you set `[mappings] shell` at root and then again at pack level, the pack's list fully replaces the root's. To extend rather than replace, re-list every default item you want to keep alongside your additions. (See [./../handlers/mappings.lex] §4.)
    - *Schema is for tooling, not humans.* `config schema` emits a machine-readable JSON Schema with every key, its type, and its default. For a human-friendly reference of available keys, `dodot config gen` is the better starting point — its inline comments are written for you to read.

:: verified ::
dodot fill

The "add starter handler files to an existing pack" command. Drops in commented templates for `install.sh`, `aliases.sh`, and `Brewfile` — but only the ones the pack doesn't already have. Existing files are never overwritten.

Useful when a pack started life as just a config file and you now want to attach shell aliases, an install script, or a Brewfile to it.

1. When you reach for it

    - You created a pack with `dodot init <pack>` (which only scaffolds `.dodot.toml`) and want the handler-template starter files in one step.
    - You added a config file to a pack and now want to attach a `Brewfile` so the pack also installs its own dependencies.
    - You're learning a new handler and want a documented placeholder to edit, rather than starting from a blank file.

2. What it creates

    Three template files, in the pack root:

    - `install.sh` — placeholder install script. Created with `0o755` (executable). Header comment explains content-hash sentinel behaviour.
    - `aliases.sh` — placeholder shell-alias file. Header comment notes it's sourced into your shell.
    - `Brewfile` — placeholder Homebrew bundle. Header comment links to standard Brewfile syntax.

    Each template substitutes `PACK_NAME` with the pack's display name in its header comment, so you see the right name in echoes and prompts when you flesh the file out.

    Existing files are skipped, not overwritten. The output reports each file as `(created)` or `(exists, skipped)` so you see what changed.

3. Examples

        dodot fill git                 # add missing templates to the git pack
        dodot status git               # confirm the pack now picks up the new files

    :: shell ::

    Common combo with `init`:

        dodot init nvim                # creates dir + .dodot.toml
        dodot fill nvim                # adds install.sh + aliases.sh + Brewfile

    :: shell ::

4. Watch out for

    - *No `bin/` directory is created.* `fill` only writes the three template files above. If you want a path-handler `bin/`, create it yourself (`mkdir nvim/bin`).
    - *No `.dodot.toml` is created.* `fill` doesn't touch it; if the pack was created with `dodot init`, you already have one. If not, `dodot config gen -o <pack>/.dodot.toml` writes a fresh starter.
    - *Existing files are kept verbatim.* If you've customised an `install.sh`, running `fill` again won't reset it — it'll just report `(exists, skipped)` and move on.
    - *The templates are starting points, not minimal-viable scripts.* They contain comments and example invocations meant to be deleted as you replace them with real content.

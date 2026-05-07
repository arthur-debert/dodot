:: verified ::
dodot init

The "start a new pack" command. Creates a directory under your dotfiles root with the given name and drops in a commented `.dodot.toml` so you have a starting point for any per-pack overrides.

Bare-bones by design — `init` only scaffolds the pack shell. To add starter handler files (`install.sh`, `aliases.sh`, `Brewfile`), run `dodot fill <pack>` afterward.

1. When you reach for it

    - You're starting a new pack from scratch and want a directory + a config stub in one step.
    - You're about to `dodot adopt --into <pack>` files and need the pack to exist first (`--into` does not auto-create packs).
    - You're laying out a fresh dotfiles repo and want each future pack to start with a documented `.dodot.toml`.

2. What it creates

    Two things, exactly:

    - The pack directory at `<dotfiles_root>/<pack>/`.
    - `<dotfiles_root>/<pack>/.dodot.toml` — a starter config with the most common keys commented out, ready to edit.

    That's it. No `install.sh`, no `aliases.sh`, no `Brewfile`, no `bin/` directory. If you want any of those, `dodot fill <pack>` adds them in a second step.

3. After init: typical next steps
        dodot init nvim                # pack directory + .dodot.toml
        cp ~/.config/nvim/init.lua nvim/
        dodot status nvim              # confirm dispatch matches your expectation
        dodot up nvim                  # deploy
    :: shell ::

    Or, if you want the handler-template starter files immediately:

        dodot init nvim
        dodot fill nvim                # add install.sh, aliases.sh, Brewfile
        dodot status nvim

    :: shell ::

4. Examples

        dodot init nvim
        dodot init work-laptop
        dodot init 010-brew            # ordering-prefix, sorts very early

    :: shell ::

5. Watch out for

    - *`init` errors on an existing directory.* It refuses to write into a path that already exists, even if that path is empty. If you want to add `.dodot.toml` to a pack you've already created by hand, write the file directly (`dodot config gen -o nvim/.dodot.toml`).
    - *`init` doesn't run handlers.* The new pack is empty (apart from `.dodot.toml`), so `dodot up nvim` after `init` is a no-op until you put source files in.
    - *Pack name is the directory name.* If you want an ordering prefix (e.g. for cross-pack deploy ordering), include it in the name: `dodot init 010-brew`. The prefix grammar is in [./../handlers/execution-order.lex] §3.

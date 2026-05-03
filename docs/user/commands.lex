Commands

    The dodot command set is intentionally small. Most days you will use `up`, `down`, and `status`. The remaining commands are for onboarding existing dotfiles, bootstrapping new packs, and inspecting configuration.

    All pack-based commands accept zero or more pack names. Without arguments they operate on every discovered pack. With arguments, they operate on just those packs.

    :: note :: See [./../reference/terms-and-concepts.lex] for terminology used throughout.

1. Daily Commands

    1.1. up

        Deploy packs. Creates symlinks, stages shell scripts, adds directories to `$PATH`, and runs code-execution handlers (install, homebrew) when they haven't been run before.

        Usage:

            dodot up                      # deploy all packs
            dodot up git nvim             # deploy specific packs
            dodot up --dry-run            # show what would be done
            dodot up --no-provision       # skip install scripts and brew
            dodot up --provision-rerun    # force re-run of install and brew
            dodot up --force              # overwrite files that already exist at target locations

        :: shell ::

    1.2. down

        Remove deployments for packs. Deletes symlinks, clears PATH entries and shell hooks, removes sentinels. Your dotfiles repository is not touched. After `down`, nothing in the datastore references the pack.

        Usage:

            dodot down                    # remove all packs
            dodot down git                # remove a specific pack
            dodot down --dry-run          # show what would be removed

        :: shell ::

    1.3. status

        Show what dodot sees for each pack: which files matched which handlers, what's pending, what's deployed. `status` never changes anything on disk; it's safe to run any time.

        Usage:

            dodot status                  # status of all packs
            dodot status git              # status of a single pack

        :: shell ::

2. Bootstrapping Commands

    2.1. init

        Create a new pack. Produces the directory, a starter `.dodot.toml`, and template files for each default handler (placeholder `install.sh`, `alias.sh`, `Brewfile`, and a `bin/` directory).

        Usage:

            dodot init mypack

        :: shell ::

    2.2. fill

        Add template files to an _existing_ pack for any handler that doesn't yet have one. Useful when you want to add shell aliases or a Brewfile to a pack that only had a config file.

        Usage:

            dodot fill git

        :: shell ::

    2.3. adopt

        Move an existing system file into a pack and replace the original with a symlink. Useful for bringing legacy `$HOME` dotfiles (`~/.bashrc`, `~/.zshrc`, `~/.gitconfig`, …), XDG-rooted configs (`~/.config/nvim/init.lua`, …), and macOS-specific paths (`~/Library/Application Support/Code/...`, `~/Library/Preferences/com.app.plist`) under dodot's control.

        Flags:
            - `--into <pack>` — force a single destination pack regardless of inference; required when the source carries no useful pack-name structure (HOME canonicals, `~/Library/...` bundle-ID filenames, etc.)
            - `--force` — overwrite an existing destination file in the pack
            - `--dry-run` — show what would be adopted without making changes
            - `--no-follow` — when the source is a symlink, move the link itself rather than its target

        Usage:

            # XDG-rooted: pack name inferred from the path
            dodot adopt ~/.config/nvim/init.lua             # pack `nvim`, in-pack `init.lua`
            dodot adopt ~/.config/helix/                    # expands children, pack `helix`

            # HOME-rooted: pack must be specified
            dodot adopt --into shell ~/.bashrc
            dodot adopt --into git ~/.gitconfig ~/.gitignore_global

            # macOS Application Support: inferred pack, _app/ encoding
            dodot adopt ~/Library/Application\ Support/Code/User/settings.json

            # macOS ~/Library/* (Preferences, LaunchAgents, Fonts, …): require --into
            dodot adopt --into mac-defaults ~/Library/Preferences/com.app.plist
            dodot adopt --into agents ~/Library/LaunchAgents/com.example.foo.plist

        Source-root recognition (see [./../reference/symlink-paths.lex] §9 for the full table):
            - `$XDG_CONFIG_HOME/<X>/<rest>` → pack `<X>`, in-pack `<rest>`
            - `$HOME/.<X>` (file/dir) → require `--into`. Usually in-pack `home.<X>` or `_home/<X>/...`; for entries on the `[symlink].force_home` list (e.g. `.ssh/`, `.gnupg/`) the in-pack path is bare `<X>` since `force_home` already routes deploys back to `$HOME/.<X>`
            - `~/Library/Application Support/<X>/<rest>` → pack `<X>`, in-pack `_app/<X>/<rest>`
            - `~/Library/<sub>/<file>` (macOS) → require `--into`, in-pack `_lib/<sub>/<file>`
            - `~/Library/Containers/...` → refused (sandboxed-app data)

        When an adopted source is a `.plist` and the dodot-plist git filter isn't yet registered, adopt prints a one-line tip pointing at `dodot git-install-filters`. See [./../reference/plists.lex] for the full plist workflow.

        :: shell ::

    2.4. addignore

        Add a `.dodotignore` marker to a pack — the "pack-ignore" mechanism — causing dodot to skip the directory during discovery. Idempotent: safe to run repeatedly. Useful for directories that live in your dotfiles root but aren't meant to be deployed.

        Usage:

            dodot addignore notes

        :: shell ::

3. Inspection and Setup

    3.1. list

        Enumerate every pack dodot can see. A quick way to confirm discovery is working and to remind yourself what exists.

        Usage:

            dodot list

        :: shell ::

    3.2. config

        Inspect or generate configuration. `config gen` writes a starter `.dodot.toml` with every option commented and documented.

        Usage:

            dodot config                       # show resolved config
            dodot config gen                   # print starter config to stdout
            dodot config gen -o .dodot.toml    # write starter config to file

        :: shell ::

    3.3. init-sh

        Print the shell integration script. You don't invoke this directly; you invoke it from your shell rc with `eval "$(dodot init-sh)"`. The script sources every shell file and adds every path directory that the datastore currently knows about.

        Usage:

            eval "$(dodot init-sh)"

        :: shell ::

4. Plist Commands (macOS)

    These are the user-facing surface for dodot's macOS plist support — git clean/smudge filters that translate `*.plist` files between binary (working tree) and canonical XML (git index). See [./../reference/plists.lex] for the full reference; this section covers the commands themselves.

    4.1. git-install-filters

        Write the `[filter "dodot-plist"]` block to the dotfiles repo's `.git/config` so future `git status` / `git diff` / `git add` invocations on tracked `*.plist` files run through dodot's clean/smudge filters automatically. Per-clone, per-machine state. Idempotent on re-run. Pair with a `*.plist filter=dodot-plist` line in `.gitattributes` (committed).

        Usage:

            dodot git-install-filters

        :: shell ::

    4.2. git-show-filters

        Print the `.git/config` block and the `.gitattributes` line without writing anything, for inspection or manual install. Each is annotated with whether it's currently in place.

        Usage:

            dodot git-show-filters

        :: shell ::

    4.3. plist clean / plist smudge

        stdin→stdout filters that git invokes via `dodot git-install-filters`. You don't normally run them by hand; running them manually is fine for inspection but not part of any workflow.

        Usage:

            dodot plist clean   < binary.plist > canonical.xml
            dodot plist smudge  < canonical.xml > binary.plist

        :: shell ::

    4.4. prompts list / prompts reset

        Inspect and reset dismissed one-time prompts. Currently the only registered prompt is `plist.install_filters` (the up-time offer to install plist filters), but the registry is content-agnostic and future onboarding nudges use the same machinery.

        Usage:

            dodot prompts list                              # show every known prompt + state
            dodot prompts reset plist.install_filters       # clear one dismissal
            dodot prompts reset --all                       # clear every dismissal

        :: shell ::

5. Global Flags

    Every command accepts:

    - `--output <format>` — select output format (`term`, `text`, `json`, `yaml`, `term-debug`)
    - `--verbose` — verbose logging to stderr
    - `--debug` — debug logging to stderr (implies `--verbose`)
    - `--help` (or `-h`, or `dodot help <command>`) — per-command help with usage, options, examples, and cross-references

    The dotfiles root is not a flag. dodot resolves it by checking `$DOTFILES_ROOT` first, then falling back to `git rev-parse --show-toplevel` (the enclosing git repo root), then the current working directory. The common case — running commands from inside your dotfiles repo — just works.

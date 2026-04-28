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

        Move an existing system file into a pack and replace the original with a symlink. Useful for bringing legacy `$HOME` dotfiles (`~/.bashrc`, `~/.zshrc`, `~/.gitconfig`, …) under dodot's control.

        Flags:
            - `--force` — overwrite an existing destination file in the pack
            - `--dry-run` — show what would be adopted without making changes
            - `--no-follow` — when the source is a symlink, move the link itself rather than its target

        Usage:

            dodot adopt shell ~/.bashrc
            dodot adopt git ~/.gitconfig ~/.gitignore_global
            dodot adopt shell ~/.bashrc --force
            dodot adopt shell ~/.bashrc --dry-run

        Adopt currently only accepts sources whose parent is `$HOME` directly — files nested under `~/.config/<tool>/...` aren't yet supported. To bring an XDG-rooted file under dodot, copy it into the pack manually.

        Naming inside the pack: a `$HOME/.<name>` source is renamed to `<pack>/home.<name>` if `<name>` isn't already a `force_home` canonical (ssh, gpg, bashrc, zshrc, …). The `home.X` prefix preserves the round-trip — re-deploying with `dodot up` puts the symlink back at `$HOME/.<name>`. For canonical force_home names the file lands under the pack as `<name>` (the `force_home` rule already routes deploys back to `$HOME/.<name>`).

        :: shell ::

    2.4. addignore

        Add a `.dodotignore` marker to a pack, causing dodot to skip it during discovery. Idempotent — safe to run repeatedly. Useful for directories that live in your dotfiles root but aren't meant to be deployed.

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

4. Global Flags

    Every command accepts:

    - `--output <format>` — select output format (`term`, `text`, `json`, `yaml`, `term-debug`)
    - `--verbose` — verbose logging to stderr
    - `--debug` — debug logging to stderr (implies `--verbose`)
    - `--help` (or `-h`, or `dodot help <command>`) — per-command help with usage, options, examples, and cross-references

    The dotfiles root is not a flag. dodot resolves it by checking `$DOTFILES_ROOT` first, then falling back to `git rev-parse --show-toplevel` (the enclosing git repo root), then the current working directory. The common case — running commands from inside your dotfiles repo — just works.

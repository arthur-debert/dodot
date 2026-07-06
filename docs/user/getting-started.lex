:: verified ::
Getting started

    dodot is a dotfiles manager that adapts to you, rather than the other way around. You group config files into directories ("packs"); dodot reads filenames as conventions and decides what to do with each file — symlink it, source it from your shell, run it once, install brew formulae, all of it. There's no apply step, no datastore drift, no separate state from git: edits at either end of the symlink chain are live immediately.

    This doc walks you from "I have a dotfiles repo" to your first `dodot up` on a single pack. About five minutes if you have a repo already, ten if you're building one.

    :: note :: Terminology — this doc uses [pack], [handler], [dotfiles root]. The full vocabulary is at [./glossary/].

1. When you reach for this doc

    - First time using dodot on this machine or this repo.
    - You've installed dodot but haven't run it yet — what's the minimum to get a working pack deployed?
    - You're walking a teammate through dodot and want a guided demo.
    - You want to confirm your mental model before reading anything deeper.

    If your existing dotfiles still live in `$HOME` and `~/.config/`, see [./adopting.lex] for the migration path; this doc assumes you have (or will quickly create) a directory of files to deploy.

2. Prerequisites

    - dodot installed (`brew install arthur-debert/tools/dodot`, or `cargo install dodot`).
    - A directory you can use as your dotfiles root. A git repo is recommended — dodot won't enforce it, but git is the source of truth dodot expects.
    - A few minutes.

3. The mental model

    Two ideas, end-to-end:

    - *Packs.* Each top-level directory under your dotfiles root is a pack. The grouping criterion is up to you — by application, by environment, by usage pattern. Packs are turned up or down as a unit.
    - *Filename conventions.* Inside a pack, filenames decide what dodot does with each file. `Brewfile` runs `brew bundle`. `*.sh` at pack root is sourced into your shell. `bin/` is added to `$PATH`. `install.sh` runs once. Everything else is symlinked under `~/.config/<pack>/`.

    The convention is rooted in common usage patterns, so for most repos the default layout is the most natural one. When it doesn't fit, you override — either by renaming files or by setting overrides in `.dodot.toml`.

    The full handler list is at [./handlers.lex]. The path-resolution rules are at [./paths.lex]. You don't need to read either to start.

4. Walkthrough — your first pack

    Imagine a `nvim` pack with this shape:

        nvim/
        +-- Brewfile    -> includes neovim, ripgrep, fd installs
        +-- aliases.sh  -> common nvim aliases (e.g. vi=nvim)
        +-- bin/        -> custom helper scripts
        +-- init.lua    -> symlinked to ~/.config/nvim/init.lua
        +-- lua/        -> symlinked wholesale to ~/.config/nvim/lua

    :: text ::

    From your dotfiles root, ask dodot what it sees:

        $ cd ~/dotfiles
        $ dodot status nvim

        nvim
            aliases.sh  ⚙ shell profile               pending
            bin         + $PATH/bin                   pending
            init.lua    ➞ ~/.config/nvim/init.lua     pending
            lua         ➞ ~/.config/nvim/lua          pending
            Brewfile    ⚙ brew install                pending

    :: shell ::

    `dodot status` shows both what dodot has already done and what it *would* do on the next `up`. This is your chance to sanity-check the conventions: does dodot's reading of each filename match what you expected? Notice how every symlinked pack-root entry lands under `~/.config/nvim/` — the pack name namespaces config under XDG by default, matching how nvim itself reads its configuration. No need to write `nvim/nvim/init.lua` to land at the right place; dodot does the namespacing.

    Preview the actual deploy without making changes:

        $ dodot up nvim --dry-run

    :: shell ::

    Then deploy for real:

        $ dodot up nvim

        Packs deployed.
        nvim
            init.lua    ➞ ~/.config/nvim/init.lua    deployed
            lua         ➞ ~/.config/nvim/lua         deployed
            aliases.sh  ⚙ shell profile               sourced
            Brewfile    ⚙ brew install              installed

    :: shell ::

    Edit your config — changes are immediate:

        $ nvim ~/.config/nvim/init.lua    # same file as ~/dotfiles/nvim/init.lua

    :: shell ::

    To reverse: `dodot down nvim` removes every dodot-owned artifact for the pack — symlinks, install sentinels, brew sentinels, staged shell init lines. The pack itself stays in your repo, untouched.

5. The three commands

    Every dodot command accepts zero or more pack names. Without arguments it operates on every discovered pack; with arguments, only those.

    Core commands:
        | Command       | Purpose                                                |
        | `dodot status`| What dodot sees per pack. Read-only.                   |
        | `dodot up`    | Deploy packs (symlinks, shell, installs).              |
        | `dodot down`  | Remove every dodot-owned artifact for packs.           |
    :: table align=ll ::

    For the full set (init, adopt, fill, list, addignore, …), see [./commands.lex].

6. Shell integration

    For the `shell` and `path` handlers to take effect, add one line to your shell rc:

        eval "$(dodot init-sh)"

    :: shell ::

    Once per machine. The init script is regenerated by every `dodot up` and `dodot down`, so adding new packs surfaces in your next shell automatically. The full story (where to put it, what belongs above it, diagnosing slow shell startup) is at [./shell-integration.lex].

7. When the conventions don't fit

    Two surfaces for overriding:

    - *Filename conventions.* Use `home.bashrc` to deploy at `~/.bashrc` instead of `~/.config/<pack>/bashrc`. Use `_xdg/foo/` to put a subtree under `~/.config/foo/` without your pack name in the path. The full set is at [./paths.lex] §4.
    - *`.dodot.toml`.* A root config (`<dotfiles-root>/.dodot.toml`) applies to every pack; a pack config (`<pack>/.dodot.toml`) applies only to that pack. Generate a commented starter:

        $ dodot config gen -o .dodot.toml

      :: shell ::

      The schema is at [./configuration.lex].

    Two preferences, two surfaces — pick whichever feels cleaner for the case at hand. For path-related decisions specifically, see [./paths.lex] for the full menu of escape hatches.

8. Watch out for

    - *Open shells lag.* `dodot up` regenerates the shell init script, but already-running shells hold their old environment. Open a new shell or re-source your rc.
    - *`dodot down` only sees discovered packs.* If you've added a `.dodotignore` marker to a pack, `down` won't reconcile it. See [./filters.lex] §3 for the safe sequence.
    - *Pack-root files only get the convention treatment.* Nested files (e.g. `pack/scripts/foo.sh`) fall through to the symlink handler — they aren't auto-sourced. That keeps window-manager helpers and similar scripts from being pulled into shell init.
    - *No apply step is the *whole* point.* Edits at the deployed location go through the symlink chain and land in your pack. Programs that file-watch (nvim with `autoread`, vscode) reload immediately; daemons usually need an explicit reload.

9. What's next

    Interactive walkthrough using your real dotfiles:

        dodot tutorial

    :: shell ::

    Twelve steps, ten minutes, no toy examples. Nothing changes without an explicit yes.

    The doc library, in roughly the order you'll need them:

    - [./adopting.lex] — moving existing dotfiles from `$HOME` and `~/.config/` into packs.
    - [./shell-integration.lex] — the `eval` line in detail.
    - [./paths.lex] — where files end up at deploy time.
    - [./handlers.lex] — index of all eight handlers.
    - [./filters.lex] — keeping files out of dispatch.
    - [./templates.lex] — per-host config via `*.tmpl` rendering.
    - [./secrets.lex] — value injection and whole-file decryption.
    - [./conditional-running.lex] — host-conditional deployment (gates).
    - [./plists.lex] — macOS app preferences under git diff.
    - [./configuration.lex] — `.dodot.toml` schema.
    - [./commands.lex] — every command, with per-command pages.
    - [./troubleshooting.lex] — symptom-first map for when something's not behaving.
    - [./../reference/philosophy.lex] — why dodot is shaped the way it is.

    `dodot --help` lists every command and flag with examples; `dodot help <command>` is the per-command deep dive.

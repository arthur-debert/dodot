dodot: Up and Running in 5 Minutes

    dodot aims to give you the full benefit of a dotfile manager (centralized control, versioning, deployment, modularity) while minimizing its costs.

    - Minimal structure imposed on your dotfiles.
    - No change to how you edit your configs.
    - Changes are always live; there is no "apply" step.
    - Git is the source of truth; no extra database or state files.
    - Minimal setup or migration required.
    - Three commands to know: `up`, `down`, `status`.

    Prerequisites:
        - Your dotfiles in a git repository
        - Files organized in directories (or willing to organize them)
        - A couple of minutes to try it

    How does dodot achieve this? The core principle is that the structure of your dotfiles is the configuration itself, as long as a few simple rules are followed.

    - Organize your dotfiles in directories. The criterion is up to you: by application, usage, environment, whatever suits. These directories are "packs," and they are turned up or down as a unit.
    - Inside each pack, dodot follows common naming conventions to decide what to do with each file.

    :: note :: dodot automatically discovers every directory in your dotfiles root as a pack.

    An example pack for git:

    Pack structure and status:

        nvim/
        +-- Brewfile    -> includes neovim, ripgrep, fd installs
        +-- aliases.sh  -> common nvim aliases (e.g. vi=nvim)
        +-- bin/        -> custom helper scripts
        +-- init.lua    -> symlinked to ~/.config/nvim/init.lua
        +-- lua/        -> symlinked wholesale to ~/.config/nvim/lua

        $ cd ~/dotfiles
        $ dodot status nvim

        nvim
            aliases.sh  ⚙  source by shell         pending
            bin         +  add to PATH             pending
            init.lua    ➞  ~/.config/nvim/init.lua pending
            lua         ➞  ~/.config/nvim/lua      pending
            Brewfile    ⚙  brew install            pending

        Legend: ⚙ shell/brew config, + PATH addition, ➞ symlink
        Status: pending (ready to deploy), deployed (active)

    :: shell ::

    Notice how every symlinked pack-root entry defaults to `~/.config/nvim/` — the pack name namespaces symlinked config under XDG by default, matching how nvim itself reads its configuration. No need to write `nvim/nvim/init.lua` to land at the right place for those symlinked entries; dodot does the namespacing. (Non-symlink handlers — Brewfile, shell, path — work on their own conventions and don't deploy under that directory.)

    `dodot status` shows both what dodot has done and what it _will_ do on the next `up`. This is your chance to sanity-check that the conventions dodot detected match what you expected. If they don't, rename the files or override the mapping in `.dodot.toml`.

    Customizing and deploying:

        $ dodot config gen -o nvim/.dodot.toml
        $ cat nvim/.dodot.toml
        [mappings]
        # path = "bin"
        # install = "install.sh"
        # shell = ["aliases.sh", "profile.sh", "login.sh"]
        # homebrew = "Brewfile"
        # ignore = []                                 # silent drop
        # skip   = ["README.*", "LICENSE.*", ...]    # listed in status as `skipped`

        # Preview what will happen without making changes
        $ dodot up nvim --dry-run

        $ dodot up nvim
        ... homebrew:  nvim/Brewfile: installed
        ... shell:     nvim/aliases.sh: sourced
        ... symlink:   nvim/init.lua -> ~/.config/nvim/init.lua: deployed

        nvim
            aliases.sh  ⚙  source by shell             deployed
            bin         +  add to PATH                 deployed
            init.lua    ➞  ~/.config/nvim/init.lua    deployed
            lua         ➞  ~/.config/nvim/lua         deployed
            Brewfile    ⚙  brew install                deployed

        # Edit your config - changes are immediate
        $ nvim ~/.config/nvim/init.lua  # or ~/dotfiles/nvim/init.lua - same file

    :: shell ::

    `dodot down git` reverses the deployment. All commands accept one or more pack names (`dodot up git nvim`) or operate on every pack when run without arguments.

    By combining directory grouping into packs and filename conventions, dodot handles most setups with no configuration at all. When the conventions don't match your files, rename them or override via `.dodot.toml`.

1. Quick Start

    - `cd ~/dotfiles` (or wherever your dotfiles live)
    - `dodot status` to see what dodot will do
    - Fine-tune by renaming files or adding a `.dodot.toml`
    - `dodot up [pack]` to deploy (or `dodot up` for everything)
    - `dodot down [pack]` to cleanly remove a pack

2. Shell Integration

    For the shell and path handlers to take effect, add one line to your shell rc:

    Shell integration:

        eval "$(dodot init-sh)"

    :: shell ::

    This is a one-time step per machine. The init script is regenerated on every `dodot up` and `dodot down`, so you never need to touch this line again.

    A small footnote on what belongs *above* this line, in raw shell rc: anything that has to exist before dodot itself can run. The two real cases are Homebrew's shell environment (the `eval "$(... brew shellenv)"` line — with whatever absolute path your install uses, typically `/opt/homebrew/bin/brew` on Apple Silicon and `/usr/local/bin/brew` on Intel — that puts `dodot` on `$PATH` in the first place) and OS-level prereqs that block any pack from succeeding (xcode-select, license acceptance). Everything else belongs in a pack. See [./handlers/execution-order.lex] for how packs are ordered relative to each other once dodot does take over.

3. What's Next

    - `dodot tutorial` — interactive 10-minute walkthrough using your real dotfiles
    - `dodot --help` — every command and flag (with examples and cross-references)
    - `dodot help <command>` — detailed help for any command
    - [./commands/up.lex], [./commands/down.lex], [./commands/status.lex] — the daily-driver commands
    - [./commands.lex] — full command index
    - [./configuration.lex] — the `.dodot.toml` schema
    - [./templates.lex] — per-machine config via templates
    - [./../reference/philosophy.lex] — why dodot is shaped the way it is
    - [./../reference/terms-and-concepts.lex] — the shared vocabulary

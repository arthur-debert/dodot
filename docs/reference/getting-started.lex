dodot: Up and Running in 5 Minutes

    dodot aims to give you the full benefit of a dotfile manager (centralized control, versioning, deployment and modularity) while minimizing its costs.

    - Impose minimal structure on your dotfiles.
    - No change to your workflow on editing your configs; edit them as you please.
    - Changes are always live, there is no "apply" step.
    - Git is the source of truth, no extra database or state files.
    - Requires minimal setup or migration.
    - Up/down/status, nothing more to learn.

    Prerequisites:
        - Your dotfiles in a git repository
        - Files organized in directories (or willing to organize them)
        - 2 minutes to try it out

    How does dodot achieve this? The core principle is that the structure of your dotfiles is the configuration itself, as long as a few simple rules are followed.

    - Organize your dotfiles in directories. The criteria is up to you: by application, usage, environment, whatever you like. These form "packs" and are turned up or down as a unit.
    - Inside each pack, dodot follows a common naming convention to determine what to do with each file.

    :: note :: dodot automatically discovers all directories in your dotfiles root as packs.

    Let's see an example pack for git.

    Pack structure and status:

        git/
        +-- Brewfile    -> includes git, gh and lazygit installs
        +-- alias.sh    -> common aliases for git (e.g. gco: git checkout)
        +-- bin/        -> custom git-related scripts
        +-- gitconfig   -> symlinked to ~/.gitconfig

        $ cd ~/dotfiles
        $ dodot status git

        git
            alias.sh    ⚙  source by shell     pending
            bin         +  add to PATH         pending
            gitconfig   ➞  ~/.gitconfig        pending
            Brewfile    ⚙ brew install        pending

        Legend: ⚙ shell/brew config, + PATH addition, ➞ symlink, × install script
        Status: pending (ready to deploy), deployed (active)

    :: shell ::

    `dodot status` not only shows you what dodot did, but what it will do on running. At this point, double check that everything looks good, that this is what you expect to happen from these files. If not, you can rename the files, or remap dodot.

    Customizing and deploying:

        $ dodot genconfig --write git
        $ cat git/.dodot.toml
        [mappings]
        # path = "bin"
        # install = "install.sh"
        # shell = ["aliases.sh", "profile.sh", "login.sh"]
        # homebrew = "Brewfile"
        # ignore = []
        # Now that everything looks good, we're ready:
        # Preview what will happen without making changes
        $ dodot up git --dry-run

        $ dodot up git
        ... homebrew:  git/Brewfile: installed
        ... shell:     git/alias.sh: sourced
        ... symlink:   git/gitconfig -> ~/.gitconfig: deployed

        git
            alias.sh    ⚙  source by shell        deployed
            bin         +  add to PATH           deployed
            gitconfig   ➞  ~/.gitconfig          deployed
            Brewfile    ⚙ brew install          deployed

        # Edit your config - changes are immediate!
        $ vim ~/.gitconfig  # or ~/dotfiles/git/gitconfig - same file!

    :: shell ::

    And `dodot down git` would do the reverse.

    All dodot commands can specify one or more packs (`dodot up git nvim`) or, when run without arguments, all packs.

    By using directory grouping into packs, and file naming conventions, dodot can get a lot done without setup. If the name convention doesn't match your files you can either rename them or remap them.

1. Quick Start Guide

    - `cd ~/dotfiles` (or wherever your dotfiles live)
    - Run `dodot status` to see what dodot will do
    - Fine-tune by renaming files or creating `.dodot.toml` configs
    - Run `dodot up [pack]` to deploy (or `dodot up` for all packs)
    - Use `dodot down [pack]` to cleanly remove

2. What's Next?

    - Run `dodot --help` for all commands
    - Check out examples at [https://github.com/arthur-debert/dodot/examples]
    - Learn about advanced features in the docs

Design Philosophy

    dodot is built on a set of core principles that drive every design decision.

1. Core Principles

    - No State Management: the filesystem IS the state
    - Everything is a Symlink: live editing, no copying
    - Convention Over Configuration: smart defaults that just work
    - Transparency: you can always see what dodot will do
    - Simplicity: do one thing well, manage dotfiles

2. The Mental Model

    *Packs* then *Handlers* then *Operations*.

    Packs:
        Directories of related dotfiles (vim/, zsh/, git/).

    Handlers:
        Convert file matches to operations (symlink, install, homebrew, shell, path).

    Operations:
        Four types: CreateDataLink, CreateUserLink, RunCommand, CheckSentinel.

3. Design Philosophy

    3.1. Live Configuration Through Symlinks

        Your dotfiles repository IS your live configuration.

        Symlink example:

            ~/.vimrc -> ~/dotfiles/vim/.vimrc

        :: shell ::

        Edit either file, changes apply immediately. No sync, no push, no rebuild.

    3.2. Convention Over Configuration

        dodot makes intelligent assumptions:

        - `.vimrc` goes to `~/.vimrc`
        - Files in `bin/` directories get added to PATH
        - `install.sh` runs once during setup
        - `Brewfile` installs Homebrew packages

        You can override these, but the defaults handle 90% of cases.

    3.3. Separation of Concerns

        - *Rules* match files to handlers
        - *Handlers* convert matches to operations (plan)
        - *Operations* execute through minimal DataStore API (do)

        Each layer is independent and testable.

4. The Execution Flow

    Most dodot commands follow a unified pipeline:

    - Discover Packs
    - Execute Command per Pack
    - Aggregate Results

    For handler-based commands (up/down), each pack execution follows:

    - Match Rules
    - Generate Operations
    - Execute Operations

    This predictability means:

    - You can always preview with `--dry-run`
    - The order is deterministic
    - No surprises or hidden behavior

5. Trade-offs and Consequences

    5.1. What We Get

        - *Zero learning curve*: if you understand symlinks, you understand dodot
        - *Live editing*: change a file, see results immediately
        - *Transparency*: `ls -la ~` shows exactly what's managed
        - *Version control*: Git tracks your actual configuration files
        - *Portability*: no dodot-specific formats or databases

    5.2. What We Accept

        - *No rollback*: symlinks don't have history (use git)
        - *No profiles*: one configuration per machine
        - *Manual organization*: you decide the structure
        - *Platform differences*: some features are OS-specific

6. File Organization Philosophy

    The *pack* is the unit of organization.

    Pack structure:

        dotfiles/
        +-- vim/          # Everything Vim
        |   +-- .vimrc
        |   +-- .vim/
        +-- zsh/          # Everything Zsh
        |   +-- .zshrc
        |   +-- aliases.sh
        +-- git/          # Everything Git
            +-- .gitconfig

    :: text ::

    Packs can be:

    - Copied between repos
    - Shared with others
    - Enabled/disabled as a unit
    - Self-contained with their own `install.sh`

7. When dodot Fits

    dodot is perfect when you want:

    - Direct control over your dotfiles
    - Live editing without rebuilds
    - Simple, predictable behavior
    - Version control for configurations
    - Cross-platform dotfile management

    dodot is not for you if you need:

    - Multiple configuration profiles
    - Complex state management
    - Rollback capabilities
    - Template transformations
    - Secret management

8. The Bottom Line

    dodot does the minimum required to be useful and nothing more. It respects that your dotfiles are yours; it just helps you put them in the right places.

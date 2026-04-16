Configuration System

1. Overview

    dodot uses a layered TOML configuration system. All settings have compiled defaults baked into the binary. Users can override any subset via `.dodot.toml` files at the dotfiles root (applies to all packs) or inside individual packs (applies to that pack only).

2. Configuration Hierarchy

    Configuration is loaded and merged in this order (later sources override earlier):

    - *Compiled defaults*: `#[config(default = ...)]` on `DodotConfig` struct fields
    - *Root config*: `$DOTFILES_ROOT/.dodot.toml`
    - *Pack config*: `$DOTFILES_ROOT/<pack>/.dodot.toml`

    2.1. Merge Rules

        Values are merged according to their type:

        - *Scalars*: override (last value wins)
        - *Arrays*: override (last value wins, no accumulation)
        - *Maps*: deep merge recursively (only Tables recurse; nested arrays/scalars override)

        Configuration is managed through the clapfig crate, which provides layered config resolution with compiled defaults, file discovery, and per-pack merging via its Resolver.

3. Configuration Files

    All configuration files use the same TOML format with the sections below.

    These can be used in `$DOTFILES_ROOT`, in which case they apply to all packs, or inside packs in which case they apply only to that pack. When both are present, the pack config overrides the root config for shared keys.

    3.1. Pack Section

        Glob patterns for files and directories to ignore during pack discovery and file scanning. These will not be symlinked or processed.

        Pack ignore:

            [pack]
            ignore = [
                ".git",
                ".svn",
                ".hg",
                "node_modules",
                ".DS_Store",
                "*.swp",
                "*~",
                "#*#",
                ".env*",
                ".terraform"
            ]

        :: toml ::

    3.2. Symlink Section

        dodot respects the user's `XDG_CONFIG_HOME`. However, some programs do not. Config files for these (such as `.ssh`, `.gnupg`, `.aws`) will be symlinked to `$HOME` instead.

        Force home:

            [symlink]
            force_home = [
                "ssh",            # .ssh/ - security critical
                "aws",            # .aws/ - credentials
                "kube",           # .kube/ - kubernetes config
                "bashrc",         # .bashrc - shell expects in $HOME
                "zshrc",          # .zshrc - shell expects in $HOME
                "profile",        # .profile - shell expects in $HOME
                "bash_profile",   # .bash_profile
                "bash_login",     # .bash_login
                "bash_logout",    # .bash_logout
                "inputrc"         # .inputrc - readline config
            ]

        :: toml ::

        Files that won't be symlinked as they are often present by mistake, for security reasons. You can remove them from this list if you want to symlink them.

        Protected paths:

            [symlink]
            protected_paths = [
                ".ssh/id_rsa",
                ".ssh/id_ed25519",
                ".ssh/id_dsa",
                ".ssh/id_ecdsa",
                ".ssh/authorized_keys",
                ".gnupg",
                ".aws/credentials",
                ".password-store",
                ".config/gh/hosts.yml",
                ".kube/config",
                ".docker/config.json"
            ]

        :: toml ::

        Custom per-file symlink target overrides. Maps relative pack filename to an absolute or relative target path. Absolute paths are used as-is; relative paths are resolved from `$XDG_CONFIG_HOME`.

        Targets:

            [symlink.targets]
            "mysterious.conf" = "/var/etc/mysterious.conf"
            "home-bound.conf" = "my-documents/home-bound.conf"

        :: toml ::

    3.3. Mappings Section

        File name mappings inside packs. These overwrite dodot's default mappings. They are valid for both directories and files, and `*` patterns are supported. Can be an array of strings or a single string.

        Mappings:

            [mappings]
            path = "bin"
            install = "install.sh"
            shell = ["aliases.sh", "profile.sh", "login.sh"]
            homebrew = "Brewfile"
            skip = []

        :: toml ::

        The `skip` field lists additional filename patterns to exclude from handler processing within a pack. This is distinct from `[pack] ignore`, which controls pack discovery and file scanning.

4. Architecture

    The configuration system is built around a `ConfigManager` that wraps a clapfig `Resolver`.

    `ConfigManager::new(dotfiles_root)` builds a clapfig Resolver configured to search for `.dodot.toml` files using ancestor-walk up to the `.git` boundary (i.e. the dotfiles root), merging all found files. This prevents stray `.dodot.toml` files above the repo from leaking in. `config_manager.root_config()` loads the root-level config by merging compiled defaults with any `$DOTFILES_ROOT/.dodot.toml` file. `config_manager.config_for_pack(pack_path)` resolves the fully merged config for a specific pack, layering the pack's `.dodot.toml` on top of the root config.

    The `DodotConfig` struct uses `#[derive(confique::Config)]` to declare fields with compiled defaults, so every config key has a known default value even when no TOML files are present.

    Generate a starter config with `dodot config gen`. This auto-generates a commented TOML template from the struct definitions and their doc comments.

    All executions should get a root config at startup and pass it into the main command pipeline. When running pack pipelines, the loop calls `config_for_pack` for each pack to produce a merged config that handlers consume.

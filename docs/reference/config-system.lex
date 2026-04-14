Configuration System

1. Overview

    dodot has two configuration parts: internal and user facing. Both are represented as TOML documents. We differentiate them by using:

    - App defaults: internal settings.
    - Configuration: the parts that are user facing, stored in `dodot.toml` (or `.dodot.toml`) files.

    The config has three layers: the pack layer, the root layer and the app layer. The latter two are optional, and can be present simultaneously. Keys in upper layers override keys in the lower ones, as they are merged.

2. Configuration Hierarchy

    Configuration is loaded and merged in this order (later sources override earlier):

    - *App Defaults*: embedded `defaults.toml`
    - *App Config*: embedded `user-defaults.toml`
    - *Root config*: `$DOTFILES_ROOT/.dodot.toml`
    - *Pack config*: `$DOTFILES_ROOT/<pack>/.dodot.toml`

    2.1. Merge Rules

        Values are merged according to their type:

        - *Arrays*: append (values accumulate)
        - *Scalars*: override (last value wins)
        - *Maps*: deep merge recursively

        Configuration is managed through the clapfig crate, which provides layered config resolution with compiled defaults, file discovery, and per-pack merging via its Resolver.

3. Configuration Files

    All configuration files use the same TOML format with the sections below.

    These can be used in `$DOTFILES_ROOT`, in which case they apply to all packs, or inside packs in which case they apply only to that pack. When both are present, the pack config overrides the root config for shared keys.

    3.1. Pack Section

        Filenames that will be ignored (not symlinked or processed) unless explicitly included in the mapping section.

        Pack ignore:

            [pack]
            ignore = [
                ".git",
                ".svn",
                ".hg",
                "node_modules",
                ".ds_store",
                "*.swp",
                "*~",
                "#*#",
                ".env*",
                ".terraform/"
            ]

        :: toml ::

    3.2. Symlink Section

        dodot respects the user's `XDG_CONFIG_HOME`. However, these programs below do not. Config files for these (such as `.ssh`, `.gnupg`, `.aws`) will be symlinked to `$HOME`.

        Force home:

            [symlink]
            force_home = [
                "ssh",        # .ssh/ - security critical
                "aws",        # .aws/ - credentials
                "kube",       # .kube/ - kubernetes config
                "bashrc",     # .bashrc - shell expects in $home
                "zshrc",      # .zshrc - shell expects in $home
                "profile"     # .profile - shell expects in $home
            ]

        :: toml ::

        Files that won't be symlinked as they are often present by mistake, for security reasons. You can remove them from this list if you want to symlink them.

        Protected paths:

            protected_paths = [
                ".ssh/authorized_keys",
                ".ssh/id_rsa",
                ".ssh/id_ed25519",
                ".gnupg",
                ".password-store",
                ".config/gh/hosts.yml",
                ".aws/credentials",
                ".kube/config",
                ".docker/config.json"
            ]

        :: toml ::

    3.3. Mappings Section

        File name mappings inside packs. These overwrite dodot's default mappings. They are valid for both directories and files, and `*` patterns are supported. Can be an array of strings or a single string.

        Mappings:

            [mappings]
            path = "bin"
            install = "install.sh"
            shell = ["aliases.sh", "profile.sh", "login.sh"]
            homebrew = "brewfile"
            ignore = []

        :: toml ::

4. Architecture

    The configuration system is built around a `ConfigManager` that wraps a clapfig `Resolver`.

    `ConfigManager::new(dotfiles_root)` builds a clapfig Resolver that knows the dotfiles root and the locations of default and user-facing config files. `config_manager.root_config()` loads the root-level config by merging compiled defaults with any `$DOTFILES_ROOT/.dodot.toml` file. `config_manager.config_for_pack(pack_path)` resolves the fully merged config for a specific pack, layering the pack's `.dodot.toml` on top of the root config.

    The `DodotConfig` struct uses `#[derive(confique::Config)]` to declare fields with compiled defaults, so every config key has a known default value even when no TOML files are present.

    All executions should get a root config at startup and pass it into the main command pipeline. When running pack pipelines, the loop calls `config_for_pack` for each pack to produce a merged config that handlers consume.

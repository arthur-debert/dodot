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

        Should be done through the koanf library.

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

    The code should have a consistent pattern that does, in pseudo code:

    Config loading:

        GetRootConfig(<the /pkg/config path/object for dep injection,
                       dotfile root path,
                       pack path or none>)

    :: text ::

    This code will search for the `.dodot.toml` files if present and do the merging. All executions should get a RootConfig object (merged with the default) at the execution start, and that should be passed to the main command pipeline.

    When running pack pipelines, the loop should create a pack config for each pack (using the central function) and pass that down so that the actions can run (these will use this config object).

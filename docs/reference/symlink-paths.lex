Symlink Deployment Paths

    As symlink is the core of a dotfiles manager, dodot is designed with smart defaults with the ability to override them.

1. The Config Home Directory

    Dodot respects the `XDG_CONFIG_HOME` specification. In essence, it means that if the user has set the `XDG_CONFIG_HOME` environment variable, dodot will honor it, otherwise it will default to `~/.config`. Therefore your config home is either `$XDG_CONFIG_HOME` or `$HOME`. For brevity's sake, we will refer to it as `$XDG_CONFIG_HOME` in the rest of this document.

    Hence in the simplest cases `<pack>/<file-or-dir>` will be symlinked to `$XDG_CONFIG_HOME/<file-or-dir>`.

    Note that symlinks are flat: dodot will create symlinks for files and directories, but it will not create symlinks for files inside directories.

2. The `dot.<file>` Convention

    Most dotfiles are, predictably, prefixed with a dot. While that's very useful for keeping your home dir tidy, it does mean that these files inside your dotfiles root repo are hidden by default (i.e. for `ls`). This can be confusing, requiring you to ensure you are seeing hidden files on whatever tool or editor you are using.

    To make this easier, if a symlink path starts with "dot.", dodot will strip the "dot" prefix. For example `<pack>/dot.bashrc` will be symlinked to `$XDG_CONFIG_HOME/.bashrc`.

    If you have a file that actually starts with "dot.", you can use a `.dodot.toml` config to override the symlink path.

3. Forced Home for Unix Canons

    While by far most unix tools are `XDG_CONFIG_HOME` compliant, there are some files and directories that are expected to be in certain locations by convention. For example, `~/.bashrc` is expected to be in the home directory, not in `$XDG_CONFIG_HOME`. This is mainly because after decades of unix tradition, many tools still expect these files to be in the home directory.

    Dodot keeps a list of these files that are forced to be in the home directory, even if your `XDG_CONFIG_HOME` is set to something else. Like usual, you can change this behavior with a `.dodot.toml` config.

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

    Overriding this list allows you to change this behavior in case you need to, including adding other paths to force to home.

4. Linking Outside of `XDG_CONFIG_HOME`

    You can tell dodot to link a file to any arbitrary location by using the `.dodot.toml` config.

    Custom paths:

        [symlink.targets]
        "misterious.conf" = "/var/etc/misterious.conf"
        "home-bound.conf" = "my-documents/home-bound.conf"

    :: toml ::

    This will link `<pack>/misterious.conf` to `/var/etc/misterious.conf`. If the path is a relative path, it will be relative to your `XDG_CONFIG_HOME`. In the example above, `<pack>/home-bound.conf` will be linked to `$XDG_CONFIG_HOME/my-documents/home-bound.conf`.

5. Explicit `$HOME` or `XDG_CONFIG_HOME` via Directory Prefix

    If you want to explicitly link to one of the above you can also do so by inserting links inside `<pack>/_home` or `<pack>/_xdg`. For example, `some-pack/_home/aconf.ini` will be linked to `$HOME/.aconf.ini` regardless of the `XDG_CONFIG_HOME` setting. Likewise, `some-pack/_xdg/aconfig.ini` will be linked to `$XDG_CONFIG_HOME/aconfig.ini` always.

6. Security Restricted Symlink File Names

    To avoid accidental security issues, dodot will not create symlinks for the following files and directories. This can also be configured.

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

7. Ignored File Patterns

    These are unlikely to be useful as symlinks, and are often present by accident or auto generated. These will not be linked, something you can override through config.

    Ignored patterns:

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
            ".terraform/"
        ]

    :: toml ::

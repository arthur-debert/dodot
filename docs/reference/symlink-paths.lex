Symlink Deployment Paths

    Symlinking is the core of a dotfile manager, and dodot ships with smart defaults plus overrides for every case where the defaults are wrong. This document is the full reference for where files end up on deploy.

    :: note :: See [./terms-and-concepts.lex] for terminology used throughout.

1. The Default Rule

    Dodot respects the `XDG_CONFIG_HOME` specification. If the user has set the `XDG_CONFIG_HOME` environment variable, dodot honors it; otherwise it defaults to `~/.config`. For brevity, this document refers to it as `$XDG_CONFIG_HOME`.

    The default rule for every pack-root entry — file or directory — is:

        <pack>/<rel_path>  →  $XDG_CONFIG_HOME/<pack>/<rel_path>

    So `nvim/init.lua` → `~/.config/nvim/init.lua`, and `warp/themes/` → `~/.config/warp/themes/`. The pack name namespaces the deploy path under XDG, matching the convention modern tools (nvim, helix, ghostty, kitty, alacritty, lazygit, starship, …) already follow without forcing you to write `pack/program/` doubled paths.

    The escape hatches in §2–§5 cover the cases where this default isn't what you want (canonical $HOME tools, single-file overrides, namespace-skipping, custom paths).

    Symlinks are flat: dodot creates one symlink per top-level entry of the pack. For a top-level directory, the directory itself is linked, not each nested file. Per-file mode can be re-enabled for a specific directory by adding an `[symlink.targets]` entry that reaches inside it or by listing a file inside it in `[symlink] protected_paths` — either triggers per-file mode for that directory (and only that directory).

2. The `home.<file>` Convention

    Most legacy dotfiles are, predictably, prefixed with a dot. The default rule routes pack-root files under the pack's XDG dir, but some files genuinely belong at `$HOME/.<name>` — either because the consuming tool hardcodes that path or because the user prefers the legacy location.

    For per-file opt-in to `$HOME/.<name>` placement, prefix the pack file with `home.`:

        <pack>/home.bashrc  →  $HOME/.bashrc
        <pack>/home.vimrc   →  $HOME/.vimrc

    Two reasons the prefix uses `home.` rather than literally `.`:

    1. Files starting with `.` are hidden by default in editors and `ls`, which makes pack contents harder to scan visually.
    2. The `home.` prefix reads as "deploy to home as .X" — explicit intent rather than a syntactic accident.

    The convention applies to top-level files only. Nested `home.X` filenames are treated literally (and end up at `$XDG_CONFIG_HOME/<pack>/<subdir>/home.X`).

    For per-subtree opt-in to $HOME (a whole directory of files routed there), see §5 (`_home/` directory prefix).

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
        "mysterious.conf" = "/var/etc/mysterious.conf"
        "home-bound.conf" = "my-documents/home-bound.conf"

    :: toml ::

    This will link `<pack>/mysterious.conf` to `/var/etc/mysterious.conf`. If the path is a relative path, it will be relative to your `XDG_CONFIG_HOME`. In the example above, `<pack>/home-bound.conf` will be linked to `$XDG_CONFIG_HOME/my-documents/home-bound.conf`.

5. Explicit `$HOME` or `XDG_CONFIG_HOME` via Directory Prefix

    For a whole subtree of files, the `_home/` and `_xdg/` directory prefixes route every file under them to a fixed root, **skipping the pack-name namespace**:

        <pack>/_home/aconf.ini   →  $HOME/.aconf.ini
        <pack>/_xdg/aconfig.ini  →  $XDG_CONFIG_HOME/aconfig.ini

    `_home/` is the per-subtree counterpart of the per-file `home.` convention (§2): use it when a group of files belongs at `$HOME/.X` rather than `$XDG_CONFIG_HOME/<pack>/X`.

    `_xdg/` is the escape hatch for when your pack name doesn't match the target program — e.g. a `term-config` pack containing configs for several terminals would put each at `term-config/_xdg/ghostty/config`, `term-config/_xdg/kitty/kitty.conf`, etc., and dodot deploys them straight to `$XDG_CONFIG_HOME/ghostty/config` and `$XDG_CONFIG_HOME/kitty/kitty.conf`. The pack name plays no role inside `_xdg/`.

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
            ".terraform"
        ]

    :: toml ::

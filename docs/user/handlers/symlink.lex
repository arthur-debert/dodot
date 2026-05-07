:: verified ::
The symlink handler

Creates a symlink from the version-controlled source in your dotfiles repo to the live location where it is used. This is the catch-all: anything no other handler claims flows through here.

1. Default deploy path

    Every pack-root entry — file or directory — defaults to `$XDG_CONFIG_HOME/<pack>/<name>`. The pack name namespaces config under XDG, matching how modern tools (nvim, helix, ghostty, kitty, alacritty, …) actually read their configuration.

    Examples:

        nvim/init.lua          ->  ~/.config/nvim/init.lua
        nvim/lua/              ->  ~/.config/nvim/lua/      (whole dir, one symlink)
        warp/themes/dark.yaml  ->  ~/.config/warp/themes/dark.yaml

    :: text ::

    A pack with an ordering prefix has the prefix stripped from the deployed name: `010-nvim/init.lua` deploys to `~/.config/nvim/init.lua`, not `~/.config/010-nvim/init.lua`.

2. Resolution priority

    When a file matches more than one routing rule, dodot resolves the deploy path in priority order (highest first):

    1. `[symlink.targets]` custom path
    2. File-level prefixes — top-level files only, skip pack namespace:
        - `home.X` → `$HOME/.X`
        - `app.X` → `<app_support_dir>/X`
        - `xdg.X` → `$XDG_CONFIG_HOME/X`
        - `lib.X` → `$HOME/Library/X` (macOS only)
    3. Directory prefixes — per-subtree, skip pack namespace:
        - `_home/<rest>` → `$HOME/.<rest>`
        - `_xdg/<rest>` → `$XDG_CONFIG_HOME/<rest>`
        - `_app/<rest>` → `<app_support_dir>/<rest>` (macOS: `~/Library/Application Support`; Linux: collapses to `$XDG_CONFIG_HOME`)
        - `_lib/<rest>` → `$HOME/Library/<rest>` (macOS only; non-macOS warns and skips)
    4. `force_home` — first path segment matches a list entry, routes to `$HOME/.<...>`
    5. `force_app` — first path segment matches a curated GUI-app folder name, routes to `<app_support_dir>/<...>`
    6. `app_aliases[<pack>]` — pack-level rewrite, routes the default rule to `<app_support_dir>/<alias>/<rel_path>`
    7. Default — `$XDG_CONFIG_HOME/<pack>/<rel_path>`

    The file and directory prefixes (priorities 2 and 3) skip pack namespacing — they route the literal path under the chosen root. Everything else, including `force_home`, `force_app`, and `app_aliases`, applies *after* pack-name resolution.

    For full path-resolution rules with edge cases, see [./../../reference/symlink-paths.lex].

3. Configuration

    Under `[symlink]` in `.dodot.toml`:

        [symlink]

        # Files/dirs whose first segment lands in $HOME instead of XDG.
        # Defaults cover canonical $HOME tools.
        force_home = [
            "ssh", "aws", "kube",
            "bashrc", "zshrc", "profile",
            "bash_profile", "bash_login", "bash_logout",
            "inputrc",
        ]

        # GUI-app folder names whose first segment routes to
        # <app_support_dir>/<name>/... without a `_app/` prefix.
        # Case-sensitive (Library folder names are case-sensitive on macOS).
        # Capped at 100 entries.
        force_app = ["Code", "Cursor", "Zed", "Emacs"]

        # Files dodot refuses to symlink — almost always a mistake to deploy.
        # Defaults cover SSH keys, GnuPG dir, AWS credentials, and so on.
        protected_paths = [
            ".ssh/id_rsa", ".ssh/id_ed25519", ".ssh/id_dsa",
            ".ssh/id_ecdsa", ".ssh/authorized_keys",
            ".gnupg",
            ".aws/credentials",
            ".password-store",
            ".config/gh/hosts.yml",
            ".kube/config",
            ".docker/config.json",
        ]

        # macOS only: when false, `_app/` and `app_aliases` collapse to
        # `~/.config/` instead of `~/Library/Application Support/`.
        # `_lib/` is unaffected — it explicitly targets `~/Library/`.
        app_uses_library = true

        # Pack-name -> GUI-app folder rewrite. Routes the default rule
        # for matching packs to `<app_support_dir>/<alias>/<rel_path>`.
        [symlink.app_aliases]
        # vscode = "Code"

        # Per-file custom targets. Absolute paths used as-is; relative
        # paths resolved from $XDG_CONFIG_HOME.
        [symlink.targets]
        # "mysterious.conf" = "/var/etc/mysterious.conf"
        # "home-bound.conf" = "my-documents/home-bound.conf"

    :: toml ::

4. Per-file mode for directories

    By default, a top-level directory is wholesale-linked — one symlink for the entire directory. dodot drops into per-file mode for that directory if any file inside it appears in `protected_paths` or as a key in `[symlink.targets]`. Per-file mode emits one symlink per file, each resolved independently — protected files are skipped.

5. Routing-conflict error

    Declaring a file in `[symlink.targets]` *and* giving it a filesystem-naming prefix (`home.X`, `_home/X`, `_app/X`, …) is rejected with `DodotError::RoutingOverrideConflict`. Two ways to say where one file goes is bug-bait — the user must pick one.

6. Live edits

    Once a source file is symlinked by `dodot up`, edits to the source are live at the deployed location immediately — the deployed path IS the source via the symlink chain. Whether the program reading the file picks up the edit depends on the program:

    - File-watching editors (nvim with `autoread`, vscode, …) reload immediately.
    - Shells re-read their rc on next launch.
    - Programs that only read configuration at start (systemd units, window managers, X resources, daemons) need an explicit reload command.
    - SSH re-reads its config on each new connection.

    Adding or removing a source file in the pack needs another `dodot up`. `up` reconciles per-pack state on every run: new sources get symlinks; removed sources have their stale symlinks cleaned up. You don't need a separate `dodot down` step to clear deletions.

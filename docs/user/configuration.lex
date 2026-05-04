Configuration

    dodot works out of the box with no configuration at all. The `.dodot.toml` file is there for when the defaults don't fit: when you want to rename a handler's default file, symlink to an unusual location, or disable a preprocessor for a single pack. This document is the reference for every configuration key.

    :: note :: See [./../reference/terms-and-concepts.lex] for terminology used throughout.

1. Where Configuration Lives

    Configuration is loaded from `.dodot.toml` files in two locations:

    - _Root config_: `$DOTFILES_ROOT/.dodot.toml`. Applies to every pack.
    - _Pack config_: `$DOTFILES_ROOT/<pack>/.dodot.toml`. Applies to that pack only.

    Both are optional. Every key has a compiled-in default; you only put into `.dodot.toml` the values you want to override. Pack configuration layers on top of root configuration, so you can set a sensible default at the root and override it per pack.

    Merge rules:

    - Scalars and arrays: override (the later-layer value replaces the earlier one, no accumulation).
    - Maps: deep-merge (nested keys combine across layers, but any scalar or array within still overrides).

    Generate a fully commented starter with `dodot config gen -o .dodot.toml`.

2. The `[pack]` Section

    Controls pack-level behavior.

    Pack ignore patterns:

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

    `ignore` is glob patterns that dodot skips during pack discovery and file scanning. Matching files are not considered for any handler. The defaults cover version-control noise, editor swapfiles, and a few directories that are notoriously never meant to be deployed.

    Note: to skip an _entire pack_, drop a `.dodotignore` marker file in that pack's directory (the "pack-ignore" mechanism). `[pack] ignore` is for patterns within a pack.

    2.1. `os`

        OS allowlist for the pack. When set, the whole pack is
        short-circuited at scan time on hosts whose OS isn't in the
        list — no preprocessing, no handlers, no symlinks. Inactive
        packs surface in `dodot status` under "Inactive on this OS"
        rather than disappearing silently.

        Pack-level OS gating:

            [pack]
            os = ["darwin"]

        :: toml ::

        Values match `dodot.os`; the alias `macos` resolves to
        `darwin`. Empty or absent means "all OSes" (today's default).
        See [./conditional-running.lex] §5.

        Only meaningful at pack level — root-level `[pack] os` is a
        configuration error (it would gate every pack against the
        current host, almost always unintended).

3. The `[symlink]` Section

    Controls how the symlink handler resolves targets. Full path-resolution rules live in [./../reference/symlink-paths.lex]; this section is the config knobs.

    By default, every pack-root entry deploys to `$XDG_CONFIG_HOME/<pack>/<name>` (so `nvim/init.lua` → `~/.config/nvim/init.lua`). Use `force_home`, the per-file `home.X` prefix, or the `_home/` directory prefix to opt files out of XDG when needed.

    3.1. `force_home`

        Files that must land in `$HOME/.<name>` regardless of the default XDG rule. These are decades-old conventions that precede XDG and are hardcoded by other tools (shell init, ssh interop, etc.).

        Force home:

            [symlink]
            force_home = [
                "ssh",            # .ssh/ - security critical
                "aws",            # .aws/ - credentials
                "kube",           # .kube/ - kubernetes config
                "bashrc",         # .bashrc - shell expects in $HOME
                "zshrc",          # .zshrc
                "profile",        # .profile
                "bash_profile",
                "bash_login",
                "bash_logout",
                "inputrc"         # readline config
            ]

        :: toml ::

        Override to add your own entries or remove ones you don't need.

    3.2. `protected_paths`

        Files dodot refuses to symlink by default, because doing so is almost always a mistake. Private SSH keys, GPG state, cloud credentials.

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

        Remove an entry to allow dodot to symlink it anyway.

    3.3. `targets`

        Per-file symlink target overrides. Maps a pack-relative filename to an absolute or relative target path. Absolute paths are used as-is; relative paths resolve against `$XDG_CONFIG_HOME`.

        Targets:

            [symlink.targets]
            "mysterious.conf" = "/var/etc/mysterious.conf"
            "home-bound.conf" = "my-documents/home-bound.conf"

        :: toml ::

4. The `[mappings]` Section

    Overrides the default filename-to-handler map. Each key is a handler name; each value is either a single pattern or a list of patterns.

    Mappings:

        [mappings]
        path = "bin"
        install = ["install.sh", "install.bash", "install.zsh"]
        shell = [
            "aliases.sh", "aliases.bash", "aliases.zsh",
            "profile.sh", "profile.bash", "profile.zsh",
            "login.sh",   "login.bash",   "login.zsh",
            "env.sh",     "env.bash",     "env.zsh",
        ]
        homebrew = "Brewfile"
        ignore = []
        skip = ["README", "README.*", "LICENSE", "LICENSE.*", "CHANGELOG", "CHANGELOG.*", "CONTRIBUTING", "CONTRIBUTING.*", "AUTHORS", "AUTHORS.*", "NOTICE", "NOTICE.*", "COPYING", "COPYING.*"]

    :: toml ::

    Shell extensions (`.sh`, `.bash`, `.zsh`) carry real meaning in dodot. For `install`, the extension selects the interpreter that runs the script: `.sh` and `.bash` run under `bash`, `.zsh` runs under `zsh`. For `shell`, the files are sourced into whatever shell reads `dodot-init.sh` — put zsh-only syntax in `.zsh`, bash-only syntax in `.bash`, and portable snippets in `.sh`. The user's login shell does not affect which `install.*` interpreter is picked; the extension is the contract.

    `install` is list-only: even a single install script must be written as a TOML array (`install = ["install.sh"]`). The older single-string form (`install = "install.sh"`) no longer parses — update any older configs that use it.

    Two of the keys map to _filter handlers_ — real handlers that claim a match but produce no executable intent. Their job is to keep matching files away from the deploying handlers (precise mappings, catchall symlink):

    - `ignore` — the `ignore` filter handler claims matches and drops them silently, mirroring `.gitignore`. Nothing surfaces in `dodot status`.
    - `skip` — the `skip` filter handler claims matches and surfaces them in `dodot status` as `skipped`, but does not deploy them. Defaults cover the documentation/legal files (`README`, `LICENSE`, `CHANGELOG`, `CONTRIBUTING`, `AUTHORS`, `NOTICE`, `COPYING` and their `.*` variants), matched case-insensitively. Override per-pack with `skip = []` to deploy a README intentionally.

    When both could match, `ignore` wins over `skip`; both win over precise mappings (`shell`, `install`, …) and the catchall symlink — so a file the user said to drop is dropped, full stop.

    Distinct from `[pack] ignore`: `[mappings] ignore`/`skip` apply only to handler dispatch within a known pack, while `[pack] ignore` affects pack discovery and scanning. To skip an entire pack, drop a `.dodotignore` marker file (the "pack-ignore" mechanism).

    4.1. `[mappings.gates]`

        Glob → gate-label map for repos that can't rename files. Each
        entry says "this glob inherits this gate"; on a non-matching
        host the file is dropped (same effect as a filename suffix).

        Glob-based gating:

            [mappings.gates]
            "install-mac.sh" = "darwin"
            "Brewfile"       = "darwin"

        :: toml ::

        Patterns match the top-level pack entries the scanner
        surfaces. A file carrying both a filename gate (`._<label>`)
        and a matching `[mappings.gates]` entry is a hard error — pick
        one source of truth. Invalid glob patterns are also a hard
        error at scan time. See [./conditional-running.lex] §7.

5. The `[gates]` Section

    User-defined gate labels. Each entry maps a label name to a table
    of `(dimension, value)` equality checks AND-ed together. Gates can
    then be referenced from filename suffixes (`install._<label>.sh`),
    directory segments (`_<label>/`), and the `[mappings.gates]` map.

    User-defined labels:

        [gates]
        laptop  = { hostname = "mbp-arthur" }
        work    = { hostname = "work-laptop" }
        arm-mac = { os = "darwin", arch = "aarch64" }

    :: toml ::

    Dimensions: `os`, `arch`, `hostname`, `username` — same set
    templates expose under `dodot.*`. Label names must match
    `[A-Za-z0-9_-]+` and must not collide with routing-prefix tokens
    (`home`/`xdg`/`app`/`lib`); both rules are hard errors at config
    load.

    User entries deep-merge over the built-in seed (`darwin`, `linux`,
    `macos`, `arm64`, `aarch64`, `x86_64`). For the full surface and
    composition rules, see [./conditional-running.lex].

6. The `[preprocessor]` Section

    Controls the preprocessing pipeline. For the concept, see [./../reference/pre-processors.lex].

    5.1. Global kill switch

        Global preprocessor toggle:

            [preprocessor]
            enabled = true

        :: toml ::

        Set to `false` to disable _all_ preprocessors. `.tmpl` files (and other preprocessor-matched files) will deploy verbatim.

    5.2. `[preprocessor.template]`

        Template engine configuration.

        Template configuration:

            [preprocessor.template]
            extensions = ["tmpl", "template"]

            [preprocessor.template.vars]
            editor = "nvim"
            host_tier = "workstation"

        :: toml ::

        `extensions` is the list of trigger extensions. Both `".j2"` and `"j2"` are tolerated (leading dot optional).

        `[preprocessor.template.vars]` defines variables available in templates under their bare names. See [./templates.lex] for usage.

7. Inheritance Model

    All sections follow the same three-layer model: compiled defaults, then root `.dodot.toml`, then pack `.dodot.toml`. The outermost layer that sets a key wins for scalars and arrays; for maps, the layers deep-merge.

    Example: you set `[preprocessor.template.vars] editor = "nvim"` at the root. In a pack for work configs, you set `[preprocessor.template.vars] editor = "vscode"`. That pack renders templates with `editor = "vscode"`; all others render with `editor = "nvim"`. All other keys under `[preprocessor.template]` (enabled, extensions) remain as defined at the root.

    To see the fully resolved configuration for a context, run `dodot config`. This shows exactly what dodot is using.

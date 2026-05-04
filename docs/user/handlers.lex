Handlers

    A handler is the thing that decides what to do with a file in your
    pack. Each handler has exactly one job: link configs, source shell
    scripts, add directories to `$PATH`, run install scripts once,
    install Brewfiles, or filter files out of dispatch entirely. dodot
    ships with eight handlers, and most users never need to think about
    them — the defaults match common dotfile conventions.

    This document is your reference for what each handler claims by
    default, what it does, and how to configure it. For the conceptual
    overview (matching model, execution order, why handlers look the
    way they do), see [./../reference/handlers.lex].

1. Defaults at a Glance

    These are the file patterns each handler claims by default.
    Anything not matched here flows to `symlink`.

    Default claims:
        | Handler  | Claims by default                          | What happens                                            |
        | ignore   | (empty by default)                          | Drop silently — same contract as `.gitignore`           |
        | skip     | `README`/`README.*`, `LICENSE`/`LICENSE.*`, etc. (case-insensitive) | List in `dodot status` as `skipped`; do not deploy |
        | gate     | (dynamic — see [./conditional-running.lex]) | Drop on host mismatch; surface in `status` as `gated out` |
        | homebrew | `Brewfile`                                  | `brew bundle` once per content hash                     |
        | install  | `install.sh`, `install.bash`, `install.zsh` | Script runs once per content hash                       |
        | path     | `bin/` (directory)                          | Directory prepended to `$PATH`                          |
        | shell    | `aliases.{sh,bash,zsh}`, `profile.{sh,bash,zsh}`, `login.{sh,bash,zsh}`, `env.{sh,bash,zsh}` | File sourced at shell login |
        | symlink  | Anything else (catchall)                    | File or directory linked to `~/.config/<pack>/` or `~/` |
    :: table align=lll ::

    Override any of these in `.dodot.toml` under `[mappings]`. Handler
    patterns are fully replaceable; you cannot, however, add a
    brand-new handler from config — the handler list itself is fixed.

    Default `[mappings]`:

        [mappings]
        path     = "bin"
        install  = ["install.sh", "install.bash", "install.zsh"]
        shell    = [
            "aliases.sh", "aliases.bash", "aliases.zsh",
            "profile.sh", "profile.bash", "profile.zsh",
            "login.sh",   "login.bash",   "login.zsh",
            "env.sh",     "env.bash",     "env.zsh",
        ]
        homebrew = "Brewfile"
        ignore   = []
        skip     = [
            "README", "README.*",
            "LICENSE", "LICENSE.*",
            "CHANGELOG", "CHANGELOG.*",
            "CONTRIBUTING", "CONTRIBUTING.*",
            "AUTHORS", "AUTHORS.*",
            "NOTICE", "NOTICE.*",
            "COPYING", "COPYING.*",
        ]

    :: toml ::

    Run `dodot config gen -o .dodot.toml` to write a fully-commented
    starter.

2. Execution Order

    Within a single pack, handlers run in this fixed order:

    1. _filter phase_ (`ignore` / `skip` / `gate`) — drop matched
       files before any deploying handler can claim them.
    2. _homebrew_ — install packages first, so anything later can
       depend on them.
    3. _install_ — run user setup scripts after `brew` is available.
    4. _path_ — stage `bin/` onto `$PATH` before shell init reads it.
    5. _shell_ — source shell startup files, which may reference
       binaries from `path`.
    6. _symlink_ — catchall, runs last so precise handlers claim their
       files first.

    Across packs, dodot processes packs in lexicographic order of
    their on-disk directory names. For the small handful of cases
    where pack ordering matters (Homebrew shellenv before anything
    that calls `brew`, `compinit` after completion plugins), name your
    directories with a numeric prefix: `010-brew`, `100-zsh`,
    `900-starship`. The prefix is invisible to user-facing surfaces —
    `010-nvim/init.lua` deploys to `~/.config/nvim/init.lua`, not
    `~/.config/010-nvim/`.

3. The Eight Handlers

    3.1. symlink

        Creates a symlink from a deployed location back to a file or
        directory in your pack. This is the default for any file that
        no other handler claims.

        Default deploy path: every pack-root entry — file or directory —
        defaults to `$XDG_CONFIG_HOME/<pack>/<name>`. So `nvim/init.lua`
        → `~/.config/nvim/init.lua`, `warp/themes/` →
        `~/.config/warp/themes/`. The pack name namespaces config under
        XDG, matching how modern tools (nvim, helix, ghostty, kitty,
        alacritty, …) actually read their configuration.

        Escape hatches for the cases where the XDG default is wrong:

            - `home.<file>` prefix on a top-level file → `$HOME/.<file>`.
              So `git/home.gitconfig` → `~/.gitconfig`. Top-level files
              only; nested `home.X` is treated literally.
            - `_home/<rest>` directory → `$HOME/.<rest>` (raw, no pack
              namespace). Useful when a pack groups files that all
              belong in `$HOME`.
            - `_xdg/<rest>` directory → `$XDG_CONFIG_HOME/<rest>` (raw,
              no pack namespace). Useful when a pack name doesn't match
              the target program (a `term-config` pack with
              `_xdg/ghostty/config`).
            - `[symlink] force_home` config list → routes
              legacy-shell-and-credential paths to `$HOME` regardless
              of XDG.
            - `[symlink.targets]` config map → fully custom paths.

        For the full path-resolution rules with examples, see
        [./../reference/symlink-paths.lex].

        Configurability under `[symlink]` in `.dodot.toml`:

            [symlink]

            # Files/dirs that must land in $HOME instead of $XDG_CONFIG_HOME.
            # Matched against the first path segment, leading dot ignored.
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
                "inputrc",        # readline config
            ]

            # Files dodot refuses to symlink — almost always a mistake to deploy these.
            # Override to remove an entry and allow it.
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
                ".docker/config.json",
            ]

            # Per-file custom symlink targets.
            # Absolute paths used as-is; relative paths resolved from $XDG_CONFIG_HOME.
            [symlink.targets]
            "mysterious.conf" = "/var/etc/mysterious.conf"
            "home-bound.conf" = "my-documents/home-bound.conf"

        :: toml ::

        Per-file mode for directories. By default, a top-level
        directory is wholesale-linked (one symlink for the whole
        directory). dodot drops into per-file mode for that directory
        if any file inside it appears in `protected_paths` or as a key
        in `[symlink.targets]`. Per-file mode emits one symlink per
        file, each resolved independently — protected files are
        skipped.

    3.2. shell

        Sources shell scripts at login. Matched files are staged into
        the datastore; the generated `dodot-init.sh` (which you load
        with `eval "$(dodot init-sh)"`) emits a `source` line for each.

        Default claims: `aliases.{sh,bash,zsh}`, `profile.{sh,bash,zsh}`,
        `login.{sh,bash,zsh}`, `env.{sh,bash,zsh}`.

        Extensions are load-bearing. Sourced files run *in your
        interactive shell*, so `.zsh` files only parse cleanly in zsh
        sessions and `.bash` files in bash sessions. `.sh` is the
        portable bucket — use it for snippets that work in either.
        Most users only run one shell and never hit the mismatch; if
        you switch shells, split your config by extension.

        Configurability:

            [mappings]
            shell = ["aliases.sh", "myextras.zsh", "work.bash"]

        :: toml ::

        The shell handler has no `[shell]` section of its own — the
        mapping list IS the configuration.

    3.3. path

        Adds a directory to `$PATH`. The conventional match is a `bin/`
        directory inside a pack; its contents become directly
        executable from any shell. Like `shell`, this rides on
        `dodot-init.sh` — the datastore records which directories
        should be on PATH, and the init script prepends them.

        Default claim: `bin/` (directory).

        Configurability:

            [mappings]
            path = "scripts"   # rename the matched dir; trailing slash auto-added

            [path]
            # Auto-chmod +x on files inside path-handler directories. On by default.
            # Useful because git on macOS defaults to core.fileMode=false, so cloned
            # scripts may not have the execute bit. With this on, dodot ensures every
            # file in a path-handler directory is executable on `dodot up`.
            # Failures report as warnings, not hard errors.
            # Set to false if you have non-executable files in `bin/` (data files,
            # library scripts sourced by other scripts).
            auto_chmod_exec = true

        :: toml ::

    3.4. install

        Runs an arbitrary shell script once, tracked by a sentinel
        keyed on the script's content hash. Use this for
        machine-specific setup that the other handlers don't cover:
        language toolchains, window manager configuration, system
        defaults.

        Default claims: `install.sh`, `install.bash`, `install.zsh`.

        Interpreter is picked by extension, not by your login shell:

            - `.sh`, `.bash`, or unknown extension → run with `bash`
            - `.zsh` → run with `zsh`

        The script runs in a fresh subprocess, so your interactive
        shell state (aliases, functions, options) is invisible to it
        regardless. The extension is the contract the pack author
        declares: `install.zsh` announces zsh-specific syntax;
        `install.sh` announces portability.

        Sentinels. When an install script runs successfully, dodot
        writes `<filename>-<checksum>` (e.g.
        `install.sh-a1b2c3d4e5f6a7b8`) into the datastore. On
        subsequent `dodot up`, the script is skipped if its sentinel
        exists. Edit the script and the checksum changes, the sentinel
        name changes, and the script re-runs automatically. Override:

            - `--no-provision` skips install and homebrew entirely for
              this run.
            - `--provision-rerun` forces them to re-run even when
              sentinels exist. Use after changing an install script
              when the change is too subtle to alter the content
              (rare), or to rerun without an input change.

        Multiple matches. A pack with both `install.sh` and
        `install.zsh` runs *all of them*, each tracked by its own
        sentinel. There is no "pick the best one" logic — if you want
        only one to run, ship only one.

        Output. By default `dodot up` keeps install-script output
        quiet — only start/end markers and a couple of conventions are
        surfaced:

            - Header block. When a script starts, the leading comment
              block (the contiguous `#`-prefixed lines after the
              optional shebang) is printed so you see what's about to
              run. Document your script the way you'd want a teammate
              to read it.
            - `# status:` markers. Lines on stdout matching
              `# status: <message>` (or `#status: <message>`) are
              printed as live progress while the script runs. Sprinkle
              these at phase boundaries so a long-running script
              doesn't look hung.

        Status-marker example:

            #!/bin/bash
            # Install nvm
            # Requires curl

            # status: downloading installer
            curl -sL https://example.com/install.sh -o /tmp/inst
            # status: running installer
            bash /tmp/inst

        :: shell ::

        The convention is tool-agnostic: the `# status:` lines are
        just shell comments when the script is run by hand outside
        dodot.

            - `--verbose`. Pass `--verbose` (or `--debug`) to `dodot up`
              to also stream the script's raw stdout/stderr in real
              time — useful when debugging a misbehaving install. On
              failure, captured stderr is dumped automatically even
              without `--verbose`.

        Configurability:

            [mappings]
            install = ["setup.sh", "bootstrap.zsh"]

        :: toml ::

        `install` is list-only — even a single script must be written
        as `install = ["install.sh"]`. The single-string form does not
        parse.

        The install handler has no dedicated `[install]` section.

    3.5. homebrew

        Runs `brew bundle` against a `Brewfile`, once per content-hash.
        macOS-only in practice. Functionally a specialization of
        `install` with a more ergonomic default for its common case.

        Default claim: `Brewfile`.

        Sentinel behavior is identical to `install`: edit the Brewfile,
        the checksum changes, `brew bundle` runs again. `--no-provision`
        and `--provision-rerun` apply here too.

        Configurability:

            [mappings]
            homebrew = "MyBrewfile"

        :: toml ::

        Single-string only, unlike `install`. The homebrew handler has
        no dedicated section.

4. Configuration vs Code Execution

    Handlers fall into two categories:

        - Configuration handlers — `symlink`, `shell`, `path`.
          Idempotent filesystem work. `dodot up` always runs them in
          full and wipes per-pack state before re-applying, so a
          deleted source file doesn't leave an orphan link behind.
        - Code execution handlers — `install`, `homebrew`. Run
          user-authored shell commands that may not be idempotent.
          Tracked by sentinels, skipped on subsequent runs unless the
          input content changes. Use `--no-provision` to skip them
          entirely or `--provision-rerun` to force re-execution.

    This split is why `--no-provision` exists. On a daily basis you
    want fast `dodot up` runs that re-link configuration without
    re-running multi-second `brew bundle` calls; on a fresh machine
    you want everything to run.

5. Keeping Files Out of Handler Dispatch

    Three handlers in the filter phase exist solely to keep files away
    from the deploying handlers. They differ in *visibility* and in
    *why* the file was kept out.

    5.1. ignore (filter)

        Claims matches and drops them silently — same contract as
        `.gitignore`. No entry in `dodot status`, no executable intent.
        Configured via `[mappings] ignore` (default empty). Useful for
        build artifacts, scratch files, anything you don't want dodot
        to know about.

        Drop silently:

            [mappings]
            ignore = ["*.bak", "scratch.txt"]

        :: toml ::

    5.2. skip (filter)

        Claims matches, surfaces them in `dodot status` as `skipped`,
        but produces no executable intent — `dodot up` will not deploy
        them. Configured via `[mappings] skip`. The defaults cover the
        common documentation and legal files that packs ship alongside
        real config (`README`, `LICENSE`, `CHANGELOG`, `CONTRIBUTING`,
        `AUTHORS`, `NOTICE`, `COPYING`, plus their `.*` variants),
        matched case-insensitively against the basename. Override
        per-pack with `skip = []` to deploy a `README` intentionally,
        or replace the list to use your own conventions.

        Pack-local override:

            # deploy our README, but skip TODO.md
            [mappings]
            skip = ["TODO.md"]

        :: toml ::

    5.3. gate (filter)

        Claims matches whose host predicate evaluates false on this
        host — e.g. `install._darwin.sh` on a linux box. Surfaces in
        `dodot status` as `gated out (<label>)` with a footnote
        showing the expected vs actual host facts. The gate handler
        produces no executable intent; the file is preserved on disk
        and will deploy on a matching host.

        Unlike `ignore` and `skip`, gate matches are *dynamic* — they
        depend on host facts (OS, arch, hostname, …) and on the
        filename grammar (`._<label>`, `_<label>/`) plus the
        `[mappings.gates]` config. The full surface is documented in
        [./conditional-running.lex]; the short version is:

        Gate examples:

            install._darwin.sh                # filename suffix
            _darwin/foo.sh                    # directory segment
            [pack] os = ["darwin"]            # whole-pack (pack .dodot.toml)
            [mappings.gates]                  # glob escape hatch
              "install-mac.sh" = "darwin"

        :: text ::

    5.4. Choosing between them

        Choosing a filter handler:
            | You want…                                        | Use                                |
            | The file invisible to dodot                      | `[mappings] ignore` (or `[pack] ignore`) |
            | The file visible in `dodot status`, undeployed   | `[mappings] skip`                  |
            | The whole pack ignored                           | `.dodotignore` marker file         |
            | The file invisible during pack scanning          | `[pack] ignore`                    |
            | The file deployed only on certain hosts          | gate (see [./conditional-running.lex]) |
        :: table align=ll ::

        `[pack] ignore` is the broadest hammer — its glob patterns
        are excluded from pack discovery and file scanning entirely,
        so matched files never become candidates for any handler. The
        defaults (`.git`, `node_modules`, `.DS_Store`, `*.swp`, …)
        cover version-control noise and editor swapfiles.
        `[mappings] ignore`, `[mappings] skip`, and gate operate one
        layer down: the file is discovered, but a filter handler
        claims it before any deploying handler sees it.

        When multiple filters could match, `ignore` wins over `skip`
        (silent-drop is the stronger signal); both win over precise
        mappings (`shell`, `install`, …) and the catchall symlink. A
        gated-out file behaves like `skip` — visible in status, not
        deployed — but specifically because the host doesn't match,
        not because the user marked it for documentation skipping.

        To skip an *entire pack*, drop a `.dodotignore` marker file in
        that pack's directory.

    See [./configuration.lex] for the full set of `.dodot.toml` keys.

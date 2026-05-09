Configuration

    One of dodot's principles is to minimize both adoption and migration
    cost (off dodot into the next dotfiles tool). One way it does that is
    by using the dotfiles themselves as the description of what to do:
    files grouped into packs, and filenames the scanner can infer behavior
    from. dodot is convention-based — given a name, it knows what to do
    with the file. Adding `bin/` directories to `$PATH`, running `brew`
    against `Brewfile`, sourcing `aliases.sh` at shell startup — all of
    it falls out of names you would pick anyway.

    The convention is rooted in common usage patterns, so for most people
    the default layout is the most natural approach, and is useful in its
    own right beyond dodot.

    When the convention doesn't fit, you can customize. For path-related
    behavior, most knobs are reachable two ways: a `.dodot.toml` entry,
    or a filename / directory naming convention like `_home/` or `home.X`.
    The two surfaces serve two preferences — a cleaner file tree with the
    choices captured in `.dodot.toml`, or an explicit setup where the
    names themselves carry the intent and no extra config file is needed.

    Scope is the other axis. Filesystem naming acts per file.
    `.dodot.toml` acts per pack, or repo-wide from the root config — so
    when many files need the same adjustment a config entry is usually
    the cleanest path.

    dodot ships a small set of CLI helpers for working with config:

        $ dodot config list                  # show resolved values
        $ dodot config get <key>             # show one key with its inline doc
        $ dodot config set <key> <value>
        $ dodot config unset <key>
        $ dodot config gen > .dodot.toml     # write a commented starter file

    :: shell ::

1. Where Configuration Lives

    Configuration is loaded from `.dodot.toml` files in two locations:

    - _Root config_: `$DOTFILES_ROOT/.dodot.toml`. Applies to every pack.
    - _Pack config_: `$DOTFILES_ROOT/<pack>/.dodot.toml`. Applies to that pack only.

    Both are optional. Every key has a compiled-in default; you only put into `.dodot.toml` the values you want to override. Pack configuration layers on top of root configuration, so you can set a sensible default at the root and override it per pack.

    Merge rules:

    - Scalars and arrays: override (the later-layer value replaces the earlier one, no accumulation).
    - Maps: deep-merge (nested keys combine across layers, but any scalar or array within still overrides).

    Some sections are _root-only_ — they're read from the root
    `.dodot.toml` and per-pack overrides are ignored. `[secret]` and
    `[profiling]` fall in this bucket; `[pack] os` is the mirror image
    (pack-only — root-level entries are rejected).

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

        The canonical OS value for macOS is `"darwin"`; `"macos"` is
        accepted as an alias that resolves to `"darwin"`. On Linux the
        canonical value is `"linux"`. Empty or absent means "all OSes"
        (today's default). Note: template variables expose `dodot.os`
        as `"macos"` on macOS — gate OS values and template OS values
        are not the same surface. See [./conditional-running.lex] §5.

        Only meaningful at pack level — root-level `[pack] os` is a
        configuration error (it would gate every pack against the
        current host, almost always unintended).

3. The `[symlink]` Section

    Controls how the symlink handler resolves targets. Full path-resolution rules live in [./../reference/symlink-paths.lex]; this section is the config knobs.

    By default, every pack-root entry deploys to `$XDG_CONFIG_HOME/<pack>/<name>` (so `nvim/init.lua` → `~/.config/nvim/init.lua`). The escape hatches come in pairs — a config list and a per-file naming convention:

    - `force_home` / `home.X` filename / `_home/` directory → `$HOME/.<name>`
    - `force_app` / `app.X` filename / `_app/` directory → `~/Library/Application Support/<name>` (macOS; falls back to XDG on Linux or when `app_uses_library = false`)
    - `xdg.X` filename / `_xdg/` directory → escape from a routing override back to plain XDG
    - `lib.X` filename / `_lib/` directory → `~/Library/<name>` (macOS only)

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

        Override to add your own entries or remove ones you don't need. Equivalent per-file: prefix the filename with `home.` or place it under `_home/`.

    3.2. `force_app`

        macOS: GUI-app folder names that route a pack-root entry to
        `~/Library/Application Support/<name>/<rest>` automatically,
        without needing the `_app/` directory prefix. Matching is
        case-sensitive (Library folder names are case-sensitive on
        macOS) and only against the first path segment.

        Force app:

            [symlink]
            force_app = ["Code", "Cursor", "Zed", "Emacs"]

        :: toml ::

        On Linux (or on macOS with `app_uses_library = false`),
        `app_support_dir` collapses to `$XDG_CONFIG_HOME` — but
        `force_app` is still applied. So `force_app = ["Code"]` on
        Linux routes `Code/init.json` to
        `$XDG_CONFIG_HOME/Code/init.json` (skipping the per-pack
        namespace), not to the macOS Application Support root. If
        you don't want that behavior on Linux, drop the entry from
        the list there (or override per-pack).

        The shipped defaults stay under a 100-entry budget — that's
        a build-time invariant on `force_app`'s seed, not a user
        validation. You can set a longer list yourself; it just
        won't be a sane thing to do.

        Equivalent per-file: prefix with `app.` or place under
        `_app/`.

    3.3. `app_aliases`

        macOS: pack-name → app-folder rewrites. When a pack's name
        matches a key, every entry in that pack reroutes from the
        default `<xdg_config_home>/<pack>/<rest>` to
        `<app_support_dir>/<value>/<rest>`. Useful when your pack
        directory and the app's Library folder have different names.

        App aliases:

            [symlink.app_aliases]
            "vscode"        = "Code"
            "cursor-editor" = "Cursor"

        :: toml ::

        Defaults empty. Same Linux fallback as `force_app`.

    3.4. `app_uses_library`

        macOS toggle. When `true` (the default), `_app/` directories,
        `app.X` filenames, `force_app`, and `app_aliases` route
        through `~/Library/Application Support`. When `false`, they
        all collapse to plain `~/.config` (Linux-style placement).
        `_lib/` and `lib.X` are unaffected — those explicitly target
        `~/Library/`.

        App-uses-library toggle:

            [symlink]
            app_uses_library = false   # opt out of Application Support routing on this Mac

        :: toml ::

        Ignored on non-macOS hosts (the resolver collapses
        `app_support_dir` to `xdg_config_home` everywhere except
        macOS).

    3.5. `protected_paths`

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

    3.6. `targets`

        Per-file symlink target overrides. Maps a pack-relative filename to an absolute or relative target path. Absolute paths are used as-is; relative paths resolve against `$XDG_CONFIG_HOME`.

        Targets:

            [symlink.targets]
            "mysterious.conf" = "/var/etc/mysterious.conf"
            "home-bound.conf" = "my-documents/home-bound.conf"

        :: toml ::

    3.7. `plist_extensions`

        Filename suffixes (without leading dot) that `dodot
        git-install-filters` and the `.gitattributes` writer treat
        as plists. Some apps store plists with non-standard suffixes
        (`.binplist`, `.savedState`); register the extra extensions
        here to flow them through the same clean/smudge pipeline.

        Plist extensions:

            [symlink]
            plist_extensions = ["plist", "binplist"]

        :: toml ::

        Default `["plist"]`. Comparison is case-insensitive, and the list honors the standard root → pack inheritance.

4. The `[path]` Section

    Settings for the PATH handler (the one that adds `bin/` directories to `$PATH`).

    4.1. `auto_chmod_exec`

        Whether `dodot up` automatically adds the execute bit (`+x`)
        to files inside `bin/` directories. Default `true`.

        Auto-chmod:

            [path]
            auto_chmod_exec = true

        :: toml ::

        Files in a `bin/` directory are there because they're meant
        to be commands on `$PATH`, but the execute bit gets lost
        easily — git on macOS defaults to `core.fileMode = false`,
        and manual file creation often forgets `chmod +x`. Without
        `+x` the shell finds the file via PATH lookup but fails
        with "permission denied", a confusing error when the file
        is clearly in the right place.

        Files that are already executable are left untouched;
        failures are reported as warnings rather than hard errors.
        Set to `false` if you have `bin/` files that intentionally
        should not be executable (data files, sourced library
        scripts).

5. The `[mappings]` Section

    Overrides the default filename-to-handler map. Each key is a handler name; each value is either a single pattern or a list of patterns.

    Mappings:

        [mappings]
        path = "bin"
        install = ["install.sh", "install.bash", "install.zsh"]
        shell = ["*.sh", "*.bash", "*.zsh"]
        homebrew = "Brewfile"
        ignore = []
        skip = ["README", "README.*", "LICENSE", "LICENSE.*", "CHANGELOG", "CHANGELOG.*", "CONTRIBUTING", "CONTRIBUTING.*", "AUTHORS", "AUTHORS.*", "NOTICE", "NOTICE.*", "COPYING", "COPYING.*"]

    :: toml ::

    The shell wildcards match at depth-1 only — any `.sh`/`.bash`/`.zsh` file at the *pack's root* is sourced. A `.sh` script tucked inside a subdirectory of the pack (for example `hypr/scripts/foo.sh`) is not pulled in; nested files flow through the symlink handler the same way every other nested file does. That carve-out is what keeps window-manager and tmux helper scripts (which live at `~/.config/<app>/scripts/*.sh` and are invoked by other tools, not the shell) from being silently sourced into your login shell.

    Shell extensions (`.sh`, `.bash`, `.zsh`) carry real meaning in dodot. For `install`, the extension selects the interpreter that runs the script: `.sh` and `.bash` run under `bash`, `.zsh` runs under `zsh`. For `shell`, the files are sourced into whatever shell reads `dodot-init.sh` — put zsh-only syntax in `.zsh`, bash-only syntax in `.bash`, and portable snippets in `.sh`. The user's login shell does not affect which `install.*` interpreter is picked; the extension is the contract.

    `install` is list-only: even a single install script must be written as a TOML array (`install = ["install.sh"]`). The older single-string form (`install = "install.sh"`) no longer parses — update any older configs that use it.

    Two of the keys map to _filter handlers_ — real handlers that claim a match but produce no executable intent. Their job is to keep matching files away from the deploying handlers (precise mappings, catchall symlink):

    - `ignore` — claims matches and drops them silently, mirroring `.gitignore`. Nothing surfaces in `dodot status`. Priority 100.
    - `skip` — claims matches and surfaces them in `dodot status` as `skipped`, but does not deploy them. Defaults cover the documentation/legal files (`README`, `LICENSE`, `CHANGELOG`, `CONTRIBUTING`, `AUTHORS`, `NOTICE`, `COPYING` and their `.*` variants), matched case-insensitively. Override per-pack with `skip = []` to deploy a README intentionally. Priority 50.

    `install` sits at priority 20, above the priority-10 shell wildcard, so an `install.sh` filename always routes to the install handler — the install hook never gets accidentally sourced. The other precise mappings (`shell`, `path`, `homebrew`) sit at priority 10; the catchall symlink at priority 0. So a file the user said to drop is dropped, full stop — `ignore` over `skip` over `install` over the rest of the precise mappings over catchall.

    Distinct from `[pack] ignore`: `[mappings] ignore`/`skip` apply only to handler dispatch within a known pack, while `[pack] ignore` affects pack discovery and scanning. To skip an entire pack, drop a `.dodotignore` marker file (the "pack-ignore" mechanism).

    5.1. `[mappings.gates]`

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

6. The `[gates]` Section

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

    User entries extend the built-in seed (`darwin`, `linux`,
    `macos`, `arm64`, `aarch64`, `x86_64`). Redefining an existing
    built-in label (e.g. `darwin = { … }` in `[gates]`) replaces its
    predicate entirely — it does not merge dimensions into the
    built-in. Shadowing a built-in is allowed but unusual; most user
    labels use names the seed doesn't claim. For the full surface and
    composition rules, see [./conditional-running.lex].

7. The `[preprocessor]` Section

    Controls the preprocessing pipeline. For the concept, see [./../reference/pre-processors.lex].

    7.1. Global kill switch

        Global preprocessor toggle:

            [preprocessor]
            enabled = true

        :: toml ::

        Set to `false` to disable _all_ preprocessors. `.tmpl` files (and other preprocessor-matched files) will deploy verbatim.

    7.2. `[preprocessor.template]`

        Template engine configuration.

        Template configuration:

            [preprocessor.template]
            extensions = ["tmpl", "template"]
            no_reverse = ["complex-config.toml.tmpl", "*.gen.tmpl"]

            [preprocessor.template.vars]
            editor    = "nvim"
            host_tier = "workstation"

        :: toml ::

        `extensions` is the list of trigger extensions. Both `".j2"` and `"j2"` are tolerated (leading dot optional).

        `[preprocessor.template.vars]` defines variables available in templates under their bare names. See [./templates.lex] for usage. Reserved: `dodot` and `env` are built-in namespaces, so using them as var names is a hard error at load time.

        `no_reverse` is glob patterns (matched against the source file's basename) whose reverse-merge in `dodot transform check` is bypassed. Templates listed here still render normally on `dodot up` and stay in the divergence cache; they just skip the heuristic that tries to backport changes from the deployed copy into the source. Useful for templates that are mostly dynamic — the heuristic degrades there and produces more conflict markers than usable diffs.

    7.3. `[preprocessor.age]`

        Opt-in `*.age` whole-file decryption. Off by default so a fresh dodot install never shells out to `age` against random files.

        Age decryption:

            [preprocessor.age]
            enabled    = true
            extensions = ["age"]
            identity   = ""    # empty: defer to $AGE_IDENTITY, then ~/.config/age/identity.txt

        :: toml ::

        `extensions` matches the same shape as `template.extensions`; add entries when your repo uses non-standard suffixes (e.g. `["age", "age.txt"]`). `identity` is the path to the age identity file used for decryption; leaving it empty defers to the runtime, which checks `$AGE_IDENTITY` first, then falls back to `~/.config/age/identity.txt` (the conventional `age-keygen` destination).

    7.4. `[preprocessor.gpg]`

        Opt-in `*.gpg` whole-file decryption. Same posture as `age`. gpg-agent supplies the identity, so there's no `identity` field — auth is your existing gpg setup, not dodot's job to configure.

        Gpg decryption:

            [preprocessor.gpg]
            enabled    = true
            extensions = ["gpg"]

        :: toml ::

        :: warning :: Do not add `asc` to `extensions` unless your repo only stores ASCII-armored _encrypted_ payloads under that suffix. `.asc` is conventionally used for armored public keys and detached signatures (release signatures, package-manager keys), neither of which gpg will decrypt; routing them through `gpg --decrypt` produces confusing failures.

8. The `[profiling]` Section

    _Root-only_. Controls shell-init timing instrumentation. Per-pack overrides are ignored — the init script is one thing, you can't half-profile it.

    Profiling configuration:

        [profiling]
        enabled        = true
        keep_last_runs = 100

    :: toml ::

    When `enabled = true` (the default), the generated `dodot-init.sh` carries a timing wrapper around each `source` and PATH line. bash 5+ / zsh sessions emit one TSV per shell startup under `<data_dir>/probes/shell-init/`; older shells fall through to the no-op path even with the wrapper present. Set `enabled = false` to make the init script byte-identical to the pre-Phase-2 form.

    `keep_last_runs` caps retained TSV files; older ones get pruned at the end of every `dodot up`. At ~4 KB per run, the default budget is roughly 400 KB on disk.

9. The `[secret]` Section

    _Root-only_. Configuration for secret resolution in templates (the `secret(...)` template function). Per-pack overrides are ignored — secret tooling (binaries, env vars like `$PASSWORD_STORE_DIR`, `OP_SERVICE_ACCOUNT_TOKEN`) is a property of the host, not of any individual pack.

    All providers default to `enabled = false`. Templates that call `secret(...)` against an unregistered scheme surface a "no provider for scheme" render error at `dodot up` time.

    9.1. Global kill switch

        Master switch:

            [secret]
            enabled = true

        :: toml ::

        Default `true`. Setting `enabled = false` disables the entire secret subsystem without removing per-provider blocks.

    9.2. `[secret.providers]`

        One block per supported provider. Each is opt-in; flip `enabled = true` to register the scheme.

        Providers:

            [secret.providers.pass]
            enabled   = true
            store_dir = ""    # optional: overrides $PASSWORD_STORE_DIR

            [secret.providers.op]
            enabled = true    # 1Password CLI, scheme `op://`

            [secret.providers.bw]
            enabled = false   # Bitwarden CLI, scheme `bw:`

            [secret.providers.sops]
            enabled = false   # Mozilla SOPS, scheme `sops:`

            [secret.providers.keychain]
            enabled = false   # macOS Keychain, scheme `keychain:` (macOS only)

            [secret.providers.secret_tool]
            enabled = false   # freedesktop Secret Service, scheme `secret-tool:` (Linux-first)

        :: toml ::

        :: note :: The TOML key is `secret_tool` (underscore), but the scheme prefix in `secret(...)` calls is `secret-tool:` (hyphen, matching the binary name). Error messages translate between the two so a "no provider for scheme `secret-tool`" hint points at the right `[secret.providers.secret_tool]` block.

        `pass.store_dir` overrides `$PASSWORD_STORE_DIR` for the `pass` provider. Empty (the default) leaves dodot reading the env var, which itself falls back to `$HOME/.password-store`.

        For details on schemes, providers, and the `secret(...)` template function, see [./secrets.lex].

10. Inheritance Model

    Most sections follow the same three-layer model: compiled defaults, then root `.dodot.toml`, then pack `.dodot.toml`. The outermost layer that sets a key wins for scalars and arrays; for maps, the layers deep-merge. The exceptions: `[secret]` and `[profiling]` are root-only (per-pack entries are ignored), and `[pack] os` is pack-only (root-level entries are rejected).

    Example: you set `[preprocessor.template.vars] editor = "nvim"` at the root. In a pack for work configs, you set `[preprocessor.template.vars] editor = "vscode"`. That pack renders templates with `editor = "vscode"`; all others render with `editor = "nvim"`. All other keys under `[preprocessor.template]` (enabled, extensions) remain as defined at the root.

    To see the fully resolved configuration for a context, run `dodot config list`. To inspect a single key with its inline doc, run `dodot config get <key>`.

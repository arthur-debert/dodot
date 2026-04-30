Symlink Deployment Paths

    Symlinking is the core of a dotfile manager, and dodot ships with smart defaults plus overrides for every case where the defaults are wrong. This document is the full reference for where files end up on deploy.

    Dodot makes the extra effort to be simple and predictable, but path handling is anything but, and in the service of being useful, there is some magic behaviour around paths. This document goes over them. 

0. The Scenario

    After decades of dotfiles crowding `$HOME`, the XDG spec tackled the issue — and it has succeeded. Adoption took years and the ecosystem still has rough edges, so public perception of XDG is more skeptical than the actual state warrants. Calling that out matters because it sets the tone for this document: paths in dodot are XDG paths by default.

    There are two exceptions to that rule:

        1. The holdouts: a handful of Unix files (`.ssh`, `.zshrc`, `.gnupg`, …) have decades of tooling that expects them in `$HOME`. Breaking that would cascade into the rest of the ecosystem. There are about ten of these in total; dodot handles them via the `force_home` list (§3).
        2. macOS GUI apps: a schism exists between modern CLI tools (which use `~/.config`) and GUI apps (which use `~/Library/Application Support`). If you're not on Darwin or don't use dodot to manage GUI app config, this is immaterial — and dodot exposes the macOS-side routing via `_app/`, `_lib/`, `force_app`, and `app_aliases` (§6).

    In a nutshell:
        dodot uses XDG by default, except for Unix canons. Where you need to break convention, the mechanism is explicit:
        - config: resolve to `$HOME` (Unix canons)
        - file/dir names:
            - prefix files with `home.` (e.g. `home.some-config` → `~/.some-config`)
            - place links under a `_home/` or `_xdg/` directory for whole-subtree routing.





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

    3.1. Force App for GUI Application Folders

        On macOS, GUI app config lives at `~/Library/Application Support/<App>/`, a third filesystem coordinate alongside `$HOME` and `$XDG_CONFIG_HOME`. Dodot ships a curated companion to `force_home` — `force_app` — listing common GUI-app folder names whose first path segment routes to `<app_support_dir>/<name>/<rest>` without requiring a `_app/` prefix in the pack tree.

        Force app:

            [symlink]
            force_app = [
                "Code",       # VS Code
                "Cursor",     # Cursor (AI fork of VS Code)
                "Zed",        # Zed editor
                "Emacs"       # Emacs.app
            ]

        :: toml ::

        Matching is case-sensitive and on the first path segment only — Library folder names are case-sensitive on macOS, and `Code` (VS Code) must not collide with a hypothetical `code` CLI tool's `~/.config/code/` directory.

        On Linux (or on macOS with `app_uses_library = false`, see §6) the `app_support_dir` collapses onto `$XDG_CONFIG_HOME` and `force_app` routes through XDG instead — same mechanism, same rules, different destination.

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

        <pack>/_home/a-conf.ini   →  $HOME/.a-conf.ini
        <pack>/_xdg/a-config.ini  →  $XDG_CONFIG_HOME/a-config.ini

    `_home/` is the per-subtree counterpart of the per-file `home.` convention (§2): use it when a group of files belongs at `$HOME/.X` rather than `$XDG_CONFIG_HOME/<pack>/X`.

    `_xdg/` is the escape hatch for when your pack name doesn't match the target program — e.g. a `term-config` pack containing configs for several terminals would put each at `term-config/_xdg/ghostty/config`, `term-config/_xdg/kitty/kitty.conf`, etc., and dodot deploys them straight to `$XDG_CONFIG_HOME/ghostty/config` and `$XDG_CONFIG_HOME/kitty/kitty.conf`. The pack name plays no role inside `_xdg/`.

6. macOS: `_app/`, `_lib/`, and Application Support

    On macOS, GUI applications read configuration from `~/Library/Application Support/<App>/` — a third filesystem coordinate alongside `$HOME` and `$XDG_CONFIG_HOME`. Dodot models this as `app_support_dir` and exposes two new directory prefixes plus a pack-level alias mechanism so the same pack tree can deploy correctly on both Linux and macOS without `if os == "darwin"` branching inside packs.

    Roots:
        | Symbol             | macOS                                 | Linux / other                |
        | `$HOME`            | `/Users/<user>`                       | `/home/<user>`               |
        | `$XDG_CONFIG_HOME` | `~/.config` (unless env-set)          | `~/.config` (unless env-set) |
        | `app_support_dir`  | `~/Library/Application Support`       | `$XDG_CONFIG_HOME`           |
    :: table align=lll ::

    On Linux the second and third coordinates collapse to one location, so `_app/` and `app_aliases` route through `~/.config` — indistinguishable from `_xdg/` in effect, but the same pack tree still works. On macOS the third coordinate diverges and the routing kicks in.

    6.1. The `_app/` Directory Prefix

        `_app/` is the per-subtree opt-in for "this is GUI-application config". Like `_xdg/` and `_home/`, it skips pack namespacing entirely:

            <pack>/_app/<name>/<rest>  →  <app_support_dir>/<name>/<rest>

        A portable `vscode` pack laid out as:

            vscode/
                _app/
                    Code/
                        User/
                            settings.json
                            keybindings.json

        :: text ::

        deploys to:

            - Linux:  `~/.config/Code/User/settings.json`
            - macOS:  `~/Library/Application Support/Code/User/settings.json`

        :: text ::

        The pack literally states "this is GUI-app config under name `Code`". Dodot picks the root per platform.

    6.2. The `_lib/` Directory Prefix (macOS Only)

        `_lib/` is the macOS-only counterpart to `_app/`. Where `_app/` cross-routes between platforms, `_lib/` declares a hard macOS-only target — appropriate for apps with no Linux equivalent:

            <pack>/_lib/<rest>  →  $HOME/Library/<rest>          # macOS only

        :: text ::

        Note that `_lib/` maps to `~/Library/`, *not* to `~/Library/Application Support/`. This gives access to other Library subtrees (`LaunchAgents/`, `Fonts/`, `Services/`) without further prefix proliferation. The user writes the full subpath:

            <pack>/_lib/Application Support/Rectangle Pro/RectanglePro.json
                →  ~/Library/Application Support/Rectangle Pro/RectanglePro.json

            <pack>/_lib/LaunchAgents/com.example.foo.plist
                →  ~/Library/LaunchAgents/com.example.foo.plist

        :: text ::

        On non-macOS platforms, `_lib/` emits no symlink intent and produces a soft warning:

            warning: pack `<pack>` contains `_lib/<rest>` — macOS-only path,
                     skipping on this platform

        :: text ::

        The pack is otherwise unaffected; other entries deploy normally.

    6.3. The `[symlink.app_aliases]` Map

        Cross-platform packs frequently want a *natural* lowercase pack name (`vscode`) without writing `_app/Code/` for every entry. The `[symlink.app_aliases]` table lets a user declare a pack-level rewrite:

        Pack-level alias:

            [symlink.app_aliases]
            vscode = "Code"
            warp   = "dev.warp.Warp-Stable"

        :: toml ::

        When a pack name appears as a key in `app_aliases`, the *default rule* for that pack is rerouted: instead of `$XDG_CONFIG_HOME/<pack>/<rel_path>`, the deploy path becomes `<app_support_dir>/<value>/<rel_path>`. The pack `vscode` with `User/settings.json` then deploys to `~/Library/Application Support/Code/User/settings.json` on macOS and `~/.config/Code/User/settings.json` on Linux — without any `_app/` prefix in the pack tree.

        Aliases compose with the rest of the priority ladder: `home.X` (Priority 1) and the directory prefixes (`_home/`, `_xdg/`, `_app/`, `_lib/` — Priority 2) all outrank the alias-driven default. A `[symlink.targets]` entry (Priority 0) still wins absolutely.

    6.4. The `app_uses_library` Switch

        On macOS the `app_support_dir` defaults to `~/Library/Application Support`. To opt the entire pack tree into Linux-style `~/.config` placement on macOS (e.g. for a user who keeps everything XDG-style), set:

        Override:

            [symlink]
            app_uses_library = false

        :: toml ::

        With this, `_app/` and `app_aliases` route through `~/.config/...` instead. `_lib/` is unaffected — it explicitly targets `~/Library/`, not `app_support_dir`.

    6.5. Priority Ladder Summary

        With the macOS additions, the resolver evaluates the following rules in order. The first matching rule wins:

        Priorities (highest first):

            0. `[symlink.targets]` custom target
            1. `home.X` prefix (top-level files only) → `$HOME/.X`
            2. Directory prefixes (per-subtree, skip pack namespace):
                a. `_home/<rest>` → `$HOME/.<rest>`
                b. `_xdg/<rest>`  → `$XDG_CONFIG_HOME/<rest>`
                c. `_app/<rest>`  → `<app_support_dir>/<rest>`
                d. `_lib/<rest>`  → `$HOME/Library/<rest>` (macOS only; warn elsewhere)
            3. `force_home` list → `$HOME/.<first-segment>/<rest>`
            4. `force_app` list → `<app_support_dir>/<first-segment>/<rest>`
            5. `app_aliases[pack]` → `<app_support_dir>/<alias>/<rel_path>`
            6. Default → `$XDG_CONFIG_HOME/<pack-display-name>/<rel_path>`

        :: text ::

7. Security Restricted Symlink File Names

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

8. Ignored File Patterns

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

9. `dodot adopt`: Source-Path Inference

    `dodot adopt` accepts a *deployed* path (where the file lives now) and works backwards to figure out which pack it belongs in and what to call it inside that pack — so re-deploying with `dodot up` lands the symlink back at the original location. The inference rules below are the inverse of §1–§6.

    Calling shape:

        dodot adopt <path>...                # pack name inferred per source
        dodot adopt <path>... --into <pack>  # all sources land in <pack>

    :: text ::

    9.1. Inference Per Source Root

        The source path's deployed location decides everything:

        Source-root inference:
            | Source path                                | Inferred pack | Pack-relative path                 |
            | `~/.config/<X>/<rest>`                     | `<X>`             | `<rest>`                       |
            | `~/.config/<X>/` (the directory itself)    | `<X>`             | (children expand individually) |
            | `~/.<X>` (dotted file in $HOME)            | (require `--into`)| `home.<X>`                     |
            | `~/.<X>/...` (dotted dir in $HOME)         | (require `--into`)| `_home/<X>/...`                |
            | `~/.<X>` matching `force_home` (file/dir)  | (require `--into`)| `<X>` (bare, see §3)           |
            | `~/<X>` non-dotted, not in `force_home`    | refused           | —                              |
            | `~/Library/Application Support/<X>/<rest>` | `<X>`             | `_app/<X>/<rest>` (macOS only) |
            | `~/Library/Containers/...`                 | refused           | (sandboxed app data)           |
            | anything else                              | refused           | use `[symlink.targets]`        |
        :: table align=lll ::

        XDG sources auto-infer because the first path segment under `~/.config/` *is* the pack name — the resolver's default rule (§1) handles the round-trip with no prefix gymnastics.

        $HOME-rooted dotfiles don't infer a pack name because the structure isn't there to mine: `~/.bashrc` could plausibly belong in a `shell`, `bash`, or `dotfiles` pack, and adopt won't guess. What inference *does* compute is the in-pack path — the `home.X` / `_home/X/` / bare-name conventions from §2, §3, §5 — so the round-trip works regardless of the chosen pack name.

    9.2. Pack-Root Directory Expansion

        Adopting `~/.config/<X>/` (the whole directory) doesn't make the directory itself a single symlink. Instead, adopt enumerates its children and adopts each as a top-level pack member:

        Example expansion of `~/.config/helix/`:

            ~/.config/helix/config.toml      → helix/config.toml
            ~/.config/helix/themes/          → helix/themes/

        :: text ::

        After adoption, each child of `~/.config/helix/` is its own symlink; `~/.config/helix/` itself stays a real directory. This matches what `dodot up` would do for a hand-built pack and avoids creating a pack whose root *is* a symlink target.

        `~/.X/` directory sources keep the existing whole-subtree behavior (`_home/X/`): the directory itself becomes the symlink, because in $HOME the user's mental model is the directory *is* the file.

    9.3. The `--into` Override

        `--into <pack>` forces a destination pack regardless of inference. Two cases:

            - **Override matches inferred pack** (or no inference happened, e.g. HOME source): the natural in-pack path is used.
            - **Override differs from inferred pack** (XDG sources only): the in-pack path switches to `_xdg/<X>/<rest>` so the explicit `_xdg/` prefix from §5 bypasses pack-namespacing and lands the deployed file at the same place.

        Concrete: `dodot adopt ~/.config/lazygit/config.yml --into toolbox` lands the file at `toolbox/_xdg/lazygit/config.yml`. Re-deploying still lands the symlink at `~/.config/lazygit/config.yml` even though the pack is `toolbox`.

        Mixing HOME and XDG sources in one invocation is allowed: the HOME ones use their pack-name-independent prefixes, the XDG ones contribute the inferred pack name (or use `--into` if it differs). If two XDG sources infer different packs and no `--into` is given, adopt refuses and names both candidates so the user can split the invocation.

    9.4. Auto-Creating Packs

        When inference picks a single pack name and that pack does not exist on disk, adopt creates it (an empty directory). `--into <pack>` does *not* auto-create — the explicit name is a typo guard. Run `dodot init <pack>` first to bootstrap an explicit pack.

10. macOS Advisory Probes

    Everything in §1–§9 is deterministic: the resolver operates on textual prefixes and configured lists, nothing else. On top of that deterministic core, dodot ships an *advisory* layer that consults macOS-native metadata to make adopt suggestions richer and `up`/`status` warnings more specific. The cardinal rule: *probes are advisory, never authoritative*. The resolver in §5 never consults them; a probe failure surfaces as a less-rich suggestion, not a wrong deployment.

    10.1. The `dodot probe app` Subcommand

        Diagnostic introspection for a single pack:

            dodot probe app <pack> [--refresh]

        :: text ::

        Reports each app-support folder the pack would route to (via `[symlink.app_aliases]`, `force_app`, or `_app/<X>/` subtree), folder existence under `app_support_dir`, the matching homebrew cask token and install state, the `.app` bundle name and `kMDItemCFBundleIdentifier`, and sibling-adoption candidates surfaced from the cask's zap stanza.

        On non-macOS hosts (or when `app_uses_library = false` collapses `app_support_dir` onto `$XDG_CONFIG_HOME`) the brew/Spotlight enrichment is skipped — the subcommand still renders the basic folder layout and existence check.

        `--refresh` invalidates the brew cache for this pack's tokens, forcing a fresh `brew info` lookup on the next call. The cache is otherwise refreshed automatically every 24 hours.

    10.2. Adopt Enrichment

        When `dodot adopt ~/Library/Application Support/<X>/...` succeeds and `<X>` matches an installed cask's app-support folder, adopt appends two informational lines to `PackStatusResult.warnings`:

            - a confirmation: "homebrew cask `<token>` confirms this is the app-support directory for pack `<X>`",
            - a sibling-adoption suggestion when the cask's zap stanza lists Preferences plists: "homebrew also reports preferences for cask `<token>`: <plist-list>. Adopt them too with `dodot adopt ~/Library/Preferences/<file> --into <X>`."

    10.3. Missing-Target Hints

        `dodot up` and `dodot status` plan-pack passes check whether each `<app_support_dir>/<X>/` folder a pack will deploy into actually exists on disk. Missing folders surface a soft warning, optionally enriched with a matching cask token: "looks like cask `<token>` isn't installed yet — `<X>/...` will deploy but the app isn't here to read it." Resolver state is unaffected; the symlinks still get created.

    10.4. What Probes Don't Do

        Out of scope, deliberately:

            - Auto-derive `force_app` from cask zap data — the curated list (§3.1) stays curated.
            - Run a long-running daemon that watches LaunchServices.
            - Use `defaults read/write` to manage plist contents — see [./../proposals/plists.lex] for that path.
            - Mirror mackup's bundled app database. The combination of `_app/`, `app_aliases`, the curated `force_app`, and homebrew probing covers the same ground without the curation cost.

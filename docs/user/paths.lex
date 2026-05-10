:: verified ::
Paths

    When you run `dodot up`, every file in your packs lands somewhere on disk. This document is about the *somewhere* — how dodot decides, what the defaults give you for free, and what knobs you reach for when the defaults aren't right.

    The short version: dodot deploys to `$XDG_CONFIG_HOME/<pack>/...` by default. The handful of files that don't belong there — Unix-tradition holdouts like `.ssh` and `.bashrc`, macOS GUI app config, the occasional one-off custom path — have explicit escape hatches that read at a glance.

    :: note :: Terminology — this doc uses [pack], [handler], and [dotfiles root]. If any of those are unfamiliar, see [./glossary/].

1. When you reach for this doc

    - You ran `dodot status` and a file landed somewhere unexpected. You want to understand why.
    - You have a Unix-tradition file (`.bashrc`, `.ssh/config`) that needs to be at `~/.X` rather than under `~/.config/`.
    - You have a directory of files that should not have your pack's name in their deployed path.
    - You're adding a macOS pack and need to deploy to `~/Library/Application Support/`.
    - You need one file at a fully custom absolute path (`/etc/...`, `/opt/...`).

2. The three roots

    Every path dodot computes is `<root>/<rest>`, where `<root>` is one of three filesystem coordinates:

    Roots:
        | Symbol             | macOS                                | Linux / other                    |
        | `$HOME`            | `/Users/<you>`                       | `/home/<you>`                    |
        | `$XDG_CONFIG_HOME` | `~/.config` (unless env-set)         | `~/.config` (unless env-set)     |
        | `app_support_dir`  | `~/Library/Application Support`      | collapses to `$XDG_CONFIG_HOME`  |
    :: table align=lll ::

    On Linux, the second and third coordinates point at the same place. That's deliberate: a portable pack written with `_app/` prefixes (§4.4) routes correctly on both platforms without per-OS branches inside the pack. On macOS the third root diverges, and the `_app/` machinery starts to matter.

    The dotfiles root — where dodot *reads* from — is a separate concern. dodot picks it from `$DOTFILES_ROOT`, then `git rev-parse --show-toplevel`, then the current directory. See [./glossary/dotfiles-root.lex].

3. The default rule

    Pack-root entries — files or directories — deploy to `$XDG_CONFIG_HOME/<pack>/<rel_path>`. The pack name namespaces things under XDG, matching how nvim, helix, ghostty, kitty, alacritty, lazygit, starship, and most other modern tools read their configuration: from `~/.config/<tool>/`.

    Default rule:

        nvim/init.lua          ->  ~/.config/nvim/init.lua
        nvim/lua/              ->  ~/.config/nvim/lua/      (whole dir, one symlink)
        warp/themes/dark.yaml  ->  ~/.config/warp/themes/dark.yaml

    :: text ::

    A pack with an ordering prefix has the prefix stripped from the deployed name: `010-nvim/init.lua` deploys to `~/.config/nvim/init.lua`, not `~/.config/010-nvim/init.lua`. The numeric prefix governs `dodot up` execution order, not the deployed path.

    Anything not claimed by a more specific handler (shell, path, install, homebrew) flows through the symlink handler and the rules below.

4. Escape hatches, by what you want to do

    The cases where the default isn't right map onto specific mechanisms. Pick the one that matches your goal — they compose, but reaching for the simplest one that works keeps your packs readable.

    4.1. One file to `~/.X` (Unix canon)

        For Unix-tradition files (`bashrc`, `zshrc`, `ssh`, …) the convention demands `$HOME/.X`. Two ways to ask for that:

        - *Rely on the defaults.* dodot ships a `force_home` list that auto-routes the canonical names: `ssh`, `aws`, `kube`, `bashrc`, `zshrc`, `profile`, `bash_profile`, `bash_login`, `bash_logout`, `inputrc`. A pack-root file or directory whose name matches lands at `$HOME/.<name>` without any prefix in your pack tree.

        - *Use the `home.` filename prefix* for any file you want at `$HOME/.X` that isn't already covered:

            shell/home.bashrc      ->  ~/.bashrc
            git/home.gitconfig     ->  ~/.gitconfig

        :: text ::

        The prefix is `home.` (a word, not a bare `.`) so editors and `ls` don't hide your pack's source files behind dotfile-hiding. The leading `.` is added back on the deployed side automatically.

        Top-level only — a nested `subdir/home.X` is treated as a literal filename and goes through the default rule.

    4.2. A subtree to `$HOME`

        If a whole directory of files belongs under `$HOME/.X` rather than `$XDG_CONFIG_HOME/<pack>/X`, use the `_home/` directory prefix:

            <pack>/_home/vim/vimrc      ->  ~/.vim/vimrc
            <pack>/_home/.local/bin/x   ->  ~/.local/bin/x

        :: text ::

        Like every directory prefix, `_home/` skips pack namespacing — the pack name doesn't appear in the deployed path.

    4.3. A subtree to XDG without your pack name in the path

        Sometimes a single pack holds config for several tools, and you don't want the pack name to show up at the deploy site. The `_xdg/` directory prefix solves that:

            term-config/_xdg/ghostty/config            ->  ~/.config/ghostty/config
            term-config/_xdg/kitty/kitty.conf          ->  ~/.config/kitty/kitty.conf
            term-config/_xdg/alacritty/alacritty.toml  ->  ~/.config/alacritty/alacritty.toml

        :: text ::

        The pack name (`term-config`) plays no role inside `_xdg/`. Each first-level child is the deploy directory under `$XDG_CONFIG_HOME`.

        For a single file, `xdg.X` is the per-file equivalent: `pack/xdg.foo.list` deploys to `$XDG_CONFIG_HOME/foo.list`, also skipping pack namespacing.

    4.4. A macOS GUI app

        macOS GUI apps read configuration from `~/Library/Application Support/<App>/`, a third coordinate beyond `$HOME` and `$XDG_CONFIG_HOME`. There are three ways to route there, in increasing order of explicitness:

        - *`force_app` defaults.* dodot ships a small curated list (`Code`, `Cursor`, `Zed`, `Emacs`) so a pack-root entry whose name matches deploys directly to `<app_support_dir>/<name>/...`. Matching is case-sensitive — Library folder names are case-sensitive on macOS.

        - *`_app/` directory prefix*, the per-subtree opt-in:

            vscode/_app/Code/User/settings.json
                ->  Linux:  ~/.config/Code/User/settings.json
                ->  macOS:  ~/Library/Application Support/Code/User/settings.json

        :: text ::

        The same pack tree works on both platforms — `_app/` routes through `app_support_dir`, which is the Library path on macOS and `$XDG_CONFIG_HOME` on Linux.

        - *`[symlink.app_aliases]` for natural pack names.* If you want a lowercase, terminal-friendly pack name (`vscode`) to deploy to a GUI-app folder name (`Code`), declare the rewrite in the root config:

            [symlink.app_aliases]
            vscode = "Code"
            warp   = "dev.warp.Warp-Stable"

        :: toml ::

        With this in place, the pack `vscode` deploys via the default rule but to `<app_support_dir>/Code/<rel_path>` instead of `$XDG_CONFIG_HOME/vscode/<rel_path>`. The pack tree stays clean — no `_app/` prefixes everywhere.

        Per-file form: `app.X` deploys a single file to `<app_support_dir>/X`.

    4.5. A macOS Library subtree that isn't Application Support

        For `~/Library/LaunchAgents/`, `~/Library/Fonts/`, `~/Library/Services/`, etc., use `_lib/`:

            <pack>/_lib/LaunchAgents/com.example.foo.plist  ->  ~/Library/LaunchAgents/com.example.foo.plist
            <pack>/_lib/Fonts/MyFont.otf                    ->  ~/Library/Fonts/MyFont.otf

        :: text ::

        `_lib/` is macOS-only. On Linux it produces a soft warning and skips the file; the rest of the pack deploys normally. The matching per-file form is `lib.X` for top-level files (also macOS-only, also warns and skips on Linux).

    4.6. Anywhere else, absolute or arbitrary

        `[symlink.targets]` is the catch-all. It maps a pack-relative source path to any destination:

            [symlink.targets]
            "mysterious.conf" = "/etc/mysterious.conf"
            "home-bound.conf" = "my-documents/home-bound.conf"

        :: toml ::

        Absolute paths are used as-is. Relative paths are resolved from `$XDG_CONFIG_HOME`. `[symlink.targets]` overrides every other rule — except for the conflict case in §6.

5. The resolution priority, in one paragraph

    When more than one rule could apply to a file, dodot resolves in this order: a `[symlink.targets]` entry wins absolutely; then file-level prefixes (`home.X`, `app.X`, `xdg.X`, `lib.X`); then directory prefixes (`_home/`, `_xdg/`, `_app/`, `_lib/`); then the `force_home` and `force_app` lists; then `[symlink.app_aliases]`; then the default XDG rule. Higher-priority rules skip pack namespacing; lower-priority rules apply *after* the pack name is in the deployed path.

    For the full ladder with corner cases, see [./../reference/symlink-paths.lex].

6. Watch out for

    - *Routing override conflict.* Declaring a file in `[symlink.targets]` *and* giving it a routing prefix (`home.X`, `_home/X`, `app.X`, `_app/X`, …) raises a hard error rather than silently picking one. Two ways to say where one file goes is bug-bait — pick one. Other files in the pack continue to deploy.
    - *Filename prefixes are top-level only.* `home.bashrc` at the pack root deploys to `~/.bashrc`. `subdir/home.bashrc` deploys to `~/.config/<pack>/subdir/home.bashrc` literally — no rewrite. Same for `app.X`, `xdg.X`, `lib.X`.
    - *`_lib/` and `lib.X` are macOS-only.* On Linux they warn and skip. Other entries in the same pack deploy normally.
    - *Per-file mode for directories.* By default, a pack-root directory is wholesale-linked: one symlink for the entire directory. Listing a file inside it in `[symlink.targets]`, or having one match `protected_paths`, flips that directory into per-file mode — one symlink per file, each resolved independently.
    - *Empty remainders fall through.* A literal filename `home.` (nothing after the dot) is treated as the default rule, not as "deploy to bare `$HOME/`". Same for `app.`, `xdg.`, `lib.`.
    - *Pack ordering prefixes are stripped.* `010-nvim/init.lua` deploys to `~/.config/nvim/init.lua`. The numeric prefix governs execution order, not the deployed path.
    - *Protected paths refuse to deploy.* dodot ships a default list (SSH private keys, `.gnupg`, AWS credentials, `.kube/config`, etc.) that the symlink handler refuses to touch. Override under `[symlink] protected_paths` if you have a justified case.

7. Live edits

    Once a file is symlinked, the deployed path *is* your source via the symlink chain — edits to either end show up at the other immediately. Whether the program reading the config picks up the change is up to the program: shells re-read on next launch, file-watching editors (nvim with `autoread`, vscode) reload immediately, daemons and window managers usually need an explicit reload. Adding or removing a source file in your pack still requires another `dodot up` to update the symlink set.

8. See also

    - [./handlers/symlink.lex] — the symlink handler from the handler side.
    - [./../reference/symlink-paths.lex] — full priority ladder, all edge cases, adopt inference.
    - [./adopting.lex] — the inverse direction: deployed path back into a pack.
    - [./plists.lex] — macOS plist clean/smudge filters; the third-coordinate use case in detail.
    - [./configuration.lex] — the `[symlink]` section schema.
    - [./glossary/dotfiles-root.lex], [./glossary/pack.lex], [./glossary/handler.lex].

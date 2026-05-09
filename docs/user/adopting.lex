:: verified ::
Adopting existing dotfiles

    You already have a working setup — `~/.bashrc`, `~/.config/nvim/`, maybe a `Code/User/settings.json` on macOS — and you want to bring it under dodot without breaking your shell session, your editor, or anything else that's currently reading those files. This doc is about that move.

    `dodot adopt` is the command. For each path you point at, dodot moves the file into the right pack inside your dotfiles repo and replaces the original location with a symlink back. Programs reading the original location see the same content; you've just gained a versioned home for it.

    :: note :: Terminology — this doc uses [pack], [dotfiles root], and [handler]. If unfamiliar, see [./glossary/].

1. When you reach for this doc

    - First time onboarding: you've installed dodot but your dotfiles still live in `$HOME` and `$XDG_CONFIG_HOME`.
    - Adding a new tool to dodot: you just configured it manually and want it under git now.
    - Bringing macOS GUI app config under dodot: settings, themes, preferences from `~/Library/...`.
    - Bulk-importing a config tree: `dodot adopt ~/.config/helix/` rather than file-by-file.

2. The mental model — adopt is the inverse of up

    `dodot up` reads your pack and lays down symlinks at the deployed location. `dodot adopt` reads a deployed location, computes which pack it belongs in, and works backwards: moves the file into the pack and replaces the original spot with a symlink. After `adopt`, running `dodot up` would no-op for that file — the chain is already in place.

    What inference figures out:

    - The pack name (when the source path can tell).
    - The pack-relative path, including any prefix (`home.X`, `_home/X/`, `_app/X/`, …) needed to make the round-trip work — see [./paths.lex] §4 for what those prefixes mean on the deploy side.

    XDG sources (under `~/.config/<X>/`) auto-infer the pack name (`<X>`). HOME-rooted dotfiles can't — `~/.bashrc` could plausibly belong in a `shell`, `bash`, or `dotfiles` pack — so adopt asks you to pass `--into <pack>`.

    The full inference table lives in [./../reference/symlink-paths.lex] §9; the per-command surface (every flag, every error) is in [./commands/adopt.lex]. This doc is the user-need overview.

3. The common cases

    3.1. XDG sources — pack inferred

        Anything under `~/.config/<X>/` adopts cleanly without `--into`. The pack name is `<X>`, the in-pack path is whatever followed it:

            dodot adopt ~/.config/nvim/init.lua
            ; pack `nvim`, in-pack `init.lua`

            dodot adopt ~/.config/helix/
            ; pack `helix`, expands children: config.toml, themes/, ...

        :: shell ::

        If the pack doesn't exist yet, adopt creates it (an empty directory). The next `dodot up` lays down the symlinks back to the original spots.

    3.2. HOME sources — `--into` required

        Tradition tools live at `~/.X` rather than `~/.config/`. Pass `--into <pack>` and adopt encodes the right prefix automatically:

            dodot adopt --into shell ~/.bashrc
            ; lands at shell/home.bashrc — deploys back to ~/.bashrc

            dodot adopt --into git ~/.gitconfig ~/.gitignore_global
            ; lands at git/home.gitconfig and git/home.gitignore_global

        :: shell ::

        For names on the `force_home` default list (`bashrc`, `zshrc`, `ssh`, `aws`, `kube`, `profile`, `bash_profile`, `bash_login`, `bash_logout`, `inputrc`), adopt drops the prefix entirely — `~/.bashrc` adopted into pack `shell` lands as `shell/bashrc` (no `home.` prefix), since the `force_home` rule routes pack-root `bashrc` to `~/.bashrc` automatically. Either form round-trips; the prefix-less form is shorter.

    3.3. macOS Application Support — pack inferred

        `~/Library/Application Support/<App>/<rest>` adopts with the pack name auto-inferred and the `_app/<App>/...` prefix encoded:

            dodot adopt ~/Library/Application\ Support/Code/User/settings.json
            ; pack `Code`, in-pack `_app/Code/User/settings.json`

        :: shell ::

        If you'd prefer a friendlier pack name (`vscode` rather than `Code`), see §3.5.

    3.4. Other macOS Library subtrees — `--into` required

        For `~/Library/Preferences/`, `~/Library/LaunchAgents/`, `~/Library/Fonts/`, etc., pass `--into`. Adopt encodes `_lib/<rest>`:

            dodot adopt --into mac-defaults ~/Library/Preferences/com.app.plist
            ; lands at mac-defaults/_lib/Preferences/com.app.plist

            dodot adopt --into agents ~/Library/LaunchAgents/com.example.foo.plist
            ; lands at agents/_lib/LaunchAgents/com.example.foo.plist

        :: shell ::

        `~/Library/Containers/` is refused — sandboxed-app data isn't safe to externalize. Apps treat the path as private and may rebuild it on launch.

    3.5. Choosing a different pack name

        `--into` overrides per-source inference. For XDG sources, the in-pack path switches to `_xdg/<original>/<rest>` so the round-trip still works:

            dodot adopt --into tools ~/.config/lazygit/
            ; lands at tools/_xdg/lazygit/ — deploys back to ~/.config/lazygit/

        :: shell ::

        Useful for grouping unrelated XDG configs into one pack. The deployed location is unchanged; only the in-pack location moves.

        Note: explicit `--into <pack>` does *not* auto-create the pack — the explicit name is treated as a typo guard. Run `dodot init <pack>` first.

4. Bulk adoption

    Pointing adopt at a directory adopts all its children:

        dodot adopt ~/.config/helix/

    :: shell ::

    Each child becomes a top-level pack entry (one symlink per child). The pack-root directory stays a real directory — the same shape `dodot up` would produce for a hand-built pack.

    Multiple sources at once also work, as long as their inferences agree (or all decline):

        dodot adopt ~/.config/nvim/init.lua ~/.config/nvim/lua/
        ; both infer `nvim` — no --into needed

        dodot adopt ~/.config/nvim/init.lua ~/.config/helix/config.toml
        ; would infer two different packs — adopt errors and asks for --into

    :: shell ::

5. Previewing before pulling the trigger

        dodot adopt --dry-run --into git ~/.gitconfig

    :: shell ::

    Prints the moves and symlinks that would happen without making changes. Use this whenever you're unsure where a source path will land — especially for HOME and Library sources, where the prefix encoding isn't obvious from the original path.

6. Watch out for

    - *`Containers/` is refused.* `~/Library/Containers/` always errors. The fix is usually `~/Library/Application Support/<App>/` (see §3.3) or whatever the app's documented config path is.
    - *`--into` does not auto-create packs.* Inferred packs are auto-created; explicit `--into <pack>` requires the pack to exist. Run `dodot init <pack>` first.
    - *Already-symlinked sources are followed by default.* If `~/.bashrc` is already a symlink, adopt resolves it and moves the *target*. Pass `--no-follow` to move the symlink itself instead — useful when migrating from another dotfiles manager.
    - *Plist on first adopt.* Adopting a `*.plist` file when the dodot-plist git filter isn't yet registered prints a one-line tip pointing at `dodot git-install-filters`. See [./plists.lex] for the full setup story.
    - *Adopt is reversible by hand, not by command.* No `dodot un-adopt` exists. To undo: replace the symlink at the source location with the moved file (`mv <pack>/<rel> <original>`). dodot doesn't track adoption history — git does.

7. Live edits after adopt

    Adopt sets up the symlink chain; the next `dodot up` is what wires the file into dodot's deployment state. After up, edits at either end (the source in your pack, or the deployed location) flow through the symlink immediately. The full live-edits story for symlinked files is in [./paths.lex] §7.

    Programs already running keep their old in-memory configuration until reloaded — same as for any config edit.

8. See also

    - [./commands/adopt.lex] — full command reference: flags, errors, edge cases.
    - [./../reference/symlink-paths.lex] §9 — the inference table in full, including macOS advisory probes.
    - [./paths.lex] — where files end up at deploy time (the inverse of inference).
    - [./getting-started.lex] — the broader onboarding flow.
    - [./plists.lex] — adopting macOS plists with the dodot-plist git filter.
    - [./glossary/pack.lex], [./glossary/dotfiles-root.lex], [./glossary/handler.lex].

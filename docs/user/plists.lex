:: verified ::
macOS plists

    macOS GUI applications store their preferences as binary plists at `~/Library/Preferences/<bundle-id>.plist` and `~/Library/Application Support/<App>/...`. Out of the box, those binaries are unreadable in `git diff`, can't be sensibly merged, and can't really be reviewed in a PR. dodot brings them under the same review/diff/cherry-pick workflow as plain-text dotfiles by translating them through git clean/smudge filters.

    The deal: the file in your pack stays binary (so the OS and the app see what they expect), but what git stores is canonical, alphabetically-sorted XML. `git status` and `git diff` show the *real semantic* changes — no encoder noise, no spurious diffs.

    :: note :: Terminology — this doc uses [pack], [handler], [dotfiles root]. See [./glossary/]. The full reference (mechanism, determinism contract, edge cases) is at [./../reference/plists.lex] — this doc is the user-need overview.

1. When you reach for this doc

    - You're on macOS and want to keep app settings (Terminal, Rectangle, Karabiner, …) under version control.
    - You ran `dodot adopt` on a `.plist` file and got a tip about installing git filters.
    - You committed a plist and `git diff` showed binary garbage instead of an XML diff.
    - You're moving plist-managed settings between machines.

2. The mental model — three artifacts, one symlink

    Three different views of "the same" file:

        | Where                                   | Format                              |
        | `<pack>/foo.plist` (working tree)       | binary — the bytes the app reads    |
        | git index entry for `<pack>/foo.plist`  | canonical XML — what `git diff` shows |
        | `~/Library/Preferences/foo.plist`       | symlink → `<pack>/foo.plist`        |
    :: table align=ll ::

    The deployed location is just dodot's normal symlink (no second copy, no datastore artifact). The trick is in git's clean/smudge filters: on `git add`, dodot's `clean` filter renders canonical XML for the index; on `git checkout`, dodot's `smudge` filter renders binary back into the working tree.

    Result: the macOS app writes settings normally; the next `git status` sees the change; `git diff` shows what changed in human-readable XML form. No `dodot render`, no `dodot refresh`, no separate workflow.

3. One-time setup

    Two pieces wire git to dodot's filters. Both are documented in detail at [./commands/git-install-filters.lex] and [./../reference/plists.lex] §2; the short version:

    3.1. `.gitattributes` (committed in the repo)

        Add a single line:

            *.plist filter=dodot-plist

        :: text ::

        This file ships with the repo. Without the matching `.git/config` registration below, `.gitattributes` alone is harmless — git falls back to the identity filter.

    3.2. `.git/config` (per clone, per machine)

        One command, idempotent:

            dodot git-install-filters

        :: shell ::

        Writes the `[filter "dodot-plist"]` block to `.git/config` so git knows how to invoke `dodot plist clean` and `dodot plist smudge`. To inspect without writing, run `dodot git-show-filters` instead.

    3.3. The up-time prompt

        On the first `dodot up` of a pack containing `*.plist` files, dodot offers to install the filters interactively if they're not yet registered. Y to install, n to skip, `show` to inspect first. Non-interactive (CI, scripts) skips silently.

4. Day-to-day workflow

    Once filters are wired, plist work uses vanilla git:

        # The app writes settings via its UI; the binary file in the pack
        # has new bytes, mtime updates.

        $ git status
        modified:   mac-defaults/com.app.plist

        $ git diff mac-defaults/com.app.plist
        # ... XML diff showing exactly which keys changed ...

        $ git add mac-defaults/com.app.plist
        $ git commit -m "tweak terminal font size"

    :: shell ::

    No dodot command is invoked in normal use. The clean filter renders canonical XML for the index on `git add`; the smudge filter renders binary for the working tree on `git checkout`.

    On another machine:

        $ git pull
        $ dodot up
        $ killall cfprefsd        # see §6

    :: shell ::

    `git pull` invokes the smudge filter on any updated `*.plist` files, putting the new binary in the working tree. `dodot up` ensures the symlinks are in place.

5. Adopting existing plists

    To bring a plist already on this machine under dodot:

        dodot adopt --into mac-defaults ~/Library/Preferences/com.app.plist
        ; lands at mac-defaults/_lib/Preferences/com.app.plist

    :: shell ::

    The `_lib/` prefix routes the deploy back to `~/Library/Preferences/` on `dodot up`. `--into <pack>` is required — plist filenames are typically reverse-DNS bundle IDs (`com.colliderli.iina.plist`) that don't make useful pack names.

    Same flow works for `~/Library/LaunchAgents/...`, `~/Library/Fonts/...`, and other `~/Library/<sub>/` paths. `~/Library/Application Support/<App>/` adopts via `_app/` instead — see [./adopting.lex] §3.3. `~/Library/Containers/` is refused (sandboxed-app data).

    If filters aren't yet installed when you adopt, dodot prints a one-line tip pointing at `dodot git-install-filters`. You can install before or after; the next `git add` runs the clean filter regardless.

6. cfprefsd — why `killall` shows up

    macOS's `cfprefsd` daemon caches plist values in memory. Writing to the on-disk binary — even via the deploy symlink — may not be picked up by a running app until cfprefsd re-reads its preferences. After pulling plist changes from another machine and running `dodot up`:

        killall cfprefsd

    :: shell ::

    cfprefsd auto-respawns immediately, no data is lost. dodot prints this reminder in the `git-install-filters` success output. Some apps also need a relaunch to re-read their preferences — if a setting doesn't appear after `killall cfprefsd`, restart the app.

7. Watch out for

    - *Binary in the working tree.* `cat pack/com.app.plist` shows noise; an editor opening the pack file sees binary garbage. That's the trade-off — the deployed file IS the working-tree file, so it has to be in the format the OS reads. `git diff` and PR reviews still show XML.
    - *`dodot` must be on `$PATH` for whatever invokes git.* The filter entry runs `dodot plist clean/smudge`, so the binary needs to be findable. GUI git clients sometimes run from a reduced environment that doesn't see your shell's `$PATH`. If a GUI client errors on plist files, either put `dodot` on the system path (`/usr/local/bin/dodot`) or use absolute paths in `.git/config`.
    - *`required = true` is loud, deliberately.* The filter is registered with `required = true` so git aborts hard if the filter binary is missing or fails. Silently storing the wrong representation would be worse.
    - *Plist editing is via the app, not by hand.* Settings are authored by editing through the app's UI (the normal way) or by editing the deployed file with a plist editor (Xcode, PlistBuddy, or VSCode's plist extension). Hand-editing the binary in the pack works but is rare; for that case `plutil -convert xml1 <file>` / edit / `plutil -convert binary1 <file>` is the path.
    - *macOS-only at deploy time, but the CLI is portable.* The `_lib/` resolver and `~/Library/...` adopt inference are macOS-only. The clean/smudge filters, the canonical-XML transform, and the up-time install prompt all work everywhere — exercised in CI on Linux. So a dotfiles repo authored on Linux can still ship plist filters for collaborators on macOS.

8. Configuration

    One knob: `[symlink] plist_extensions` controls which filename suffixes detection treats as plists (for adopt hints, the up-time prompt, and the `.gitattributes` lines emitted by `git-install-filters`).

        [symlink]
        plist_extensions = ["plist", "binplist", "savedState"]

    :: toml ::

    Default is `["plist"]`. Add extensions only if your apps use non-standard suffixes for plist data. Comparison is case-insensitive.

    There is no `[preprocessor.plist]` section. Plists don't go through the preprocessing pipeline (which is for `.tmpl`, `.age`, `.gpg` files); they're handled by the symlink handler at deploy time and by git filters at commit time. See [./../reference/plists.lex] §9 for why.

9. Live edits

    Editing through the app's UI: writes the binary directly; next `git status` sees the change; `git diff` shows the XML; `git add` + commit captures it.

    Editing on another machine and pulling: `git pull` materialises the new binary via the smudge filter; `dodot up` makes sure the symlink is in place; `killall cfprefsd` (and possibly an app relaunch) makes the running app re-read its preferences.

    The symlink chain itself stays live the same way every other dodot symlink does — see [./paths.lex] §7 for the general story.

10. See also

    - [./../reference/plists.lex] — full reference: determinism contract, filter failure modes, hand-editing path, the prompts registry.
    - [./commands/git-install-filters.lex], [./commands/git-show-filters.lex] — the CLI surface for installing the filters.
    - [./commands/plist.lex] — the underlying clean/smudge transform commands.
    - [./adopting.lex] — adopt for non-plist files (and `_app/` routing for Application Support).
    - [./paths.lex] §4.5 — `_lib/` deploy routing, the macOS Library escape hatch.
    - [./glossary/handler.lex], [./glossary/pack.lex].

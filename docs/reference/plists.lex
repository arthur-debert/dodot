macOS Plists

    macOS GUI applications store their preferences as binary plists at `~/Library/Preferences/<bundle-id>.plist` and `~/Library/Application Support/<App>/...`. dodot brings these under source control by translating them to canonical XML on `git add` and back to binary on `git checkout` — the file in your pack is the binary the app reads, and what git stores is human-diffable XML. There is no "render step", no datastore copy, no separate workflow. `git status` and `git diff` show settings changes the way they show every other dotfile change.

    This document is the user-facing reference. The architectural rationale (why clean/smudge filters rather than a preprocessing pipeline; why working-tree binary rather than working-tree XML) is in [./../proposals/plists.lex].

    :: note :: macOS-only behaviour is the deploy-side `_lib/` resolver (which routes `_lib/<rest>` to `~/Library/<rest>`) and the `~/Library/...` adopt inference. The CLI surface (`dodot plist clean/smudge`, `dodot git-install-filters/show-filters`), the canonical-XML transform, the determinism property, and the up-time install prompt all work on every platform — exercised in CI on Linux runners against real git. macOS is not a hard requirement to *use* plist filters in a dotfiles repo; it is the platform where the deployed file is what an OS app reads.

1. The Mechanism

    Three files; one symlink:

    Layout:

        <pack>/foo.plist                    # working-tree file: BINARY
        git index entry for <pack>/foo.plist # canonical XML (what `git diff` shows)
        ~/Library/Preferences/foo.plist     # symlink → <pack>/foo.plist

    :: text ::

    The deployed file IS the working-tree file (via dodot's normal symlink). When the macOS app writes settings, the bytes land at `<pack>/foo.plist` directly and that file's mtime updates. The next `git status` stats the working-tree path, sees the mtime mismatch against the index, and re-runs the clean filter on it. No injected timestamps, no `dodot refresh`, no separate datastore copy.

    What's in the index is canonical XML: dictionary keys sorted recursively (Unicode codepoint order), array order preserved, deterministic formatting, single trailing LF. The same logical plist always produces byte-identical XML regardless of the encoder's internal layout, so `git diff` only reports semantic changes — never encoder noise.

2. Setup

    Two pieces wire git to dodot's filters:

    2.1. `.gitattributes` (committed in the repo)

        Add a single line that binds `*.plist` files to the dodot-plist filter:

            *.plist filter=dodot-plist

        :: text ::

        This file is part of the repo and travels with every clone. Without the matching `.git/config` registration described below, `.gitattributes` alone is harmless but inert — git falls back to the identity filter.

    2.2. `.git/config` (per clone, per machine)

        Run once per machine:

            dodot git-install-filters

        :: text ::

        Writes the `[filter "dodot-plist"]` block:

            [filter "dodot-plist"]
                clean  = dodot plist clean
                smudge = dodot plist smudge
                required = true

        :: text ::

        `required = true` means git aborts loudly if the filter binary is missing or fails — preferable to silently storing the wrong representation. Re-running `dodot git-install-filters` is idempotent.

        To inspect or install by hand instead:

            dodot git-show-filters

        :: text ::

        prints both snippets (the `.git/config` block and the `.gitattributes` line) without writing anything. Each is annotated with whether it is currently in place.

    2.3. The Up-Time Prompt

        On the first `dodot up` of a pack containing `*.plist` files, dodot offers to install the filters interactively if they are not yet registered:

            dodot detected N .plist file(s) in pack `mac-defaults`.
            Plist support uses git clean/smudge filters to keep the source diffable.
            Install filters now? Run `dodot git-show-filters` to inspect first.
            [Y/n/show]

        :: text ::

        - **Y** (or empty input): runs `dodot git-install-filters` and dismisses the prompt for this machine.
        - **n**: skips. The prompt fires again on the next `up` until you install (or your filter setup is complete via some other path).
        - **show**: prints the config without installing. Doesn't dismiss the prompt.

        The prompt fires only on a TTY — non-interactive invocations (CI, shell scripts) skip it silently. To re-enable a dismissed prompt, run `dodot prompts reset plist.install_filters`.

3. Day-to-Day Workflow

    Once filters are wired, plist work uses vanilla git:

    Workflow:

        # The app writes settings via its UI. The working-tree binary
        # has new bytes; mtime updates.

        $ git status
        modified:   mac-defaults/com.app.plist

        $ git diff mac-defaults/com.app.plist
        # ... XML diff showing exactly which keys changed ...

        $ git add mac-defaults/com.app.plist
        $ git commit -m "tweak terminal font size"

    :: text ::

    No dodot command is invoked. The clean filter renders canonical XML for the index on `git add`; the smudge filter renders binary for the working tree on `git checkout`.

    On another machine:

        $ git pull
        $ dodot up
        $ killall cfprefsd  # see §5

    :: text ::

    `git pull` invokes the smudge filter on any updated `.plist` files, materialising the new binary in the working tree. `dodot up` ensures the symlinks are in place.

4. Adopting Existing Plists

    To bring a plist already in `~/Library/Preferences/` (or any `~/Library/<sub>/`) under dodot:

        dodot adopt --into <pack> ~/Library/Preferences/com.app.plist

    :: text ::

    The file moves to `<pack>/_lib/Preferences/com.app.plist` (the `_lib/` prefix routes back to `~/Library/Preferences/` on `dodot up`). A symlink replaces the original. `--into <pack>` is required — plist filenames are typically reverse-DNS bundle IDs that don't make useful pack names.

    Same flow works for `~/Library/LaunchAgents/...`, `~/Library/Fonts/...`, etc. — anything under `~/Library/` not nested in `Application Support` (which routes through `_app/` instead — see [./symlink-paths.lex] §6) or `Containers` (refused as sandboxed-app data).

    If filters aren't yet installed when you adopt, dodot prints a one-line tip pointing at `dodot git-install-filters`. You can install before or after adopting; the next `git add` runs the clean filter regardless.

5. cfprefsd: Why You May Need `killall`

    macOS's `cfprefsd` daemon caches plist values in memory. Writing to the on-disk binary — even via the deploy symlink — may not be picked up by a running app until cfprefsd re-reads its preferences. After pulling plist changes from another machine and running `dodot up`, run:

        killall cfprefsd

    :: text ::

    cfprefsd auto-respawns immediately; no data is lost. dodot prints this reminder in the `git-install-filters` success output.

6. Filter Failure Modes

    `required = true` makes filter failures hard errors. Three things can trigger them:

    6.1. `dodot` not on `$PATH`

        The filter entry uses bare `dodot plist clean/smudge`, so the binary must be on `$PATH` for whatever process invokes git — shell, editor, GUI git client, etc. GUI git clients sometimes run from a reduced environment that doesn't see your shell's `$PATH`. The fix is to either put `dodot` on the system `$PATH` (e.g. `/usr/local/bin/dodot`) or hand-edit the `.git/config` block to use an absolute path.

    6.2. Corrupt or non-plist input

        If `.gitattributes` binds `*.plist` to the filter but a file matching that pattern isn't a valid plist (typo'd extension, corrupt download), the filter exits non-zero and git refuses the operation. dodot's error message names both common causes and points at `dodot git-show-filters` for inspection. To diagnose:

            plutil -lint <file>            # is the file a valid plist?
            plutil -convert xml1 <file>    # what does it look like as XML?
            dodot git-show-filters          # which files are bound to the filter?

        :: text ::

    6.3. Determinism contract violations

        Two consecutive `git status` invocations on an unchanged file should produce the same blob SHA. If they don't, the canonicalisation has a non-deterministic dependency. dodot's unit suite includes a property test (`determinism_property_test`) that round-trips `binary → clean → smudge → clean` five times and asserts byte-equality; the e2e suite (`tests/e2e/bats/test_plists.bats`) verifies the same property through real `git add`. If you see drift in practice, file an issue with the offending plist attached.

7. The Working-Tree-Binary Trade-Off

    The cost of this architecture, stated plainly: the file in your pack is binary. `cat pack/com.app.plist` shows noise. An editor opening the pack file directly sees binary garbage.

    What is *not* lost:

        - `git diff`, `git show <ref>:<path>`, `git log -p` all show XML — that is what git stores.
        - `git status` shows whether the binary differs from the canonical XML in the index.
        - PR reviews show XML diffs.
        - Settings are authored by editing through the app's normal GUI, or by editing the deployed file with a plist editor — almost no one wants to hand-edit the file in the pack.

    For the rare case where you do want to hand-edit the XML in the pack:

        plutil -convert xml1 pack/com.app.plist
        $EDITOR pack/com.app.plist
        plutil -convert binary1 pack/com.app.plist

    :: text ::

    dodot does not ship a sugar command for this — it's a one-liner against `plutil`, and surfacing it as a dodot command would imply a workflow that should remain rare.

8. The Generic Prompt Registry

    The up-time install offer goes through a content-agnostic registry at `<XDG_DATA_HOME>/dodot/prompts.json`. Two CLI verbs:

    Commands:

        dodot prompts list                              # show every known prompt + state
        dodot prompts reset plist.install_filters       # un-dismiss one prompt
        dodot prompts reset --all                       # un-dismiss all

    :: text ::

    `dodot prompts list` shows known prompts as `active` (will fire when the condition is next met) or `dismissed` (suppressed). Future onboarding nudges use the same machinery — there's only one prompt today, but the registry is the right place to look when something does or doesn't fire.

9. Configuration

    Plist support currently has no configuration knobs. Plist detection (for adopt hints and the up-time prompt) is hard-coded to the `.plist` extension; deployment is governed by the symlink handler and the file's location, the same as any other file. There is no `[preprocessor.plist]` section either — plists do not go through the preprocessing pipeline.

    A configurable `plist_extensions` list was sketched in [./../proposals/plists.lex] §8.1 to cover non-`.plist` suffixes some apps use (`.savedState`, `.mobileconfig`, …). It has not been implemented; if real-world demand surfaces, it would slot in under `[symlink]`.

10. Commands at a Glance

    Plist-related verbs and what they do:

    Reference:
        | Command                              | Purpose                                                  |
        | `dodot plist clean`                  | stdin binary → stdout canonical XML (filter direction 1) |
        | `dodot plist smudge`                 | stdin XML → stdout binary plist (filter direction 2)     |
        | `dodot git-install-filters`          | write `[filter "dodot-plist"]` to `.git/config`          |
        | `dodot git-show-filters`             | print config + .gitattributes snippets without writing   |
        | `dodot adopt --into <p> <path>`      | move existing `~/Library/<sub>/` files into pack `<p>`   |
        | `dodot prompts list`                 | show dismissed-prompt state                              |
        | `dodot prompts reset <key>`/`--all`  | clear dismissals so the prompt fires again               |
    :: table align=ll ::

    `dodot plist clean/smudge` are filters: they read stdin and write stdout. You don't normally invoke them by hand; git invokes them via the filter mechanism. Running them manually is fine for inspection (`dodot plist clean < some.plist | head` shows the canonical XML form) but not part of any workflow.

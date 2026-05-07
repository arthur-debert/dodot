:: verified ::
dodot git-install-filters

The "wire up plist clean/smudge filters" command. Writes the `[filter "dodot-plist"]` block to your dotfiles repo's `.git/config` so future `git status`, `git diff`, and `git add` invocations on tracked `*.plist` files run through dodot's clean (binary → canonical XML) and smudge (XML → binary) filters automatically.

Plist-only despite the generic name. The template clean filter is `dodot template install-filter`, and the pre-commit hook is `dodot transform install-hook`. See [./git-augmentation.lex] for the conceptual map.

1. When you reach for it

    - You have macOS plist files in a pack and want git diffs to be readable text instead of binary noise.
    - The post-`up` install ladder offered the plist filter and you said `No`, but now you've changed your mind: `dodot git-install-filters` (or `dodot prompts reset plist.install_filters` to bring back the ladder offer).
    - You're cloning your dotfiles on a new machine. The `.gitattributes` line travels with the repo; this command writes the matching `.git/config` half so the binding is active.

2. What it writes

    Three lines into `.git/config` under `[filter "dodot-plist"]`:

    - `clean = dodot plist clean`
    - `smudge = dodot plist smudge`
    - `required = true`

    Per-clone, per-machine — `.git/config` is not carried by the repo. Run once per machine after cloning (or let the install ladder do it for you).

3. Pair with `.gitattributes`

    The `.git/config` block makes the filter *available*; you also need a line in `.gitattributes` to *bind* it to plist files. Commit this once and every clone gets the binding for free:

        *.plist filter=dodot-plist

    :: text ::

    `dodot git-show-filters` prints both halves with annotations indicating whether each is currently in place. See [./git-show-filters.lex].

4. Examples

        dodot git-install-filters       # write the .git/config block
        dodot git-show-filters          # inspect or install by hand

    :: shell ::

5. Watch out for

    - *Plist-only.* Despite the generic command name, this writes only the dodot-plist filter — not template, not anything else. The other rungs have their own installers.
    - *`dodot` must be on `$PATH` for whatever invokes git.* The filter block uses the bare command, so a GUI git client running from a reduced environment can fail loudly. Fix by putting dodot on the system `$PATH` (e.g. `/usr/local/bin/dodot`) or hand-edit the block to use an absolute path.
    - *Idempotent.* Re-running on an already-installed filter is a no-op success — no duplicate blocks, no overwrite.
    - *macOS cfprefsd cache.* After pulling plist changes from another machine, `cfprefsd` may keep serving stale values to running apps from its in-memory cache. dodot offers a `killall cfprefsd` prompt after `up` detects a plist change; you can also run that by hand at any time.

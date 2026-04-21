The Data Layer

    dodot's data layer is the design decision that makes the rest of the tool work. It is how dodot answers "what is managed, what is deployed, what ran" without a database, without a manifest, and without imposing a sync step on you. This document explains what the data layer is, why it is shaped the way it is, and what properties fall out of that shape.

    :: note :: See [./terms-and-concepts.lex] for terminology used throughout.

1. The Problem

    A dotfile manager needs answers to questions like: _is `~/.vimrc` managed by me? was `install.sh` already run? is this pack deployed or pending?_ The obvious way to answer them is to keep a record — a state file, a lockfile, a small database — that the tool reads and writes. That's how most tools work.

    It is also how most tools fall apart. Any bespoke state representation has two problems. First, it drifts: the tool's record of what it did stops matching what's actually on the filesystem. Second, it locks you in: uninstalling the tool strands the state, and any outside observer — your shell, another program, you with `ls` — has no way to read it.

    dodot's constraints rule out both. Git is the source of truth, not the tool's database. There is no apply step, so a "write the record then execute the effect" flow doesn't fit either. The filesystem has to be the state. The question is how.

2. The Double-Link

    A conventional dotfiles tool creates one symlink per managed file.

    Conventional layout:

        ~/.gitconfig  ->  ~/dotfiles/git/gitconfig

    :: text ::

    dodot inserts a second link in the middle.

    dodot layout:

        ~/.gitconfig  ->  ~/.local/share/dodot/packs/git/symlink/gitconfig  ->  ~/dotfiles/git/gitconfig

    :: text ::

    The intermediate link lives under `$XDG_DATA_HOME/dodot/` (the _datastore_). It points to the file in your dotfiles repo. The user-side link in your home directory points to the intermediate link, not to the repo file. The content flows through two hops instead of one.

    This is the entirety of the mechanism. Everything in the rest of this document is a consequence of that second hop.

3. What the Second Hop Buys You

    3.1. Queryable State Without a Database

        The datastore directory IS the state. A symlink under `<datastore>/packs/vim/symlink/.vimrc` means dodot has deployed `.vimrc`. Its absence means it hasn't. No record to consult, no separate file to keep in sync — the thing and the fact about the thing are the same object.

        Inspecting state is `ls` and `readlink`. Debugging a broken deployment is walking the symlink chain. There is nothing dodot-specific in the toolchain.

    3.2. Managed vs Unmanaged Files Are Distinguishable

        Because the datastore link is in the middle, dodot can tell the difference between "you manually linked `~/.vimrc` to a file you wrote yourself" and "dodot linked `~/.vimrc` through the datastore." A conventional tool that links home directly to the repo cannot tell these cases apart — the deployed symlink looks identical either way. The double-link makes dodot's footprint legible and, more importantly, bounded: commands like `dodot down` touch only paths that go through the datastore.

    3.3. Lock-In Free

        The datastore is a regular directory full of regular symlinks and a handful of small marker files. If dodot disappears tomorrow, your deployments keep working — the symlinks still resolve. You can move the datastore, copy it, diff it, `rsync` it, delete parts of it, restore it from backups. Nothing about it requires dodot to interpret. This is the "minutes to leave" property from the philosophy doc.

    3.4. Free Live Editing

        Because both hops are symlinks, edits to the source file in your dotfiles repo are visible at the deployed location immediately. No sync step, no regeneration. This isn't new to the double-link — conventional symlink managers have the same property — but it is preserved.

4. Live Receipts

    The double-link has a second, subtler property: the datastore isn't a receipt of what happened, it is the input that makes things happen. When the shell integration runs at login, it walks the datastore directory tree. For every file under `<pack>/shell/`, it emits a `source` line. For every directory under `<pack>/path/`, it prepends that directory to `$PATH`.

    In other words, writing to the datastore doesn't _describe_ the effect, it _produces_ the effect. Delete an entry and the corresponding behavior disappears on the next shell open. Add one and the behavior appears. You cannot have a "the tool's record says X but the actual behavior is Y" divergence, because the tool's record _is_ the actual behavior.

    This is why inspecting the datastore is a real debugging tool rather than a log to interpret. `ls -la ~/.local/share/dodot/packs/` shows you what is happening right now, not what was intended to happen at the last deploy.

5. Handlers Within the Layout

    Each handler owns a subdirectory under its pack's datastore entry. The name of the subdirectory is the handler's name.

    Datastore shape:

        ~/.local/share/dodot/
        +-- packs/
        |   +-- vim/
        |   |   +-- symlink/     # intermediate links for vim's config files
        |   |   +-- shell/       # shell scripts sourced at login
        |   +-- git/
        |   |   +-- symlink/
        |   +-- tools/
        |       +-- path/        # directories added to PATH
        |       +-- homebrew/    # sentinels for Brewfile runs
        |       +-- install/     # sentinels for install.sh runs
        +-- shell/
            +-- dodot-init.sh    # the generated shell integration script

    :: text ::

    Configuration handlers (symlink, shell, path) fill their subdirectories with symlinks that point back to source files. Code-execution handlers (install, homebrew) fill theirs with _sentinels_ — empty marker files that record "this has already run, don't run it again."

    The exact API between handlers and the datastore lives in [./../dev/storage.lex]. The shape above is the conceptual picture.

6. State Changes Without dodot

    Because the datastore is legible, you can manipulate it by hand. Some cases where that is actually useful:

    - Removing a single deployment without running `dodot down` on the whole pack — delete the specific symlink.
    - Resetting a provisioning sentinel to make `install.sh` re-run without `--provision-rerun` — delete the sentinel file.
    - Moving a deployment to a different pack — move the directory.
    - Inspecting what the next shell session will source — `ls <datastore>/packs/*/shell/`.

    None of these require dodot to be involved. None of them will confuse dodot next time you run it: on the next `dodot up` or `dodot status`, dodot re-reads the datastore from scratch and produces the correct answer, because the datastore _is_ the answer.

7. What This Costs

    The double-link is not free, and the cost is worth naming.

    - One extra symlink per deployed file. Disk cost is negligible; one more `readlink` hop when resolving paths.
    - Two places where a link can break instead of one. Broken-symlink handling matters more. `dodot status` specifically flags dangling intermediates.
    - A datastore directory that users have to accept exists. It lives at an XDG-standard location, but it is visible, and people occasionally ask what it is.

    In exchange you get: no database, no drift, a clean uninstall, and a state-as-behavior invariant that a conventional symlink tool can't offer. We think that trade is obviously worth it, which is why dodot is built around it.

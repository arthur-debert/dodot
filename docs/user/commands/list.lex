:: verified ::
dodot list

The "what does dodot consider a pack?" command. Walks the dotfiles root and prints the display name of every directory dodot will treat as a pack — nothing more, nothing less. Read-only.

If a pack is missing from `list`, `up` and `status` won't see it either; that's the question `list` is built to answer.

1. When you reach for it

    - You added a new directory to your dotfiles root and want to confirm dodot is picking it up.
    - You expected a pack to be visible and it isn't — `list` tells you whether the issue is at discovery (`.dodotignore`, `[pack] ignore`, malformed ordering prefix) or somewhere later in the dispatch.
    - You want a quick reminder of what's around without the per-file detail of `dodot status`.

2. What counts as a pack

    A directory under your dotfiles root is included in `list` when:

    - It's a regular directory (not a symlink, not a file).
    - It does not contain a `.dodotignore` marker file.
    - Its name doesn't match a glob in `[pack] ignore` (defaults: `.git`, `.svn`, `.hg`, `node_modules`, `.DS_Store`, `*.swp`, `*~`, `#*#`, `.env*`, `.terraform`).
    - If it carries an ordering prefix (`010-foo` / `020_foo`), the prefix grammar parses cleanly. A directory whose name is *just* a prefix with nothing after the separator (`010-`, `020_`) is rejected as a malformed pack.

    What you see is the *display* name, not the on-disk directory name. A directory `010-nvim/` shows up as `nvim`. See [./../handlers/execution-order.lex] for the prefix grammar.

3. Examples

        dodot list                     # every visible pack name

    :: shell ::

4. Watch out for

    - *Discovery only.* `list` doesn't show files inside packs, doesn't inspect `.dodot.toml`, doesn't render previews. For "what would `up` do?", reach for `dodot status`.
    - *`.dodotignore`'d packs are invisible here.* If a pack you expect to see is missing, check whether someone (you?) dropped a `.dodotignore` into it. See [./addignore.lex] for the command that adds the marker, and [./../handlers/controlling-activation.lex] for the broader filter story.
    - *Ordering prefixes are stripped in the output.* If you want to confirm the on-disk name (e.g. to remember whether you used `010-foo` or `010_foo`), `ls ~/dotfiles/` is the more direct check.

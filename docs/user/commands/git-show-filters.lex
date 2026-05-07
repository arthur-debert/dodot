:: verified ::
dodot git-show-filters

The "show me the plist filter config without writing it" command. Prints the two snippets that make plist clean/smudge work — the `.git/config` block and the `.gitattributes` line — each annotated with whether it is currently in place.

Read-only. If you want to install the `.git/config` block automatically, [./git-install-filters.lex] is the writer.

1. When you reach for it

    - You want to see what `dodot git-install-filters` would do without committing to running it.
    - You'd rather hand-edit `.git/config` and `.gitattributes` instead of letting dodot write them.
    - You're auditing a teammate's setup or troubleshooting a "git diff shows binary garbage" surprise — `git-show-filters` tells you what's installed and what isn't.
    - You're piping the output to a teammate or a setup script: `dodot git-show-filters | tee /tmp/snippet`.

2. What it prints

    Two snippets, in order:

    - The `[filter "dodot-plist"]` block for `.git/config` (per-clone, per-machine).
    - The `*.plist filter=dodot-plist` line for `.gitattributes` (committed with the repo).

    Each snippet carries an annotation showing its current state — already in place, missing, or in place with content that diverges from what dodot would write.

3. Examples

        dodot git-show-filters                          # inspect
        dodot git-show-filters | tee /tmp/snippet       # capture for hand-install elsewhere

    :: shell ::

4. Watch out for

    - *Read-only.* This command never writes. To install automatically, run `dodot git-install-filters`. To install by hand, copy the printed `.git/config` block into your repo's `.git/config` and append the `.gitattributes` line.
    - *Plist-only.* Like its installer counterpart, this command shows *only* the plist filter — not the template filter, not the pre-commit hook, not the shell alias. See [./git-augmentation.lex] for the full set of git wiring.

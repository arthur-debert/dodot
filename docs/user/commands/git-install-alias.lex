:: verified ::
dodot git-install-alias

The "install the Tier-2 shell alias" command. Writes one line to your shell rc that wraps `git` in `dodot refresh --quiet` so `git status`, `git diff`, `git stash`, and friends always see deployed-side template edits without you having to remember.

The alias is intentionally *not* part of the install ladder during `dodot up` — it's an opinionated workflow choice that ships separately. See [./git-augmentation.lex] for how this fits into the broader git-augmentation picture.

1. When you reach for it

    - You're using templates and you're tired of running `dodot refresh` by hand before `git status` shows the truth.
    - The pre-commit hook covers the commit case authoritatively, but you want `git status` / `git diff` to also reflect deployed-side edits during your day.
    - You're setting up a new machine and want the workflow defaults that match how the dotfiles author uses dodot day-to-day.

2. What it writes

    Adds this line to your shell rc:

        alias git='dodot refresh --quiet && command git'

    :: shell ::

    The rc file is `~/.bashrc` or `~/.zshrc`, picked from `$SHELL` by default. Pass `--shell <name>` to target explicitly. The write is idempotent and additive — re-running won't duplicate the line, and existing rc content is preserved.

3. Flags

    Flags:
        | Flag                  | Effect                                                              |
        | `--shell <SHELL>`     | Target shell (`bash`, `zsh`). Auto-detected from `$SHELL` by default. |

    :: table align=ll ::

4. Examples

        dodot git-install-alias                # auto-detect shell
        dodot git-install-alias --shell zsh    # write to ~/.zshrc explicitly
        dodot git-show-alias                   # see the line without writing

    :: shell ::

5. Watch out for

    - *Open shells lag.* The alias takes effect in *new* shell sessions. Already-open shells need to re-source the rc (or open a new shell) to pick up the alias.
    - *The alias wraps `git`, not `git foo`.* All git invocations (`status`, `diff`, `commit`, `log`, `stash`, `branch`, …) run `dodot refresh --quiet` first. That's the point — but it means every git command in this shell pays the refresh cost. The cost is small (refresh is a no-op when nothing diverges), but if you script around git in a hot loop and want to bypass, prefix with `command git` to skip the alias.
    - *No corresponding `un-install-alias`.* To remove, edit your rc by hand. The alias is one line, with comment markers around it; `dodot git-show-alias` prints the same line so you know exactly what to delete.
    - *Tier-2 by name, optional by intent.* You don't have to install it. The pre-commit hook (`dodot transform install-hook`) is the authoritative protection at commit time; the alias just makes day-to-day `git` calls reflect current truth.

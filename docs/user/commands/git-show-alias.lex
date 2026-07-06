:: verified ::
dodot git-show-alias

The "show me the Tier-2 shell alias without writing it" command. Prints the line that `dodot git-install-alias` would write, plus a hint pointing at the rc file you'd put it in. Read-only.

See [./git-augmentation.lex] for how the alias fits into the broader git-augmentation picture, and [./git-install-alias.lex] for the writer that installs it for you.

1. When you reach for it

    - You'd rather hand-edit your rc than let dodot write to it.
    - You want to see what the alias actually does before committing to it.
    - You're capturing the alias for a teammate, a dotfiles README, or a setup script.

2. What it prints

    The alias line, plus the suggested rc target:

        alias git='dodot refresh --quiet && command git'
        # then run `source ~/.zshrc` (or whichever rc) or open a new shell

    :: shell ::

    Pass `--shell <name>` to target a specific shell; the printed rc-file hint changes accordingly.

3. Flags

    Flags:
        | Flag                  | Effect                                                              |
        | `--shell <SHELL>`     | Target shell (`bash`, `zsh`). Auto-detected from `$SHELL` by default. |

    :: table align=ll ::

4. Examples

        dodot git-show-alias                   # auto-detect shell
        dodot git-show-alias --shell bash      # show the bash-targeted form
        dodot git-show-alias | grep alias      # capture just the line

    :: shell ::

5. Watch out for

    - *Read-only.* No rc files are touched. To install, run [./git-install-alias.lex] or copy the printed line into your rc by hand and re-source.

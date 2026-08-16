:: verified ::
dodot install

Wires dodot into your shell startup, and then checks its own work. `dodot up` proves your packs are deployed; it proves nothing about whether any shell ever *loads* them. That takes one line in your shell's startup file, and this command owns that line.

Run it bare and it writes nothing — it reports which shell you use, which file it would write to, whether the hook is already there, and the exact line:

    dodot install

:: shell ::

Run it with `--write` and it splices its own marked block into that file, then starts a shell to find out whether the hook actually fires — so you end on a measured answer rather than a promise:

    dodot install --write

:: shell ::

:: note :: Not to be confused with the `install` *handler*, which runs a pack's `install.sh` once. See [./../handlers/install.lex].

1. When you reach for it

    - Setting up dodot on a new machine, right after your first `dodot up`.
    - `dodot up` or `dodot status` told you no shell has loaded dodot yet.
    - Your aliases and `$PATH` additions stopped appearing in new terminals and you want to know whether the hookup is the reason.
    - You moved your data directory (or your dotfiles repo) and want the rc line pointed at the new location.

2. What the bare command reports

    Nothing is written. dodot prints the decision it would make, so you can check it before delegating it:

        $ dodot install

        dodot install — nothing written yet. Re-run with --write to apply.

        Shell
          zsh
        Startup file
          ~/.zshrc (does not exist yet; --write creates it)
        Hook
          not found in that file
          [ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ] && . "$HOME/.local/share/dodot/shell/dodot-init.sh"

    :: shell ::

    The `Hook` line reports one of three states:

    - `not found in that file` — nothing wires dodot up there yet.
    - `present (dodot's managed block)` — dodot's own block is in place; `--write` would leave it alone.
    - `present (wired by hand — dodot leaves it alone)` — an `eval "$(dodot init-sh)"` (or an equivalent source line) is already there. dodot reports it and does not claim it.

    A bare run reads evidence only; it never starts a shell. If a `Shell hookup` verdict appears below the report, it is the same evidence-based state `dodot status` shows — see [#6].

3. `--write` and the marked block

    `--write` writes exactly this block into the target file — appended at the end, or replaced in place when the block is already there:

        # >>> dodot shell hookup >>>
        [ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ] && . "$HOME/.local/share/dodot/shell/dodot-init.sh"
        # <<< dodot shell hookup <<<

    :: shell ::

    Only that block. Everything else in the file is left untouched, and deleting the block is a complete uninstall of the hookup.

    Properties worth knowing:

    - *Idempotent.* An existing block is replaced in place, never duplicated. The report names what happened: `created`, `appended`, `replaced`, or `already has dodot's hook block — nothing to change`.
    - *It sources the generated script directly; it does not `eval` dodot.* No dodot process runs on every shell start, and there is no bootstrap cycle when dodot itself is not on `$PATH` until init has run — which is exactly the case when dodot is installed by a package manager whose `$PATH` entry dodot's own init provides.
    - *The `[ -f … ]` guard* makes a missing or reset datastore a silent no-op instead of a startup error.
    - *The path is resolved at write time.* Re-running `dodot install --write` after moving your data directory rewrites the block with the new path.

4. Which file dodot writes to

    The target is a guess; the verification in [#5] is the truth; the marked block is what makes the guess reversible. Resolution ladder, in order:

    1. The shell, from `$SHELL`. bash and zsh are supported; anything else is refused rather than guessed at (see [#7]).
    2. The canonical per-session rc file for that shell:
        - zsh → `${ZDOTDIR:-$HOME}/.zshrc`. When `ZDOTDIR` is not in the environment, dodot peeks at `~/.zshenv` for an assignment, since that is where it legally gets set.
        - bash → `~/.bashrc`. On macOS, dodot additionally chains `~/.bash_profile` (or `~/.profile`) to `~/.bashrc` when nothing there sources it already, because Terminal opens login shells that never read `~/.bashrc`. The chain is its own marked block, and it is written only when the ladder itself picked `~/.bashrc` — never for a `--rc` override.
    3. A missing file is created. A fresh machine has no `~/.zshrc`, and that is still the right target.
    4. A symlinked rc is written *through*, to the file it resolves to, and dodot says so:

            Notes
              ~/.zshrc → /home/you/dotfiles/zsh/zshrc — writing there

        :: shell ::

        For dotfiles users that is the desired outcome — the hook lands in the repo. Doing it silently is not.

    5. `--rc <file>` overrides the whole ladder, including the shell detection and the macOS bash chain.

5. The verdict: what `--write` measures

    After writing, `dodot install --write` starts your shell — interactive, non-login, stdin from `/dev/null`, output captured, on a five-second timeout — and checks whether the generated init script actually ran in it. It announces itself while doing so:

        $ dodot install --write

        verifying shell integration (zsh)…
        Wired dodot into ~/.zshrc (created).

        Shell
          zsh
        Startup file
          ~/.zshrc
        Hook
          present (dodot's managed block)
          [ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ] && . "$HOME/.local/share/dodot/shell/dodot-init.sh"
        Notes
          created ~/.zshrc

        ✓ Shell hookup: dodot is sourced in new shells.
        Last loaded just now by dodot 5.6.0.

    :: shell ::

    Three outcomes, kept apart because they call for different actions:

    Verdicts:
        | Outcome         | What it means                                              |
        | verified        | The shell started and the init script ran. You are done.   |
        | verified broken | The shell started and the init script did not run. dodot names why — see the diagnoses below. |
        | couldn't verify | The shell could not be started, or it timed out. dodot falls back to reporting what is *configured*, saying so in as many words: `dodot could not verify by running your shell — … — so this reports your configuration, not measured activation.` It never wedges on this. |

    :: table align=ll ::

    A broken verdict always carries a diagnosis, because "it didn't work" is not actionable on its own:

    - *The hook is missing from the file.* `The dodot hook is missing from ~/.zshrc — run dodot install --write to add it.`
    - *The hook is there but nothing reached it.* `The dodot hook is in ~/.zshrc but was never reached — something earlier in that file is failing before it.` This is the case a static scan of your rc would call fine.
    - *An older init script was loaded.* `Your shell sourced an older init script (generation …, current is …) — check for a second dodot hook or a stale copy.`
    - *dodot can't name the file.* `dodot could not tell which rc file your shell reads. Add this line to it: …`

    Only `--write` measures. A bare `dodot install` reports evidence it can read for free. If no packs have been deployed yet there is no init script to load, so `--write` wires the hook and reports evidence instead of measuring — your next `dodot up` is what gives the hook something to source.

6. Activation states

    `dodot up`, `dodot down`, and `dodot status` end on the same activation footer this command does. Every state and its exact message — including version skew — are documented once, in [./../shell-integration.lex] §5.

7. Shells dodot will not guess for

    Only bash and zsh have an rc ladder. For anything else — plain `sh`, fish, an unset `$SHELL` — dodot refuses to guess and exits non-zero with the line you need:

        $ SHELL=/bin/sh dodot install

        Error: dodot can wire up bash and zsh; it will not guess an rc file for your shell (/bin/sh). Add this line to whatever file your shell reads at startup: [ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ] && . "$HOME/.local/share/dodot/shell/dodot-init.sh" — or point dodot at it with `dodot install --write --rc <file>`.

    :: shell ::

    Either paste the line yourself, or name the file with `--rc` and let dodot own the block there. fish is not supported: dodot does not generate fish-syntax init, so there is nothing for a fish hookup to source.

8. Examples

        # What would happen, and where. Writes nothing.
        dodot install

        # Do it, then verify it.
        dodot install --write

        # Somewhere other than the file dodot picks.
        dodot install --write --rc ~/.config/sh/rc

        # Re-point the block after moving your data directory.
        dodot install --write

    :: shell ::

9. Watch out for

    - *Bare is dry, always.* dodot never modifies shell startup unasked. If you did not type `--write`, nothing on disk changed.
    - *An existing manual hook is not replaced — but it is not detected as a conflict either.* If your rc already has `eval "$(dodot init-sh)"`, the bare command tells you so; running `--write` anyway adds dodot's block *in addition*, and the init script gets sourced twice. Skip `--write`, or remove your hand-written line first.
    - *`--rc` turns off the ladder entirely.* That includes the macOS `~/.bash_profile` chain. If you override to a file your login shells never read, the hook will not fire — which is precisely what the verification will tell you.
    - *A verified hookup is not a deployed pack.* `install` wires the loading mechanism; `dodot up` is what puts anything in it.
    - *Open shells lag.* The hook affects shells started after it. Your current terminal is unchanged until you open a new one.

10. See also

    - [./../shell-integration.lex] — the whole hookup story, including the activation states and the manual alternative.
    - [./init-sh.lex] — the script the hook sources; wire it by hand if you prefer.
    - [./status.lex] — reports whether shells are loading dodot, without ever starting one.
    - [./up.lex] — where the "no shell has loaded dodot yet" warning first fires.

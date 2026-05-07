:: verified ::
dodot prompts

The "inspect and reset dismissed-prompt state" command family. dodot occasionally surfaces one-time prompts — onboarding hints, install offers, configuration nudges — and remembers your dismissals so the same prompt doesn't fire repeatedly. Two subcommands:

- `dodot prompts list` — show every prompt and its current state.
- `dodot prompts reset <key>` (or `--all`) — clear a dismissal so the prompt re-evaluates next time its trigger fires.

The most common reason you'd reach for this: bringing back the install ladder you said `No` to last time. See [./git-augmentation.lex] §6.

1. When you reach for it

    - You said `No` to the post-`up` install ladder and now want it back.
    - You want to know what one-time nudges dodot might still surface, and on what conditions.
    - You're cleaning up an old dotfiles state and want every dismissal cleared.

2. prompts list

    Shows every known prompt with its state (`active` or `dismissed`) and a one-line description. Stale dismissals from older dodot versions appear too, so you can clear them.

    Examples:

        dodot prompts list

    :: shell ::

3. prompts reset

    Clears a dismissal so the prompt's caller will re-evaluate next time. It does *not* trigger the prompt itself — only flags it for re-evaluation. The next condition that would normally surface the prompt (e.g. the next `dodot up` for the install ladder) is what fires it again.

    Two forms:

    - `dodot prompts reset <key>` — clear one dismissal by key.
    - `dodot prompts reset --all` — clear every dismissal.

    Common keys (see `dodot prompts list` for the live set):

        | Key                          | What gets re-offered                                  |
        | `magic.install_ladder`       | The full install ladder during `dodot up`.            |
        | `plist.install_filters`      | The plist-filter rung of the install ladder.          |
        | `template.install_filter`    | The template-filter rung of the install ladder.       |
        | `template.install_hook`      | The pre-commit hook rung of the install ladder.       |

    :: table align=ll ::

    Examples:

        dodot prompts reset magic.install_ladder        # bring back the whole ladder
        dodot prompts reset plist.install_filters       # bring back just the plist rung
        dodot prompts reset --all                       # clear every dismissal

    :: shell ::

4. Registry location

    State lives in `<XDG_DATA_HOME>/dodot/prompts.json` (typically `~/.local/share/dodot/prompts.json`). Removing the file resets every dismissal — equivalent to `prompts reset --all`. Useful as a one-step "I'm starting fresh" if you'd rather edit on disk.

5. Watch out for

    - *Reset doesn't trigger the prompt.* It clears the dismissal so the *next* time the prompt's condition is met, the prompt fires. For the install ladder, that means the next `dodot up` after a reset will re-offer whichever rungs apply.
    - *Stale keys are a feature, not a bug.* If a prompt was renamed or removed in a newer dodot version, its old dismissal lingers in `prompts.json`. `prompts list` shows it so you can clear it; the runtime ignores it harmlessly.
    - *No "dismiss this for me" command.* You only dismiss prompts by answering them in the actual interactive prompt, or by editing `prompts.json` by hand. There's no `dodot prompts dismiss` — the registry is for clearing answers, not pre-answering.

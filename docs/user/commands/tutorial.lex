:: verified ::
dodot tutorial

The "guide me through using dodot on my real dotfiles" command. An interactive walkthrough that takes one of your packs from "discovered" to "live on this machine," explaining what dodot is doing as it goes. About ten minutes, no toy examples — uses your actual repo, and changes nothing without your explicit yes at every step.

The recommended starting point if you've never run `dodot up` on this repo before.

1. When you reach for it

    - First time using dodot on a particular machine or repo: `dodot tutorial` once and you'll have one pack deployed plus the mental model.
    - You're showing dodot to a teammate and want a guided demo using their actual files.
    - You walked through it before, got partway, hit Ctrl-C — re-running offers to resume.

2. What it does

    The tutorial walks twelve steps end-to-end:

    1. Locate your dotfiles repo (env var, git toplevel, or cwd).
    2. List the packs dodot can see, or offer help if there are none.
    3. Pick a pack to start with — preferably one with just config files, no install scripts.
    4. Show its `dodot status` output, line by line, explaining what each row means.
    5. Walk through the deploy targets and the shell-integration step.
    6. Run `dodot up --dry-run`. If you say go, run it for real.
    7. Confirm the deployment landed.

    Saved position: if you Ctrl-C partway through, your place is kept. The next `dodot tutorial` offers to resume from where you left off, or you can `--reset` to start over.

3. Flags

    Flags:
        | Flag             | Effect                                                                            |
        | `--reset`        | Discard saved tutorial state and start over.                                      |
        | `--from <STEP>`  | Jump to a specific step (e.g. `intro`, `pick_pack`, `dry_run`, `real_up`).        |

    :: table align=ll ::

    Step IDs accepted by `--from`: `intro`, `check_root`, `list_packs`, `no_packs`, `pick_pack`, `show_status`, `annotate_status`, `concept_targets`, `concept_shell`, `dry_run`, `real_up`, `outro`.

4. Examples

        dodot tutorial                 # start, or resume if interrupted
        dodot tutorial --reset         # discard saved progress and start fresh
        dodot tutorial --from dry_run  # jump to the dry-run step

    :: shell ::

5. Watch out for

    - *No silent changes.* The tutorial pauses for an explicit `y/n` before the first filesystem-changing step (`dodot up`). Cancelling at that point exits cleanly with no deployment.
    - *`--from real_up` skips the dry-run step.* Useful when resuming or when scripted, but it means you don't get the preview pass — the next prompt is the real `up`. Use `--from dry_run` if you want both.
    - *Tutorial uses one of YOUR packs.* It doesn't create scratch directories or fake examples. If the pack you pick has a dodgy `install.sh`, the tutorial will run it (with your consent). Pick a config-only pack for the first time through if you want minimum risk.
    - *Resume requires the same dotfiles root.* Saved tutorial state is keyed to where it ran. Switching to a different repo or rebuilding it means a fresh start.

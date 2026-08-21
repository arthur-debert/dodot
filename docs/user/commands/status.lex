:: verified ::
dodot status

The read-only "what does dodot see?" command. For every pack and every source file inside, shows which handler claimed it, where it would deploy, and whether the deployed state is pending, live, or in error. `status` never changes anything on disk — safe to run any time.

1. When you reach for it

    - Before `dodot up`: sanity-check that the conventions dodot detected match what you expected. If a source file is going to the wrong handler or the wrong target, fix it now.
    - After `dodot up`: confirm everything landed. Any `error` row is a deployment that didn't take.
    - Diagnosing a "this isn't working" moment: `dodot status` shows the dispatch path, so you can see whether the source file is even being claimed.
    - Sharing a snapshot: pair with `--output json` (or `yaml`) to capture the same view in machine-readable form.

2. What it shows

    For each active pack, `status` shows these rows.

    - Every source file dodot saw, as separate icon, pack, filename, and status columns. The icon and filename are muted, the pack is regular, and only the status carries its severity colour.
    - Files the `skip` and `gate` handlers dropped, and why. Files the `ignore` handler claimed get no row at all — that drop is silent by design, the same contract as `.gitignore`.
    - Files affected by preprocessing — under their *post-preprocessing* filename, not the source filename. (A source `config.toml.tmpl` shows as `config.toml`.)

    Across packs, three more things surface.

    - Cross-pack conflicts surface as warnings on the affected rows, with both packs named so the conflict is visible without having to run `up`.
    - Packs whose `[pack] os` doesn't match the current host show in a separate "inactive on this OS" section.
    - Packs carrying `.dodotignore` follow the active rows after a blank line. They use `∅` as the icon, `.dodotignore` as the filename, and `ignored` as the status; there is no separate heading.

    Every handler words its own status column. The three-state shape below is what you'll see day to day:

    Status states by handler:

        | Handler    | Not deployed                  | Deployed                | Third state                                    |
        | `symlink`  | `pending`                     | `linked`                | `broken: …` when the link chain doesn't verify |
        | `shell`    | `not sourced`                 | `sourced`               | `syntax error`, or `exited N`                  |
        | `path`     | `not in PATH`                 | `in PATH`               | `broken: …` when the link chain doesn't verify |
        | `install`  | `never run`                   | `installed`             | `older version`                                |
        | `homebrew` | `brew packages not installed` | `installed`             | `brew packages older version`                  |
        | `nix`      | `nix packages not installed`  | `nix packages installed`| `nix packages older version`                   |
        | `external` | `pending`                     | `deployed`              | —                                              |
        | `skip`     | —                             | —                       | `skipped`                                      |
        | `gate`     | —                             | —                       | `gated out (<label>)`                          |
        | `ignore`   | —                             | —                       | no row — the drop is silent                    |

    :: table align=llll ::

    Reading the third column:

    - *`older version` is not an error.* It is the third normal state of the three run-once handlers (`install`, `homebrew`, `nix`): a recorded run exists, but the file's content has changed since. dodot does not re-run code you edited on its own — it reports the drift and holds. The label carries a line summary (`older version (3 lines added, 1 removed)`), and a warning footnote names the remedy: `dodot up --provision-rerun` applies the current content. `dodot status --diff` prints the full diff between what ran and what's on disk. A sentinel written before dodot started keeping snapshots reads `older version (no diff data)`.
    - *A `symlink` or `path` row goes `broken:` when the chain doesn't verify* — a missing source file, a data link that isn't a symlink, a link pointing somewhere else. A row can also come back *stale* (the link is healthy but current config would put it elsewhere, so a re-deploy moves it), or, when a file that isn't dodot's already occupies the target path, as a warning with the reason in a footnote — that's the one `dodot up --force` exists for.
    - *`shell` rows can carry runtime evidence.* `syntax error` means `dodot up`'s pre-flight recorded a parse failure in the file; `exited N` means the newest shell-init run that sourced it exited non-zero. The `exited N` verdict only appears with `[profiling]` enabled, since nothing else observes those runs.

    Three states cut across handlers rather than belonging to one:

    - `skipped (--no-provision)` on any file claimed by `install`, `homebrew`, `nix`, or `external` when that flag was passed to `up` — the run reports the choice you made instead of the datastore state it never consulted.
    - `homebrew not installed` / `nix not installed` (skipped style) when the package manager isn't on this machine and the file has never run. dodot writes no sentinel and reports no error; the rest of the pack deploys normally. The warning footnote names every location it probed, in probe order, and what installing the manager would change: "homebrew is not installed — probed /opt/homebrew/bin/brew, …, /usr/local/bin/brew. Nothing was recorded, so installing homebrew (https://brew.sh) and re-running `dodot up` runs this file." The real footnote spells the paths out in full; see [./../handlers/homebrew.lex] and [./../handlers/nix.lex] for each manager's list.
    - `cannot probe homebrew` / `cannot probe nix` (broken style) when a candidate path could not be examined at all. That one is an error footnote: dodot could not answer the question it was asked.

    A receipt outlives the manager. If a `Brewfile` ran and brew has since been removed, the row stays `installed` — the file *did* run, and brew leaving afterwards doesn't undo it — with a warning footnote saying so: "homebrew is not installed (probed …) — the receipt stands: Brewfile ran when it was". If that same file is also `older version`, the footnote adds that the re-run will skip until the manager is back.

    `install` never reports an absent manager: it runs a script through `bash` or `zsh`, not a package manager, so there is nothing to probe for.

    Below the rows, `status` reports whether shells are actually loading dodot — see [#3].

3. Shell hookup

    Deploying a pack and a shell loading it are two different things, and `status` reports both. After the per-pack rows it ends with a two-line *activation footer*: whether dodot is sourced in new shell sessions, and when it last was, by which dodot. The healthy case:

        ✓ Shell hookup: dodot is sourced in new shells.
        Last loaded 4 minutes ago by dodot 5.6.0.

    :: shell ::

    When something needs doing — no shell has loaded dodot yet, this shell predates your last `up`, your shells load a *different* dodot than the one you are running — line one names the state and a hint carries the fix. Every state and its exact message are documented once, in [./../shell-integration.lex] §5.

    `status` reads evidence the init script leaves behind — a version-stamped generation in your environment, a heartbeat file, and, when you run it from a terminal, the fact that the shell in front of you exported no stamp — and never starts a shell to find out. That keeps it as passive as the rest of the command. The consequence is that `status` can report every evidence-based state, including version skew, but it can never report a fresh measured verdict. `dodot up` and `dodot install --write` can attach that verdict to their activation footer; bare `dodot probe shell-init` performs the same targeted check in its own `Hook verification` section. For the deepest cut — which binary `dodot` resolves to at the hook line — run `dodot probe shell-init --trace-hook` ([./probe.lex] §4).

    Before anything is deployed there is no init script to load, so `status` says nothing about the hookup at all.

4. Display options

    Display options:
        | Flag            | Effect                                                                                 |
        | `--full`        | Show every file per pack (the default).                                                |
        | `--short`       | Collapse each pack to a one-line summary.                                              |
        | `--by-name`     | List packs in discovery order (the default).                                           |
        | `--by-status`   | Group packs by aggregated status: deployed / pending / error.                           |
        | `--diff`        | For run-once files reporting `older version`, print a unified diff of what ran vs. what's on disk. |
        | `--check-drift` | Hash the deployed externals and report divergence. Opt-in because it can be slow.       |

    :: table align=ll ::

    File-column icons:

    - `➞` symlink
    - `⚙` shell source, `Brewfile`, or `packages.nix` — the three handlers that share it are told apart by the filename and the status text
    - `+` added to `$PATH`
    - `×` install script
    - `↓` external (fetched into place from `externals.toml`)
    - `·` filtered out by `skip` or `gate`
    - `∅` a pack carrying `.dodotignore`

    A file the `ignore` handler claimed has no icon because it has no row.

    Full rows expand to the detected terminal width. The pack column is padded so filenames align, status is right-aligned to the terminal edge, and long filenames are clipped in the middle with an ellipsis. When terminal width cannot be detected, dodot uses 80 columns.

5. Examples

        # Daily drivers
        dodot status                   # everything
        dodot status git               # one pack
        dodot status git nvim          # several

        # Different views
        dodot status --short           # one line per pack
        dodot status --by-status       # group by deployed / pending / error

        # Machine-readable
        dodot status --output json | jq '.packs[] | select(.error_count > 0)'

    :: shell ::

6. Watch out for

    - *A hookup that broke reads as stale, not broken.* `status` only has evidence to work with: if a shell once loaded dodot and the hookup has since been broken, the heartbeat is still on disk and `status` reports _stale shell_. Opening a new shell won't help, and `dodot up` or `dodot install --write` is what will actually measure it and say so.
    - *Status is Passive.* It never calls secret providers, never renders templates against live secrets, never writes to the datastore. A row showing as `pending` because its preprocessor wasn't evaluated is *expected* — actual evaluation happens during `dodot up`. This also means `status` is safe to run when your secret backend is offline or locked.
    - *Conflicts are warnings, not errors.* A cross-pack conflict in `status` is a heads-up; `up` is what halts. So a clean `status` is reassuring; a conflict in `status` means `up` will fail until you resolve it.
    - *Status reflects the current host.* Gated rows depend on host facts (OS, arch, hostname). Running `status` on macOS and on Linux can show different rows for the same pack — that's the gate machinery working as intended.
    - *Look for post-preprocessing names.* If you're hunting for `config.toml.tmpl` and don't see it in the listing, look for `config.toml` — `status` shows what your apps will actually read on disk, not the source filename.

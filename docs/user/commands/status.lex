:: verified ::
dodot status

The read-only "what does dodot see?" command. For every pack and every source file inside, shows which handler claimed it, where it would deploy, and whether the deployed state is pending, live, or in error. `status` never changes anything on disk — safe to run any time.

1. When you reach for it

    - Before `dodot up`: sanity-check that the conventions dodot detected match what you expected. If a source file is going to the wrong handler or the wrong target, fix it now.
    - After `dodot up`: confirm everything landed. Any `error` row is a deployment that didn't take.
    - Diagnosing a "this isn't working" moment: `dodot status` shows the dispatch path, so you can see whether the source file is even being claimed.
    - Sharing a snapshot: pair with `--output json` (or `yaml`) to capture the same view in machine-readable form.

2. What it shows

    For each active pack:

    - Every source file dodot saw, as separate icon, pack, filename, and status columns. The icon and filename are muted, the pack is regular, and only the status carries its severity colour.
    - Files filtered out (`ignore` / `skip` / `gate`) and why they were filtered.
    - Files affected by preprocessing — under their *post-preprocessing* filename, not the source filename. (A source `config.toml.tmpl` shows as `config.toml`.)

    Across packs:

    - Cross-pack conflicts surface as warnings on the affected rows, with both packs named so the conflict is visible without having to run `up`.
    - Packs whose `[pack] os` doesn't match the current host show in a separate "inactive on this OS" section.
    - Packs carrying `.dodotignore` follow the active rows after a blank line. They use `∅` as the icon, `.dodotignore` as the filename, and `ignored` as the status; there is no separate heading.

    Status states for a single row, by handler family:

        | Handler family    | Pending                                  | Deployed            | Error                                                |
        | symlink           | pending                                  | linked              | target exists but isn't dodot's, or the link is broken |
        | shell / path      | not sourced / not in PATH                | sourced / in PATH   | (rare — registration writes are atomic)              |
        | install / homebrew| brew packages not installed              | ran / installed     | sentinel exists but its checksum no longer matches   |

    :: table align=llll ::

    Below the rows, `status` reports whether shells are actually loading dodot — see [#3].

3. Shell hookup

    Deploying a pack and a shell loading it are two different things, and `status` reports both. After the per-pack rows it prints one line about *activation*: whether any shell has sourced dodot's init script.

        shell hookup: ok

    :: shell ::

    That is the healthy case. The others:

        ⚠ Deployed, but no shell has loaded dodot yet.
        Run `dodot install --write` to wire it up, or add this to your shell rc file yourself: [ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ] && . "$HOME/.local/share/dodot/shell/dodot-init.sh"

    :: shell ::

        This shell started before your last `dodot up`.
        Open a new shell to pick up the current deployment.

    :: shell ::

    `status` reads evidence the init script leaves behind — a generation stamp in your environment, and a heartbeat file — and never starts a shell to find out. That keeps it as passive as the rest of the command. The consequence is that `status` can say "no shell has loaded dodot yet" or "this shell is stale", but it can never report the *measured* verdict: only `dodot up` and `dodot install --write` start a shell, and only they can tell you a hookup is verified broken.

    The four activation states and what each means are documented once, in [./../shell-integration.lex] §5.

    Before anything is deployed there is no init script to load, so `status` says nothing about the hookup at all.

4. Display options

    Display options:
        | Flag           | Effect                                                                 |
        | `--full`       | Show every file per pack (the default).                                |
        | `--short`      | Collapse each pack to a one-line summary.                              |
        | `--by-name`    | List packs in discovery order (the default).                           |
        | `--by-status`  | Group packs by aggregated status: deployed / pending / error.          |

    :: table align=ll ::

    File-column icons:

    - `➞` symlink
    - `⚙` shell source / homebrew
    - `+` added to `$PATH`
    - `×` install script

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

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

    - Every source file dodot saw, with the handler symbol, the deploy target, and the current deployment status.
    - Files filtered out (`ignore` / `skip` / `gate`) and why they were filtered.
    - Files affected by preprocessing — under their *post-preprocessing* filename, not the source filename. (A source `config.toml.tmpl` shows as `config.toml`.)

    Across packs:

    - Cross-pack conflicts surface as warnings on the affected rows, with both packs named so the conflict is visible without having to run `up`.
    - Packs whose `[pack] os` doesn't match the current host show in a separate "inactive on this OS" section.

    Status states for a single row, by handler family:

        | Handler family    | Pending             | Deployed                          | Error                                                |
        | symlink           | not yet linked      | linked at the deploy target       | target exists but isn't dodot's, or the link is broken |
        | shell / path      | not registered      | registered in shell-init / PATH   | (rare — registration writes are atomic)              |
        | install / homebrew| sentinel missing    | sentinel matches current source   | sentinel exists but its checksum no longer matches   |

    :: table align=llll ::

3. Display options

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

4. Examples

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

5. Watch out for

    - *Status is Passive.* It never calls secret providers, never renders templates against live secrets, never writes to the datastore. A row showing as `pending` because its preprocessor wasn't evaluated is *expected* — actual evaluation happens during `dodot up`. This also means `status` is safe to run when your secret backend is offline or locked.
    - *Conflicts are warnings, not errors.* A cross-pack conflict in `status` is a heads-up; `up` is what halts. So a clean `status` is reassuring; a conflict in `status` means `up` will fail until you resolve it.
    - *Status reflects the current host.* Gated rows depend on host facts (OS, arch, hostname). Running `status` on macOS and on Linux can show different rows for the same pack — that's the gate machinery working as intended.
    - *Look for post-preprocessing names.* If you're hunting for `config.toml.tmpl` and don't see it in the listing, look for `config.toml` — `status` shows what your apps will actually read on disk, not the source filename.

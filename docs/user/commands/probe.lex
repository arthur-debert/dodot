:: verified ::
dodot probe

The "lower-level introspection" command family. Read-only. Where `dodot status` shows you per-pack deployment, `probe` shows you what dodot wrote on disk, what the running shell init looks like in practice, and what the system around your packs (macOS apps, brew casks) actually has.

Reach for `probe` when `status` isn't enough — when something appears deployed but isn't behaving, when shell startup feels slow, or when you want to see exactly what dodot wrote where.

1. Subcommands at a glance

    Subcommands:
        | Subcommand          | Answer it gives                                                                |
        | `deployment-map`    | Every dodot-owned symlink: where it lives, what it points back to.            |
        | `show-data-dir`     | Tree view of dodot's data directory (`$XDG_DATA_HOME/dodot`).                 |
        | `shell-init`        | Per-source timings + exit codes from your most recent shell startup.          |
        | `app` (macOS)       | App-support routing for a pack: folders, casks, bundles.                       |

    :: table align=ll ::

2. probe deployment-map

    Lists every symlink dodot has created — one row per link, source first, deployed target second. The simplest answer to "is this file actually a dodot symlink, and what does it point at?"

    Useful when:

    - A deployed config isn't behaving as expected and you want to confirm what's symlinked to what.
    - You're debugging a routing surprise (`home.X` vs `_home/X/` vs default XDG) and want to see the resulting target without running `up`.
    - You want a flat machine-readable view of every link dodot owns: pipe through `grep`, `awk`, or pair with `--output json`.

    Examples:

        dodot probe deployment-map
        dodot probe deployment-map | grep nvim   # filter to one pack

    :: shell ::

3. probe show-data-dir

    Renders a `tree`-style view of dodot's data directory (`$XDG_DATA_HOME/dodot`, typically `~/.local/share/dodot`). This is where dodot stages files for shell init, records install / Brewfile sentinels, and keeps deployment metadata.

    Useful when you want to see dodot's bookkeeping without poking at the directory by hand. The output is organised by pack and handler, so a row like `packs/nvim/install/install.sh-a1b2c3d4e5f6a7b8` immediately tells you "the nvim pack's install handler has a sentinel for that content hash."

    Flags:

        | Flag         | Effect                                          |
        | `--depth N`  | Maximum tree depth (default 4).                 |

    :: table align=ll ::

    Examples:

        dodot probe show-data-dir
        dodot probe show-data-dir --depth 2     # shallow tree
        dodot probe show-data-dir --depth 8     # everything

    :: shell ::

4. probe shell-init

    The shell init script (the one `eval "$(dodot init-sh)"` produces) optionally records, for every source it runs, how long the source took and what exit code resulted. `probe shell-init` is the read side of that data.

    Three views, by flag:

        | View                      | Effect                                                                         |
        | (default)                 | Most recent run, sorted by time. Each source as a row.                         |
        | `--runs [N]`              | Aggregate the last N runs into per-target `p50` / `p95` / `max`. Default N=10. |
        | `--history`               | One summary row per recent run, newest first.                                  |
        | `--errors-only`           | Every target with a non-zero exit across recent runs, sorted by failure count. |
        | `<PACK>` / `<PACK/FILE>`  | Drill into one source — per-run exit codes and stderr.                         |

    :: table align=ll ::

    Useful for: hunting slow shell startup, finding a pack whose `aliases.sh` is failing silently, auditing what a teammate's pack actually does on login.

    Examples:

        dodot probe shell-init                     # most recent run
        dodot probe shell-init --runs              # last 10 runs, p50/p95/max
        dodot probe shell-init --runs 50           # last 50 runs
        dodot probe shell-init --history           # one row per run
        dodot probe shell-init --errors-only       # only failures
        dodot probe shell-init gpg                 # drill into one pack
        dodot probe shell-init gpg/env.sh          # drill into one file

    :: shell ::

5. probe app (macOS)

    Shows the app-support folders a pack will deploy to, whether they exist on disk, the matching homebrew cask (if any), the `.app` bundle dodot found, and its bundle identifier. On macOS the data is enriched via `brew info` and Spotlight (`mdls` / `mdfind`); on other platforms only folder existence is reported.

    Useful when you're working with a GUI-app pack (vscode, Cursor, …) and want to confirm the routing dodot will pick — especially if you're using `[symlink.app_aliases]` to retarget a pack name to a folder name.

    Probes are *advisory*: `dodot up` and `dodot status` may consult cached probe data for warnings or hints, but stale or missing probe data never affects deployment routing or resolver decisions.

    Flags:

        | Flag         | Effect                                                                            |
        | `--refresh`  | Invalidate the brew cache for this pack's tokens before probing (otherwise 24h).  |

    :: table align=ll ::

    Examples:

        dodot probe app vscode             # typical pack with [symlink.app_aliases]
        dodot probe app cursor --refresh   # force a fresh `brew info` lookup

    :: shell ::

6. Watch out for

    - *`probe app` is macOS-acute.* On Linux, `app_support_dir` collapses onto `$XDG_CONFIG_HOME`, the brew/Spotlight enrichment doesn't apply, and the output is correspondingly thinner. The command isn't an error elsewhere; it just has less to say.
    - *`probe shell-init` requires the timing wrapper.* The init script only writes timing data when `[shell_init].profiling.enabled = true` (see `dodot config get shell_init.profiling.enabled`). Without it the command reports "no profiles yet, open a new shell that sources `dodot-init.sh`."
    - *`probe deployment-map` shows only what dodot owns.* Files at the deploy target that dodot didn't create (regular files, foreign symlinks) don't appear here — `dodot status` is where those surface as conflicts or `error` rows.
    - *`show-data-dir --depth 8` can be a lot.* A repo with many packs and many handlers fills the tree quickly. Start at the default depth 4; deepen only when you're hunting a specific path.

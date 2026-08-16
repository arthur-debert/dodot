:: verified ::
dodot probe

The "lower-level introspection" command family. Where `dodot status` shows you per-pack deployment, `probe` shows you what dodot wrote on disk, what the running shell init looks like in practice, and what the system around your packs (macOS apps, brew casks) actually has.

The family is read-only with respect to dodot's state and your files: nothing in the datastore changes, and your rc is never written. One command goes further than reading: `probe shell-init` additionally *executes* your shell rc, once, to measure it — announced before it runs, under a hard timeout, and opt-out (see [#4]). Everything else only reads.

Reach for `probe` when `status` isn't enough — when something appears deployed but isn't behaving, when shell startup feels slow, when the activation footer says your shells load a different dodot, or when you want to see exactly what dodot wrote where.

1. Subcommands at a glance

    Subcommands:
        | Subcommand          | Answer it gives                                                                |
        | `deployment-map`    | Every dodot-owned symlink: where it lives, what it points back to.            |
        | `show-data-dir`     | Tree view of dodot's data directory (`$XDG_DATA_HOME/dodot`).                 |
        | `shell-init`        | Startup timings + a live diagnosis of what `dodot` resolves to at the hook line. |
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

    Two answers in one command: what your shell startup *cost*, and what your hookup actually *does*. The first is read from recorded data; the second is measured live, by default, every time you run the bare command.

    4.1. The hook-line trace (default)

        A hookup can be wired, fire on every shell start, and still be dead — because the `dodot` your rc resolves at the hook line is not the dodot you run at the prompt. No static scan can see that. So the bare `probe shell-init` spawns your shell and runs your whole rc under tracing, announcing itself first:

            tracing shell startup (zsh)… (runs your rc file, up to twice)

        :: text ::

        The spawn is guarded the same way as every dodot shell probe: stdin from `/dev/null`, output captured, a hard timeout with a process-group kill, dodot's own evidence variables scrubbed. It never writes your rc. If your rc defeats the tracing (a `PS4` override, say), dodot retries against a temporary *copy* with a report line inserted — and says so: `(traced via a temporary copy of your rc — your real files were not touched)`. If your rc has no dodot hook at all, nothing is spawned and the report says so:

            no dodot hook in ~/.zshrc — run `dodot install --write` to add one

        :: text ::

        The result is the `Hook resolution` section: the rc file, the hook's line number, and a verdict. For the hand-wired `eval "$(dodot init-sh)"` hook, dodot reads `$PATH` *at the hook line* out of the trace and resolves `dodot` against it itself — naming every candidate it passed over (`passed over /usr/local/bin/dodot — dangling symlink`, `— not executable`, `— its directory does not exist`), which is exactly what `command -v` silently skips. There are four verdicts.

        Hook-line verdicts (eval hook), quoted verbatim:
            | Verdict                                                                        | What to do                                                                          |
            | the hook at <rc>:<line> never ran in a fresh shell                             | The hook sits inside a branch that did not run, or the rc exits or execs away before reaching it. Move it somewhere unconditional. |
            | \`dodot\` is not resolvable at <rc>:<line>                                     | At that point of your rc, nothing on `$PATH` provides dodot. The report prints the PATH it searched and the entries it skipped. |
            | your shells load a different dodot than the one running now                    | The wired-but-dead case: both paths and both versions are printed. Remove or update the stale install your PATH finds first. |
            | \`dodot\` at <rc>:<line> resolves to the running binary — the hookup is sound  | The hookup is fine; whatever you are chasing lives elsewhere.                       |

        :: table align=ll ::

        For the managed file-source hook there is no PATH involved. The question is instead *which dodot wrote the script that line sources* — and both halves of that matter. dodot reads the path off the hook line itself, never from where `dodot up` would have written one, so a hand-wired hook pointing at a copy elsewhere is judged on the copy it actually sources; and it reads that file rather than merely checking it is there, because "a file exists at this path" is a fact about your disk and "the hookup is sound" is a claim about which dodot your shells load. A 5.0.0 init script sitting exactly where the hook says is present, readable, and completely broken.

        File-source verdicts, quoted verbatim:
            | Verdict                                                                        | What to do                                                                          |
            | the init script sourced at <rc>:<line> was written by the running dodot — the hookup is sound | Nothing; whatever you are chasing lives elsewhere.                    |
            | the init script sourced at <rc>:<line> does not exist                          | Run `dodot up` to regenerate it. A directory or a dangling symlink at that path counts as missing — your shell cannot source either. |
            | the init script sourced at <rc>:<line> was written by a different dodot        | The file-source form of version skew. Your hook loads that script, not the one `dodot up` maintains — re-point the hook, or `dodot install --write`. |
            | the init script sourced at <rc>:<line> is older than the one \`dodot up\` maintains | A copy left behind. Same fix: re-point the hook or re-run `dodot install --write`.  |
            | dodot could not tell which dodot wrote the script sourced at <rc>:<line>       | The file is unreadable, or carries no dodot stamp and so is not a script dodot generated. Look at it yourself. |

        :: table align=ll ::

        And a hook line whose path dodot cannot resolve without running a shell — another variable, a command substitution, a relative path — gets an honest non-answer rather than a guess about a file it may not source:

            dodot could not tell which file the hook at <rc>:<line> sources

        :: text ::

        Opting out: `--no-trace` skips the spawn and reports recorded timings only. Every other view — a `<PACK>`/`<PACK/FILE>` filter, `--runs`, `--history`, `--errors-only` — is passive and never traces.

    4.2. Recorded timings

        The shell init script optionally records, for every source it runs, how long the source took and what exit code resulted. The rest of `probe shell-init` is the read side of that data:

            | View                      | Effect                                                                         |
            | (default)                 | Most recent run, sorted by time. Each source as a row.                         |
            | `--runs [N]`              | Aggregate the last N runs into per-target `p50` / `p95` / `max`. Default N=10. |
            | `--history`               | One summary row per recent run, newest first.                                  |
            | `--errors-only`           | Every target with a non-zero exit across recent runs, sorted by failure count. |
            | `<PACK>` / `<PACK/FILE>`  | Drill into one source — per-run exit codes and stderr.                         |

        :: table align=ll ::

        Useful for: hunting slow shell startup, finding a pack whose `aliases.sh` is failing silently, auditing what a teammate's pack actually does on login.

    Examples:

        dodot probe shell-init                     # timings + the live hook trace
        dodot probe shell-init --no-trace          # timings only, nothing spawned
        dodot probe shell-init --runs              # last 10 runs, p50/p95/max
        dodot probe shell-init --runs 50           # last 50 runs
        dodot probe shell-init --history           # one row per run
        dodot probe shell-init --errors-only       # only failures
        dodot probe shell-init gpg                 # drill into one pack (no trace)
        dodot probe shell-init gpg/env.sh          # drill into one file (no trace)

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
    - *`probe shell-init`'s timings require the timing wrapper.* The init script only writes timing data when `[shell_init].profiling.enabled = true` (see `dodot config get shell_init.profiling.enabled`). Without it the command reports "no profiles yet, open a new shell that sources `dodot-init.sh`." The hook-line trace needs no profiling — it measures live.
    - *The default `probe shell-init` runs your rc.* Announced, in a throwaway shell — your rc's side effects (a `neofetch`, a network call) run with it. Up to twice: if the first pass comes back unreadable (a `PS4` override, or macOS's bash 3.2, where it usually does) dodot re-runs it on a copy. `--no-trace` if that matters right now.
    - *`probe deployment-map` shows only what dodot owns.* Files at the deploy target that dodot didn't create (regular files, foreign symlinks) don't appear here — `dodot status` is where those surface as conflicts or `error` rows.
    - *`show-data-dir --depth 8` can be a lot.* A repo with many packs and many handlers fills the tree quickly. Start at the default depth 4; deepen only when you're hunting a specific path.

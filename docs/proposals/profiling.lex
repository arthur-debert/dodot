Design Specification: Profiling and the Probe Command

    This document specifies `dodot probe` — a small introspection surface that lets users see what dodot is doing, where it lives on disk, and how much time it costs their shell to start. It also specifies the shell-side instrumentation that collects the timing data, and the plain-text records dodot persists on its behalf.

    Profiling sits alongside the other introspection-leaning features (status, list) but answers a different question: not "what is deployed?" but "what does the deployment look like at runtime, and what is it costing me?" It is explicitly *not* a performance-tuning framework — it is a small, honest lens.

    The design also produces a deliverable that the sync/refresh work in @./magic.lex needs for its own reasons: a plain-text source-to-deployed map. Packaging that map here keeps the profiling and refresh stories from each growing their own bespoke mini-stores.


1. Motivation

    1.1. The Init Script Is a Small Critical Path

        A dotfile manager runs exactly once per shell — at init — but that one run is inside every subsequent terminal the user will ever open. The current `dodot-init.sh` [../crates/dodot-lib/src/shell/mod.rs] is trivially small: a few `export PATH=...` lines and one `. "<path>"` per sourced file. On its own it costs almost nothing. But the files the user writes inside those sourced scripts can and do become slow. When a new shell starts feeling heavy, the question "which file is to blame?" is answerable today only by reaching for `zprof` or `zsh/xtrace`, neither of which is particularly friendly, and both of which the user has to rig up themselves.

        dodot already owns the script that sources these files. It is the obvious place to carry a lightweight, always-on timer that never bothers the user — until the user asks.

    1.2. Other Things Worth Knowing

        While we are instrumenting the init script, two other questions are cheap to answer once we have the plumbing:

            - Did any sourced file exit non-zero? The shell uses `[ -f ... ] && . "..."` today, which silently swallows failures. Recording the exit status turns hidden breakage into visible data.
            - What deployed file corresponds to which source file? The datastore knows this, but only through symlink chains that are inconvenient to walk by hand. A plain-text map is useful to both the user (for audit) and to dodot itself (for `refresh`, per @./magic.lex §2).

        Neither of these is performance-related, but both share the `probe` framing: they let the user see inside an otherwise opaque deployment.


2. Principles

    2.1. Thou Shalt Not Slow Down the Shell

        This is the binding constraint. Profiling that costs the user more than it gives them is worse than no profiling. We hold ourselves to a budget: the per-file instrumentation must add less than a few microseconds per sourced file, using only shell builtins, with no forks, and writing at most one line per file to a single append-only TSV.

        Where the shell does not support cheap high-resolution timing (classic `/bin/sh`, bash before 5.0), the instrumentation silently degrades to a zero-cost no-op and the probe output plainly says "timings unavailable in this shell". We do not emulate precision we don't have, and we do not slow a user down to collect data we can't collect well.

    2.2. Plain Text, No Ceremony

        Every file dodot writes in service of this feature is plain text (TSV). No JSON, no TOML, no heavyweight schema envelope. A file may begin with one or more `#`-prefixed comment lines — a short `# dodot <name> v1` marker plus a column legend — and that is the extent of any "header" we allow ourselves. Comments must be ignorable by line-oriented readers, so `awk '$1 !~ /^#/'` is a complete parser. The reader is dodot itself and, when the user cares, `awk`. This matters because the shell writer has to be simple enough to be obviously correct by inspection, and because the user should never feel they need dodot to read dodot's own records.

    2.3. Opt-Out, Not Opt-In

        Profiling is on by default. The cost is paid; paying it and discarding the data is silly. Users who object can set `profiling.enabled = false` in the root `.dodot.toml` and the init script is regenerated without the timing wrapper on the next `dodot up`. There is no middle ground and no runtime flag — the instrumentation lives in the generated script or it doesn't.


3. What We Record

    3.1. Per-Run Report

        Each shell-init run writes one file to `<data_dir>/probes/shell-init/`, named:

            profile-<unix-ts>-<pid>-<rand>.tsv

        :: text ::

        The `<unix-ts>` is seconds since epoch (builtin, no fork). The `<pid>` and a small `$RANDOM`-derived suffix give uniqueness across concurrent shells — relying on timestamp alone is not safe, since tmux or a `foot --server` session may fire several inits within the same second.

        The file begins with a short `#`-prefixed preamble (shell, version, init-script path, start timestamp, total elapsed) and then one TSV row per instrumented step:

            phase  pack  handler  target  duration_us  exit_status

        :: text ::

        `phase` distinguishes `path` (a PATH export — near-instant, but recorded for completeness and ordering) from `source` (a sourced shell file). `target` is the absolute path being sourced or added to PATH. `duration_us` is measured with `EPOCHREALTIME` on bash 5+ / zsh, which gives microsecond resolution from a pair of variable reads. `exit_status` is the return code of the sourced script; for PATH entries it is always 0.

        A realistic file on a well-groomed dotfiles install runs 20–40 rows and lands under 4 KB.

        Alongside the TSV, when a sourced file emits anything to stderr the wrapper writes a sibling errors log:

            profile-<unix-ts>-<pid>-<rand>.errors.log

        :: text ::

        Format follows the same plain-text discipline as the TSV — a `# dodot shell-init errors v1` banner, then one record per source-with-stderr in this shape:

            @@<TAB><target><TAB><exit_status>
            <captured stderr line 1>
            <captured stderr line 2>
            ...

        :: text ::

        The `@@` sentinel marks the start of a record; subsequent lines until the next `@@` (or EOF) are the captured stderr verbatim. The wrapper appends a trailing newline after each record so the next header lands on its own line. The file is created lazily — when nothing prints to stderr (the common case), no errors log exists for that run, and the cost on the hot path is one `[ -s ]` test per source. The reader (`probe shell-init <pack>/<file>`, `--errors-only`) loads the sibling automatically when present and silently treats absence as "no errors captured".

    3.2. Deployment Map

        At the end of every `dodot up` and `dodot down`, dodot also writes `<data_dir>/deployment-map.tsv` with one row per deployed artifact:

            pack  handler  source_path  deployed_path

        :: text ::

        Unlike the per-run profile, this file is overwritten, not rotated: there is only ever one current map. It is the operational complement to the init script — the init script is "what shell commands run"; the map is "what files are in play".

        This file's primary consumer is `dodot refresh` from @./magic.lex, which needs to know which source templates back which deployed files so it can mtime-touch the right subset. `dodot probe deployment-map` is simply a friendly reader for the same file.

    3.3. What We Do Not Record

        We do not try to stand in for `zprof`. Deep intra-file profiling — which function inside `aliases.sh` was slow — is outside our scope, and suggesting we solve it would be dishonest. When a user's probe report shows a single 200 ms `source`, the correct next step is pointing them at `zprof` or `zsh-trace` with a one-line note in the probe output.

        We also do not record environment-size growth, function counts, or any other "inside the shell" state. Collecting those cheaply would require running functions inside the user's shell, which violates principle 2.1.


4. Retention

    4.1. Rotation Policy

        The default is `profiling.keep_last_runs = 100`. This is a root-only config key; per-pack scoping is meaningless. At 4 KB per file, 100 runs is ~400 KB — small enough that the number can safely default high, and historical data is genuinely useful for spotting "this got slower last Tuesday" regressions.

        Rotation runs at `dodot up` time, not inside the init script. Pruning inside the init script would require an `ls` or equivalent, which costs a fork; keeping pruning on the write side (dodot, which is already running its own process) is free. The rare user who never runs `dodot up` again will accumulate indefinitely, but that same user has no active profiling audience either — the accumulation is harmless.

    4.2. Concurrent Writers

        Two shells starting at the same instant write to distinct filenames by construction (`<pid>-<rand>`), so they do not collide. We do not use any locking. A partial write from a crashed shell leaves a truncated TSV, which the reader tolerates: rows are independent and a short file is just a short report.


5. The `probe` Command Tree

    5.1. Subcommand Shape

        `probe` is registered as a standout command group following the pattern of the existing `status`/`up`/`down` commands [../crates/dodot-cli/src/main.rs]:

            dodot probe                          # help + summary of last run
            dodot probe shell-init               # detailed timings, last run
            dodot probe shell-init --runs N      # aggregate over last N runs
            dodot probe shell-init --history     # one-row-per-run trend, newest first
            dodot probe shell-init <pack>        # all targets in <pack> across recent runs
            dodot probe shell-init <pack>/<file> # one target's per-run history + stderr
            dodot probe shell-init --errors-only # every failing target, ranked by frequency
            dodot probe show-data-dir            # tree of <data_dir>
            dodot probe deployment-map           # source -> deployed table

        :: text ::

        Each subcommand returns structured data to a Jinja template under `crates/dodot-lib/src/templates/` and is rendered through standout with the existing theme. Table output uses standout's built-in column rendering — the semantic classes in `dodot.css` handle the styling, so no new theme work is needed.

    5.2. `probe shell-init`

        The default output groups timings first by pack and then by handler, with a total per group and a grand total at the bottom:

            PACK         HANDLER  TARGET              TIME     STATUS
            [nvm]        shell    lazy-nvm.sh         2.4 ms   ok
            [git]        shell    aliases.sh          0.3 ms   ok
                         path     bin                 <1 μs    ok
            [brew]       shell    shellenv.sh        18.7 ms   ok
            ---
            total (user)                              21.4 ms
            dodot framing                              0.2 ms
            grand total                               21.6 ms

        :: text ::

        The `--runs N` flag aggregates across the most recent N reports and shows p50 / p95 / max per target. This is the view for spotting regressions: the eye catches a `p95` that is much worse than `p50`. `--history` is the same data collapsed to one row per run, dated, newest first, for a trend glance.

        The positional `<pack>[/<file>]` filter is the drill-down view. Pack-only (`gpg`) lists every target in that pack across the last 20 runs; `<pack>/<file>` (`gpg/env.sh`) narrows to one target. Per-run rows show duration, exit status, and — when the run captured any — the stderr inlined under each row. This is the view that answers "*why* did this fail?" instead of just "did it fail?". The `--errors-only` flag is the inverse query: every target with a non-zero exit somewhere in the window, sorted by failure count desc, so the most-broken file is at the top. Both views read the same `*.errors.log` sidecar described in §3.1.

        A footer always prints "for intra-file profiling, see `zprof` (zsh) or `PS4` tracing (bash)" — this keeps us honest about our scope (§3.3).

    5.3. `probe show-data-dir`

        A tree view of `<data_dir>` with file sizes, limited to a reasonable depth so the output doesn't scroll off-screen. The intent is "I'm debugging; show me what dodot has on disk" — the same role that `brew doctor` serves for Homebrew, minus the diagnostic assertions.

    5.4. `probe deployment-map`

        Renders the source→deployed mapping as a table grouped by pack. The view is derived live from the datastore — not from the on-disk `deployment-map.tsv` — so `dodot probe deployment-map` tells the truth even if the user has never run `dodot up` since the feature shipped, and even if `up`/`down` last ran with `--dry-run`. The TSV on disk (§3.2) is a separate artifact produced by `up`/`down` for machine-to-machine consumers such as `refresh`; both views are derived from the same datastore, and under normal operation they agree.


6. Prior Art

    We looked at what the shell-startup-tuning ecosystem already offers, because duplicating existing tools would be wasteful and because their limitations informed our scope:

    zprof (zsh builtin):
        Function-level profiler loaded via `zmodload zsh/zprof`. Accurate, zero-install, but only profiles function calls — plain inline code in sourced files is invisible. Users have to wrap suspect code in self-destructive functions to make it measurable.

    zsh-trace (ddribin/zsh-trace):
        Uses `xtrace` with a microsecond-formatted `PS4` to capture every executed line, then post-processes into summaries and flamegraphs. Powerful but heavyweight to set up and to read; the output is much richer than most users want.

    chezmoi doctor, brew doctor:
        Diagnostic surfaces shaped like "check for problems and report yes/no". Useful as a framing precedent for our `probe` subcommand (low ceremony, friendly output) but not timing-related.

    yadm, stow:
        No built-in profiling. Users fall back to generic shell profiling tools.

    Our niche sits below zprof/zsh-trace and above "do nothing": we time only the top-level sources dodot itself controls, free of charge, always on. For anything deeper, we point at the specialists.


7. Phased Implementation

    Phase 1: deployment map and `probe show-data-dir`. **(Shipped.)**
        No init-script changes. Wire `deployment-map.tsv` writing into the tail of `commands::up` and `commands::down` alongside `shell::write_init_script`. Ship `probe`, `probe show-data-dir`, and `probe deployment-map`. This alone unblocks `dodot refresh` from @./magic.lex.

    Phase 2: shell-init timing. **(Shipped.)**
        Extend `shell::generate_init_script` to emit the per-file timing wrapper and the preamble writer. Wire `probe shell-init` reader and the default (single-run) view. Add the `profiling` config section. Rotation also lands here (cheaper to ship together than to defer): the writer side has no rotation logic, but `dodot up` prunes `<data_dir>/probes/shell-init/` to `keep_last_runs` at the end of every run.

    Phase 3: aggregation. **(Shipped.)**
        `--runs N` and `--history` flags on `probe shell-init` for cross-run views — p50/p95/max per target (nearest-rank, no interpolation) and a per-run trend table. Rotation already ran in Phase 2, so this phase was purely additive on the reader side. The history row's `failed_entries` column also subsumes part of what regression-hints (now phase 5) would have done: silent source failures across runs are visible at a glance.

    Phase 4: error visibility. **(Shipped.)**
        The exit-status column from Phase 2 turned silent failures into a visible counter, but a count without context still leaves the user reaching for a separate tool. This phase closes that loop:

        - The shell wrapper now redirects each `. file` stderr into a per-shell scratch, re-emits live to the user's TTY (preserving the existing breadcrumb), and on non-empty stderr appends a record to a sibling `*.errors.log` next to the TSV (§3.1). Empty-stderr sources still take the fast path — one `[ -s ]` test of overhead.
        - `dodot probe shell-init <pack>[/<file>]` is the drill-down view: per-run rows with duration, exit, and inlined stderr for one target across recent runs. `--errors-only` is the cross-history "what's broken" listing, sorted by failure count.
        - `dodot status` was already surfacing recent-run failures via `recent_runtime_failures`. The footnote was upgraded to inline a stderr excerpt from the most recent failing run (when captured) and to point at `dodot probe shell-init <pack>/<file>` for the full picture, not at `--history` (which only shows aggregate counts).

        Rotation was extended to remove the sibling errors log alongside its profile so long-running installs don't accumulate orphan sidecars. The `--history` view's row order was also corrected to newest-first, matching every other dated-listing in the tool.

    Phase 5: regression hints (optional, deferred).
        Would highlight targets whose current-run duration is above their historical p95. Pure presentation; same data. Skipped for now — phase 3's tabular view of p95 next to current run already tells the story for users who care to look, and adding automatic flagging is the kind of thing that's better implemented after observing real usage.


8. What This Costs the User

    Being honest about the price tag:

        - roughly 100 microseconds added to shell startup on bash 5+ / zsh, per the instrumentation math in §3.1 (twenty sources × two variable reads × one appended line)
        - a TSV file per shell start under `<data_dir>/probes/`, bounded at the configured retention
        - nothing on shells that do not support `EPOCHREALTIME` — instrumentation compiles out

    Being honest about what this does not cost:

        - no new setup the user has to remember
        - no external dependencies
        - no change to when `dodot up` / `dodot down` fire
        - no network, no background daemon, no watcher

    We think that is a good deal. The cost is bounded and, for most users, imperceptible. The data is there when they want it and invisible when they don't. That is the shape a probe should have.

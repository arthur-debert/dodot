Design Specification: PATH Precedence

    This document specifies how dodot composes the final `$PATH` a shell inherits after login: which contributions win, in what order, and how a raw mutation is preserved and attributed instead of silently overwritten. It extends the pack-ordering contract [./shipped/pack-ordering.lex] to a second cross-pack effect rather than inventing a parallel one. Epic `PAT01`, successor to issue `#260`.

    :: note ::
        *Scope.* Homebrew's own bootstrap — capture, emission order, its `PATH_HELPER_ROOT` behaviour — is settled by RCS01 [./shell-hookup-ergonomics.lex] §4 and is not reopened here; this proposal treats it as a fixed, lower tier. Cross-pack ordering itself is settled by [./shipped/pack-ordering.lex]; this proposal applies that contract to PATH rather than replacing it.
    ::


1. The Problem

    1.1. Two Needs, One Already Solved

        A shell setup needs commands available while it is still initializing, and it needs a predictable final order once initialization finishes. The first need — availability — is closed: Homebrew's environment is captured at `up` time and emitted before any pack [./shell-hookup-ergonomics.lex] §4, and every pack's path-handler `bin/` directory is emitted before any pack's shell scripts source. Both are breadth-first, already shipped, not reopened here.

        The second need — final precedence — is not closed, and is what this document specifies.

    1.2. What Is Actually Broken

        Today's generated init script builds `$PATH` by sequential runtime prepend: one `export PATH="<dir>:$PATH"` line per pack, executed in pack-scan order at shell startup. Three consequences fall out of that mechanism, none of them decided on purpose:

        - No deduplication exists anywhere in the pipeline. Two packs staging the same directory — or a stale `001-homebrew` pack duplicating the now-built-in Homebrew capture — produce two lines, forever.
        - "The last pack processed wins the front of `$PATH`" is true today, but only as an accident of how many prepends happened to run before it, not as a documented or tested rule.
        - A raw `export PATH=...` inside a pack's own shell script changes the final `$PATH` and leaves no trace that it did, or why.


2. Principles

    2.1. One Mechanism, Not Two

        dodot already has exactly one cross-pack ordering mechanism: lexicographic order of the on-disk pack directory name, with the numeric-prefix grammar as its documented escape hatch [./shipped/pack-ordering.lex] §2.2, §5.2. This proposal does not add a second one for PATH. No priority field, no `after:` dependency list, no personal-pack config key that ranks one pack's contributions above another's by declaration rather than by name. If a user wants a bin directory to shadow everything else, they name the pack so it sorts last — same tool, same lesson, already documented.

    2.2. Ordering Is Not Dependency

        dodot does not manage shell-sourcing dependency order, inter-pack or intra-pack, and this proposal does not change that. If one script references a variable or function another script defines, that is an ordinary `source` dependency, solved by naming — the same mechanism and the same answer as pack ordering [./shipped/pack-ordering.lex] §2.3. dodot detects none of this, warns about none of it, and gains no scaffolding to reason about it. PATH composition and script dependency happen to share their ordering primitive; they are not the same problem, and this document only addresses the first.

    2.3. Prepend Direction Is Structural, Not a Style Choice

        Ranking pack contributions above the inherited system `$PATH` requires prepending, not appending — appending would leave system entries winning, which is backwards. Prepending means whichever pack runs last ends up first in the resulting string. That direction is forced by the goal (packs above system), not chosen for its own sake, and it produces a real inversion worth stating in the docs rather than leaving implicit: on-disk reading order and PATH-precedence order run in opposite directions.

        Worked example:

            on disk (ascending):  001-foo  200-bar  baz
            final $PATH:          baz : bar : foo : ...system

        :: text ::

        `001-foo` reads first in a directory listing and loses every collision to `baz`, which reads last. Not the other way round.


3. The Contract

    3.1. The Tiers

        Three tiers, fixed in this order, highest precedence first:

            packs:
                Every pack that stages a directory via the `path` handler, or that mutates `$PATH` in its own shell script. Ranked among each other purely by pack lex order — last pack wins, per §2.3. There is no separate "personal" tier: a pack a user wants to win is a pack they name to sort last.

            homebrew / toolchains:
                Homebrew's captured `shellenv` block. Fixed below every pack, unconditionally — settled by RCS01, not reopened here. No other toolchain (nix, asdf, mise) gets special-cased; a pack that bootstraps one is just a pack, and its contribution lives in the tier above, like any other.

            system:
                Whatever `$PATH` held immediately before dodot's init script ran. Captured once, its internal relative order preserved, placed as the fixed floor.

        :: text ::

    3.2. Deduplication

        A directory that appears more than once resolves to its first occurrence in the tier order above. A stale `001-homebrew` pack that duplicates the built-in Homebrew capture collapses into one entry rather than two — no special-case code, no migration step, straight fallout of dedup existing at all.


4. Composition, Computed Once

    4.1. Today

        `generate_init_script` walks packs in scan order and appends one `export PATH="<dir>:$PATH"` line per staged directory to the script text. The shell executes those lines sequentially at every login, which is what makes composition an accident of prepend count rather than a designed outcome.

    4.2. The Fix

        Compute the whole tiered, deduplicated `$PATH` once, in Rust, at `dodot up` time, and emit it as a single `export PATH="..."` line. Every declared contribution (pack path-handler directories, the cached Homebrew capture) is fully known at generation time — this costs nothing at shell startup beyond what the script already pays. Only the system-tier capture happens at runtime, and it is a single variable read, not logic.

    4.3. One Owner

        Cross-pack PATH ordering and cross-pack shell-source ordering currently agree by coincidence, not by construction: `shell/mod.rs` hardcodes "all PATH lines, then all shell-source lines," while the `ExecutionPhase` enum in `handlers/mod.rs` separately encodes `PathExport < ShellInit` for intra-pack ordering via `grouping.rs`. The new composition function should be the one place this rule lives; nothing else should re-derive it. This is exactly the risk issue `#260`'s planning constraints name under "centralized ownership" — worth fixing here rather than leaving a second implicit copy of the rule to drift.


5. Provenance

    5.1. Why This Is Not Cosmetic

        Once composition stops being sequential runtime prepend, a raw `export PATH=` mutation made by a pack's own shell script has nowhere to land unless something captures it — recomposing purely from the declared tiers in §3.1 would silently drop it, which is a real regression against "no workflow change." Capturing raw contributions is therefore load-bearing for correctness, not only for debugging.

    5.2. Per-Pack Diff, Not Per-Script

        Capture `$PATH` once before a pack's shell scripts run and once after; any new entry is attributed to that pack, tagged as raw rather than declared, and folded into the composition immediately adjacent to that pack's declared entries — same pack, same lex position, one block. This is bounded (one diff per pack with shell scripts, no forks, pure string comparison) and matches the granularity a user would actually want when debugging: which pack, not which line.

    5.3. Portability

        The diff has to run inside whichever real shell sourced the script, so it is the one piece of this design that cannot be precomputed in Rust — and the one place shell differences are real. Zsh does not word-split unquoted parameter expansions by default even when sourcing a `.sh` file (it does not switch into sh-emulation), so a diff written assuming bash-style splitting on `:` can behave differently under zsh. Write it as substring matching (`case ":$PATH:" in *":$dir:"*) ... ;; esac`), which is identical under POSIX sh, bash, and zsh — no `$ZSH_VERSION` branch needed. Reserve that branch for cases where the underlying tool's own output differs per shell, the way Homebrew's `fpath` assignment already does.

    5.4. Surface

        Attribution feeds a probe view — new, or folded into the existing `probe shell-init` family — that answers "where did this entry come from," nothing more. It does not warn, does not classify raw entries as a problem, and does not gate on them. Per §2.2, this is read-only attribution, not a step toward detecting or migrating raw PATH exports.


6. What This Proposal Does Not Build

    - No dependency graph, `after:` field, or priority config — for packs or for PATH specifically. §2.1.
    - No detection, warning, or migration path for raw `export PATH=` lines in pack scripts. They are an ordinary Unix pattern; dodot does not evangelize users out of it.
    - No shell-sourcing dependency detection, intra- or inter-pack. §2.2.
    - No special-casing for individual toolchains (nix, asdf, mise) beyond what a normal pack already gets.


7. Work Stream Hints

    Three streams, sequenced deliberately rather than run independently — WS2 and WS3 each depend on their predecessor landing first.

    1. *Composition* — the core fix: §4's compute-once function, the tier order and dedup from §3, "last pack wins" as an explicit tested rule, the three-pack example from §2.3 as the pinning fixture. Touches `shell/mod.rs` only; no handler or datastore changes.
    2. *Provenance* — §5's per-pack diff and probe surface. The heaviest lift here, and the one piece this epic can ship without: land WS1, then decide separately whether this ships now or later as a standalone follow-up. Sequenced second-to-last on purpose, so that decision does not block documenting what shipped.
    3. *Documentation* — a `pack-ordering.lex`-style contract section using the worked example from §2.3, updated `handlers.lex` / `shell-integration.lex` cross-references, and a troubleshooting note pointing at whatever probe surface WS2 produced — or, if WS2 did not ship, at what is available without it. Sequenced last so it documents what actually shipped, not what was planned.


8. Further Notes

    Related: [./shipped/pack-ordering.lex] (the ordering contract this extends), [./shell-hookup-ergonomics.lex] (Homebrew bootstrap, settled and not reopened), issue `#260` (the tracking issue this Spec answers).

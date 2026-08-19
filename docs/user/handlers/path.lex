:: verified ::
The path handler

Adds a version-controlled directory from your pack to `$PATH`. The matched source directory is staged in the datastore; every pack's staged directory is then composed, once per `dodot up`, into a single `export PATH=` line that the generated `dodot-init.sh` (loaded via `eval "$(dodot init-sh)"` or the managed hookup — see [./../shell-integration.lex]) prepends ahead of your shell's inherited `$PATH`. Where a given pack's directory lands in that composed line, and what wins when two collide, is a fixed rule — [#3].

1. Default claim

    A top-level directory named `bin/` inside the pack. The handler matches source directories only — a file named `bin` flows to the catch-all symlink instead. The directory as a whole goes on `$PATH`; its contents become directly executable from any shell, but each file inside is not handled individually.

    Each pack contributes at most one path-handler directory, so a setup with `git/bin/`, `nvim/bin/`, and `tools/bin/` produces three PATH entries.

2. Configuration

    Under `[mappings]` to rename the matched directory:

        [mappings]
        path = "scripts"

    :: toml ::

    Single string, not a list — the path handler claims one directory name per pack.

    Under `[path]` for handler behaviour:

        [path]
        # Automatically chmod +x files inside path-handler directories
        # on `dodot up`. On by default.
        #
        # Useful because git on macOS defaults to `core.fileMode = false`,
        # so cloned scripts may not have the execute bit set. With this
        # on, dodot ensures every file in the directory is executable;
        # files already executable are left alone, failures report as
        # warnings (not hard errors).
        #
        # Set to false if you have non-executable files in the directory
        # (data files, library scripts sourced by other scripts).
        auto_chmod_exec = true

    :: toml ::

3. Precedence — where a pack's directory lands in the final `$PATH`

    Composition happens once, in three fixed tiers, highest precedence first:

    Packs:
        Every pack's staged `path`-handler directory (this handler), plus any raw `export PATH=` mutation a pack's own shell script makes ([./shell.lex]) — see §3.3. Ranked purely by cross-pack lexicographic order, the same rule that orders everything else across packs ([./execution-order.lex] §2): no separate priority field, and the *last* pack on disk wins the front of `$PATH`. A pack you want to win, you name so it sorts last.

    Homebrew / toolchains:
        Homebrew's captured `shellenv` block, fixed below every pack unconditionally ([./../shell-integration.lex] §3).

    System:
        Whatever `$PATH` your shell inherited before dodot's init script ran, its own internal order preserved, placed as the floor.

    3.1. Reading order runs backwards from precedence order

        Ranking packs above the inherited system `$PATH` requires prepending, not appending — appending would leave system entries winning. Prepending means whichever pack composes *last* ends up *first* in the resulting string, so on-disk reading order and `$PATH`-precedence order run in opposite directions:

            on disk (ascending):  001-foo  200-bar  baz
            final $PATH:          baz : bar : foo : ...system

        :: text ::

        `001-foo` reads first in a directory listing and loses every collision to `baz`, which reads last — not the other way round.

    3.2. Deduplication

        A directory that appears more than once within the packs tier resolves to its first occurrence in pack-precedence order: the highest-precedence pack (last on disk) keeps the entry, and every lower-precedence repeat of the same directory is dropped. Homebrew's `bin`/`sbin` sit outside that ordering — they're excluded from the packs tier before any pack is even considered, so a pack directory that duplicates one of them is always dropped, regardless of that pack's own precedence. Homebrew's single entry at its own fixed tier position is what survives, never a second copy promoted into the packs tier.

    3.3. Raw `$PATH` mutations are attributed, not dropped

        A pack's own shell script can still run a raw `export PATH=...`; dodot does not detect, warn about, or discourage this. Composing `$PATH` once rather than by sequential runtime prepend means the composed line itself never sees such a mutation — instead dodot diffs `$PATH` around each pack's shell-source lines and records what changed, attributing it to that pack (tagged `raw` rather than `declared`) in the provenance view (§3.4) instead of losing track of it.

    3.4. Troubleshooting: where did this directory come from

        `dodot probe shell-init` carries a `PATH provenance` block: every directory attributed to a pack, tagged `declared` (staged via this handler, part of the composed packs tier) or `raw` (§3.3, captured separately at shell startup) — grouped by pack in the same pack order the composed `$PATH` places them, each pack's declared entries immediately followed by that same pack's raw entries ([./../commands/probe.lex] §4.4). That grouping is for attribution; it isn't a claim that a raw entry's position matches where the mutation actually landed in the live `$PATH`. The block only attributes — it never warns or gates on a raw entry.

4. Live edits

    Once a source `bin/` is staged by `dodot up`, new executables you drop into the source directory are immediately runnable from any shell that already has the directory on `$PATH` — the directory is staged, not the individual files inside it. Just make sure new files have the execute bit set; `auto_chmod_exec` handles this on the next `dodot up`, or `chmod +x` by hand.

    Adding a *new* pack with its own `bin/` — or removing a pack — does need another `dodot up` so the init script regenerates with the updated set of PATH entries. New shells then pick up the new `$PATH`.

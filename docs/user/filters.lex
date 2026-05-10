:: verified ::
Filters & ignores — keeping files out of dodot's way

    By default, dodot processes every directory in your dotfiles root and dispatches every source file inside to a handler. Sometimes that's not what you want — a directory of notes, a `*.bak` file, a README that should ship in your repo but not deploy. This doc surveys the five mechanisms for keeping files out of dodot's way, and when to reach for each.

    Two layers, five mechanisms:

    - *Discovery layer* — stop dodot from looking at the file at all (`.dodotignore`, `[pack] ignore`).
    - *Dispatch layer* — let dodot see the file but route it to a filter handler (`[mappings] ignore`, `[mappings] skip`, gates).

    :: note :: Terminology — this doc uses [pack], [handler], [rule]. See [./glossary/] if any are unfamiliar. The handler-side perspective is at [./handlers/controlling-activation.lex]; this is the user-need framing.

1. When you reach for this doc

    - You have a directory in your dotfiles repo that isn't a real pack (notes, scratch, work-in-progress).
    - You have files inside a pack that shouldn't deploy (READMEs, `*.bak`, `TODO.md`, build artifacts).
    - You want a file visible in `dodot status` so you remember it exists, but not deployed.
    - You want a file deployed only on certain hosts (macOS-only, ARM-only, hostname-specific) — that's a *gate*, covered in detail at [./conditional-running.lex] but its relationship to filters is summarized here.

2. The mental model

    Two questions decide which mechanism fits:

    - *How wide is the rule?* Whole pack, or a pattern inside a pack?
    - *Should it show up in `dodot status` or not?*

    Picking from those:

        | Width            | Visible in status? | Mechanism                                |
        | Whole pack       | No                 | `.dodotignore`                           |
        | Pattern in pack  | No                 | `[pack] ignore`                          |
        | Pattern in pack  | No                 | `[mappings] ignore`                      |
        | Pattern in pack  | Yes (`skipped`)    | `[mappings] skip`                        |
        | Pattern in pack  | Yes (`gated out`)  | gate (filename / dir / `[pack] os`)      |
    :: table align=lll ::

    The two pack-pattern ignores look similar but differ in *layer*: `[pack] ignore` stops file scanning inside a pack at discovery time; `[mappings] ignore` lets the file be discovered, then claims it at handler dispatch. In practice the difference rarely matters. Reach for `[pack] ignore` for repo-wide noise (`*.swp`, `.DS_Store`); reach for `[mappings] ignore` for the occasional one-off in a single pack.

3. Whole pack invisible — `.dodotignore`

    A marker file inside a directory tells dodot to skip that directory as a pack. Pure file-presence check; the contents are never read.

        # In your dotfiles root:
        $ touch notes/.dodotignore
        $ dodot list
        ; notes/ no longer appears

    :: shell ::

    Or, equivalently, the dedicated command:

        dodot addignore notes

    :: shell ::

    Useful for directories that live in your repo but aren't meant to be deployed: scratch space, notes, README-only directories, half-migrated state, packs you've turned off but don't want to delete.

    To reverse: `rm <dir>/.dodotignore`. dodot doesn't ship an `addignore --remove`; deletion is by hand on purpose, matching the "git is your history" posture.

    Critical sequencing: if the pack was previously deployed, run `dodot down <pack>` *first*, then add the marker. `up` and `down` only walk *discovered* packs — adding the marker first hides the pack from discovery and leaves the deployed symlinks behind.

4. Whole pattern invisible at scan time — `[pack] ignore`

    A list of glob patterns that pack discovery skips entirely. Defaults cover version-control noise, editor swapfiles, and common build artifacts:

        [pack]
        ignore = [
            ".git", ".svn", ".hg",
            "node_modules", ".DS_Store",
            "*.swp", "*~", "#*#",
            ".env*", ".terraform",
        ]

    :: toml ::

    Override the default list to add or remove patterns. Setting `[pack] ignore = [...]` *replaces* the defaults — re-list the standard noise alongside your additions if you want to keep them.

    `[pack] ignore` is the broadest in-pack hammer. Everything below operates on files dodot has already discovered.

5. Silent drop at dispatch — `[mappings] ignore`

    Glob patterns that drop matched files silently from handler dispatch. No entry in `dodot status`. Same mental model as `.gitignore`:

        [mappings]
        ignore = ["*.bak", "scratch.txt"]

    :: toml ::

    The defaults are empty — common noise is handled by `[pack] ignore` one layer earlier.

    Reach for `[mappings] ignore` when:

    - You want a per-pack drop list (set in `<pack>/.dodot.toml`) without touching the wider `[pack] ignore`.
    - You want the file dropped at dispatch but still discovered (rare, but useful when a separate tool reads the file).

6. Visible drop — `[mappings] skip`

    Same shape as `[mappings] ignore`, but matched files surface in `dodot status` as `skipped`. The defaults cover common docs/legal files:

        [mappings]
        skip = [
            "README", "README.*",
            "LICENSE", "LICENSE.*",
            "CHANGELOG", "CHANGELOG.*",
            "CONTRIBUTING", "CONTRIBUTING.*",
            "AUTHORS", "AUTHORS.*",
            "NOTICE", "NOTICE.*",
            "COPYING", "COPYING.*",
        ]

    :: toml ::

    Matched case-insensitively against the basename.

    Override per-pack to deploy a `README` intentionally:

        # in <pack>/.dodot.toml
        [mappings]
        skip = []

    :: toml ::

    Or replace with your own list:

        [mappings]
        skip = ["TODO.md", "DESIGN.md"]

    :: toml ::

    `skip` is the right answer when you want to *see* in status that a file exists and was deliberately not deployed. `ignore` is the right answer when you want the file invisible.

7. Conditional drop — gates

    A *gate* drops a file only when a host predicate doesn't match. Examples: `install._darwin.sh` is dropped on Linux, anything inside `_darwin/` is dropped on Linux, anything in a pack with `[pack] os = ["darwin"]` is dropped on Linux. In `dodot status` these surface as `gated out (<label>)` with the expected vs actual host facts.

    Gates compose with the rest of the filters: a gated-out file is treated as filtered (no executable intent, status visible).

    The full surface — filename suffix `._<label>`, directory segment `_<label>/`, `[pack] os`, `[mappings.gates]` glob escape hatch — lives at [./conditional-running.lex]. This doc only flags the relationship: gates *are* a filter, but they're conditional on the host rather than unconditional.

8. Order of evaluation

    When more than one filter could match a file:

    - `.dodotignore` short-circuits everything below — the directory isn't scanned at all.
    - `[pack] ignore` runs at scan time, before handler dispatch — matched files never reach the dispatch layer.
    - At dispatch, `ignore` wins over `skip` (silent-drop is the stronger signal).
    - Both `ignore` and `skip` win over precise mappings (`shell`, `install`, …) and the catch-all symlink.
    - Gates run at scan time alongside `[pack] ignore`; a gated-out file behaves like `skip` in status, but for a host-conditional reason.

    For the priority numbers and rule-matcher details, see [./handlers/mappings.lex].

9. Watch out for

    - *Override replaces, doesn't merge.* Setting `[mappings] skip = ["TODO.md"]` drops the README/LICENSE defaults from that scope. Re-list the defaults if you want to keep them.
    - *`.dodotignore` after `down`, not before.* Adding the marker to a previously-deployed pack hides it from discovery; `up` and `down` only reconcile discovered packs. Run `dodot down <pack>` first, then add the marker — otherwise the deployed symlinks stay live.
    - *`[mappings] ignore` is per-pack-or-root.* Like every `[mappings]` key, it's set in `.dodot.toml` and replaces (not merges) at the pack level. Pack-level setting wins for that pack; root-level applies to every pack that doesn't override.
    - *`protected_paths` is separate.* dodot also refuses to symlink a curated list of security-sensitive paths (SSH private keys, `.gnupg`, AWS credentials, …). That's enforced by the symlink handler, not the filter layer — see [./paths.lex] §6.

10. Live edits

    Filter and gate decisions are recomputed on every `dodot up`, `dodot status`, and `dodot down`. There's no persisted "filter state" — edit `[mappings] ignore`, `[mappings] skip`, `[mappings.gates]`, or `[pack] ignore`, and the change takes effect on the next dodot invocation.

    For `[mappings]` filters, `dodot up` reconciles per-pack state on every run, so adding `*.bak` to `[mappings] ignore` cleans up any previously-deployed `*.bak` symlinks for that pack on the next `up`.

    `.dodotignore` is the exception (see §9). The marker hides discovery; reconciliation walks discovery; so `down` *before* adding the marker is the safe sequence.

11. See also

    - [./handlers/controlling-activation.lex] — the same material from the handler side, with dispatch-layer details.
    - [./handlers/mappings.lex] — the priority ladder and config shape for `[mappings]`.
    - [./conditional-running.lex] — gates in full.
    - [./commands/addignore.lex] — the shortcut for dropping `.dodotignore`.
    - [./paths.lex] §6 — `protected_paths`, the security-flavored sibling of these filters.
    - [./glossary/dodotignore.lex], [./glossary/pack.lex], [./glossary/handler.lex].

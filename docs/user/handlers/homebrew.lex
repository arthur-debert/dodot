:: verified ::
The homebrew handler

Runs `brew bundle` against your source `Brewfile` once per content-hash, tracked by a sentinel. Functionally a specialization of the install handler with a more ergonomic default for the common case: "install these packages on every machine I use."

1. Default claim

    A source file named `Brewfile` at the pack root. Single-string match — the homebrew handler claims one Brewfile per pack.

    macOS-only in practice, since `brew` itself is macOS-and-linux-but-mostly-macOS. dodot does not gate the handler by OS; on a host without `brew` installed, the bundle simply fails. Use a `[pack] os` predicate or a `_darwin/` directory-gate if you need the pack itself to no-op on non-mac hosts.

2. Sentinels

    On success, dodot writes a sentinel file `<filename>-<checksum>` into the datastore — for example `Brewfile-a1b2c3d4e5f6a7b8`. The checksum is the first 8 bytes (16 hex chars) of a SHA-256 of the source Brewfile's bytes. Alongside it dodot also writes a sibling file `<filename>-<checksum>.snapshot` containing the Brewfile bytes as they were at the time of that run, so a future `dodot status` can show what changed.

    Same flag set as install:

    - `--no-provision` — skip both install and homebrew handlers entirely on this run.
    - `--provision-rerun` — the canonical "apply pending content edits" escape hatch for run-once handlers. Re-executes install / homebrew even when a sentinel exists. Use it after editing the Brewfile to opt back into running the new content.
    - `--force` — overwrite pre-existing files at symlink target paths. Distinct from `--provision-rerun`; does **not** trigger run-once re-execution.

3. Editing a Brewfile after it ran (the three states)

    When you edit your `Brewfile` after a successful run, dodot does **not** re-run `brew bundle` automatically. The conservative posture is to *notify* and let you decide.

    `dodot up` and `dodot status` report one of three states for the Brewfile:

    - **`brew packages not installed`** — no sentinel exists. `dodot up` will run `brew bundle` on the next invocation.
    - **`brew packages installed`** — a sentinel exists for the *current* content hash. The bundle has run, and the source hasn't changed since. `dodot up` is a no-op.
    - **`brew packages older version (N lines added, M removed)`** — a sentinel exists, but for a *different* content hash. The bundle ran successfully against an earlier version of the Brewfile, and you've edited it since. `dodot up` does not auto-rerun. To apply the edits, run `dodot up --provision-rerun`.

    For sentinels written before the snapshot convention was introduced, the third state shows `brew packages older version (no diff data)` — the run state is still tracked, but dodot has no record of the prior content to summarize what changed. Manual `brew uninstall` of packages the Brewfile still lists likewise stays sticky: the sentinel records "we ran with this content," and dodot considers the work done until the file changes or `--provision-rerun` is passed.

    To inspect the actual diff before deciding to re-run:

        dodot status --diff           # all packs
        dodot status dev --diff       # one pack

    For each `older version` entry, `--diff` prints a unified diff between the snapshot (the bytes that were last successfully run) and the current source.

    Snapshots live alongside sentinels in the handler data dir: `<datastore>/packs/<pack>/homebrew/<filename>-<hash>.snapshot`. If you want to manage state directly, removing the sentinel + snapshot pair flips the file back to `brew packages not installed`.

4. Configuration

    Under `[mappings]`:

        [mappings]
        homebrew = "MyBrewfile"

    :: toml ::

    Single string only — unlike `install`, the homebrew handler claims one filename. There's no dedicated `[homebrew]` section.

5. Live edits

    Edits to the source Brewfile — adding or removing a `brew "..."` line, changing a `cask` — change its content hash. dodot detects the change but **does not re-run `brew bundle` automatically** — instead `dodot status` reports `brew packages older version` and `dodot up` skips it with the same notice. Apply the edits explicitly with `dodot up --provision-rerun`. See section 3 for the full three-state model and `--diff` workflow.

    `brew bundle` itself is mostly idempotent: running it with the same Brewfile installs nothing new and leaves your system as it was. So `--provision-rerun` is cheap if you want to reconfirm; the only cost is brew's own work to check each entry.

    Removing the source Brewfile from the pack stops dodot from running the bundle, but does not uninstall the packages it installed earlier — `brew bundle cleanup` is the brew-side mechanism for that, run by hand against the previous Brewfile.

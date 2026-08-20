:: verified ::
The homebrew handler

Runs `brew bundle` against your source `Brewfile` once per content-hash, tracked by a sentinel. Functionally a specialization of the install handler with a more ergonomic default for the common case: "install these packages on every machine I use."

1. Default claim

    A source file named `Brewfile` at the pack root. Single-string match — the homebrew handler claims one Brewfile per pack.

    macOS-only in practice, since `brew` itself is macOS-and-linux-but-mostly-macOS. dodot does not gate the handler by OS; on a host without `brew` installed, dodot skips the file and says where it looked, rather than running a bundle that cannot work. Use a `[pack] os` predicate or a `_darwin/` directory-gate if you need the pack itself to no-op on non-mac hosts.

2. Sentinels

    On success, dodot writes a sentinel file `<filename>-<checksum>` into the datastore — for example `Brewfile-a1b2c3d4e5f6a7b8`. The checksum is the first 8 bytes (16 hex chars) of a SHA-256 of the source Brewfile's bytes. Alongside it dodot also writes a sibling file `<filename>-<checksum>.snapshot` containing the Brewfile bytes as they were at the time of that run, so a future `dodot status` can show what changed.

    Same flag set as install:

    - `--no-provision` — skip every code-execution handler entirely on this run: install, homebrew, nix, and external. The skipped files still get a row, labelled `skipped (--no-provision)`, so a run you asked to be partial doesn't look like a pack dodot found nothing in.
    - `--provision-rerun` — the canonical "apply pending content edits" escape hatch for the run-once handlers: install, homebrew, and nix. Re-executes them even when a sentinel exists. Use it after editing the Brewfile to opt back into running the new content.
    - `--force` — overwrite pre-existing files at symlink target paths. Distinct from `--provision-rerun`; does **not** trigger run-once re-execution.

3. Editing a Brewfile after it ran (the three states)

    When you edit your `Brewfile` after a successful run, dodot does **not** re-run `brew bundle` automatically. The conservative posture is to *notify* and let you decide.

    `dodot up` and `dodot status` report one of three states for the Brewfile:

    - **`brew packages not installed`** — no sentinel exists. `dodot up` will run `brew bundle` on the next invocation.
    - **`installed`** — a sentinel exists for the *current* content hash. The bundle has run, and the source hasn't changed since. `dodot up` is a no-op.
    - **`brew packages older version (N lines added, M removed)`** — a sentinel exists, but for a *different* content hash. The bundle ran successfully against an earlier version of the Brewfile, and you've edited it since. `dodot up` does not auto-rerun. To apply the edits, run `dodot up --provision-rerun`.

    For sentinels written before the snapshot convention was introduced, the third state shows `brew packages older version (no diff data)` — the run state is still tracked, but dodot has no record of the prior content to summarize what changed. Manual `brew uninstall` of packages the Brewfile still lists likewise stays sticky: the sentinel records "we ran with this content," and dodot considers the work done until the file changes or `--provision-rerun` is passed.

    To inspect the actual diff before deciding to re-run:

        dodot status --diff           # all packs
        dodot status dev --diff       # one pack

    For each `older version` entry, `--diff` prints a unified diff between the snapshot (the bytes that were last successfully run) and the current source.

    Snapshots live alongside sentinels in the handler data dir: `<datastore>/packs/<pack>/homebrew/<filename>-<hash>.snapshot`. If you want to manage state directly, removing the sentinel + snapshot pair flips the file back to `brew packages not installed`.

4. How dodot invokes brew

    The command is `brew bundle --no-upgrade --file <your Brewfile>`, with `HOMEBREW_NO_AUTO_UPDATE=1` set for that invocation only. Both turn off a brew default that would do work you did not ask for in this run:

    - *No upgrades.* `brew bundle` upgrades every outdated formula it encounters by default. dodot passes `--no-upgrade`, so a run installs what the Brewfile declares and leaves the rest of your machine's packages at the versions they were. Upgrading stays something you ask for, with `brew upgrade`.
    - *No auto-update.* The first brew invocation of a day normally runs a `brew update` first — a network round-trip that upgrades brew and its taps and takes seconds before your packages get a look in. dodot suppresses it, so `dodot up` costs what your Brewfile costs. Updating brew stays something you ask for, with `brew update`.

    Suppressing the auto-update has one consequence dodot then has to cover for you. A `Brewfile` may declare `go`, `cargo`, `uv`, `npm`, and `krew` entries alongside `brew` and `cask`, and those entry types need Homebrew 5.1.2 or newer — brew's own auto-update would normally have carried an older installation past that line without you noticing. With auto-update off, an old brew stays old, so before running your `Brewfile` dodot asks `brew --version` and tells you if it is below 5.1.2, with `brew update` as the remedy:

        homebrew at /opt/homebrew/bin/brew is 5.0.16, older than 5.1.2: `Brewfile` entries
        for go, cargo, uv, npm, and krew fail to parse. Run `brew update` to update it.
        dodot runs this file anyway — everything that does not need the newer brew still works.

    :: console ::

    It is a report, not a refusal: dodot does not read your `Brewfile`, so it does not know whether yours uses any of those entry types, and a `Brewfile` of plain `brew` lines runs perfectly on an older installation. The point is that if a line does fail afterwards, you are looking at a named condition with a remedy instead of a parse error out of `brew bundle`.

    A brew at or above the floor says nothing at all, and the question is asked only by `dodot up`, only when a `Brewfile` is actually about to run, and once per machine per run however many packs declare one. `dodot status` and `dodot up --dry-run` never spawn brew.

    The variable is layered onto the environment dodot is running with, not substituted for it: the `brew` process still sees your `PATH`, `HOME`, and the rest of your `HOMEBREW_*` settings. The one exception is `HOMEBREW_NO_AUTO_UPDATE` itself — dodot's value wins for the bundle it runs, so a `HOMEBREW_NO_AUTO_UPDATE=0` you export does not re-enable auto-update here. Run `brew bundle` yourself if you want brew's defaults back for a particular run.

5. Configuration

    Under `[mappings]`:

        [mappings]
        homebrew = "MyBrewfile"

    :: toml ::

    Single string only — unlike `install`, the homebrew handler claims one filename. There's no dedicated `[homebrew]` section.

6. Live edits

    Edits to the source Brewfile — adding or removing a `brew "..."` line, changing a `cask` — change its content hash. dodot detects the change but **does not re-run `brew bundle` automatically** — instead `dodot status` reports `brew packages older version` and `dodot up` skips it with the same notice. Apply the edits explicitly with `dodot up --provision-rerun`. See section 3 for the full three-state model and `--diff` workflow.

    `brew bundle` itself is mostly idempotent: running it with the same Brewfile installs nothing new and leaves your system as it was. So `--provision-rerun` is cheap if you want to reconfirm; the only cost is brew's own work to check each entry.

    Removing the source Brewfile from the pack stops dodot from running the bundle, but does not uninstall the packages it installed earlier — `brew bundle cleanup` is the brew-side mechanism for that, run by hand against the previous Brewfile.

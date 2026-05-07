:: verified ::
The homebrew handler

Runs `brew bundle` against your source `Brewfile` once per content-hash, tracked by a sentinel. Functionally a specialization of the install handler with a more ergonomic default for the common case: "install these packages on every machine I use."

1. Default claim

    A source file named `Brewfile` at the pack root. Single-string match — the homebrew handler claims one Brewfile per pack.

    macOS-only in practice, since `brew` itself is macOS-and-linux-but-mostly-macOS. dodot does not gate the handler by OS; on a host without `brew` installed, the bundle simply fails. Use a `[pack] os` predicate or a `_darwin/` directory-gate if you need the pack itself to no-op on non-mac hosts.

2. Sentinels

    On success, dodot writes a sentinel file `<filename>-<checksum>` into the datastore — for example `Brewfile-a1b2c3d4e5f6a7b8`. The checksum is the first 8 bytes (16 hex chars) of a SHA-256 of the source Brewfile's bytes. The next `dodot up` skips `brew bundle` if the sentinel exists. Edit the source Brewfile and the checksum changes — sentinel name changes — bundle runs again.

    Same two flags as install:

    - `--no-provision` — skip both install and homebrew handlers entirely on this run.
    - `--provision-rerun` — force them to re-run even when sentinels exist.

3. Configuration

    Under `[mappings]`:

        [mappings]
        homebrew = "MyBrewfile"

    :: toml ::

    Single string only — unlike `install`, the homebrew handler claims one filename. There's no dedicated `[homebrew]` section.

4. Live edits

    The homebrew handler is gated. Edits to the source Brewfile — adding or removing a `brew "..."` line, changing a `cask` — change its content hash, which changes its sentinel name, so the next `dodot up` runs `brew bundle` again. Leave it untouched and `up` is a no-op.

    `brew bundle` itself is mostly idempotent: running it again with the same Brewfile installs nothing new and leaves your system as it was. So `--provision-rerun` is cheap if you want to reconfirm; the only cost is brew's own work to check each entry.

    Removing the source Brewfile from the pack stops dodot from running the bundle, but does not uninstall the packages it installed earlier — `brew bundle cleanup` is the brew-side mechanism for that, run by hand against the previous Brewfile.

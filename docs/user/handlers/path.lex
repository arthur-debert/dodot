:: verified ::
The path handler

Adds a version-controlled directory from your pack to `$PATH`. The matched source directory is staged in the datastore; the generated `dodot-init.sh` (which you load with `eval "$(dodot init-sh)"`) emits an `export PATH=` line that prepends the source directory's live location to your shell's `$PATH`.

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

3. Live edits

    Once a source `bin/` is staged by `dodot up`, new executables you drop into the source directory are immediately runnable from any shell that already has the directory on `$PATH` — the directory is staged, not the individual files inside it. Just make sure new files have the execute bit set; `auto_chmod_exec` handles this on the next `dodot up`, or `chmod +x` by hand.

    Adding a *new* pack with its own `bin/` — or removing a pack — does need another `dodot up` so the init script regenerates with the updated set of PATH entries. New shells then pick up the new `$PATH`.

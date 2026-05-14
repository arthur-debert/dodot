:: verified ::
The install handler

Runs your source install script once on this host, tracked by a content-hashed sentinel so it doesn't re-run on every `dodot up`. Use this for machine-specific setup that the other handlers don't cover — language toolchains, window-manager configuration, system defaults, anything that's a one-shot action rather than a continuously-deployed file.

1. Default claims

    Source filenames matched by the `[mappings] install` default:

    - `install.sh`
    - `install.bash`
    - `install.zsh`

    A pack with more than one matched source (e.g. both `install.sh` and `install.zsh`) runs *all of them*, each tracked by its own sentinel. There's no "pick the best one" logic — if you only want one, only ship one.

2. The extension picks the interpreter

    The interpreter comes from the source filename's extension, *not* your login shell:

    - `.sh`, `.bash`, or unknown extension → `bash`
    - `.zsh` → `zsh`

    Each script runs in a fresh subprocess, so your interactive shell state (aliases, functions, options) is invisible to it regardless. The extension is the contract the pack author declares: `install.zsh` announces zsh-specific syntax; `install.sh` announces portability.

3. Sentinels

    On success, dodot writes a sentinel file `<filename>-<checksum>` into the datastore — for example `install.sh-a1b2c3d4e5f6a7b8`. The checksum is the first 8 bytes (16 hex chars) of a SHA-256 of the source script's bytes. Alongside it dodot also writes a sibling file `<filename>-<checksum>.snapshot` containing the script bytes as they were at the time of that run, so a future `dodot status` can show what changed.

    Three flags interact with the gating:

    - `--no-provision` — skip both install and homebrew handlers entirely on this run.
    - `--provision-rerun` — force them to re-run even when sentinels exist. Useful when you want to re-execute without changing the source.
    - `--force` — same effect as `--provision-rerun` for run-once handlers; the canonical "apply pending content edits" escape hatch.

4. Editing an install script after it ran (the three states)

    When you edit `install.sh` after a successful run, dodot does **not** re-run it automatically. Re-running arbitrary user code on every content edit is a surprising default; the conservative posture is to *notify* and let you decide.

    `dodot up` and `dodot status` report one of three states for each install file:

    - **`never run`** — no sentinel exists for this file. `dodot up` will run it on the next invocation.
    - **`installed`** — a sentinel exists for the *current* content hash. The script has run, and the source hasn't changed since. `dodot up` is a no-op.
    - **`older version (N lines added, M removed)`** — a sentinel exists, but for a *different* content hash. The script ran successfully against an earlier version of the file, and you've edited the source since. `dodot up` does not auto-rerun. To apply the edits, run `dodot up --force` (or `dodot up --provision-rerun`).

    For sentinels written before the snapshot convention was introduced, the third state shows `older version (no diff data)` — the run state is still tracked, but dodot has no record of the prior content to summarize what changed.

    To inspect the actual diff before deciding to `--force`:

        dodot status --diff           # all packs
        dodot status nvim --diff      # one pack

    For each `older version` entry, `--diff` prints a unified diff between the snapshot (the bytes that were last successfully run) and the current source.

    Snapshots live alongside sentinels in the handler data dir: `<datastore>/packs/<pack>/install/<filename>-<hash>.snapshot`. They are plain files; if you want to manage state directly, removing the sentinel + snapshot pair flips the file back to `never run`.

5. Output

    `dodot up` keeps install-script output quiet by default. Three things are surfaced live:

    - *Header block.* The contiguous `#`-prefixed comment lines after the optional shebang are printed when the script starts, so you see what's about to run. Document the source script the way you'd want a teammate to read it.
    - *`# status:` markers.* Lines matching `# status: <message>` (or `#status: <message>`) on stdout are printed as live progress while the script runs. Sprinkle them at phase boundaries so a long-running script doesn't look hung.
    - *Failure stderr.* If the script exits non-zero, captured stderr is dumped automatically.

    Pass `--verbose` (or `--debug`) to `dodot up` to also stream the script's raw stdout/stderr in real time — useful when debugging.

    Status markers in a source script:

        #!/bin/bash
        # Install nvm
        # Requires curl

        # status: downloading installer
        curl -sL https://example.com/install.sh -o /tmp/inst
        # status: running installer
        bash /tmp/inst

    :: shell ::

    The convention is tool-agnostic — `# status:` lines are just shell comments when the script is run by hand outside dodot.

6. Configuration

    Under `[mappings]`:

        [mappings]
        install = ["setup.sh", "bootstrap.zsh"]

    :: toml ::

    `install` is list-only, even for a single script — the single-string form does not parse. There's no dedicated `[install]` section.

7. Live edits

    Edits to the source script change its content hash. dodot detects the change but **does not re-run the script automatically** — instead `dodot status` reports `older version` and `dodot up` skips it with the same notice. Apply the edits explicitly with `dodot up --force` (or `--provision-rerun`). See section 4 for the full three-state model and `--diff` workflow.

    Removing a source script from the pack stops dodot from running it, but does not roll back side-effects from prior runs — dodot has no history of what the script did. Cleanup of side-effects is on the script author. Adding a new source script picks it up on the next `dodot up`.

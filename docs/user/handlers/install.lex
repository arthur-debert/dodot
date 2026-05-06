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

    On success, dodot writes a sentinel file `<filename>-<checksum>` into the datastore — for example `install.sh-a1b2c3d4e5f6a7b8`. The checksum is the first 8 bytes (16 hex chars) of a SHA-256 of the source script's bytes. The next `dodot up` skips the script if its sentinel exists. Edit the source and the checksum changes, the sentinel name changes, and the script re-runs automatically.

    Two flags override the gating:

    - `--no-provision` — skip both install and homebrew handlers entirely on this run.
    - `--provision-rerun` — force them to re-run even when sentinels exist. Useful when you want to re-execute without changing the source.

4. Output

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

5. Configuration

    Under `[mappings]`:

        [mappings]
        install = ["setup.sh", "bootstrap.zsh"]

    :: toml ::

    `install` is list-only, even for a single script — the single-string form does not parse. There's no dedicated `[install]` section.

6. Live edits

    The install handler is gated. Edits to the source script change its content hash, which changes its sentinel name, so the next `dodot up` re-runs the script. Leave the source untouched and `up` is a no-op for that script — the existing sentinel matches.

    Removing a source script from the pack stops dodot from running it, but does not roll back side-effects from prior runs — dodot has no history of what the script did. Cleanup of side-effects is on the script author. Adding a new source script picks it up on the next `dodot up`.

:: verified ::
Handler:
    What dodot does with a file once a rule has matched it. Ten handlers ship with dodot today, in three groups.

    Deploy handlers — put files in place, idempotently, on every `dodot up`:

    - `symlink`: links the file into place; the catch-all for anything no other handler claims
    - `shell`: arranges for the file to be sourced at login
    - `path`: puts a directory on `$PATH`

    Code-execution handlers — run something on your machine:

    - `install`: runs a one-shot setup script (`install.sh`), tracked so it doesn't run again
    - `homebrew`: runs `brew bundle` on a `Brewfile`, tracked the same way
    - `nix`: runs `nix profile install` on a `packages.nix` manifest, tracked the same way
    - `external`: fetches the remote files, git repos, and archives declared in `externals.toml`

    The first three of those are the _run-once_ handlers: each records the content it ran, and editing that content does not re-run it — dodot reports `older version` and waits for `dodot up --provision-rerun`.

    Filter handlers — drop matches without deploying:

    - `ignore`: silent drop, like `.gitignore`
    - `skip`: drops, but lists the file as `skipped` in `dodot status` (defaults cover common doc/legal files like `README`, `LICENSE`)
    - `gate`: drops on hosts where the predicate doesn't match (OS, arch, hostname, …)

    Handlers are the built-in vocabulary; you don't author new ones. You point existing ones at the files you want them to claim by editing `[mappings]` in `.dodot.toml`.

:: verified ::
Handler:
    What dodot does with a file once a rule has matched it. Eight handlers ship with dodot today, in two groups.

    Deploy handlers — produce work on the filesystem:

    - `symlink`: links the file into place; the catch-all for anything no other handler claims
    - `shell`: arranges for the file to be sourced at login
    - `path`: puts a directory on `$PATH`
    - `install`: runs a one-shot setup script, tracked so it doesn't re-run
    - `homebrew`: runs `brew bundle` on a `Brewfile`

    Filter handlers — drop matches without deploying:

    - `ignore`: silent drop, like `.gitignore`
    - `skip`: drops, but surfaces as `skipped` in `dodot status` (defaults cover common doc/legal files like `README`, `LICENSE`)
    - `gate`: drops on hosts where the predicate doesn't match (OS, arch, hostname, …)

    Handlers are the built-in vocabulary; you don't author new ones. You point existing ones at the files you want them to claim by editing `[mappings]` in `.dodot.toml`.

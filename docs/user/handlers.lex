:: verified ::
Handlers — index

dodot dispatches each source file in a pack to a handler. The handler decides what dodot will do with that file: link it, source it, run it, drop it. This index points at the per-topic snippets that make up the user-facing handler documentation.

For terminology, see [./glossary/handler.lex].

1. The ten handlers

    Seven deploy handlers, one per snippet:

    :: handler-roster:begin ::

    - `symlink` ([./handlers/symlink.lex]) — link source files into deployed locations. The catch-all.
    - `shell` ([./handlers/shell.lex]) — source shell scripts at login.
    - `path` ([./handlers/path.lex]) — add a source `bin/` directory to `$PATH`; covers the compose-once precedence contract for where each pack's directory lands.
    - `install` ([./handlers/install.lex]) — run a one-shot setup script, tracked by a content hash.
    - `homebrew` ([./handlers/homebrew.lex]) — run `brew bundle` against a source `Brewfile`, tracked by a content hash.
    - `nix` ([./handlers/nix.lex]) — run `nix profile install` against a source `packages.nix`, tracked by a content hash.
    - `external` ([./handlers/external.lex]) — fetch upstream content — a file, a git repo, an archive — declared in a source `externals.toml`, and symlink it into place.

    Three filter handlers, bundled in one snippet because they share a usage story:

    - [./handlers/controlling-activation.lex] — `ignore` (silent drop), `skip` (visible drop), `gate` (host-conditional drop), plus pack-level `[pack] ignore` and `.dodotignore`.

    :: handler-roster:end ::

2. The dispatch model

    - [./handlers/mappings.lex] — how source files map to handlers, the priority ladder, the default mappings table, and how to override them.
    - [./handlers/execution-order.lex] — the order in which handlers run within a pack, plus cross-pack ordering with the `NNN-` prefix grammar (and how that order composes into `$PATH` specifically, [./handlers/path.lex] §3).

3. Concepts

    For the conceptual frame (configuration vs code-execution, idempotency, the trait shape), see [./../reference/handlers.lex]. For the contributor-side reference (registry, intent shapes, datastore layout), see [./../dev/handlers.lex].

    The roster itself — every handler, its phase and category, and what it claims by default — is generated from the code at [./../reference/handler-registry.lex], so it can't drift from the registry.

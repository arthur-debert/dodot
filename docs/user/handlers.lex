:: verified ::
Handlers — index

dodot dispatches each source file in a pack to a handler. The handler decides what dodot will do with that file: link it, source it, run it, drop it. This index points at the per-topic snippets that make up the user-facing handler documentation.

For terminology, see [./glossary/handler.lex].

1. The eight handlers

    Five deploy handlers, one per snippet:

    - [./handlers/symlink.lex] — link source files into deployed locations. The catch-all.
    - [./handlers/shell.lex] — source shell scripts at login.
    - [./handlers/path.lex] — add a source `bin/` directory to `$PATH`.
    - [./handlers/install.lex] — run a one-shot setup script, content-hashed.
    - [./handlers/homebrew.lex] — run `brew bundle` against a source `Brewfile`, content-hashed.

    Three filter handlers, bundled in one snippet because they share a usage story:

    - [./handlers/controlling-activation.lex] — `ignore` (silent drop), `skip` (visible drop), `gate` (host-conditional drop), plus pack-level `[pack] ignore` and `.dodotignore`.

2. The dispatch model

    - [./handlers/mappings.lex] — how source files map to handlers, the priority ladder, the default mappings table, and how to override them.
    - [./handlers/execution-order.lex] — the order in which handlers run within a pack, plus cross-pack ordering with the `NNN-` prefix grammar.

3. Concepts

    For the conceptual frame (configuration vs code-execution, idempotency, the trait shape), see [./../reference/handlers.lex]. For the contributor-side reference (registry, intent shapes, datastore layout), see [./../dev/handlers.lex].

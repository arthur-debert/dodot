# Copilot Instructions — dodot

dodot is a Rust CLI dotfile manager. Cargo workspace; binary crate at
`crates/dodot-cli`; library crates at `crates/dodot-*`.

## Before suggesting a fix

- Run `scripts/check` (umbrella for fmt + clippy + nextest). CI runs the same
  script; suggestions that don't pass it won't merge.
- Never propose changes that leave tests failing.
- Update `CHANGELOG_UNRELEASED.md` for user-visible changes.

## Style and scope

- Keep changes minimal. Don't add features, refactor, or introduce abstractions
  beyond what the task requires.
- No backwards-compatibility hacks: no `// removed` comments, no renaming unused
  vars to `_var`, no shim modules. If something is unused, delete it.
- No fallbacks, defaults, or feature flags unless the PR explicitly asks for them.
- Default to no comments. Well-named identifiers carry the *what*. Reserve
  comments for non-obvious *why* (hidden constraint, workaround, surprising
  invariant).
- Trust internal code and framework guarantees. Only validate at system
  boundaries (user input, external commands, filesystem entry).

## What will get pushed back on

- Suggestions that ignore content under `docs/`.
- Style nits in code that already follows the project's style.
- Defensive error handling for invariants the type system already enforces.
- Comments that restate what the code does.
